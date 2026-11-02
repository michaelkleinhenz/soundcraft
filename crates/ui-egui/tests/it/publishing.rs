//! The podcast publishing chain in the UI: File › Episode Info, File › Export › Chapters and MP3
//! in Bounce Mix.

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use soundcraft_audio_io::FileFormat;
use soundcraft_ui_egui::menus::{self, MenuNode};
use soundcraft_ui_egui::{Services, SoundApp, dialogs::Dialog, podcast};

fn has(nodes: &[MenuNode], path: &str) -> bool {
    nodes.iter().any(|n| n.path == path || has(&n.children, path))
}

fn harness(app: SoundApp) -> Harness<'static, SoundApp> {
    Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    )
}

#[test]
fn episode_info_and_chapter_export_are_in_the_podcast_file_menu() {
    let tree = podcast::visible_tree(menus::tree(), true);
    for path in ["File > Episode Info...", "File > Export > Chapters..."] {
        assert!(has(&tree, path), "{path} missing");
    }
}

#[test]
fn the_episode_info_window_edits_the_session_with_undo() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.run("window.episode_info", json!({})).unwrap();
    assert!(app.ui.show_episode_info);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_role_and_label(Role::TextInput, "Title").click();
    h.run_steps(2);
    h.event(egui::Event::Text("Pilot".into()));
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Episode").click();
    h.run_steps(2);
    h.event(egui::Event::Text("1x".into()));
    h.run_steps(2);
    assert_eq!(h.state().engine.session().episode.title, "", "nothing stored before Apply");
    h.get_by_label("Apply").click();
    h.run_steps(3);
    let episode = h.state().engine.session().episode.clone();
    assert_eq!((episode.title.as_str(), episode.episode), ("Pilot", Some(1)), "digits only");
    h.state_mut().run("edit.undo", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().engine.session().episode.title, "");
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(!h.state().ui.show_episode_info);
}

#[test]
fn bounce_in_podcast_mode_starts_as_mp3_with_a_bitrate() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.ui.podcast_mode = true;
    assert!(app.dialogs.open_for_command(&app.engine, "file.bounce_mix"));
    let mut h = harness(app);
    h.run_steps(3);
    match &h.state().dialogs.open {
        Some(Dialog::Bounce { path, format, bitrate, loudness, .. }) => {
            assert_eq!(*format, FileFormat::Mp3);
            assert!(path.ends_with(".mp3"), "{path}");
            assert_eq!(*bitrate, 128);
            assert_eq!(loudness.as_deref(), Some("podcast"));
        }
        other => panic!("{other:?}"),
    }
    h.get_by_label("Bitrate").click();
    h.run_steps(2);
    let _ = h.get_by_label("192 kbps");
}

#[test]
fn bounce_outside_podcast_mode_stays_wav() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    assert!(app.dialogs.open_for_command(&app.engine, "file.bounce_mix"));
    let mut h = harness(app);
    h.run_steps(3);
    match &h.state().dialogs.open {
        Some(Dialog::Bounce { path, format, .. }) => assert_eq!((*format, path.ends_with(".wav")), (FileFormat::Wav, true)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn in_podcast_mode_m_adds_a_chapter_marker_and_e_an_edit_marker() {
    use soundcraft_model::MarkerKind;
    let count = |app: &SoundApp, kind: MarkerKind| app.engine.session().markers.iter().filter(|m| m.kind == kind).count();
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.ui.podcast_mode = true;
    let chapters = count(&app, MarkerKind::Marker);
    let mut h = harness(app);
    h.run_steps(2);
    for key in [egui::Key::M, egui::Key::E] {
        h.key_press(key);
        h.run_steps(2);
    }
    assert_eq!((count(h.state(), MarkerKind::Marker), count(h.state(), MarkerKind::Edit)), (chapters + 1, 1));
    let names: Vec<&str> = h.state().engine.session().markers.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"Edit 1") && names.contains(&format!("Chapter {}", chapters + 1).as_str()), "{names:?}");
    let tree = podcast::visible_tree(menus::tree(), true);
    for path in ["Edit > Add Chapter Marker", "Edit > Add Edit Marker", "Edit > Go to Next Edit Marker", "Edit > Go to Previous Edit Marker"] {
        assert!(has(&tree, path), "{path} missing");
    }
}

#[test]
fn the_podcast_tools_open_from_their_menus() {
    let tree = podcast::visible_tree(menus::tree(), true);
    for path in ["Edit > Tighten Pauses...", "Track > Auto-Duck Music...", "File > Episode Pre-flight Check...", "Track > Voice Preset > Broadcast"] {
        assert!(has(&tree, path), "{path} missing");
    }
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    menus::invoke_menu(&mut app, "session.preflight", "File > Episode Pre-flight Check...");
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Run Check").click();
    h.run_steps(3);
    assert!(h.state().ui.podcast_report.as_ref().is_some_and(|r| r["issues"].is_array()));
}
