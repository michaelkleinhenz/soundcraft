//! The podcast production tools' window: Tighten Pauses and Insert Segment (Edit menu), Auto-Duck
//! Music and Sync Remote Recordings (Track menu) and the Episode Pre-flight Check (File menu).

use super::SoundApp;
use crate::theme::Tokens;
use egui::vec2;
use serde_json::{Value, json};

/// Which tool the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodcastTool {
    Pauses,
    Duck,
    Preflight,
    Sync,
    Segments,
}

/// What a segment in the library is for.
pub const SEGMENT_KINDS: &[&str] = &["Intro", "Outro", "Sponsor", "Stinger", "Other"];

/// One entry of Insert Segment's library: an audio file and what it is for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LibrarySegment {
    pub name: String,
    pub kind: String,
    pub path: String,
}

fn id(name: &str) -> egui::Id {
    egui::Id::unique(("podcast_tools", name))
}

/// A remembered slider value for this window.
fn slider(ui: &mut egui::Ui, label: &str, key: &str, range: std::ops::RangeInclusive<f32>, default: f32, suffix: &str) -> f32 {
    let mut v: f32 = ui.ctx().data(|d| d.get_temp(id(key))).unwrap_or(default);
    // The slider's own text names it (and its value box) for screen readers.
    ui.add(egui::Slider::new(&mut v, range).suffix(suffix).text(label));
    ui.end_row();
    ui.ctx().data_mut(|d| d.insert_temp(id(key), v));
    v
}

fn run(app: &mut SoundApp, cmd: &str, params: Value) {
    match app.run(cmd, params) {
        Ok(v) => app.ui.podcast_report = Some(v),
        Err(e) => app.ui.status = e,
    }
}

pub(super) fn podcast_tools(app: &mut SoundApp, ctx: &egui::Context) {
    let Some(tool) = app.ui.podcast_tools else { return };
    let title = match tool {
        PodcastTool::Pauses => "Tighten Pauses",
        PodcastTool::Duck => "Auto-Duck Music",
        PodcastTool::Preflight => "Episode Pre-flight Check",
        PodcastTool::Sync => "Sync Remote Recordings",
        PodcastTool::Segments => "Insert Segment",
    };
    let mut open = true;
    egui::Window::new(title).id(id("window")).open(&mut open).default_size(vec2(420.0, 320.0)).collapsible(false).show(ctx, |ui| match tool {
        PodcastTool::Pauses => pauses(app, ui),
        PodcastTool::Duck => duck(app, ui),
        PodcastTool::Preflight => preflight(app, ui),
        PodcastTool::Sync => sync(app, ui),
        PodcastTool::Segments => segments(app, ui),
    });
    if !open {
        app.ui.podcast_tools = None;
        app.ui.podcast_report = None;
    }
}

fn dim(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().color(Tokens::current().text_dim));
}

fn pauses(app: &mut SoundApp, ui: &mut egui::Ui) {
    dim(
        ui,
        "Shortens every pause where nobody speaks, on all speaker (mono) tracks at once, so the conversation stays in sync. Later markers move with it.",
    );
    let (max, keep) = egui::Grid::new(id("pause_grid"))
        .num_columns(2)
        .show(ui, |ui| (slider(ui, "Longer than", "max_pause", 0.5..=5.0, 1.5, " s"), slider(ui, "Shorten to", "keep", 0.1..=2.0, 0.7, " s")))
        .inner;
    let params = |dry: bool| json!({"max_pause_ms": f64::from(max) * 1000.0, "keep_ms": f64::from(keep.min(max)) * 1000.0, "dry_run": dry});
    ui.horizontal(|ui| {
        if ui.button("Preview").clicked() {
            run(app, "edit.tighten_pauses", params(true));
        }
        if ui.button("Tighten").clicked() {
            run(app, "edit.tighten_pauses", params(false));
        }
    });
    if let Some(r) = &app.ui.podcast_report {
        ui.label(format!("{} pauses, {:.1} s shorter", r["pauses"].as_u64().unwrap_or(0), r["removed_ms"].as_f64().unwrap_or(0.0) / 1000.0));
    }
}

