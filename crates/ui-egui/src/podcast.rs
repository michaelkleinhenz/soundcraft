//! Podcast mode: PodCraft's default profile. It hides the music-production features (MIDI,
//! instruments and notation; tempo, meter and click; video and timecode; surround) from the menus,
//! the toolbar, the rulers and the New Tracks dialog.
//!
//! Only the presentation changes: every command stays registered, so shortcuts, the CLI, the
//! control channel and MCP still reach it, and switching the mode off brings the full workstation
//! back.

use crate::SoundApp;
use crate::menus::MenuNode;
use serde_json::{Value, json};
use soundcraft_model::{ChannelFormat, TrackKind};

/// Menu paths hidden in Podcast mode, with everything under them.
pub const HIDDEN_MENU_PATHS: &[&str] = &[
    // MIDI, instruments and notation.
    "Event",
    "File > Send To...",
    "File > Import > MIDI...",
    "File > Export > MIDI...",
    "File > Export > Sibelius...",
    "File > Score Setup...",
    "File > Print Score...",
    "Track > Extract MIDI to New Track",
    "Track > Write MIDI Real-Time Properties",
    "Track > ARA Plugins",
    "Clip > ARA Plugins",
    "Clip > New MIDI Clip",
    "Edit > Copy Special > Audio as MIDI",
    "Edit > Paste Special > Merge MIDI",
    "Setup > MIDI",
    "Options > Mirrored MIDI Editing",
    "Options > MIDI Thru",
    "Options > MIDI Live Mode",
    "Window > MIDI Editor",
    "Window > MIDI Event List",
    "Window > MIDI Keyboard",
    "Window > MIDI Editors",
    "Window > Score Editor",
    "View > Other Displays > MIDI Input Display",
    "View > Other Displays > Lower Dock > MIDI Editor",
    "View > Transport > MIDI Controls",
    "View > Edit Window Views > Instruments",
    "View > Edit Window Views > Real-Time Properties",
    "View > Mix Window Views > Instruments",
    // Music timing: bars and beats, tempo, meter, key, chords, click, warping.
    "View > Rulers > Bars|Beats",
    "View > Rulers > Tempo",
    "View > Rulers > Meter",
    "View > Rulers > Key Signature",
    "View > Rulers > Chord Symbols",
    "View > Main Counter > Bars|Beats",
    "View > Transport > Ableton Link",
    "Track > Create Click Track",
    "Options > Click",
    "Setup > Click/Countoff...",
    "Clip > Elastic Properties",
    "Clip > Conform to Tempo",
    "Clip > Remove Warp",
    "Clip > Remove Pitch Shift",
    "Edit > TCE Edit to Timeline Selection",
    // Video and post-production.
    "File > Import > Video...",
    "File > Import > VENUE Channel Names as Track Names",
    "File > Export > Selected Tracks as New AAF/OMF...",
    "File > Export > Selected Tracks as New Media Composer Compatible Session...",
    "Window > Video",
    "Window > Video Universe",
    "View > Rulers > Timecode",
    "View > Rulers > Timecode 2",
    "View > Rulers > Feet+Frames",
    "View > Main Counter > Timecode",
    "View > Main Counter > Feet+Frames",
    "View > Dropped Video Frame Indicator",
    "View > Transport > Synchronization",
    "Setup > Video Sync Offset...",
    "Setup > Current Feet+Frames Position...",
    "Setup > Current Timecode Position...",
    "Setup > External Timecode Offset...",
    "Setup > Machine Track Arming Profiles...",
    "Options > Video Track Online",
    // Surround and immersive.
    "Track > Change Track Width > Show / Hide...",
    "Edit > Automation > Duplicate Dolby Atmos Plugin Automation to Pan Automation",
    "Edit > Automation > Duplicate Pan Automation to Dolby Atmos Plugin Automation",
    "View > Edit Window Views > Object",
    "View > Mix Window Views > Object",
    // Music effects (A5): pitch, modulation, lo-fi distortion, signal generation, time stretch.
    "AudioSuite > Pitch Shift",
    "AudioSuite > Modulation",
    "AudioSuite > Delay",
    "AudioSuite > Harmonic > Lo-Fi",
    "AudioSuite > Harmonic > Recti-Fi",
    "AudioSuite > Harmonic > SansAmp PSA-1",
    "AudioSuite > Harmonic > Eleven Lite",
    "AudioSuite > Other > Signal Generator",
    "AudioSuite > Other > Time Compression Expansion",
    // Studio and console extras (A6).
    "Track > Convert Aux to Routing Folder",
    "Track > Freeze",
    "Track > Commit...",
    "Track > Bypass Inserts",
    "Track > Coalesce VCA Controller Automation",
    "Track > Designate as Target Playlist",
    "Track > Show Target Playlist",
    "Track > Toggle Recent Playlist",
    "Edit > Copy Selection to...",
    "Edit > Move Selection to...",
    "View > Automation > Trim Playlist",
    "View > Automation > Composite Playlist",
    "View > Narrow Mix",
    "View > Other Displays > Universe",
    "File > Open > Project...",
    "File > Open > Sketch...",
    "File > Dashboard...",
    "File > Start Collaboration...",
    "File > Sign In",
    "File > Import > Clip Groups...",
    "Setup > Disk Allocation...",
    "Setup > Peripherals...",
    "Window > Configurations",
    "Window > Sketch to Front",
    "Window > ARA Plugins",
    "Window > Splice",
    "Window > SoundFlow",
    "Window > Task Manager",
    "Window > New Workspace",
    "Window > Workspaces",
    "Window > Machine Track Arming",
    "Window > Collaboration",
];

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
const HIDDEN_RULERS: &[&str] = &["bars_beats", "tempo", "tempo_editor", "meter", "key", "chords", "timecode", "timecode2", "feet_frames"];

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

/// Whether Podcast mode hides the menu item or submenu at `path`.
pub fn hides_path(path: &str) -> bool {
    if let Some(width) = path.strip_prefix("Track > Change Track Width > ") {
        return !matches!(width, "Mono" | "Stereo");
    }
    HIDDEN_MENU_PATHS.iter().any(|h| path == *h || path.strip_prefix(h).is_some_and(|rest| rest.starts_with(" > ")))
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
    let card = egui::Rect::from_center_size(area.center(), egui::vec2(360.0, 230.0).min(area.size()));
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
            if let Some(id) = pick
                && !crate::menus::invoke_shortcut_dialog(app, id)
                && let Err(e) = app.run(id, json!({}))
            {
                app.ui.status = e;
            }
        });
    });
}
