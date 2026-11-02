//! File › Episode Info: the podcast episode details an MP3 export writes into its tags (title,
//! show, author, season and episode, date, description, cover art, explicit).

use super::SoundApp;
use crate::theme::Tokens;
use egui::vec2;
use serde_json::json;
use soundcraft_model::EpisodeInfo;

/// A season or episode number field: digits, or empty for none.
fn number_field(ui: &mut egui::Ui, label: &str, value: &mut Option<u32>) {
    let mut text = value.map(|n| n.to_string()).unwrap_or_default();
    ui.label(label);
    let resp = crate::widgets::named_text_field(ui.add(egui::TextEdit::singleline(&mut text).desired_width(56.0)), label);
    if resp.changed() {
        let digits: String = text.chars().filter(char::is_ascii_digit).take(9).collect();
        *value = digits.parse().ok();
    }
}

fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(label);
    crate::widgets::named_text_field(ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(340.0)), label);
    ui.end_row();
}

/// The `episode.set` parameters that store `d`.
pub(crate) fn params(d: &EpisodeInfo) -> serde_json::Value {
    json!({
        "show": d.show, "title": d.title, "author": d.author, "season": d.season, "episode": d.episode,
        "date": d.date, "description": d.description, "artwork": d.artwork, "explicit": d.explicit,
    })
}

pub(super) fn episode_info(app: &mut SoundApp, ctx: &egui::Context) {
    let mut open = app.ui.show_episode_info;
    if !open {
        return;
    }
    let mut draft = app.ui.episode_draft.take().unwrap_or_else(|| app.engine.session().episode.clone());
    let mut apply = false;
    let mut close = false;
    egui::Window::new("Episode Info").open(&mut open).default_size(vec2(480.0, 420.0)).collapsible(false).show(ctx, |ui| {
        egui::Grid::new("episode_info_grid").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            text_row(ui, "Show", &mut draft.show, "Podcast name");
            text_row(ui, "Title", &mut draft.title, "Episode title");
            text_row(ui, "Author", &mut draft.author, "Host or author");
            ui.label("Number");
            ui.horizontal(|ui| {
                number_field(ui, "Season", &mut draft.season);
                number_field(ui, "Episode", &mut draft.episode);
            });
            ui.end_row();
            text_row(ui, "Date", &mut draft.date, "2026-11-02");
            ui.label("Cover art");
            ui.horizontal(|ui| {
                crate::widgets::named_text_field(
                    ui.add(egui::TextEdit::singleline(&mut draft.artwork).hint_text("JPEG or PNG, 1400–3000 px square").desired_width(260.0)),
                    "Cover art",
                );
                if let Some(pick) = &app.services.pick_open
                    && ui.button("Browse…").clicked()
                    && let Some(p) = pick("episode.artwork", &["jpg", "jpeg", "png"])
                {
                    match crate::dialogs::unicode(&p) {
                        Ok(text) => draft.artwork = text.to_owned(),
                        Err(e) => app.ui.status = e.to_string(),
                    }
                }
            });
            ui.end_row();
            ui.label("");
            crate::widgets::checkbox(ui, &mut draft.explicit, "Explicit content");
            ui.end_row();
        });
        ui.label("Description");
        crate::widgets::named_text_field(
            ui.add(egui::TextEdit::multiline(&mut draft.description).desired_rows(5).desired_width(f32::INFINITY)),
            "Description",
        );
        ui.label(
            egui::RichText::new("MP3 and FLAC bounces carry these details, the cover art and one chapter per marker.")
                .small()
                .color(Tokens::current().text_dim),
        );
        ui.horizontal(|ui| {
            let changed = draft != app.engine.session().episode;
            if ui.add_enabled(changed, egui::Button::new("Apply")).clicked() {
                apply = true;
            }
            if ui.add_enabled(changed, egui::Button::new("Revert")).clicked() {
                draft = app.engine.session().episode.clone();
            }
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    if apply {
        if let Err(e) = app.run("episode.set", params(&draft)) {
            app.ui.status = e;
        }
        draft = app.engine.session().episode.clone();
    }
    app.ui.show_episode_info = open && !close;
    // Kept while open: an edit in progress survives frames; closing drops it.
    app.ui.episode_draft = app.ui.show_episode_info.then_some(draft);
}
