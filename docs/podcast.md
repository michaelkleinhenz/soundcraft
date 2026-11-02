# PodCraft: podcast features

PodCraft is SoundCraft set up for making podcasts. Everything below is in addition to the full
workstation, which stays one switch away.

## Podcast mode

**PodCraft menu ▸ Podcast Mode** (command `ui.podcast_mode {value?: bool}`, toggles without
`value`). A first run starts in it; the choice is saved with the other settings.

It hides, from the menus, the toolbar, the rulers and the New Tracks dialog:

- **MIDI, instruments and notation:** the Event menu, MIDI editor, event list, MIDI keyboard,
  score editor, MIDI import/export, Sibelius, MIDI setup, MIDI, Instrument and Video track kinds.
- **Music timing:** Bars|Beats, tempo, meter, key and chord rulers, the tempo/meter toolbar
  readout, click, elastic audio and warping.
- **Video and post-production:** video windows and import, timecode and feet+frames, AAF/OMF and
  Media Composer export, sync offsets.
- **Surround:** every track width other than mono and stereo, object views and Atmos automation.

Nothing is unregistered: shortcuts, the CLI, the control channel and MCP still run every command.
The list lives in `crates/ui-egui/src/podcast.rs`; a test checks that every hidden path is a real
menu item.

## New episode template

**File ▸ New… ▸ Template: Podcast episode** (the default in Podcast mode), or
`session.new {template: "podcast", speakers?: ["Host", "Guest"]}`. It creates:

- one **mono track per speaker** (at most 8), each with a speech chain: high-pass EQ at 80 Hz with
  a small mud cut and presence lift, a gentle expander for room noise, a 3:1 compressor and a
  de-esser; all routed to a **Voices** bus and aux;
- **Music** (-12 dB) and **SFX** (-6 dB) stereo tracks;
- a **Master** with a limiter (ceiling -1.5 dBFS);
- Min:Secs and Markers rulers.

## Loudness-target export

**File ▸ Bounce Mix… ▸ Loudness** (Podcast, stereo is the default in Podcast mode), or
`file.bounce_mix {loudness: podcast|podcast_mono|video|broadcast|<LUFS>, true_peak?: -1}`.

| Preset | Integrated loudness | True-peak ceiling |
|---|---|---|
| `podcast` (Apple Podcasts, Spotify) | -16 LUFS | -1 dBTP |
| `podcast_mono` | -19 LUFS | -1 dBTP |
| `video` | -14 LUFS | -1 dBTP |
| `broadcast` (EBU R128) | -23 LUFS | -1 dBTP |

The bounce renders twice: once to measure integrated loudness (ITU-R BS.1770, gated) and true
peak, once to write with one gain. The gain is the smaller of the one that reaches the target and
the one that keeps the true peak under the ceiling, so a mix with hot peaks lands below the target
instead of over the ceiling (put a limiter on the master to get closer). The result reports
`lufs`, `true_peak_db` and the `gain_db` applied.
