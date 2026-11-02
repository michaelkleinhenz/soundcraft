//! Podcast production tools: Tighten Pauses (B4), Auto-Duck (B5) and the episode pre-flight
//! check (B11). All three find speech with Strip Silence's detector on the speaker tracks.

use super::{CommandSpec, always, bad, f32_or, f64_or, range_param, str_param, tracks_param};
use crate::preflight::{analyze_mix, merge, pauses, speaker_tracks, speech, track_loudness};
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_model::{AutoParam, TrackId};
use soundcraft_time::{Range, Samples};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "edit.tighten_pauses",
            "Tighten Pauses...",
            ["Edit"],
            None,
            "{max_pause_ms?: 1500, keep_ms?: 700, threshold_db?: -45, tracks?: speakers (default: mono audio tracks), start?, end?, dry_run?: false} — shortens every pause where nobody speaks to keep_ms, on all speaker tracks at once (ripple), and moves later markers with them",
            always,
            tighten_pauses
        ),
        cmd!(
            "mix.auto_duck",
            "Auto-Duck Music...",
            ["Track"],
            None,
            "{tracks?: what to duck (default: stereo audio tracks), voices?: speaker track ids (default: mono audio tracks), depth_db?: -12, attack_ms?: 300, release_ms?: 800, threshold_db?: -45, start?, end?} — writes volume automation that lowers the tracks while anyone speaks",
            always,
            auto_duck
        ),
        cmd!(
            query "session.preflight",
            "Episode Pre-flight Check...",
            ["File"],
            None,
            "{start?, end?, target?: podcast|podcast_mono|video|broadcast|<LUFS>} — clipping, loudness and true peak against the target, dead air, silence at the start and end, speaker balance",
            always,
            preflight
        ),
    ]
}

/// `start`/`end`, else the edit selection, else the whole session.
fn range_or_session(e: &Engine, id: &str, p: &Value) -> Result<Range> {
    let r = range_param(e, id, p)?;
    Ok(if r.is_empty() { Range::new(0, e.session().content_end()) } else { r })
}

fn ms(e: &Engine, p: &Value, key: &str, default: f64, max: f64) -> Samples {
    e.session().sample_rate.samples(f64_or(p, key, default).clamp(0.0, max) / 1000.0)
}

fn speakers(e: &Engine, id: &str, p: &Value, key: &str) -> Result<Vec<TrackId>> {
    let named = if key == "tracks" { tracks_param(e, id, p)? } else { ids(p, key) };
    let t = if named.is_empty() { speaker_tracks(e.session()) } else { named };
    if t.is_empty() {
        return Err(bad(id, "no speaker tracks: add mono audio tracks with recordings, or name them"));
    }
    Ok(t)
}

fn ids(p: &Value, key: &str) -> Vec<TrackId> {
    p.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(TrackId).collect()).unwrap_or_default()
}

fn tighten_pauses(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "edit.tighten_pauses";
    let max = ms(e, p, "max_pause_ms", 1500.0, 60_000.0).max(1);
    let keep = ms(e, p, "keep_ms", 700.0, 60_000.0).min(max);
    let thr = f32_or(p, "threshold_db", -45.0).clamp(-96.0, 0.0);
    let tracks = speakers(e, id, p, "tracks")?;
    let r = range_or_session(e, id, p)?;
    let found = speech(e.session(), &tracks, r, thr, max)?;
    // Each long pause keeps half of `keep` after the last word and half before the next.
    let cuts: Vec<Range> =
        pauses(&found, max).iter().map(|g| Range::new(g.start + keep / 2, g.end - (keep - keep / 2))).filter(|c| !c.is_empty()).collect();
    let removed: Samples = cuts.iter().map(Range::len).sum();
    let sr = e.session().sample_rate.as_f64();
    let report = json!({"pauses": cuts.len(), "removed_ms": (removed as f64 * 1000.0 / sr).round(), "cuts": cuts.iter().map(|c| [c.start, c.end]).collect::<Vec<_>>()});
    if p.get("dry_run").and_then(Value::as_bool).unwrap_or(false) || cuts.is_empty() {
        return Ok(report);
    }
    let s = e.session_mut();
    // Last to first, so earlier cut positions stay valid.
    for c in cuts.iter().rev() {
        for t in &tracks {
            crate::edit::clear_range(s, *t, *c, true);
        }
        for m in &mut s.markers {
            if m.start >= c.end {
                m.start -= c.len();
                m.end -= c.len();
            }
        }
    }
    Ok(report)
}

