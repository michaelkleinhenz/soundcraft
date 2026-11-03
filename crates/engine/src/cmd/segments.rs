//! Segments (B6): drops a reusable piece of audio (an intro, an outro, a sponsor read, a stinger)
//! into the episode at the insertion point, at a marker or at every ad slot marker, opening room
//! for it on every track (ripple) or laying it over what is there.

use super::{CommandSpec, always, bad, bool_or, f64_or, position_param, str_param, track_param};
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_model::{ChannelFormat, Clip, Fade, MarkerKind, SourceId, TrackId, TrackKind};
use soundcraft_time::{Range, Samples};

/// The track segments go on when none is named; made when missing.
pub const SEGMENTS_TRACK: &str = "Segments";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "segment.insert",
        "Insert Segment...",
        ["Edit"],
        None,
        "{path | source: audio file or source id, name?, at? | marker?: number | ad_slots?: true (every ad slot marker), ripple?: true (opens room on every track; false lays it over), track?: (default: the Segments track, made if missing), fade_ms?: 20} — drops an intro, outro, sponsor read or stinger into the episode",
        always,
        insert_segment
    )]
}

fn insert_segment(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "segment.insert";
    // Where: every ad slot, a marker, a position, else the insertion point.
    let s = e.session();
    let mut spots: Vec<Samples> = if bool_or(p, "ad_slots", false) {
        let ads: Vec<Samples> = s.markers.iter().filter(|m| m.kind == MarkerKind::Ad).map(|m| m.start).collect();
        if ads.is_empty() {
            return Err(bad(id, "there are no ad slot markers: add one where each ad goes"));
        }
        ads
    } else if let Some(n) = p.get("marker").and_then(Value::as_u64) {
        let m = s.markers.iter().find(|m| u64::from(m.number) == n).ok_or_else(|| bad(id, format!("no marker {n}")))?;
        vec![m.start]
    } else {
        vec![position_param(e, id, p, "at")?.unwrap_or(s.edit.selection.start).max(0)]
    };
    spots.sort_unstable();
    spots.dedup();
    let named = track_param(e, id, p, "track")?;
    if let Some(t) = named
        && e.session().track(t).is_none_or(|tr| tr.kind != TrackKind::Audio)
    {
        return Err(bad(id, "segments go on an audio track"));
    }
    // What: a file (decoded once) or a source already in the session.
    let (src, len, name) = match (str_param(p, "path"), p.get("source").and_then(Value::as_u64)) {
        (Some(path), _) => {
            crate::file_access::check_read(std::path::Path::new(path)).map_err(crate::EngineError::Io)?;
            let file = std::path::Path::new(path).file_name().and_then(std::ffi::OsStr::to_str).unwrap_or("Segment").to_string();
            let bytes = crate::io::read_file(path)?;
            let (src, len) = crate::io::import_source(e, &file, &bytes, Some(path))?;
            (src, len, file)
        }
        (None, Some(n)) => {
            let x = e.session().source(SourceId(n)).ok_or_else(|| bad(id, format!("no source {n}")))?;
            (x.id, i64::try_from(x.frames).unwrap_or(0), x.name.clone())
        }
        (None, None) => return Err(bad(id, "`path` or `source` required")),
    };
    if len <= 0 {
        return Err(bad(id, "the segment has no audio"));
    }
    let name = str_param(p, "name")
        .map_or_else(|| std::path::Path::new(&name).file_stem().and_then(std::ffi::OsStr::to_str).unwrap_or("Segment").to_string(), str::to_string);
    let ripple = bool_or(p, "ripple", true);
    let fade = e.session().sample_rate.samples(f64_or(p, "fade_ms", 20.0).clamp(0.0, 10_000.0) / 1000.0).min(len / 2);
    let s = e.session_mut();
    let track = named.unwrap_or_else(|| segments_track(s, src));
    // Last to first, so the earlier spots stay where they are.
    for at in spots.iter().rev() {
        if ripple {
            super::event::insert_time_range(s, Range::new(*at, *at + len), true);
        }
        let mut clip = Clip::audio(s.new_clip_id(), name.clone(), src, 0, *at, len);
        clip.fade_in = Fade { len: fade, ..Fade::default() };
        clip.fade_out = Fade { len: fade, ..Fade::default() };
        crate::edit::place_clip(s, track, clip);
    }
    // With ripple, each earlier insert moved the later spots by `len`.
    let placed: Vec<Samples> = spots.iter().enumerate().map(|(i, at)| if ripple { at + len * i as Samples } else { *at }).collect();
    Ok(json!({"track": track, "source": src, "length": len, "inserted": placed.len(), "at": placed}))
}

