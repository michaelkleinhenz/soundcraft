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
];

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
    !matches!(kind, TrackKind::Midi | TrackKind::Instrument | TrackKind::Video)
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
