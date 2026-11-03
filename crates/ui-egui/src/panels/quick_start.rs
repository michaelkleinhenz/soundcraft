//! The Quick Start guide (PodCraft › Quick Start Guide): making an episode in six steps, each
//! with the buttons that do it. A first run opens it.

use super::SoundApp;
use crate::theme::Tokens;
use egui::vec2;
use serde_json::json;

/// One step: title, what to do, and its buttons (label, command).
type Step = (&'static str, &'static str, &'static [(&'static str, &'static str)]);

/// The steps of making an episode, in order.
pub const STEPS: &[Step] = &[
    (
        "1. Start an episode",
        "Makes a track for each speaker with a voice chain, music and effects tracks, and a mastered mix.",
        &[("New Episode…", "session.new"), ("From a Template…", "window.templates")],
    ),
    (
        "2. Record or bring in recordings",
        "To record, click a speaker's R button and press Cmd+Space (Ctrl+Space on Windows and Linux). Each speaker recorded separately? Import their files, then line them up with the call recording.",
        &[("Import Recordings…", "file.import_audio"), ("Sync Remote Recordings…", "window.sync_recordings")],
    ),
    (
        "3. Edit the conversation",
        "In Shuffle mode, deleting a stretch closes the gap. While listening, press M to start a chapter and E to flag a spot to fix later. Tighten Pauses shortens long silences on every speaker at once.",
        &[("Tighten Pauses…", "window.tighten_pauses")],
    ),
    (
        "4. Clean up the voices",
        "Select clips, then use Process › Noise Reduction for noise, hum, clicks and pops. Track › Voice Preset sets a speaker's whole voice chain in one step.",
        &[],
    ),
    (
        "5. Add music and segments",
        "Drop in your intro, outro and sponsor reads, then let the music duck under speech by itself.",
        &[("Insert Segment…", "window.segments"), ("Auto-Duck Music…", "window.auto_duck")],
    ),
    (
        "6. Check and publish",
        "Fill in the episode details, let the pre-flight check catch problems, then export an MP3 at podcast loudness with chapters and cover art.",
        &[("Episode Info…", "window.episode_info"), ("Pre-flight Check…", "window.preflight"), ("Bounce Mix…", "file.bounce_mix")],
    ),
];

/// Runs a step's button: a dialog for the commands that have one, else the command (windows open).
fn press(app: &mut SoundApp, id: &str) {
    if crate::menus::invoke_shortcut_dialog(app, id) {
        return;
    }
    let params = if id.starts_with("window.") { json!({"value": true}) } else { json!({}) };
    if let Err(e) = app.run(id, params) {
        app.ui.status = e;
    }
}

pub(super) fn quick_start(app: &mut SoundApp, ctx: &egui::Context) {
    if !app.ui.show_quick_start {
        return;
    }
    let t = Tokens::current();
    let mut open = true;
    let mut pressed = None;
    egui::Window::new("Quick Start").id(egui::Id::unique("quick_start")).open(&mut open).default_size(vec2(460.0, 520.0)).collapsible(false).show(
        ctx,
        |ui| {
            ui.label(egui::RichText::new("Making an episode in PodCraft").size(16.0).strong());
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (title, text, buttons) in STEPS {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(*title).strong());
                    ui.label(egui::RichText::new(*text).color(t.text_dim));
                    if !buttons.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for (label, id) in *buttons {
                                if ui.button(*label).clicked() {
                                    pressed = Some(*id);
                                }
                            }
                        });
                    }
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Open this guide again from the PodCraft menu. Podcast Mode there switches back to the full workstation.")
                        .small()
                        .color(t.text_dim),
                );
            });
        },
    );
    if !open {
        app.ui.show_quick_start = false;
    }
    if let Some(id) = pressed {
        press(app, id);
    }
}
