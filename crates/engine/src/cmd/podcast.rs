//! Podcast publishing: the episode details kept with the session, and chapters from markers.
use super::{CommandSpec, always, bad, range_param, str_param};

use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_model::{EpisodeInfo, MAX_EPISODE_TEXT};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "episode.set",
            "Episode Info",
            [],
            None,
            "{show?, title?, author?, season?: number|null, episode?: number|null, date?, description?, artwork?: path (JPEG/PNG), explicit?: bool} — sets the given fields",
            always,
            set_episode
        ),
        cmd!(query "episode.get", "Episode Info", [], None, "{}", always, |e, _| Ok(json!(e.session().episode))),
        cmd!(query "chapters.list", "Chapters", [], None, "{start?, end?} — one chapter per marker in the range (default: the edit selection, else the whole session)", always, |e, p| {
            let (r, chapters) = chapters_for(e, "chapters.list", p)?;
            let s = e.session();
            Ok(json!({"start": r.start, "end": r.end, "chapters": crate::podcast::chapters_json(s, &chapters)["chapters"], "text": crate::podcast::chapters_text(s, &chapters)}))
        }),
        cmd!(noundo "file.export_chapters", "Chapters...", ["File", "Export"], None, "{path, format?: json|txt (from the extension), start?, end?, replace?: false} — Podcasting 2.0 chapters JSON or show-notes text", always, export_chapters),
    ]
}

/// The range an export covers: `start`/`end`, else the edit selection, else the whole session.
fn chapters_for(e: &Engine, id: &str, p: &Value) -> Result<(soundcraft_time::Range, Vec<crate::podcast::Chapter>)> {
    let r = range_param(e, id, p)?;
    let r = if r.is_empty() { soundcraft_time::Range::new(0, e.session().content_end().max(1)) } else { r };
    Ok((r, crate::podcast::chapters(e.session(), r)))
}

fn export_chapters(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "file.export_chapters";
    let path = str_param(p, "path").ok_or_else(|| bad(id, "`path` required"))?.to_string();
    let ext = std::path::Path::new(&path).extension().and_then(std::ffi::OsStr::to_str).unwrap_or("json").to_ascii_lowercase();
    let format = str_param(p, "format").map_or(ext, str::to_ascii_lowercase);
    let (_, chapters) = chapters_for(e, id, p)?;
    if chapters.is_empty() {
        return Err(bad(id, "there are no markers to make chapters from: add one where each chapter starts"));
    }
    let s = e.session();
    let bytes = match format.as_str() {
        "json" => serde_json::to_vec_pretty(&crate::podcast::chapters_json(s, &chapters)).map_err(|err| bad(id, err.to_string()))?,
        "txt" | "text" => crate::podcast::chapters_text(s, &chapters).into_bytes(),
        other => return Err(bad(id, format!("unknown chapters format `{other}` (json or txt)"))),
    };
    super::file::refuse_replacing(e, id, std::path::Path::new(&path), p)?;
    crate::io::write_file(&path, &bytes)?;
    Ok(json!({"path": path, "chapters": chapters.len()}))
}

/// A text field, cut to [`MAX_EPISODE_TEXT`] characters.
fn text(p: &Value, key: &str) -> Option<String> {
    str_param(p, key).map(|t| t.chars().take(MAX_EPISODE_TEXT).collect())
}

/// A number field: a non-negative integer, or null to clear it.
fn number(p: &Value, key: &str) -> Result<Option<Option<u32>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(|n| Some(Some(n)))
            .ok_or_else(|| bad("episode.set", format!("`{key}` is a whole number from 0 or null"))),
    }
}

fn set_episode(e: &mut Engine, p: &Value) -> Result<Value> {
    let mut next: EpisodeInfo = e.session().episode.clone();
    for (key, field) in [
        ("show", &mut next.show),
        ("title", &mut next.title),
        ("author", &mut next.author),
        ("date", &mut next.date),
        ("description", &mut next.description),
        ("artwork", &mut next.artwork),
    ] {
        if let Some(t) = text(p, key) {
            *field = t;
        }
    }
    if let Some(n) = number(p, "season")? {
        next.season = n;
    }
    if let Some(n) = number(p, "episode")? {
        next.episode = n;
    }
    if let Some(v) = p.get("explicit") {
        next.explicit = v.as_bool().ok_or_else(|| bad("episode.set", "`explicit` is true or false"))?;
    }
    e.session_mut().episode = next;
    Ok(json!(e.session().episode))
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    use serde_json::json;

    #[test]
    fn episode_details_are_set_field_by_field_and_undo() {
        let mut e = Engine::default();
        e.execute("episode.set", &json!({"title": "Pilot", "show": "The Show", "episode": 1, "explicit": true})).unwrap();
        e.execute("episode.set", &json!({"season": 2, "episode": null})).unwrap();
        let ep = e.execute("episode.get", &json!({})).unwrap();
        assert_eq!(ep["title"], "Pilot");
        assert_eq!(ep["season"], 2);
        assert_eq!(ep["episode"], serde_json::Value::Null);
        assert_eq!(ep["explicit"], true);
        assert!(e.is_dirty());
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(e.session().episode.episode, Some(1));
        assert!(e.execute("episode.set", &json!({"season": -1})).is_err());
        assert!(e.execute("episode.set", &json!({"explicit": "yes"})).is_err());
    }

    #[test]
    fn chapters_export_as_json_or_text() {
        let mut e = Engine::default();
        let dir = std::env::temp_dir().join(format!("sc-chapters-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let json_path = dir.join("ch.json");
        let err = e.execute("file.export_chapters", &json!({"path": json_path, "end": 48_000})).unwrap_err().to_string();
        assert!(err.contains("no markers"), "{err}");
        let s = e.session_mut();
        s.add_marker("Intro", soundcraft_model::MarkerKind::Marker, 0, 0);
        s.add_marker("Talk", soundcraft_model::MarkerKind::Marker, 96_000, 96_000);
        let r = e.execute("file.export_chapters", &json!({"path": json_path, "end": 480_000})).unwrap();
        assert_eq!(r["chapters"], 2);
        let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&json_path).unwrap()).unwrap();
        assert_eq!(doc["chapters"][1]["startTime"], 2.0);
        assert_eq!(doc["chapters"][1]["endTime"], 10.0);
        let txt = dir.join("ch.txt");
        e.execute("file.export_chapters", &json!({"path": txt, "end": 480_000})).unwrap();
        assert_eq!(std::fs::read_to_string(&txt).unwrap(), "00:00 Intro\n00:02 Talk\n");
        assert!(e.execute("file.export_chapters", &json!({"path": txt, "end": 480_000})).unwrap_err().to_string().contains("already exists"));
        assert!(e.execute("file.export_chapters", &json!({"path": dir.join("c.xml"), "end": 480_000})).is_err());
        let listed = e.execute("chapters.list", &json!({"end": 480_000})).unwrap();
        assert_eq!(listed["chapters"][0]["title"], "Intro");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