fn duck(app: &mut SoundApp, ui: &mut egui::Ui) {
    dim(
        ui,
        "Writes volume automation that lowers the music and effects tracks (stereo audio tracks) while anyone on a speaker (mono) track talks. Edit the automation afterwards like any other.",
    );
    let (depth, attack, release) = egui::Grid::new(id("duck_grid"))
        .num_columns(2)
        .show(ui, |ui| {
            (
                slider(ui, "Duck by", "depth", -30.0..=-3.0, -12.0, " dB"),
                slider(ui, "Fade down", "attack", 0.05..=2.0, 0.3, " s"),
                slider(ui, "Fade up", "release", 0.1..=3.0, 0.8, " s"),
            )
        })
        .inner;
    if ui.button("Duck").clicked() {
        run(app, "mix.auto_duck", json!({"depth_db": depth, "attack_ms": f64::from(attack) * 1000.0, "release_ms": f64::from(release) * 1000.0}));
    }
    if let Some(r) = &app.ui.podcast_report {
        ui.label(format!("{} passages ducked on {} tracks", r["ducks"].as_u64().unwrap_or(0), r["tracks"].as_array().map_or(0, Vec::len)));
    }
}

fn preflight(app: &mut SoundApp, ui: &mut egui::Ui) {
    dim(
        ui,
        "Checks the mix before you export: clipping, loudness and true peak against the podcast target, dead air, silence at the start and end, and speaker balance.",
    );
    if ui.button("Run Check").clicked() {
        run(app, "session.preflight", json!({}));
    }
    let Some(r) = app.ui.podcast_report.clone() else { return };
    ui.separator();
    ui.label(format!(
        "{:.1} LUFS (target {:.0}), true peak {:.1} dBTP",
        r["lufs"].as_f64().unwrap_or(f64::NAN),
        r["target"]["lufs"].as_f64().unwrap_or(-16.0),
        r["true_peak_db"].as_f64().unwrap_or(f64::NAN)
    ));
    let issues = r["issues"].as_array().cloned().unwrap_or_default();
    if issues.is_empty() {
        ui.label(egui::RichText::new("✓ Ready to export").strong());
    }
    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        for i in &issues {
            ui.horizontal(|ui| {
                let mark = if i["severity"] == "error" { "✖" } else { "⚠" };
                ui.label(format!("{mark} {}", i["message"].as_str().unwrap_or("")));
                if let Some(at) = i["at"].as_i64()
                    && ui.small_button("Go").clicked()
                {
                    let _ = app.run("transport.locate", json!({"at": at}));
                }
            });
        }
    });
}

fn remembered<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, key: &str, default: T) -> T {
    ui.ctx().data(|d| d.get_temp(id(key))).unwrap_or(default)
}

fn remember<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, key: &str, v: T) {
    ui.ctx().data_mut(|d| d.insert_temp(id(key), v));
}