fn auto_duck(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "mix.auto_duck";
    let voices = speakers(e, id, p, "voices")?;
    let named = tracks_param(e, id, p)?;
    let s = e.session();
    let ducked: Vec<TrackId> = if named.is_empty() {
        s.tracks
            .iter()
            .filter(|t| t.kind == soundcraft_model::TrackKind::Audio && t.format != soundcraft_model::ChannelFormat::Mono && !voices.contains(&t.id))
            .map(|t| t.id)
            .collect()
    } else {
        named.into_iter().filter(|t| !voices.contains(t)).collect()
    };
    if ducked.is_empty() {
        return Err(bad(id, "nothing to duck: name the music or effects tracks with `tracks`"));
    }
    let depth = f32_or(p, "depth_db", -12.0).clamp(-60.0, 0.0);
    let attack = ms(e, p, "attack_ms", 300.0, 10_000.0);
    let release = ms(e, p, "release_ms", 800.0, 10_000.0);
    let thr = f32_or(p, "threshold_db", -45.0).clamp(-96.0, 0.0);
    let r = range_or_session(e, id, p)?;
    // Short breaths between words don't bring the music back up.
    let talk = speech(s, &voices, r, thr, attack + release)?;
    let held = merge(talk.iter().map(|x| Range::new((x.start - attack).max(r.start), (x.end + release).min(r.end))).collect());
    let s = e.session_mut();
    for t in &ducked {
        let Some(tr) = s.track_mut(*t) else { continue };
        let base = tr.mixer.volume_db;
        let lane = tr.lane_mut(&AutoParam::Volume);
        lane.points.retain(|pt| !(r.start..=r.end).contains(&pt.at));
        lane.set_point(r.start, base);
        for h in &held {
            let down = (h.start + attack).min(h.end);
            let up = (h.end - release).max(down);
            lane.set_point(h.start, base);
            lane.set_point(down, base + depth);
            lane.set_point(up, base + depth);
            lane.set_point(h.end, base);
        }
        lane.set_point(r.end, base);
        lane.enabled = true;
    }
    Ok(json!({"tracks": ducked.iter().map(|t| t.0).collect::<Vec<_>>(), "ducks": held.len()}))
}

