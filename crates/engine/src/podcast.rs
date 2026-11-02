//! The New Episode template: a session laid out for recording and mixing a podcast episode.
//!
//! One mono track per speaker, each with a speech chain (high-pass EQ, gentle expander,
//! compressor, de-esser) feeding a Voices bus; Music and SFX tracks; and a master with a limiter
//! whose ceiling leaves room for the true-peak limit podcast platforms ask for.

use soundcraft_model::{ChannelFormat, Insert, Route, Session, TrackKind};
use soundcraft_time::SampleRate;

/// The speakers a template gets when none are named.
pub const DEFAULT_SPEAKERS: &[&str] = &["Host", "Guest"];
/// The most speaker tracks a template creates.
pub const MAX_SPEAKERS: usize = 8;

/// A built-in plugin insert with its defaults, then `params` over them (unknown ids are skipped).
fn insert(id: &str, params: &[(&str, f32)]) -> Insert {
    let mut i = Insert::new(id);
    if let Some(info) = soundcraft_dsp::plugin_info(id) {
        for p in info.params {
            i.params.insert(p.id.to_string(), p.default);
        }
        for (k, v) in params {
            if info.params.iter().any(|p| p.id == *k) {
                i.params.insert((*k).to_string(), *v);
            }
        }
    }
    i
}

/// The speech chain on every speaker track, in insert order: plugin id and the parameters set
/// over its defaults.
const VOICE_CHAIN: &[(&str, &[(&str, f32)])] = &[
    // Rumble and handling noise out; a little mud out; a touch of presence.
    (
        "eq_7band",
        &[("hpf_on", 1.0), ("hpf_freq", 80.0), ("low_mid_freq", 300.0), ("low_mid_gain", -2.0), ("high_mid_freq", 4000.0), ("high_mid_gain", 1.5)],
    ),
    // Turns room noise and headphone bleed down between phrases without chopping words.
    ("expander_gate", &[("threshold", -50.0), ("ratio", 2.0), ("range", 12.0), ("attack", 2.0), ("hold", 150.0), ("release", 250.0)]),
    // Evens out loud and quiet passages.
    ("compressor", &[("threshold", -24.0), ("ratio", 3.0), ("attack", 10.0), ("release", 120.0), ("knee", 6.0), ("makeup", 4.0)]),
    ("de_esser", &[("freq", 6500.0), ("range", 6.0), ("threshold", -30.0)]),
];

/// The master's limiter: its ceiling leaves headroom for a -1 dBTP true-peak limit.
const MASTER_LIMITER: (&str, &[(&str, f32)]) = ("maximizer", &[("threshold", -3.0), ("ceiling", -1.5), ("release", 100.0)]);

/// The speech chain on every speaker track, in insert order.
pub fn voice_chain() -> Vec<Insert> {
    VOICE_CHAIN.iter().map(|(id, params)| insert(id, params)).collect()
}

/// Speaker names from `names`: trimmed, empty ones dropped, at most [`MAX_SPEAKERS`]; the
/// defaults when nothing is left.
pub fn speaker_names(names: &[String]) -> Vec<String> {
    let named: Vec<String> = names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()).take(MAX_SPEAKERS).map(str::to_string).collect();
    if named.is_empty() { DEFAULT_SPEAKERS.iter().map(|s| (*s).to_string()).collect() } else { named }
}

