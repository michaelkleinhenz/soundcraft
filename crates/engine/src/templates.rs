//! Episode templates: a session's set-up without its recordings. That covers the tracks and their
//! formats, routing, levels, colours and heights, the plugin chains with their settings, busses and
//! groups, the export settings and the show-level episode details. Saved as `.pctemplate` files
//! (JSON) and started from with `template.new`; three built-ins need no file.

use crate::{EngineError, Result};
use serde_json::{Value, json};
use soundcraft_model::Session;
use soundcraft_time::SampleRate;
use std::path::Path;

/// The template file extension.
pub const EXTENSION: &str = "pctemplate";
/// The template file format this build writes and reads.
pub const FORMAT: u64 = 1;

/// Built-in templates: id, name, description, speakers (New Episode's tracks for each).
pub const BUILT_IN: &[(&str, &str, &str, &[&str])] = &[
    ("interview", "Interview", "A host and a guest, music and effects tracks, a voice bus and a limited master.", &["Host", "Guest"]),
    ("solo", "Solo show", "One host, music and effects tracks, a voice bus and a limited master.", &["Host"]),
    (
        "panel",
        "Panel",
        "A host and three guests, music and effects tracks, a voice bus and a limited master.",
        &["Host", "Guest 1", "Guest 2", "Guest 3"],
    ),
];

/// A template read from a file.
#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub description: String,
    pub session: Session,
}

/// `s` without its recordings and timeline content: no clips, sources, videos or markers, no
/// automation points, no selection; the episode keeps the show, author, artwork (made absolute)
/// and explicit flag but not this episode's title, number, date or notes.
pub fn strip(s: &Session) -> Session {
    let mut t = s.clone();
    for tr in &mut t.tracks {
        for pl in &mut tr.playlists {
            pl.clips.clear();
        }
        for lane in &mut tr.automation {
            lane.points.clear();
        }
    }
    t.sources.clear();
    t.videos.clear();
    t.markers.clear();
    t.midi_originals.clear();
    t.auto_names.clear();
    t.pool = Default::default();
    t.edit.selection = soundcraft_time::Range::point(0);
    t.edit.selected_clips.clear();
    t.edit.selected_tracks.clear();
    let ep = &mut t.episode;
    ep.title.clear();
    ep.episode = None;
    ep.date.clear();
    ep.description.clear();
    if !ep.artwork.is_empty()
        && Path::new(&ep.artwork).is_relative()
        && let Some(dir) = &s.folder
    {
        ep.artwork = dir.join(&ep.artwork).to_string_lossy().into_owned();
    }
    t.folder = None;
    t
}

/// The template file's text for `s` (stripped first).
pub fn to_json(name: &str, description: &str, s: &Session) -> Result<String> {
    let session = serde_json::to_value(strip(s)).map_err(|err| EngineError::Io(err.to_string()))?;
    let v = json!({"podcraft_template": FORMAT, "name": name, "description": description, "session": session});
    serde_json::to_string_pretty(&v).map_err(|err| EngineError::Io(err.to_string()))
}

/// Reads a template file's text.
pub fn from_json(text: &str) -> Result<Template> {
    let bad = |why: &str| EngineError::Io(format!("not a PodCraft template: {why}"));
    let v: Value = serde_json::from_str(text).map_err(|err| bad(&err.to_string()))?;
    match v.get("podcraft_template").and_then(Value::as_u64) {
        Some(n) if n <= FORMAT => {}
        Some(n) => return Err(bad(&format!("format {n} is newer than this PodCraft reads"))),
        None => return Err(bad("no template header")),
    }
    let session = v.get("session").ok_or_else(|| bad("no session"))?;
    let text = serde_json::to_string(session).map_err(|err| bad(&err.to_string()))?;
    let session = Session::from_json(&text).map_err(|err| bad(&err.to_string()))?;
    let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    Ok(Template { name: field("name"), description: field("description"), session })
}

/// A built-in template's session.
pub fn built_in(id: &str, name: &str, sample_rate: SampleRate) -> Option<Session> {
    let (_, _, _, speakers) = BUILT_IN.iter().find(|b| b.0 == id)?;
    let speakers: Vec<String> = speakers.iter().map(|s| (*s).to_string()).collect();
    let mut s = crate::podcast::episode_session(name, sample_rate, &speakers);
    s.export = Some(soundcraft_model::ExportSettings::default());
    Some(s)
}

/// The templates in `dir` (its `.pctemplate` files), by name; unreadable ones are left out.
pub fn in_dir(dir: &Path) -> Vec<(std::path::PathBuf, Template)> {
    let mut out: Vec<(std::path::PathBuf, Template)> = crate::io::files_in(dir, EXTENSION)
        .into_iter()
        .filter_map(|p| {
            let t = crate::io::read_text(&p).ok().and_then(|text| from_json(&text).ok())?;
            Some((p, t))
        })
        .collect();
    out.sort_by_key(|a| a.1.name.to_lowercase());
    out
}
