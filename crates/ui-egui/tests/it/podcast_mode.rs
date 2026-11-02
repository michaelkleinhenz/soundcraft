//! Podcast mode hides the music, video and surround features from the menus, the toolbar, the
//! rulers and the New Tracks dialog, without unregistering any command.

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use soundcraft_ui_egui::menus::{self, MenuNode};
use soundcraft_ui_egui::{Services, SoundApp, UiState, podcast};

fn paths(nodes: &[MenuNode], out: &mut Vec<String>) {
    for n in nodes {
        out.push(n.path.clone());
        paths(&n.children, out);
    }
}

fn all_paths(tree: &[MenuNode]) -> Vec<String> {
    let mut out = Vec::new();
    paths(tree, &mut out);
    out
}

#[test]
fn every_hidden_path_names_a_real_menu_item() {
    let full = all_paths(&menus::tree());
    for hidden in podcast::HIDDEN_MENU_PATHS {
        assert!(full.iter().any(|p| p == hidden), "`{hidden}` is not in the menu tree (renamed?)");
    }
}

#[test]
fn the_podcast_menu_tree_drops_music_video_and_surround_items() {
    let full = all_paths(&menus::tree());
    let podcast = all_paths(&podcast::visible_tree(menus::tree(), true));
    assert_eq!(all_paths(&podcast::visible_tree(menus::tree(), false)), full, "off: the full tree");
    for gone in [
        "Event",
        "Window > MIDI Editor",
        "Window > Score Editor",
        "Window > Video",
        "File > Import > MIDI...",
        "Clip > New MIDI Clip",
        "View > Rulers > Bars|Beats",
        "View > Rulers > Tempo > Tempo Editor",
        "Track > Change Track Width > 5.1",
        "Track > Change Track Width > 7th Order Ambisonics",
        "Setup > MIDI > MIDI Studio...",
    ] {
        assert!(full.iter().any(|p| p == gone), "{gone} should exist in the full tree");
        assert!(!podcast.iter().any(|p| p == gone), "{gone} should be hidden");
    }
    for kept in [
        "File > Bounce Mix...",
        "File > Import > Audio...",
        "Edit > Strip Silence",
        "Track > New...",
        "Track > Change Track Width > Mono",
        "Track > Change Track Width > Stereo",
        "View > Rulers > Min:Secs",
        "View > Rulers > Markers",
        "AudioSuite > Other > Normalize",
        "Window > Memory Locations",
    ] {
        assert!(podcast.iter().any(|p| p == kept), "{kept} should stay");
    }
    // No submenu is left empty.
    fn no_empty(nodes: &[MenuNode], top: bool) {
        for n in nodes {
            assert!(top || !n.label.is_empty());
            if !n.children.is_empty() {
                no_empty(&n.children, false);
            }
        }
    }
    no_empty(&podcast::visible_tree(menus::tree(), true), true);
}

#[test]
fn rulers_keep_a_time_scale() {
    let r = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert_eq!(podcast::visible_rulers(true, &r(&["bars_beats", "min_secs", "tempo", "meter", "markers"])), r(&["min_secs", "markers"]));
    assert_eq!(podcast::visible_rulers(true, &r(&["bars_beats", "markers"])), r(&["min_secs", "markers"]));
    assert_eq!(podcast::visible_rulers(true, &r(&["samples"])), r(&["samples"]));
    assert_eq!(podcast::visible_rulers(false, &r(&["bars_beats", "tempo"])), r(&["bars_beats", "tempo"]));
}

#[test]
fn the_new_tracks_dialog_offers_audio_kinds_in_mono_and_stereo() {
    use soundcraft_model::{ChannelFormat, TrackKind};
    let kinds: Vec<TrackKind> = TrackKind::ALL.into_iter().filter(|k| podcast::offers_track_kind(*k)).collect();
    assert!(kinds.contains(&TrackKind::Audio) && kinds.contains(&TrackKind::Aux) && kinds.contains(&TrackKind::Master));
    assert!(!kinds.contains(&TrackKind::Midi) && !kinds.contains(&TrackKind::Instrument) && !kinds.contains(&TrackKind::Video));
    let formats: Vec<ChannelFormat> = ChannelFormat::ALL.into_iter().filter(|f| podcast::offers_format(*f)).collect();
    assert_eq!(formats, [ChannelFormat::Mono, ChannelFormat::Stereo]);
}

#[test]
fn the_command_toggles_the_mode_closes_music_windows_and_keeps_commands_working() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    assert!(!app.ui.podcast_mode, "the library default is the full workstation");
    app.ui.show_midi_editor = true;
    app.ui.show_score = true;
    assert_eq!(app.run("ui.podcast_mode", json!({"value": true})).unwrap(), json!({"podcast_mode": true}));
    assert!(app.ui.podcast_mode);
    assert!(!app.ui.show_midi_editor && !app.ui.show_score);
    assert!(menus::item_state(&app, "", Some("ui.podcast_mode")).1, "checked in the app menu");
    // Hidden from the menus, still a command.
    app.run("track.new", json!({"kind": "midi", "name": "Still There"})).unwrap();
    assert!(app.engine.session().tracks.iter().any(|t| t.name == "Still There"));
    assert_eq!(app.run("ui.podcast_mode", json!({})).unwrap(), json!({"podcast_mode": false}), "no value toggles");
}

#[test]
fn settings_saved_before_podcast_mode_existed_start_in_it() {
    let mut value = serde_json::to_value(UiState::default()).unwrap();
    value.as_object_mut().unwrap().remove("podcast_mode");
    let restored: UiState = serde_json::from_value(value).unwrap();
    assert!(restored.podcast_mode);
    let off: UiState = serde_json::from_value(serde_json::to_value(UiState::default()).unwrap()).unwrap();
    assert!(!off.podcast_mode, "a saved choice is kept");
}

fn menu_bar_has(podcast_mode: bool, label: &str) -> bool {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    podcast::set(&mut app, podcast_mode);
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h.query_by_label(label).is_some()
}

#[test]
fn the_menu_bar_has_no_event_menu_in_podcast_mode() {
    assert!(menu_bar_has(false, "Event"));
    assert!(!menu_bar_has(true, "Event"));
    assert!(menu_bar_has(true, "PodCraft"));
}
