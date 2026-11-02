//! Episode templates (`crate::templates`) and the export settings they carry.

use super::{CommandSpec, always, bad, bool_or, i64_or, str_param};
use crate::templates::{BUILT_IN, EXTENSION, built_in, from_json, in_dir, to_json};
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_time::SampleRate;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            noundo "template.save",
            "Save as Episode Template",
            [],
            None,
            "{path (.pctemplate), name?, description?, replace?: false} — the session's tracks, routing, plugin chains, export settings and show details, without its recordings",
            always,
            save
        ),
        cmd!(
            query "template.list",
            "Episode Templates",
            [],
            None,
            "{dir?} — the built-in templates and the .pctemplate files in dir",
            always,
            list
        ),
        cmd!(
            noundo "template.new",
            "New from Template",
            [],
            None,
            "{template: interview|solo|panel or a .pctemplate path, name?: 'Untitled', sample_rate? (built-ins: 48000)} — a new session from the template",
            always,
            new_from
        ),
        cmd!(
            "session.export_settings",
            "Export Settings",
            [],
            None,
            "{format?: wav|aiff|flac|mp3, bit_depth?: 16|24|32f, loudness?: podcast|podcast_mono|video|broadcast|off, bitrate?: kbps, normalize?, dither?, clear?: false} — what Bounce Mix offers for this session (templates carry them)",
            always,
            export_settings
        ),
    ]
}

fn save(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "template.save";
    let path = str_param(p, "path").ok_or_else(|| bad(id, "`path` required"))?;
    let file = std::path::Path::new(path).with_extension(EXTENSION);
    super::file::refuse_replacing(e, id, &file, p)?;
    let stem = file.file_stem().and_then(std::ffi::OsStr::to_str).unwrap_or("Template");
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(stem);
    let text = to_json(name, str_param(p, "description").unwrap_or_default(), e.session())?;
    if cfg!(target_arch = "wasm32") {
        crate::io::write_file(&file.to_string_lossy(), text.as_bytes())?;
    } else {
        if let Some(dir) = file.parent() {
            crate::io::make_dir(dir)?;
        }
        crate::io::replace_file(&file.with_extension("pctemplate.tmp"), &file, text.as_bytes())?;
    }
    Ok(json!({"path": file, "name": name}))
}

fn list(_: &mut Engine, p: &Value) -> Result<Value> {
    let mut out: Vec<Value> = BUILT_IN
        .iter()
        .map(|(id, name, description, speakers)| json!({"template": id, "name": name, "description": description, "built_in": true, "speakers": speakers}))
        .collect();
    if let Some(dir) = str_param(p, "dir").filter(|d| !d.is_empty()) {
        for (path, t) in in_dir(std::path::Path::new(dir)) {
            let tracks: Vec<&str> = t.session.tracks.iter().map(|tr| tr.name.as_str()).collect();
            out.push(json!({"template": path, "name": t.name, "description": t.description, "built_in": false, "tracks": tracks}));
        }
    }
    Ok(json!({"templates": out}))
}

fn new_from(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "template.new";
    let which = str_param(p, "template").ok_or_else(|| bad(id, "`template` required"))?;
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Untitled").to_string();
    let rate = || SampleRate::new(i64_or(p, "sample_rate", 48_000).clamp(0, i64::from(u32::MAX)) as u32).map_err(|err| bad(id, err.to_string()));
    let mut s = match built_in(which, &name, rate()?) {
        Some(s) => s,
        None => {
            crate::file_access::check_read(which).map_err(crate::EngineError::Io)?;
            let t = from_json(&crate::io::read_text(which)?)?;
            let mut s = t.session;
            if p.get("sample_rate").is_some() {
                s.sample_rate = rate()?;
            }
            s
        }
    };
    s.name = name;
    e.replace_session(s);
    e.path = None;
    let s = e.session();
    Ok(json!({"name": s.name, "tracks": s.tracks.len(), "sample_rate": s.sample_rate.hz()}))
}

