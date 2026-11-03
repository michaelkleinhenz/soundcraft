//! Podcast mode: PodCraft's default profile. The menus show only a short list of podcast items
//! ([`SHOWN_MENU_PATHS`]); the toolbar, track headers, rulers, plugin lists and the New Tracks
//! dialog drop the music, video, surround and studio features; a Quick Start guide walks through
//! making an episode.
//!
//! Only the presentation changes: every command stays registered, so shortcuts, the CLI, the
//! control channel and MCP still reach it, and switching the mode off brings the full workstation
//! back.

use crate::SoundApp;
use crate::menus::MenuNode;
use serde_json::{Value, json};
use soundcraft_model::{ChannelFormat, TrackKind};

/// The menu items Podcast mode shows, in the menus' own order: everything else is hidden. An entry
/// that names a submenu keeps all of it. Kept short on purpose, so the menus read as a podcast
/// tool: recording, editing speech, cleaning it up, mixing and publishing.
pub const SHOWN_MENU_PATHS: &[&str] = &[
    // File: sessions, episode details, importing recordings and publishing.
    "File > New...",
    "File > Episode Templates...",
    "File > Open > Session...",
    "File > Close Session",
    "File > Save Session",
    "File > Save Session As...",
    "File > Revert Session to Saved...",
    "File > Import > Audio...",
    "File > Episode Info...",
    "File > Episode Pre-flight Check...",
    "File > Bounce Mix...",
    "File > Export > Chapters...",
    // Edit: the everyday edits, speech tools and markers.
    "Edit > Can't Undo",
    "Edit > Can't Redo",
    "Edit > Cut",
    "Edit > Copy",
    "Edit > Paste",
    "Edit > Clear",
    "Edit > Select All",
    "Edit > Duplicate",
    "Edit > Insert Silence",
    "Edit > Separate > At Selection",
    "Edit > Heal Separation",
    "Edit > Trim > To Selection",
    "Edit > Fades > Create...",
    "Edit > Fades > Delete",
    "Edit > Mute",
    "Edit > Strip Silence",
    "Edit > Tighten Pauses...",
    "Edit > Insert Segment...",
    "Edit > Add Chapter Marker",
    "Edit > Add Edit Marker",
    "Edit > Add Ad Slot Marker",
    "Edit > Go to Next Edit Marker",
    "Edit > Go to Previous Edit Marker",
    // View: the time scale, markers, side lists and waveform style.
    "View > Rulers > Min:Secs",
    "View > Rulers > Markers",
    "View > Main Counter > Min:Secs",
    "View > Main Counter > Samples",
    "View > Edit Window Views > Inserts A-E",
    "View > Edit Window Views > I/O",
    "View > Edit Window Views > Minimal",
    "View > Mix Window Views > Inserts A-E",
    "View > Mix Window Views > Sends A-E",
    "View > Mix Window Views > I/O",
    "View > Mix Window Views > Track Color",
    "View > Other Displays > Track List",
    "View > Other Displays > Clip List",
    "View > Waveforms > Peak",
    "View > Waveforms > Power",
    // Track: tracks, voice chains and the podcast tools.
    "Track > New...",
    "Track > Duplicate...",
    "Track > Delete",
    "Track > Change Track Width > Mono",
    "Track > Change Track Width > Stereo",
    "Track > Make Inactive",
    "Track > Voice Preset",
    "Track > Auto-Duck Music...",
    "Track > Sync Remote Recordings...",
    // Clip.
    "Clip > Rename...",
    "Clip > Group",
    "Clip > Ungroup",
    "Clip > Edit Lock/Unlock",
    // AudioSuite (shown as "Process"): speech processing only.
    "AudioSuite > Noise Reduction",
    "AudioSuite > EQ",
    "AudioSuite > Dynamics",
    "AudioSuite > Reverb",
    "AudioSuite > Other > Normalize",
    "AudioSuite > Other > Gain",
    "AudioSuite > Other > DC Offset Removal",
    // Options: playback only.
    "Options > Loop Playback",
    "Options > Insertion Follows Playback",
    "Options > Edit Window Scrolling",
    // Setup: audio device and the app.
    "Setup > Hardware...",
    "Setup > Playback Engine...",
    "Setup > Keyboard Shortcuts...",
    "Setup > Preferences...",
    "Setup > Appearance",
    // Window.
    "Window > Edit",
    "Window > Mix",
    "Window > Memory Locations",
    "Window > Clip List",
    "Window > Transport",
    "Window > Undo History",
    "Window > Hide All Floating Windows",
    "Window > Close Window",
    "Window > Reset Panel Layout",
];

/// How Podcast mode names a top-level menu: the plain word for Pro Tools jargon.
pub fn menu_label(on: bool, label: &str) -> &str {
    match label {
        "AudioSuite" if on => "Process",
        _ => label,
    }
}

/// Built-in plugins the insert menus leave out in Podcast mode (A5): music effects. EQ, dynamics,
/// the channel strip, one reverb, gain, trim, DC removal, dither, the saturator and third-party
/// plugins stay.
const HIDDEN_PLUGINS: &[&str] = &[
    "pitch_shifter",
    "time_shift",
    "plate_reverb",
    "lofi",
    "rectifier",
    "subharmonic",
    "signal_generator",
    "mod_delay",
    "chorus",
    "flanger",
    "phaser",
    "ring_mod",
    "auto_pan",
];

/// Whether the insert menus offer the built-in plugin `id` (`on`: Podcast mode).
pub fn offers_plugin(on: bool, id: &str) -> bool {
    !on || !HIDDEN_PLUGINS.contains(&id)
}