fn preflight(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "session.preflight";
    let r = range_or_session(e, id, p)?;
    let target = match str_param(p, "target").or_else(|| p.get("target").and_then(Value::as_f64).map(|_| "")) {
        None => crate::io::LoudnessTarget::PODCAST,
        Some("") => crate::io::LoudnessTarget { lufs: f32_or(p, "target", -16.0), ..crate::io::LoudnessTarget::PODCAST },
        Some(name) => crate::io::LoudnessTarget::preset(name).ok_or_else(|| bad(id, format!("unknown target `{name}`")))?,
    };
    let s = e.session();
    let sr = s.sample_rate.as_f64();
    let secs = |x: Samples| (x as f64 / sr * 10.0).round() / 10.0;
    let stats = analyze_mix(s, r, s.sample_rate.samples(3.0))?;
    let mut issues: Vec<Value> = Vec::new();
    let mut add = |kind: &str, severity: &str, text: String, at: Option<Samples>| {
        issues.push(json!({"kind": kind, "severity": severity, "message": text, "at": at}));
    };
    if stats.clipped > 0 {
        add(
            "clipping",
            "error",
            format!(
                "The mix clips: {} samples at or over 0 dBFS (peak {:.1} dBFS). Lower the master or add a limiter.",
                stats.clipped, stats.peak_db
            ),
            None,
        );
    }
    if stats.lufs.is_finite() && (stats.lufs - target.lufs).abs() > 1.0 {
        add(
            "loudness",
            "warning",
            format!("Loudness is {:.1} LUFS; the target is {:.0}. Bounce with the loudness target to fix it.", stats.lufs, target.lufs),
            None,
        );
    }
    if stats.true_peak_db > target.true_peak_db {
        add("true_peak", "warning", format!("True peak is {:.1} dBTP, over the {:.0} dBTP ceiling.", stats.true_peak_db, target.true_peak_db), None);
    }
    let two = s.sample_rate.samples(2.0);
    if stats.lead_silence >= two {
        add("silence_start", "warning", format!("{} s of silence before anything is heard.", secs(stats.lead_silence)), Some(r.start));
    }
    if stats.tail_silence >= two {
        add("silence_end", "warning", format!("{} s of silence at the end.", secs(stats.tail_silence)), Some(r.end - stats.tail_silence));
    }
    for d in stats.dead_air.iter().take(10) {
        add(
            "dead_air",
            "warning",
            format!("{} s of dead air at {}.", secs(d.len()), s.format_position(d.start, soundcraft_time::TimeFormat::MinSecs)),
            Some(d.start),
        );
    }
    let mut speakers: Vec<Value> = Vec::new();
    let voices = speaker_tracks(s);
    if voices.len() > 1 {
        let levels: Vec<(String, f32)> = voices
            .iter()
            .filter_map(|t| s.track(*t).map(|tr| (tr.name.clone(), track_loudness(s, *t, r))))
            .filter(|(_, l)| l.is_finite() && *l > -70.0)
            .collect();
        if let (Some(lo), Some(hi)) = (levels.iter().min_by(|a, b| a.1.total_cmp(&b.1)), levels.iter().max_by(|a, b| a.1.total_cmp(&b.1)))
            && hi.1 - lo.1 > 3.0
        {
            add("balance", "warning", format!("{} is {:.1} LU louder than {}. Even out their levels.", hi.0, hi.1 - lo.1, lo.0), None);
        }
        speakers = levels.iter().map(|(n, l)| json!({"track": n, "lufs": (*l * 10.0).round() / 10.0})).collect();
    }
    Ok(json!({
        "ok": issues.is_empty(),
        "lufs": stats.lufs, "true_peak_db": stats.true_peak_db, "peak_db": stats.peak_db,
        "target": {"lufs": target.lufs, "true_peak_db": target.true_peak_db},
        "speakers": speakers,
        "issues": issues,
    }))
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    use serde_json::json;
    use soundcraft_model::{AutoParam, ChannelFormat, TrackKind};

    /// A 48 kHz session with one mono speaker whose clip talks (a tone) in `spans` (seconds), and a
    /// stereo music bed for the whole length.
    fn talk(spans: &[(f64, f64)], len: f64) -> Engine {
        let mut e = Engine::default();
        let sr = 48_000usize;
        let mut voice = vec![0.0f32; (len * sr as f64) as usize];
        for (a, b) in spans {
            let (a, b) = ((a * sr as f64) as usize, (b * sr as f64) as usize);
            for (i, v) in voice.iter_mut().enumerate().take(b).skip(a) {
                *v = 0.3 * (i as f32 * 0.05).sin();
            }
        }
        let n = voice.len() as i64;
        let s = e.session_mut();
        let place = |s: &mut soundcraft_model::Session, name: &str, kind: ChannelFormat, channels: Vec<Vec<f32>>| {
            let t = s.add_track(TrackKind::Audio, kind, Some(name));
            let audio = soundcraft_audio_io::AudioBuffer { sample_rate: 48_000, channels };
            let src = crate::io::add_source(s, name, audio, None, soundcraft_audio_io::FileFormat::Wav);
            let clip = soundcraft_model::Clip::audio(s.new_clip_id(), name, src, 0, 0, n);
            crate::edit::place_clip(s, t, clip);
        };
        place(s, "Host", ChannelFormat::Mono, vec![voice]);
        let bed = vec![0.05f32; n as usize];
        place(s, "Music", ChannelFormat::Stereo, vec![bed.clone(), bed]);
        e
    }

    #[test]
    fn tighten_pauses_shortens_long_gaps_and_moves_markers() {
        let mut e = talk(&[(0.0, 1.0), (4.0, 5.0), (5.5, 6.0)], 6.0);
        e.session_mut().add_marker("Outro", soundcraft_model::MarkerKind::Marker, 288_000, 288_000);
        let r = e.execute("edit.tighten_pauses", &json!({"max_pause_ms": 1500, "keep_ms": 500})).unwrap();
        assert_eq!(r["pauses"], 1, "{r}");
        let removed = r["removed_ms"].as_f64().unwrap();
        assert!((2400.0..=2600.0).contains(&removed), "{r}");
        let end = e.session().tracks[0].clips().iter().map(|c| c.end()).max().unwrap();
        assert!((end - (6 * 48_000 - 120_000)).abs() < 4800, "{end}");
        assert!(e.session().markers[0].start < 288_000 - 110_000);
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(e.session().markers[0].start, 288_000);
    }

    #[test]
    fn auto_duck_lowers_the_music_while_someone_speaks() {
        let mut e = talk(&[(2.0, 4.0)], 6.0);
        let r = e.execute("mix.auto_duck", &json!({"depth_db": -10})).unwrap();
        assert_eq!(r["ducks"], 1, "{r}");
        let music = &e.session().tracks[1];
        let lane = music.automation.iter().find(|l| l.param == AutoParam::Volume).unwrap();
        assert_eq!(lane.value_at(48_000, 0.0), 0.0, "music up before the talk");
        assert!((lane.value_at(3 * 48_000, 0.0) + 10.0).abs() < 0.01, "ducked while talking");
        assert_eq!(lane.value_at(5 * 48_000 + 24_000, 0.0), 0.0, "back up after release");
    }

    #[test]
    fn preflight_reports_silence_dead_air_and_loudness() {
        let mut e = talk(&[(0.0, 1.0), (5.0, 6.0)], 9.0);
        e.session_mut().tracks[1].mixer.mute = true;
        let r = e.execute("session.preflight", &json!({"target": "broadcast"})).unwrap();
        let kinds: Vec<&str> = r["issues"].as_array().unwrap().iter().filter_map(|i| i["kind"].as_str()).collect();
        assert!(kinds.contains(&"dead_air") && kinds.contains(&"silence_end") && kinds.contains(&"loudness"), "{r}");
        assert!(!kinds.contains(&"clipping") && !kinds.contains(&"silence_start"), "{r}");
        assert_eq!(r["ok"], false);
    }
}