fn export_settings(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "session.export_settings";
    if bool_or(p, "clear", false) {
        e.session_mut().export = None;
        return Ok(json!({"export": null}));
    }
    let mut x = e.session().export.clone().unwrap_or_default();
    if let Some(f) = str_param(p, "format") {
        let fmt = soundcraft_audio_io::FileFormat::from_extension(f)
            .filter(|x| soundcraft_audio_io::FileFormat::EXPORT.contains(x))
            .ok_or_else(|| bad(id, format!("unknown format `{f}` (wav, aiff, flac or mp3)")))?;
        x.format = fmt.default_extension().to_string();
    }
    if let Some(b) = p.get("bit_depth") {
        let b = b.to_string().trim_matches('"').to_string();
        if !["16", "24", "32f"].contains(&b.as_str()) {
            return Err(bad(id, format!("unknown bit depth `{b}` (16, 24 or 32f)")));
        }
        x.bit_depth = b;
    }
    if let Some(l) = str_param(p, "loudness") {
        if l != "off" && crate::io::LoudnessTarget::preset(l).is_none() {
            return Err(bad(id, format!("unknown loudness target `{l}`")));
        }
        x.loudness = l.to_string();
    }
    if p.get("bitrate").is_some() {
        x.bitrate = i64_or(p, "bitrate", 128).clamp(32, 320) as u32;
    }
    x.normalize = bool_or(p, "normalize", x.normalize);
    x.dither = bool_or(p, "dither", x.dither);
    e.session_mut().export = Some(x.clone());
    Ok(json!({"export": x}))
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    use serde_json::json;

    #[test]
    fn a_saved_template_brings_back_tracks_chains_and_export_settings_without_recordings() {
        let mut e = Engine::default();
        e.execute("template.new", &json!({"template": "interview", "name": "Ep 1"})).unwrap();
        e.execute("session.export_settings", &json!({"format": "flac", "bit_depth": 16, "loudness": "broadcast"})).unwrap();
        e.execute("episode.set", &json!({"show": "The Show", "title": "Ep 1", "episode": 1})).unwrap();
        let host = e.session().tracks[0].id;
        if let Some(t) = e.session_mut().track_mut(host) {
            t.mixer.volume_db = -3.5;
        }
        let s = e.session_mut();
        let buf = soundcraft_audio_io::AudioBuffer { sample_rate: 48_000, channels: vec![vec![0.1; 4_800]] };
        let src = crate::io::add_source(s, "take", buf, None, soundcraft_audio_io::FileFormat::Wav);
        let clip = soundcraft_model::Clip::audio(s.new_clip_id(), "take", src, 0, 0, 4_800);
        crate::edit::place_clip(s, host, clip);
        e.execute("markers.add_chapter", &json!({"at": 100})).unwrap();

        let dir = std::env::temp_dir().join(format!("podcraft-templates-{}", std::process::id()));
        let path = dir.join("My Show");
        let r = e.execute("template.save", &json!({"path": path, "description": "Weekly"})).unwrap();
        assert_eq!(r["name"], "My Show");
        assert!(e.execute("template.save", &json!({"path": path})).is_err(), "won't replace without asking");

        let list = e.execute("template.list", &json!({"dir": dir})).unwrap();
        let mine = list["templates"].as_array().unwrap().iter().find(|t| t["name"] == "My Show").unwrap().clone();
        assert_eq!(mine["description"], "Weekly");
        assert_eq!(list["templates"].as_array().unwrap().len(), super::BUILT_IN.len() + 1);

        e.execute("template.new", &json!({"template": mine["template"], "name": "Ep 2"})).unwrap();
        let s = e.session();
        assert_eq!(s.name, "Ep 2");
        assert_eq!(e.path, None);
        assert_eq!(s.tracks[0].name, "Host");
        assert_eq!(s.tracks[0].mixer.volume_db, -3.5);
        assert!(s.tracks[0].mixer.inserts.iter().any(Option::is_some), "the voice chain comes along");
        assert!(s.tracks.iter().all(|t| t.clips().is_empty()) && s.sources.is_empty() && s.markers.is_empty());
        let x = s.export.as_ref().unwrap();
        assert_eq!((x.format.as_str(), x.bit_depth.as_str(), x.loudness.as_str()), ("flac", "16", "broadcast"));
        assert_eq!((s.episode.show.as_str(), s.episode.title.as_str(), s.episode.episode), ("The Show", "", None));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn built_in_templates_and_bad_input() {
        let mut e = Engine::default();
        e.execute("template.new", &json!({"template": "panel"})).unwrap();
        assert_eq!(e.session().tracks.iter().filter(|t| t.format == soundcraft_model::ChannelFormat::Mono).count(), 4);
        assert!(e.execute("template.new", &json!({"template": "/nonexistent/x.pctemplate"})).is_err());
        assert!(e.execute("session.export_settings", &json!({"format": "ogg"})).is_err());
        assert!(crate::templates::from_json("{\"format_version\": 1}").is_err());
    }
}