/// A new episode session named `name` with one speaker track per entry of `speakers` (see
/// [`speaker_names`]).
pub fn episode_session(name: &str, sample_rate: SampleRate, speakers: &[String]) -> Session {
    let mut s = Session::new(name, sample_rate);
    let voices_bus = s.add_bus("Voices", ChannelFormat::Stereo);
    let mut first = None;
    for (i, speaker) in speaker_names(speakers).iter().enumerate() {
        let t = s.add_track(TrackKind::Audio, ChannelFormat::Mono, Some(speaker));
        first.get_or_insert(t);
        if let Some(tr) = s.track_mut(t) {
            tr.mixer.output = Route::Bus(voices_bus);
            tr.color = soundcraft_model::TRACK_COLORS[(2 + i) % soundcraft_model::TRACK_COLORS.len()];
            for (slot, ins) in tr.mixer.inserts.iter_mut().zip(voice_chain()) {
                *slot = Some(ins);
            }
        }
    }
    let voices = s.add_track(TrackKind::Aux, ChannelFormat::Stereo, Some("Voices"));
    if let Some(tr) = s.track_mut(voices) {
        tr.mixer.input = Route::Bus(voices_bus);
    }
    for (name, db) in [("Music", -12.0f32), ("SFX", -6.0)] {
        let t = s.add_track(TrackKind::Audio, ChannelFormat::Stereo, Some(name));
        if let Some(tr) = s.track_mut(t) {
            tr.mixer.volume_db = db;
        }
    }
    let master = s.add_track(TrackKind::Master, ChannelFormat::Stereo, Some("Master"));
    if let Some(tr) = s.track_mut(master) {
        // `add_track` numbers generic names ("Master 1"); the only master needs no number.
        tr.name = "Master".into();
        if let Some(slot) = tr.mixer.inserts.first_mut() {
            *slot = Some(insert(MASTER_LIMITER.0, MASTER_LIMITER.1));
        }
    }
    s.edit.main_counter = soundcraft_time::TimeFormat::MinSecs;
    s.edit.rulers = vec!["min_secs".into(), "markers".into()];
    s.edit.selected_tracks = first.into_iter().collect();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_default_episode_has_host_and_guest_with_a_voice_chain() {
        let s = episode_session("Episode 1", SampleRate::HZ_48000, &[]);
        let tracks: Vec<(&str, TrackKind)> = s.tracks.iter().map(|t| (t.name.as_str(), t.kind)).collect();
        assert_eq!(
            tracks,
            [
                ("Host", TrackKind::Audio),
                ("Guest", TrackKind::Audio),
                ("Voices", TrackKind::Aux),
                ("Music", TrackKind::Audio),
                ("SFX", TrackKind::Audio),
                ("Master", TrackKind::Master)
            ]
        );
        let bus = s.bus_by_name("Voices").unwrap().id;
        for name in ["Host", "Guest"] {
            let t = s.tracks.iter().find(|t| t.name == name).unwrap();
            assert_eq!(t.format, ChannelFormat::Mono);
            assert_eq!(t.mixer.output, Route::Bus(bus));
            let chain: Vec<&str> = t.mixer.inserts.iter().flatten().map(|i| i.plugin.as_str()).collect();
            assert_eq!(chain, ["eq_7band", "expander_gate", "compressor", "de_esser"]);
        }
        let host = &s.tracks[0];
        let eq = host.mixer.inserts[0].as_ref().unwrap();
        assert_eq!(eq.params.get("hpf_on"), Some(&1.0));
        assert_eq!(eq.params.get("hpf_freq"), Some(&80.0));
        let master = s.tracks.last().unwrap();
        assert_eq!(master.mixer.inserts[0].as_ref().map(|i| i.plugin.as_str()), Some("maximizer"));
        assert_eq!(s.edit.rulers, ["min_secs", "markers"]);
    }

    #[test]
    fn speakers_are_named_trimmed_and_capped() {
        assert_eq!(speaker_names(&names(&[" Anna ", "", "Ben"])), ["Anna", "Ben"]);
        assert_eq!(speaker_names(&names(&["", "  "])), ["Host", "Guest"]);
        let many: Vec<String> = (0..20).map(|i| format!("S{i}")).collect();
        assert_eq!(speaker_names(&many).len(), MAX_SPEAKERS);
        let s = episode_session("Solo", SampleRate::HZ_48000, &names(&["Me"]));
        assert_eq!(s.tracks.iter().filter(|t| t.format == ChannelFormat::Mono).count(), 1);
    }

    #[test]
    fn every_template_parameter_is_one_the_plugin_has() {
        for (id, params) in VOICE_CHAIN.iter().chain([&MASTER_LIMITER]) {
            let info = soundcraft_dsp::plugin_info(id).unwrap();
            for (k, v) in *params {
                let p = info.params.iter().find(|p| p.id == *k).unwrap_or_else(|| panic!("{id} has no `{k}`"));
                assert!((p.min..=p.max).contains(v), "{id}.{k} = {v} is outside {}..={}", p.min, p.max);
            }
        }
    }
}
