//! Bounces stream the main mix or a bus to their file, exactly as rendered in memory.

use soundcraft_audio_io::{BitDepth, EncodeOptions, FileFormat};
use soundcraft_engine::io::{Bounce, bounce_stems};
use soundcraft_time::Range;
use std::path::PathBuf;

#[test]
fn float_bounce_preserves_headroom_and_reports_the_written_levels() {
    let mut e = soundcraft_engine::Engine::default();
    let sr = e.session().sample_rate.hz();
    let frames = sr as usize + 17;
    let samples: Vec<f32> = (0..frames).map(|i| 1.5 * (std::f32::consts::TAU * 440.0 * i as f32 / sr as f32).sin()).collect();
    let audio = soundcraft_audio_io::AudioBuffer { sample_rate: sr, channels: vec![samples.clone(), samples] };
    let s = e.session_mut();
    // A new track has no inserts, so no limiter can mask encoder clipping.
    let track = s.add_track(soundcraft_model::TrackKind::Audio, soundcraft_model::ChannelFormat::Stereo, Some("Headroom"));
    let source = soundcraft_engine::io::add_source(s, "headroom", audio, None, FileFormat::Wav);
    let clip = soundcraft_model::Clip::audio(s.new_clip_id(), "headroom", source, 0, 0, frames as i64);
    soundcraft_engine::edit::place_clip(s, track, clip);
    let mix = soundcraft_mix::render_range(e.session(), Range::new(0, frames as i64), 1024);
    assert!(soundcraft_dsp::offline::peak(&mix) > 1.0);
    let dir = scratch("bounce-float-headroom");
    for extension in ["wav", "aiff"] {
        for normalize in [false, true] {
            let path = dir.join(format!("Mix.{extension}"));
            let result = e
                .execute(
                    "file.bounce_mix",
                    &serde_json::json!({
                        "path": path, "bit_depth": "32f", "dither": false, "normalize": normalize,
                        "start": 0, "end": frames,
                    }),
                )
                .unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let (_, file) = soundcraft_audio_io::decode(&bytes, None).unwrap();
            let mut expected = mix.clone();
            if normalize {
                soundcraft_dsp::offline::normalize(&mut expected, -0.1, false);
                assert!(file.peak() < 1.0);
            } else {
                assert!(file.peak() > 1.0, "{extension}: headroom was clipped");
            }
            assert_eq!(file.channels, expected, "{extension}, normalize={normalize}");
            let mut meter = soundcraft_dsp::meter::LoudnessMeter::new(sr as f32, file.num_channels());
            meter.process(&file.channels, file.frames());
            for (key, value) in [
                ("peak_db", soundcraft_dsp::offline::peak_db(&file.channels)),
                ("lufs", meter.integrated_lufs()),
                ("true_peak_db", meter.true_peak_db()),
            ] {
                assert_eq!(result[key].as_f64(), Some(f64::from(value)), "{extension}, normalize={normalize}, {key}");
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("soundcraft-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn bounces_write_exactly_the_mix_and_report_its_levels() {
    let e = soundcraft_engine::demo::demo_engine();
    let s = e.session();
    let dir = scratch("bounce-exact");
    let path = dir.join("Mix.wav");
    let r = Range::new(s.sample_rate.samples(10.0) + 7, s.sample_rate.samples(13.0));
    let opts = EncodeOptions { format: FileFormat::Wav, bit_depth: BitDepth::Float32, dither: false, bwf: None };
    let verb = s.bus_by_name("Verb").unwrap().id;
    for bus in [None, Some(verb)] {
        let mut mix = match bus {
            Some(b) => soundcraft_mix::render_bus(s, b, r, 1024),
            None => soundcraft_mix::render_range(s, r, 1024),
        };
        assert!(mix.iter().flatten().any(|v| v.abs() > 1e-3), "{bus:?}");
        for normalize in [false, true] {
            let mut bounce = Bounce::new(e.session_arc(), &path, r, &opts, bus, normalize, false).unwrap();
            let mut progress = vec![bounce.progress()];
            let done = loop {
                if let Some(done) = bounce.step(|| false).unwrap() {
                    break done;
                }
                progress.push(bounce.progress());
            };
            let last = progress[progress.len() - 1];
            assert!(progress.len() > 100 && progress.is_sorted() && progress[0] < 0.01 && last > 0.99 && last <= 1.0, "{progress:?}");
            if normalize {
                soundcraft_dsp::offline::normalize(&mut mix, -0.1, false);
            }
            let bytes = std::fs::read(&path).unwrap();
            let (_, file) = soundcraft_audio_io::decode(&bytes, None).unwrap();
            assert_eq!(file.channels, mix, "{bus:?} normalize: {normalize}");
            let mut lm = soundcraft_dsp::meter::LoudnessMeter::new(s.sample_rate.as_f64() as f32, mix.len());
            lm.process(&mix, mix[0].len());
            let levels = (soundcraft_dsp::offline::peak_db(&mix), lm.integrated_lufs(), lm.true_peak_db());
            assert_eq!((done.bytes, (done.peak_db, done.lufs, done.true_peak_db)), (bytes.len() as u64, levels));
        }
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bounce_too_big_for_its_format_fails_and_keeps_the_old_file() {
    let e = soundcraft_engine::demo::demo_engine();
    let s = e.session();
    let dir = scratch("bounce-too-big");
    let path = dir.join("Mix.aif");
    std::fs::write(&path, b"old").unwrap();
    let opts = EncodeOptions { format: FileFormat::Aiff, bit_depth: BitDepth::Float32, dither: false, bwf: None };
    let err = Bounce::new(e.session_arc(), &path, Range::new(0, s.sample_rate.samples(3.9 * 3600.0)), &opts, None, false, false).err().unwrap();
    assert!(err.to_string().contains("4 GiB"), "{err}");
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let err = bounce_stems(&e, &dir.to_string_lossy(), Range::new(0, s.sample_rate.samples(4.1 * 3600.0)), &opts).unwrap_err();
    assert!(err.to_string().contains("4 hours"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropping_an_unfinished_bounce_cancels_it_and_keeps_the_old_file() {
    let e = soundcraft_engine::demo::demo_engine();
    let dir = scratch("bounce-cancel");
    let path = dir.join("Mix.wav");
    std::fs::write(&path, b"old").unwrap();
    let mut bounce = Bounce::new(e.session_arc(), &path, Range::new(0, 48_000), &EncodeOptions::default(), None, false, false).unwrap();
    assert_eq!(bounce.step(|| false).unwrap(), None);
    assert!(dir.join("Mix.wav.part").exists());
    drop(bounce);
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

const PINNED: [[u32; 3]; 2] = [[0xc195_6706, 0xc093_0031, 0xc093_0031], [0xc162_e790, 0xbdcc_ccbf, 0xbdcc_ccbf]];

#[test]
fn demo_bounce_levels_are_pinned() {
    let mut e = soundcraft_engine::demo::demo_engine();
    let dir = scratch("bounce-demo-pinned");
    let mut got = Vec::new();
    for (name, extra) in [("Mix.wav", serde_json::json!({})), ("Norm.wav", serde_json::json!({"normalize": true, "bit_depth": 16}))] {
        let path = dir.join(name);
        let mut p = serde_json::json!({"path": path, "start": 1_296_000, "end": 1_440_000});
        if let (Some(m), Some(x)) = (p.as_object_mut(), extra.as_object()) {
            m.extend(x.clone());
        }
        let r = e.execute("file.bounce_mix", &p).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 44, "{name} has audio");
        let level = |k: &str| r[k].as_f64().unwrap() as f32;
        got.push([level("lufs"), level("true_peak_db"), level("peak_db")]);
    }
    let _ = std::fs::remove_dir_all(&dir);
    for (g, pin) in got.iter().zip(PINNED) {
        assert!(g.iter().zip(pin).all(|(v, p)| (v - f32::from_bits(p)).abs() < 1e-3), "{got:?}");
    }
}

/// A session with one stereo track playing a 440 Hz tone at `amplitude` for `secs`.
fn tone_session(amplitude: f32, secs: f32) -> (soundcraft_engine::Engine, i64) {
    let mut e = soundcraft_engine::Engine::default();
    let sr = e.session().sample_rate.hz();
    let frames = (sr as f32 * secs) as usize;
    let samples: Vec<f32> = (0..frames).map(|i| amplitude * (std::f32::consts::TAU * 440.0 * i as f32 / sr as f32).sin()).collect();
    let audio = soundcraft_audio_io::AudioBuffer { sample_rate: sr, channels: vec![samples.clone(), samples] };
    let s = e.session_mut();
    let track = s.add_track(soundcraft_model::TrackKind::Audio, soundcraft_model::ChannelFormat::Stereo, Some("Voice"));
    let source = soundcraft_engine::io::add_source(s, "tone", audio, None, FileFormat::Wav);
    let clip = soundcraft_model::Clip::audio(s.new_clip_id(), "tone", source, 0, 0, frames as i64);
    soundcraft_engine::edit::place_clip(s, track, clip);
    (e, frames as i64)
}

#[test]
fn loudness_target_bounces_land_on_the_podcast_target() {
    // A quiet tone (about -32 LUFS) is raised to -16 LUFS; its true peak stays under -1 dBTP.
    let (mut e, frames) = tone_session(0.02, 6.0);
    let dir = scratch("bounce-loudness");
    let path = dir.join("Episode.wav");
    let result = e
        .execute(
            "file.bounce_mix",
            &serde_json::json!({"path": path, "bit_depth": "32f", "dither": false, "loudness": "podcast", "start": 0, "end": frames}),
        )
        .unwrap();
    let lufs = result["lufs"].as_f64().unwrap();
    assert!((lufs + 16.0).abs() < 0.2, "{result}");
    assert!(result["true_peak_db"].as_f64().unwrap() <= -1.0 + 1e-3, "{result}");
    assert!(result["gain_db"].as_f64().unwrap() > 10.0, "{result}");
    // The file holds what was reported.
    let (_, file) = soundcraft_audio_io::decode(&std::fs::read(&path).unwrap(), None).unwrap();
    let mut meter = soundcraft_dsp::meter::LoudnessMeter::new(file.sample_rate as f32, file.num_channels());
    meter.process(&file.channels, file.frames());
    assert_eq!(f64::from(meter.integrated_lufs()), lufs);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loudness_target_never_pushes_true_peaks_over_the_ceiling() {
    // A stereo sine's true peak sits about 0.7 dB above its loudness: at -9 LUFS it would peak
    // near -8.3 dBTP, so a -10 dBTP ceiling wins and the bounce lands quieter than asked.
    let (mut e, frames) = tone_session(0.1, 4.0);
    let dir = scratch("bounce-loudness-ceiling");
    let path = dir.join("Hot.wav");
    let result = e
        .execute(
            "file.bounce_mix",
            &serde_json::json!({"path": path, "bit_depth": "32f", "dither": false, "loudness": -9, "true_peak": -10, "start": 0, "end": frames}),
        )
        .unwrap();
    let tp = result["true_peak_db"].as_f64().unwrap();
    assert!((tp + 10.0).abs() < 0.05, "{result}");
    assert!(result["lufs"].as_f64().unwrap() < -9.5, "{result}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loudness_params_are_checked() {
    let (mut e, frames) = tone_session(0.1, 1.0);
    let dir = scratch("bounce-loudness-params");
    let path = dir.join("X.wav");
    for (params, needle) in [
        (serde_json::json!({"loudness": "loud"}), "unknown loudness"),
        (serde_json::json!({"loudness": -60}), "outside -40..-5"),
        (serde_json::json!({"loudness": "podcast", "true_peak": 3}), "outside -12..0"),
        (serde_json::json!({"loudness": true}), "preset name or a LUFS number"),
    ] {
        let mut p = params;
        p["path"] = serde_json::json!(path);
        p["end"] = serde_json::json!(frames);
        let err = e.execute("file.bounce_mix", &p).unwrap_err().to_string();
        assert!(err.contains(needle), "{p}: {err}");
    }
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loudness_target_gain_handles_silence_and_both_limits() {
    let t = soundcraft_engine::io::LoudnessTarget::PODCAST;
    assert_eq!(t.gain_db(-144.0, -144.0), 0.0, "silence is left alone");
    assert_eq!(t.gain_db(f32::NAN, 0.0), 0.0);
    assert_eq!(t.gain_db(-26.0, -12.0), 10.0, "loudness decides");
    assert_eq!(t.gain_db(-26.0, -5.0), 4.0, "the ceiling decides");
    assert_eq!(t.gain_db(-10.0, -1.0), -6.0, "too loud comes down");
    assert_eq!(soundcraft_engine::io::LoudnessTarget::preset("PODCAST_MONO").map(|t| t.lufs), Some(-19.0));
}
