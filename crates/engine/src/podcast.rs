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

// ---- chapters ---------------------------------------------------------------------------------

/// A chapter of an export: where it starts and ends relative to the export's start, its title and
/// an optional link.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Chapter {
    pub start: soundcraft_time::Samples,
    pub end: soundcraft_time::Samples,
    pub title: String,
    pub url: Option<String>,
}

/// The first `http(s)://` word of a marker's comments: its chapter link.
fn chapter_url(comments: &str) -> Option<String> {
    comments.split_whitespace().find(|w| w.starts_with("https://") || w.starts_with("http://")).map(str::to_string)
}

/// The chapters of an export of `range`: one per marker (or stored selection) starting inside it,
/// in time order, each running to the next one or the end of the range. Markers at the same
/// position collapse into the first; times are relative to `range.start`.
pub fn chapters(s: &Session, range: soundcraft_time::Range) -> Vec<Chapter> {
    let mut marks: Vec<&soundcraft_model::MemoryLocation> = s
        .markers
        .iter()
        .filter(|m| matches!(m.kind, soundcraft_model::MarkerKind::Marker | soundcraft_model::MarkerKind::Selection))
        .filter(|m| m.start >= range.start && m.start < range.end)
        .collect();
    marks.sort_by_key(|m| m.start);
    marks.dedup_by_key(|m| m.start);
    let mut out: Vec<Chapter> = marks
        .iter()
        .map(|m| Chapter { start: m.start - range.start, end: range.len(), title: m.name.clone(), url: chapter_url(&m.comments) })
        .collect();
    let starts: Vec<soundcraft_time::Samples> = out.iter().skip(1).map(|c| c.start).collect();
    for (c, next) in out.iter_mut().zip(starts) {
        c.end = next;
    }
    out
}

/// Seconds, rounded to the millisecond.
fn seconds(s: &Session, at: soundcraft_time::Samples) -> f64 {
    (s.sample_rate.seconds(at) * 1000.0).round() / 1000.0
}

/// Podcasting 2.0 chapters JSON (`application/json+chapters`, version 1.2.0).
pub fn chapters_json(s: &Session, chapters: &[Chapter]) -> serde_json::Value {
    let list: Vec<serde_json::Value> = chapters
        .iter()
        .map(|c| {
            let mut v = serde_json::json!({"startTime": seconds(s, c.start), "endTime": seconds(s, c.end), "title": c.title});
            if let (Some(url), Some(obj)) = (&c.url, v.as_object_mut()) {
                obj.insert("url".into(), serde_json::Value::String(url.clone()));
            }
            v
        })
        .collect();
    let mut doc = serde_json::json!({"version": "1.2.0", "chapters": list});
    if let Some(obj) = doc.as_object_mut() {
        for (key, value) in [("title", &s.episode.title), ("podcastName", &s.episode.show), ("author", &s.episode.author)] {
            if !value.is_empty() {
                obj.insert(key.into(), serde_json::Value::String(value.clone()));
            }
        }
    }
    doc
}

/// Chapters as show-notes lines (`00:00 Intro`, or `0:00:00 Intro` past an hour): the form video
/// sites and most podcast apps turn into links.
pub fn chapters_text(s: &Session, chapters: &[Chapter]) -> String {
    let long = chapters.iter().any(|c| s.sample_rate.seconds(c.end) >= 3600.0);
    let mut out = String::new();
    for c in chapters {
        let total = s.sample_rate.seconds(c.start).max(0.0).floor() as u64;
        let (h, m, sec) = (total / 3600, (total / 60) % 60, total % 60);
        let stamp = if long { format!("{h}:{m:02}:{sec:02}") } else { format!("{:02}:{sec:02}", total / 60) };
        out.push_str(&stamp);
        out.push(' ');
        out.push_str(&c.title);
        if let Some(url) = &c.url {
            out.push_str(" — ");
            out.push_str(url);
        }
        out.push('\n');
    }
    out
}

// ---- ID3 tags ---------------------------------------------------------------------------------

/// Milliseconds for a tag, saturating at what ID3 can hold.
fn millis(s: &Session, at: soundcraft_time::Samples) -> u32 {
    let ms = (s.sample_rate.seconds(at.max(0)) * 1000.0).round();
    if ms.is_finite() && ms < f64::from(u32::MAX) { ms as u32 } else { u32::MAX - 1 }
}

/// The artwork file `path` names, absolute or relative to the session folder.
pub fn artwork_path(s: &Session, path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(path.trim());
    match &s.folder {
        Some(dir) if p.is_relative() => dir.join(p),
        _ => p.to_path_buf(),
    }
}