/// Edit window rulers hidden in Podcast mode (music timing and picture timebases).
const HIDDEN_RULERS: &[&str] = &[
    "bars_beats",
    "tempo",
    "tempo_editor",
    "meter",
    "key",
    "chords",
    "timecode",
    "timecode2",
    "feet_frames",
    "markers2",
    "markers3",
    "markers4",
    "markers5",
];

/// Windows a music or video workflow opens; switching Podcast mode on closes them.
const CLOSE_ON_ENTER: &[&str] = &[
    "window.midi_editor",
    "window.event_list",
    "window.midi_keyboard",
    "window.score",
    "window.video",
    "window.video_universe",
    "window.beat_detective",
    "window.tempo_ops",
    "window.time_ops",
    "window.midi_ops",
];

/// Whether Podcast mode hides the menu item or submenu at `path`: it shows only
/// [`SHOWN_MENU_PATHS`], what is under them, and the submenus that lead to them.
pub fn hides_path(path: &str) -> bool {
    let under = |inner: &str, outer: &str| inner.strip_prefix(outer).is_some_and(|rest| rest.starts_with(" > "));
    !SHOWN_MENU_PATHS.iter().any(|s| path == *s || under(path, s) || under(s, path))
}

/// Whether the New Tracks dialog offers this track kind in Podcast mode.
pub fn offers_track_kind(kind: TrackKind) -> bool {
    !matches!(kind, TrackKind::Midi | TrackKind::Instrument | TrackKind::Video | TrackKind::Vca)
}

/// Whether the New Tracks dialog offers this channel format in Podcast mode.
pub fn offers_format(format: ChannelFormat) -> bool {
    matches!(format, ChannelFormat::Mono | ChannelFormat::Stereo)
}

/// Whether the Edit window shows this ruler in Podcast mode.
pub fn shows_ruler(id: &str) -> bool {
    !HIDDEN_RULERS.contains(&id)
}

/// The rulers to draw: all of them outside Podcast mode; in it the ones it keeps, and Min:Secs
/// when none of the kept ones is a timebase, so the timeline always has a time scale.
pub fn visible_rulers(on: bool, rulers: &[String]) -> Vec<String> {
    if !on {
        return rulers.to_vec();
    }
    let mut kept: Vec<String> = rulers.iter().filter(|r| shows_ruler(r)).cloned().collect();
    if !kept.iter().any(|r| r == "min_secs" || r == "samples") {
        kept.insert(0, "min_secs".into());
    }
    kept
}

/// `tree` without what Podcast mode hides, and without submenus left empty by that.
pub fn visible_tree(tree: Vec<MenuNode>, on: bool) -> Vec<MenuNode> {
    if !on {
        return tree;
    }
    fn prune(nodes: Vec<MenuNode>) -> Vec<MenuNode> {
        nodes
            .into_iter()
            .filter(|n| !hides_path(&n.path))
            .filter_map(|mut n| {
                if n.children.is_empty() {
                    return Some(n);
                }
                n.children = prune(n.children);
                (!n.children.is_empty()).then_some(n)
            })
            .collect()
    }
    prune(tree)
}

/// Switch Podcast mode on or off. Switching it on closes the MIDI, notation, tempo and video
/// windows, which have no place in its menus.
pub fn set(app: &mut SoundApp, on: bool) {
    app.ui.podcast_mode = on;
    if on {
        for id in CLOSE_ON_ENTER {
            let _ = app.run(id, json!({"value": false}));
        }
    }
}

/// `ui.podcast_mode {value?: bool}`: set Podcast mode, or toggle it without `value`.
pub fn command(app: &mut SoundApp, p: &Value) -> Value {
    let on = p.get("value").and_then(Value::as_bool).unwrap_or(!app.ui.podcast_mode);
    set(app, on);
    json!({"podcast_mode": on})
}

/// The Edit window of an empty session in Podcast mode: a card that starts an episode (the New
/// Episode template), opens a session or imports recordings.
pub fn welcome(app: &mut SoundApp, ui: &mut egui::Ui, area: egui::Rect) {
    let t = crate::theme::Tokens::current();
    let card = egui::Rect::from_center_size(area.center(), egui::vec2(360.0, 260.0).min(area.size()));
    if card.width() < 120.0 || card.height() < 80.0 {
        return;
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(card));
    egui::Frame::new().fill(t.panel_bg).stroke(egui::Stroke::new(1.0, t.counter_border)).corner_radius(8).inner_margin(16).show(&mut child, |ui| {
        ui.set_min_size(card.size() - egui::vec2(32.0, 32.0));
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Start an episode").size(18.0).strong());
            ui.label(egui::RichText::new("Speaker tracks with a voice chain, music and SFX, a mastered mix.").small().color(t.text_dim));
            ui.add_space(10.0);
            let mut pick = None;
            if ui.add(egui::Button::new("New Episode…").min_size(egui::vec2(180.0, 26.0))).clicked() {
                pick = Some("session.new");
            }
            if ui.add(egui::Button::new("From a Template…").min_size(egui::vec2(180.0, 22.0))).clicked() {
                pick = Some("window.templates");
            }
            if ui.add(egui::Button::new("Import Recordings…").min_size(egui::vec2(180.0, 22.0))).clicked() {
                pick = Some("file.import_audio");
            }
            if ui.add(egui::Button::new("Open Session…").min_size(egui::vec2(180.0, 22.0))).clicked() {
                pick = Some("session.open");
            }
            if ui.link("New to PodCraft? Quick Start Guide").clicked() {
                pick = Some("window.quick_start");
            }
            if let Some(id) = pick
                && !crate::menus::invoke_shortcut_dialog(app, id)
                && let Err(e) = app.run(id, json!({}))
            {
                app.ui.status = e;
            }
        });
    });
}
