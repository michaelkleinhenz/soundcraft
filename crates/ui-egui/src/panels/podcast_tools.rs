//! The podcast production tools' window: Tighten Pauses (Edit menu), Auto-Duck Music (Track
//! menu) and the Episode Pre-flight Check (File menu), one tab each.

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
}

fn id(name: &str) -> egui::Id {
    egui::Id::unique(("podcast_tools", name))
}

/// A remembered slider value for this window.
fn slider(ui: &mut egui::Ui, label: &str, key: &str, range: std::ops::RangeInclusive<f32>, default: f32, suffix: &str) -> f32 {
    let mut v: f32 = ui.ctx().data(|d| d.get_temp(id(key))).unwrap_or(default);
    ui.label(label);
    ui.add(egui::Slider::new(&mut v, range).suffix(suffix));
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
    };
    let mut open = true;
    egui::Window::new(title).id(id("window")).open(&mut open).default_size(vec2(420.0, 320.0)).collapsible(false).show(ctx, |ui| match tool {
        PodcastTool::Pauses => pauses(app, ui),
        PodcastTool::Duck => duck(app, ui),
        PodcastTool::Preflight => preflight(app, ui),
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