/// The ID3 tag an MP3 export of `range` carries: the episode details, its cover art and the
/// chapters inside the range.
pub fn id3_tag(s: &Session, range: soundcraft_time::Range) -> crate::Result<soundcraft_audio_io::id3::Id3Tag> {
    let e = &s.episode;
    let artwork = if e.artwork.trim().is_empty() {
        None
    } else {
        let path = artwork_path(s, &e.artwork);
        let len = std::fs::metadata(&path).map_err(|err| crate::EngineError::Io(format!("cover art {}: {err}", path.display())))?.len();
        if len > soundcraft_audio_io::id3::MAX_ARTWORK_BYTES as u64 {
            return Err(crate::EngineError::Io(format!("cover art {} is larger than 10 MiB", path.display())));
        }
        let bytes = std::fs::read(&path).map_err(|err| crate::EngineError::Io(format!("cover art {}: {err}", path.display())))?;
        Some(
            soundcraft_audio_io::id3::Id3Artwork::from_bytes(bytes)
                .map_err(|err| crate::EngineError::Io(format!("cover art {}: {err}", path.display())))?,
        )
    };
    let mut comment = e.description.clone();
    if e.explicit && !comment.is_empty() {
        comment.insert_str(0, "[Explicit] ");
    }
    Ok(soundcraft_audio_io::id3::Id3Tag {
        title: e.title.clone(),
        album: e.show.clone(),
        artist: e.author.clone(),
        track: e.episode,
        disc: e.season,
        year: e.year().unwrap_or_default().to_string(),
        comment,
        genre: if e.is_empty() { String::new() } else { "Podcast".into() },
        artwork,
        chapters: chapters(s, range)
            .into_iter()
            .map(|c| soundcraft_audio_io::id3::Id3Chapter { start_ms: millis(s, c.start), end_ms: millis(s, c.end), title: c.title, url: c.url })
            .collect(),
    })
}

#[cfg(test)]
mod chapter_tests {
    use super::*;
    use soundcraft_model::MarkerKind;
    use soundcraft_time::Range;

    fn session() -> Session {
        let mut s = Session::new("Ep", SampleRate::HZ_48000);
        let sec = |x: f64| s.sample_rate.samples(x);
        let (a, b, c, d) = (sec(0.0), sec(65.5), sec(65.5), sec(130.0));
        s.add_marker("Intro", MarkerKind::Marker, a, a);
        let id = s.add_marker("News", MarkerKind::Marker, b, b);
        if let Some(m) = s.markers.iter_mut().find(|m| m.id == id) {
            m.comments = "see https://example.com/news for more".into();
        }
        s.add_marker("Duplicate", MarkerKind::Marker, c, c);
        s.add_marker("Outro", MarkerKind::Selection, d, d + 10);
        s
    }

    #[test]
    fn markers_become_chapters_that_run_to_the_next_one() {
        let s = session();
        let end = s.sample_rate.samples(150.0);
        let ch = chapters(&s, Range::new(0, end));
        let titles: Vec<&str> = ch.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Intro", "News", "Outro"], "markers at one position collapse into the first");
        assert_eq!(ch[0].end, ch[1].start);
        assert_eq!(ch[2].end, end);
        assert_eq!(ch[1].url.as_deref(), Some("https://example.com/news"));
        // Relative to the range start; markers before it are left out.
        let later = chapters(&s, Range::new(s.sample_rate.samples(60.0), end));
        assert_eq!(later.first().map(|c| (c.title.as_str(), c.start)), Some(("News", s.sample_rate.samples(5.5))));
    }

    #[test]
    fn chapters_export_as_podcasting_json_and_show_notes() {
        let mut s = session();
        s.episode.title = "Ep 1".into();
        let ch = chapters(&s, Range::new(0, s.sample_rate.samples(150.0)));
        let json = chapters_json(&s, &ch);
        assert_eq!(json["version"], "1.2.0");
        assert_eq!(json["title"], "Ep 1");
        assert_eq!(json["chapters"][1]["startTime"], 65.5);
        assert_eq!(json["chapters"][1]["url"], "https://example.com/news");
        assert!(json["chapters"][0].get("url").is_none());
        assert_eq!(chapters_text(&s, &ch), "00:00 Intro\n01:05 News — https://example.com/news\n02:10 Outro\n");
        let long = chapters(&s, Range::new(0, s.sample_rate.samples(4000.0)));
        assert!(chapters_text(&s, &long).starts_with("0:00:00 Intro\n0:01:05 News"));
    }

    #[test]
    fn the_id3_tag_carries_the_episode_and_its_chapters() {
        let mut s = session();
        s.episode.title = "Ep 1".into();
        s.episode.show = "Show".into();
        s.episode.episode = Some(1);
        s.episode.date = "2026-11-02".into();
        let tag = id3_tag(&s, Range::new(0, s.sample_rate.samples(150.0))).unwrap();
        assert_eq!(
            (tag.title.as_str(), tag.album.as_str(), tag.track, tag.year.as_str(), tag.genre.as_str()),
            ("Ep 1", "Show", Some(1), "2026", "Podcast")
        );
        assert_eq!(tag.chapters.iter().map(|c| c.start_ms).collect::<Vec<_>>(), [0, 65_500, 130_000]);
        assert_eq!(tag.chapters[2].end_ms, 150_000);
        s.episode.artwork = "/definitely/missing.jpg".into();
        assert!(id3_tag(&s, Range::new(0, 10)).unwrap_err().to_string().contains("cover art"));
    }
}
