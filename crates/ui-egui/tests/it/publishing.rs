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
    for path in [
        "Edit > Tighten Pauses...",
        "Track > Auto-Duck Music...",
        "File > Episode Pre-flight Check...",
        "Track > Voice Preset > Broadcast",
        "Track > Sync Remote Recordings...",
        "Edit > Insert Segment...",
        "Edit > Add Ad Slot Marker",
    ] {
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

#[test]
fn the_segment_library_inserts_at_every_ad_slot() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    let dir = std::env::temp_dir().join(format!("podcraft-seg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("sponsor.wav");
    let tone = soundcraft_audio_io::AudioBuffer { sample_rate: 48_000, channels: vec![vec![0.1; 4_800]; 2] };
    std::fs::write(&file, soundcraft_audio_io::encode(&tone, &soundcraft_audio_io::EncodeOptions::default()).unwrap()).unwrap();
    app.ui.segment_library.push(soundcraft_ui_egui::panels::LibrarySegment {
        name: "Sponsor".into(),
        kind: "Sponsor".into(),
        path: file.display().to_string(),
    });
    app.run("markers.add_ad", json!({"at": 48_000})).unwrap();
    menus::invoke_menu(&mut app, "segment.insert", "Edit > Insert Segment...");
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Every ad slot").click();
    h.run_steps(2);
    h.get_by_label("Insert").click();
    h.run_steps(3);
    let s = h.state().engine.session();
    let seg = s.tracks.iter().find(|t| t.name == "Segments").expect("a Segments track");
    assert_eq!(seg.clips().first().map(|c| c.start), Some(48_000));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn every_podcast_tool_window_draws_with_named_controls() {
    for (cmd, path) in [
        ("edit.tighten_pauses", "Edit > Tighten Pauses..."),
        ("mix.auto_duck", "Track > Auto-Duck Music..."),
        ("track.sync_recordings", "Track > Sync Remote Recordings..."),
        ("segment.insert", "Edit > Insert Segment..."),
    ] {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
        menus::invoke_menu(&mut app, cmd, path);
        assert!(app.ui.podcast_tools.is_some(), "{path} opens its window");
        let mut h = harness(app);
        // The harness fails on any control without an accessible name.
        h.run_steps(3);
    }
}
