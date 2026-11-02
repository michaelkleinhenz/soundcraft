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
        cmd!(
            "markers.add_chapter",
            "Add Chapter Marker",
            ["Edit"],
            Some("M"),
            "{name?, at?} — a marker that starts a chapter, at the playhead while playing, else the insertion point",
            always,
            |e, p| { add_point_marker(e, p, "marker", "Chapter") }
        ),
        cmd!(
            "markers.add_edit",
            "Add Edit Marker",
            ["Edit"],
            Some("E"),
            "{name?, at?, comments?} — a marker for a spot that still needs an edit; not a chapter",
            always,
            |e, p| { add_point_marker(e, p, "edit", "Edit") }
        ),
        cmd!(noundo "markers.next_edit", "Go to Next Edit Marker", ["Edit"], None, "{}", always, |e, _| step_edit_marker(e, true)),
        cmd!(noundo "markers.previous_edit", "Go to Previous Edit Marker", ["Edit"], None, "{}", always, |e, _| step_edit_marker(e, false)),
        cmd!(noundo "file.export_chapters", "Chapters...", ["File", "Export"], None, "{path, format?: json|txt (from the extension), start?, end?, replace?: false} — Podcasting 2.0 chapters JSON or show-notes text", always, export_chapters),
    ]
}

/// A point marker of `kind` named "`prefix` n" (n counts that kind) unless `name` is given.
fn add_point_marker(e: &mut Engine, p: &Value, kind: &str, prefix: &str) -> Result<Value> {
    let mut q = json!({"kind": kind});
    for key in ["at", "comments"] {
        if let Some(v) = p.get(key) {
            q[key] = v.clone();
        }
    }
    let k = if kind == "edit" { soundcraft_model::MarkerKind::Edit } else { soundcraft_model::MarkerKind::Marker };
    let n = e.session().markers.iter().filter(|m| m.kind == k).count() + 1;
    q["name"] = json!(str_param(p, "name").map_or_else(|| format!("{prefix} {n}"), str::to_string));
    super::event::add_marker(e, &q)
}

/// Locates to the next (or previous) edit marker after (or before) the insertion point.
fn step_edit_marker(e: &mut Engine, forward: bool) -> Result<Value> {
    let cur = e.session().edit.selection.start;
    let edits = e.session().markers.iter().filter(|m| m.kind == soundcraft_model::MarkerKind::Edit).map(|m| (m.number, m.start));
    let target =
        if forward { edits.filter(|(_, t)| *t > cur).min_by_key(|(_, t)| *t) } else { edits.filter(|(_, t)| *t < cur).max_by_key(|(_, t)| *t) };
    match target {
        Some((number, _)) => super::event::recall_marker(e, &json!({"number": number})),
        None => Ok(json!({"moved": false})),
    }
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
    fn chapter_and_edit_markers_are_two_kinds_and_only_chapters_export() {
        use soundcraft_model::MarkerKind;
        let mut e = Engine::default();
        e.execute("markers.add_chapter", &json!({"at": 0})).unwrap();
        e.execute("markers.add_edit", &json!({"at": 48_000, "comments": "cough"})).unwrap();
        e.execute("markers.add_chapter", &json!({"at": 96_000})).unwrap();
        e.execute("markers.add_edit", &json!({"at": 144_000})).unwrap();
        let s = e.session();
        let named: Vec<(&str, MarkerKind)> = s.markers.iter().map(|m| (m.name.as_str(), m.kind)).collect();
        assert_eq!(
            named,
            [("Chapter 1", MarkerKind::Marker), ("Edit 1", MarkerKind::Edit), ("Chapter 2", MarkerKind::Marker), ("Edit 2", MarkerKind::Edit)]
        );
        assert_eq!(s.markers[1].comments, "cough");
        let chapters = e.execute("chapters.list", &json!({"start": 0, "end": 192_000})).unwrap();
        let titles: Vec<&str> = chapters["chapters"].as_array().unwrap().iter().filter_map(|c| c["title"].as_str()).collect();
        assert_eq!(titles, ["Chapter 1", "Chapter 2"], "edit markers are not chapters");
        // Next/previous edit marker skip the chapters.
        e.execute("transport.locate", &json!({"at": 0})).unwrap();
        e.execute("edit.select", &json!({"start": 0, "end": 0})).unwrap();
        assert_eq!(e.execute("markers.next_edit", &json!({})).unwrap()["at"], 48_000);
        assert_eq!(e.execute("markers.next_edit", &json!({})).unwrap()["at"], 144_000);
        assert_eq!(e.execute("markers.next_edit", &json!({})).unwrap()["moved"], false);
        assert_eq!(e.execute("markers.previous_edit", &json!({})).unwrap()["at"], 48_000);
        // Undoable like any marker.
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(e.session().markers.len(), 3);
    }

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