fn sync(app: &mut SoundApp, ui: &mut egui::Ui) {
    dim(
        ui,
        "For a double-ender: each speaker recorded on their own machine. Pick the track that hears everyone (the call recording); the speakers' tracks (mono audio tracks) move to line up with it, and a recorder whose clock ran fast or slow is sped up or slowed down to match.",
    );
    let tracks: Vec<(u64, String)> = app
        .engine
        .session()
        .tracks
        .iter()
        .filter(|t| t.kind == soundcraft_model::TrackKind::Audio && !t.clips().is_empty())
        .map(|t| (t.id.0, t.name.clone()))
        .collect();
    let guess = tracks
        .iter()
        .find(|(_, n)| ["call", "zoom", "riverside", "reference"].iter().any(|k| n.to_lowercase().contains(k)))
        .or(tracks.first())
        .map_or(0, |t| t.0);
    let mut reference: u64 = remembered(ui, "sync_ref", guess);
    if !tracks.iter().any(|t| t.0 == reference) {
        reference = guess;
    }
    let mut clap: bool = remembered(ui, "sync_clap", false);
    let mut drift: bool = remembered(ui, "sync_drift", true);
    egui::Grid::new(id("sync_grid")).num_columns(2).show(ui, |ui| {
        ui.label("Reference");
        let current = tracks.iter().find(|t| t.0 == reference).map_or("None", |t| t.1.as_str());
        let combo = craft_ui::ComboBox::from_id_salt(id("sync_ref_combo")).selected_text(current).show_ui(ui, |ui| {
            for (t, name) in &tracks {
                ui.selectable_value(&mut reference, *t, name);
            }
        });
        crate::widgets::named_text_field(combo.response, "Reference");
        ui.end_row();
        ui.label("Match by");
        ui.horizontal(|ui| {
            ui.radio_value(&mut clap, false, "Voices");
            ui.radio_value(&mut clap, true, "Clap at the start");
        });
        ui.end_row();
        ui.label("");
        ui.add_enabled(!clap, egui::Checkbox::new(&mut drift, "Correct clock drift"));
        ui.end_row();
    });
    remember(ui, "sync_ref", reference);
    remember(ui, "sync_clap", clap);
    remember(ui, "sync_drift", drift);
    let params = |dry: bool| json!({"reference": reference, "method": if clap { "clap" } else { "auto" }, "drift": drift, "dry_run": dry});
    ui.horizontal(|ui| {
        if ui.add_enabled(!tracks.is_empty(), egui::Button::new("Analyze")).clicked() {
            run(app, "track.sync_recordings", params(true));
        }
        if ui.add_enabled(!tracks.is_empty(), egui::Button::new("Sync")).clicked() {
            run(app, "track.sync_recordings", params(false));
        }
    });
    if let Some(r) = &app.ui.podcast_report {
        for t in r["tracks"].as_array().into_iter().flatten() {
            let name = t["name"].as_str().unwrap_or("");
            if t["synced"] == true {
                ui.label(format!(
                    "{name}: {:+.1} ms, drift {:+.1} ms an hour (match {:.0}%)",
                    t["offset_ms"].as_f64().unwrap_or(0.0),
                    t["drift_ms_per_hour"].as_f64().unwrap_or(0.0),
                    t["confidence"].as_f64().unwrap_or(0.0) * 100.0
                ));
            } else {
                ui.label(format!("{name}: no match found, left where it is"));
            }
        }
    }
}