/// The Segments track, made (wide enough for `src`, routed like any new track) when missing.
fn segments_track(s: &mut soundcraft_model::Session, src: SourceId) -> TrackId {
    if let Some(t) = s.tracks.iter().find(|t| t.kind == TrackKind::Audio && t.name == SEGMENTS_TRACK) {
        return t.id;
    }
    let channels = s.source(src).map_or(2, |x| x.channels as usize);
    s.add_track(TrackKind::Audio, ChannelFormat::for_channels(channels), Some(SEGMENTS_TRACK))
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    use serde_json::json;
    use soundcraft_model::{ChannelFormat, MarkerKind, TrackKind};

    #[test]
    fn insert_segment_fills_every_ad_slot_and_ripples_the_conversation() {
        let mut e = Engine::default();
        let s = e.session_mut();
        let talk = s.add_track(TrackKind::Audio, ChannelFormat::Mono, Some("Host"));
        let voice = soundcraft_audio_io::AudioBuffer { sample_rate: 48_000, channels: vec![vec![0.1; 480_000]] };
        let v = crate::io::add_source(s, "Host", voice, None, soundcraft_audio_io::FileFormat::Wav);
        let clip = soundcraft_model::Clip::audio(s.new_clip_id(), "Host", v, 0, 0, 480_000);
        crate::edit::place_clip(s, talk, clip);
        let ad = soundcraft_audio_io::AudioBuffer { sample_rate: 48_000, channels: vec![vec![0.2; 48_000]; 2] };
        let src = crate::io::add_source(s, "Sponsor", ad, None, soundcraft_audio_io::FileFormat::Wav);
        e.execute("markers.add_ad", &json!({"at": 96_000})).unwrap();
        e.execute("markers.add_ad", &json!({"at": 240_000})).unwrap();
        e.execute("markers.add_chapter", &json!({"at": 300_000})).unwrap();
        let r = e.execute("segment.insert", &json!({"source": src.0, "ad_slots": true})).unwrap();
        assert_eq!(r["at"], json!([96_000, 288_000]), "{r}");
        let s = e.session();
        let seg = s.tracks.iter().find(|t| t.name == super::SEGMENTS_TRACK).unwrap();
        assert_eq!(seg.format, ChannelFormat::Stereo);
        assert_eq!(seg.clips().iter().map(|c| c.start).collect::<Vec<_>>(), vec![96_000, 288_000]);
        assert!(seg.clips()[0].fade_in.len > 0);
        // The host's recording opened up at both slots and ends two ads later.
        let host = &s.tracks[0];
        assert_eq!(host.clips().iter().map(|c| c.end()).max(), Some(480_000 + 96_000));
        assert!(host.clips().iter().all(|c| !c.range().overlaps(&soundcraft_time::Range::new(96_000, 144_000))));
        // Ad markers stay at the start of their ad; the chapter after them moves.
        let ads: Vec<i64> = s.markers.iter().filter(|m| m.kind == MarkerKind::Ad).map(|m| m.start).collect();
        assert_eq!(ads, vec![96_000, 288_000]);
        assert_eq!(s.markers.iter().find(|m| m.kind == MarkerKind::Marker).map(|m| m.start), Some(396_000));
        // Chapters leave ad slots out.
        let ch = e.execute("chapters.list", &json!({})).unwrap();
        assert_eq!(ch["chapters"].as_array().map(Vec::len), Some(1), "{ch}");
    }

    #[test]
    fn insert_segment_overlays_without_ripple() {
        let mut e = Engine::default();
        let s = e.session_mut();
        let sting = soundcraft_audio_io::AudioBuffer { sample_rate: 48_000, channels: vec![vec![0.2; 4_800]] };
        let src = crate::io::add_source(s, "Sting", sting, None, soundcraft_audio_io::FileFormat::Wav);
        e.execute("markers.add_chapter", &json!({"at": 1_000})).unwrap();
        let r = e.execute("segment.insert", &json!({"source": src.0, "marker": 1, "ripple": false})).unwrap();
        assert_eq!(r["at"], json!([1_000]));
        assert_eq!(e.session().markers[0].start, 1_000);
        assert!(e.execute("segment.insert", &json!({"source": src.0, "ad_slots": true})).is_err());
    }
}