fn segments(app: &mut SoundApp, ui: &mut egui::Ui) {
    dim(
        ui,
        "Your intros, outros, sponsor reads and stingers, kept for every episode. Insert drops one in at the insertion point, at a marker, or at every ad slot marker (Edit › Add Ad Slot Marker).",
    );
    // Where it goes.
    let markers: Vec<(u32, String)> = app.engine.session().markers.iter().map(|m| (m.number, m.name.clone())).collect();
    let mut place: u8 = remembered(ui, "seg_place", 0);
    let mut marker: u32 = remembered(ui, "seg_marker", markers.first().map_or(0, |m| m.0));
    let mut ripple: bool = remembered(ui, "seg_ripple", true);
    egui::Grid::new(id("seg_grid")).num_columns(2).show(ui, |ui| {
        ui.label("Insert at");
        ui.horizontal(|ui| {
            ui.radio_value(&mut place, 0, "Insertion point");
            ui.radio_value(&mut place, 1, "Marker");
            ui.radio_value(&mut place, 2, "Every ad slot");
        });
        ui.end_row();
        if place == 1 {
            ui.label("Marker");
            let current = markers.iter().find(|m| m.0 == marker).map_or("None", |m| m.1.as_str());
            let combo = craft_ui::ComboBox::from_id_salt(id("seg_marker_combo")).selected_text(current).show_ui(ui, |ui| {
                for (n, name) in &markers {
                    ui.selectable_value(&mut marker, *n, name);
                }
            });
            crate::widgets::named_text_field(combo.response, "Marker");
            ui.end_row();
        }
        ui.label("");
        crate::widgets::checkbox(ui, &mut ripple, "Make room (move everything after it later)");
        ui.end_row();
    });
    let fade = slider_row(ui, "Fade", "seg_fade", 0.0..=2000.0, 20.0, " ms");
    remember(ui, "seg_place", place);
    remember(ui, "seg_marker", marker);
    remember(ui, "seg_ripple", ripple);
    ui.separator();
    let mut insert: Option<String> = None;
    let mut remove: Option<usize> = None;
    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        egui::Grid::new(id("seg_list")).num_columns(3).striped(true).show(ui, |ui| {
            for (i, seg) in app.ui.segment_library.iter_mut().enumerate() {
                ui.label(&seg.name).on_hover_text(&seg.path);
                let combo =
                    craft_ui::ComboBox::from_id_salt(id(&format!("seg_kind_{i}"))).selected_text(seg.kind.as_str()).width(90.0).show_ui(ui, |ui| {
                        for k in SEGMENT_KINDS {
                            ui.selectable_value(&mut seg.kind, (*k).to_string(), *k);
                        }
                    });
                crate::widgets::named_text_field(combo.response, &format!("{} kind", seg.name));
                ui.horizontal(|ui| {
                    if ui.button("Insert").clicked() {
                        insert = Some(seg.path.clone());
                    }
                    if ui.small_button("✖").on_hover_text("Remove from the library (the file stays)").clicked() {
                        remove = Some(i);
                    }
                });
                ui.end_row();
            }
        });
    });
    if app.ui.segment_library.is_empty() {
        ui.label("The library is empty: add an audio file.");
    }
    if let Some(i) = remove {
        app.ui.segment_library.remove(i);
    }
    let mut path: String = remembered(ui, "seg_path", String::new());
    ui.horizontal(|ui| {
        crate::widgets::named_text_field(ui.add(egui::TextEdit::singleline(&mut path).hint_text("Audio file").desired_width(220.0)), "Segment file");
        if let Some(pick) = &app.services.pick_open
            && ui.button("Browse…").clicked()
            && let Some(p) = pick("segment.insert", &soundcraft_audio_io::FileFormat::readable_extensions())
        {
            match crate::dialogs::unicode(&p) {
                Ok(text) => path = text.to_owned(),
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        if ui.add_enabled(!path.trim().is_empty(), egui::Button::new("Add")).clicked() {
            let name = std::path::Path::new(path.trim()).file_stem().and_then(std::ffi::OsStr::to_str).unwrap_or("Segment").to_string();
            app.ui.segment_library.push(LibrarySegment { name, kind: "Other".into(), path: path.trim().to_string() });
            path.clear();
        }
    });
    remember(ui, "seg_path", path);
    if let Some(path) = insert {
        let mut p = json!({"path": path, "ripple": ripple, "fade_ms": fade});
        match place {
            1 => p["marker"] = json!(marker),
            2 => p["ad_slots"] = json!(true),
            _ => {}
        }
        run(app, "segment.insert", p);
    }
    if let Some(r) = &app.ui.podcast_report
        && let Some(n) = r["inserted"].as_u64()
    {
        ui.label(format!("Inserted {n} time{}", if n == 1 { "" } else { "s" }));
    }
}

/// [`slider`] in a grid of its own.
fn slider_row(ui: &mut egui::Ui, label: &str, key: &str, range: std::ops::RangeInclusive<f32>, default: f32, suffix: &str) -> f32 {
    egui::Grid::new(id(&format!("{key}_grid"))).num_columns(2).show(ui, |ui| slider(ui, label, key, range, default, suffix)).inner
}
