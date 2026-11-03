# PodCraft: podcast features

PodCraft is SoundCraft set up for making podcasts. Everything below is in addition to the full
workstation, which stays one switch away.

## Podcast mode

**PodCraft menu ▸ Podcast Mode** (command `ui.podcast_mode {value?: bool}`, toggles without
`value`). A first run starts in it; the choice is saved with the other settings.

**Menus.** Podcast mode shows a short list of podcast items (about 90 of the workstation's 500)
and hides the rest: recording, editing speech, cleaning it up, mixing and publishing. The list,
`SHOWN_MENU_PATHS` in `crates/ui-egui/src/podcast.rs`, is an allowlist, so new workstation
features stay out of Podcast mode until someone adds them; a test checks every entry is a real
menu item. AudioSuite is called **Process** in the menu bar.

**Everything else it hides**, from the toolbar, the track headers, the rulers, the plugin lists and
the New Tracks dialog:

- **MIDI, instruments and notation:** MIDI, Instrument and Video track kinds and the music windows.
- **Music timing:** Bars|Beats, tempo, meter, key and chord rulers, the tempo/meter readout, the
  grid and nudge values.
- **Video and surround:** timecode and feet+frames rulers, every track width but mono and stereo.
- **Music effects:** pitch, time shift, modulation and delay, lo-fi, rectifier, subharmonic, amp
  sims, the plate reverb and the signal generator in the insert menus. EQ, dynamics, the channel
  strip, the room reverb, gain, trim, DC removal, dither, the saturator, the voice cleanup plugins
  and third-party plugins stay.
- **Studio extras:** VCA tracks; in the track headers, playlists, freeze, voice, automation mode,
  timebase and Elastic Audio; the Markers 2–5 rulers.

The Edit window toolbar offers Shuffle and Slip (Spot and Grid stay on F3 and F4) and four tools,
Zoom, Trim, Selector and Grabber, whose tooltips say what each does (Scrubber and Pencil stay on F9
and F10). An empty session shows a **Start an episode** card with New Episode…, From a Template…,
Import Recordings…, Open Session… and the Quick Start Guide. Speaker tracks from the template are
Large, so speech waveforms are easy to read.

**Quick Start Guide** (PodCraft menu, `window.quick_start`; a first run opens it): making an
episode in six steps (start, record or import, edit, clean up, music and segments, check and
publish), each with buttons that open the tools for that step.

Nothing is unregistered: shortcuts, the CLI, the control channel and MCP still run every command,
and switching Podcast mode off brings the full workstation back.

## New episode template

**File ▸ New… ▸ Template: Podcast episode** (the default in Podcast mode), or
`session.new {template: "podcast", speakers?: ["Host", "Guest"]}`. It creates:

- one **mono track per speaker** (at most 8), each with a speech chain: high-pass EQ at 80 Hz with
  a small mud cut and presence lift, a gentle expander for room noise, a 3:1 compressor and a
  de-esser; all routed to a **Voices** bus and aux;
- **Music** (-12 dB) and **SFX** (-6 dB) stereo tracks;
- a **Master** with a limiter (ceiling -1.5 dBFS);
- Min:Secs and Markers rulers.

## Episode templates

**File ▸ Episode Templates…** (also **From a Template…** on the welcome card). A template is an
episode's set-up without its recordings: the tracks with their formats, names, colours and heights,
routing, levels, busses and groups, every plugin chain with its settings, the **export settings**
and the show's details (show, author, cover art, explicit). Clips, sources, markers, automation
points and this episode's title, number, date and notes are left out.

- **New Episode** on a row starts a session from it (asking to save the open one first).
  Built-ins: **Interview** (Host, Guest), **Solo show** (Host) and **Panel** (Host and three
  guests), each the New Episode set-up with podcast export settings.
- **Save Template** writes the open session as `<name>.pctemplate` (JSON) into the templates
  folder, by default `PodCraft Templates` in the home folder (changeable in the window, kept with
  the app settings). Saving over an existing name asks once.

Commands: `template.list {dir?}`, `template.new {template: interview|solo|panel|<path>, name?,
sample_rate?}`, `template.save {path, name?, description?, replace?}`.

### Export settings

`session.export_settings {format?: wav|aiff|flac|mp3, bit_depth?: 16|24|32f, loudness?:
podcast|podcast_mono|video|broadcast|off, bitrate?, normalize?, dither?, clear?}` keeps the
session's export choices. Bounce Mix opens with them, and a bounce with different choices stores
the new ones (one undo step), so a template saved afterwards carries them.

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

## Episode info

**File ▸ Episode Info…**, or `episode.set {show?, title?, author?, season?, episode?, date?,
description?, artwork?, explicit?}` (`episode.get` reads it back). The details are stored in the
session, and **Apply** is one undoable step. Cover art is a JPEG or PNG (up to 10 MiB; stores
want 1400–3000 px square); a relative path is read from the session's folder.

## Chapter and edit markers

Two kinds of marker, each one key in Podcast mode (they work whatever the keyboard focus, and
take over Commands Focus' M and E there):

| Key | Command | Marker | Use |
|---|---|---|---|
| **M** | `markers.add_chapter` (Edit ▸ Add Chapter Marker) | yellow, "Chapter n" | starts a chapter in the exports |
| **E** | `markers.add_edit {comments?}` (Edit ▸ Add Edit Marker) | red triangle, "Edit n" | flags a spot to fix later (a flub, a cough, a retake) |
| | `markers.add_ad {comments?}` (Edit ▸ Add Ad Slot Marker, or **+ Ad** in Memory Locations) | green diamond, "Ad n" | where an ad or sponsor read goes ([Segments](#segments-and-ad-slots)) |

Both go at the playhead while playing, else at the insertion point, so you can tap them while
listening back. Edit ▸ Go to Next / Previous Edit Marker (`markers.next_edit`,
`markers.previous_edit`) step through the edit markers only; delete one in Window ▸ Memory
Locations once it's fixed. Edit and ad slot markers are never chapters. Outside Podcast mode the same keys
work with Commands Keyboard Focus off, and Enter still adds a plain marker.

## Chapters

Every chapter marker (or selection memory location) is a chapter, edit markers aside: its name is the title, it runs to the
next marker or the end of the export, and the first web address in its comments becomes the
chapter link. `chapters.list {start?, end?}` shows them.

**File ▸ Export ▸ Chapters…**, or `file.export_chapters {path, format?: json|txt, start?, end?,
replace?}`, writes either a [Podcasting 2.0](https://github.com/Podcastindex-org/podcast-namespace/blob/main/chapters/jsonChapters.md)
`chapters.json` (version 1.2.0, with the show and episode title) for the RSS
`<podcast:chapters>` tag, or plain `MM:SS Title` lines (`H:MM:SS` from an hour on) for show notes
and YouTube descriptions. Times are relative to the exported range.

## MP3

**File ▸ Bounce Mix… ▸ Format ▸ MP3** (the default in Podcast mode), or `file.bounce_mix
{path: "….mp3", bitrate?: 128}`. The bitrate is constant, 8 to 320 kbps (the dialog offers 64 to 320, 128 by default; 64 or
96 suit speech-only mono). An MP3 bounce:

- applies the loudness target like any other bounce;
- starts with an ID3v2.3 tag holding the episode info (title, show as album, author, season as
  disc, episode as track, the year of the date, the description as a comment marked
  `[Explicit]` when set, genre Podcast) and the cover art;
- carries the chapters as ID3 `CHAP`/`CTOC` frames (at most 255), which Apple Podcasts, Overcast
  and Pocket Casts show;
- ends its first frame with an Info/LAME tag, so players know the exact length (gapless, with
  sample-accurate seeking).

MP3 holds mono or stereo at 48, 44.1 or 32 kHz (and lower rates): a session at 96 kHz or a
surround mix is refused before any file is written. The encoder is
[`rusty_mp3`](https://crates.io/crates/rusty_mp3), a pure-Rust Apache-2.0 encoder (LAME and its
ports are LGPL, which this project does not take).

## FLAC

**File ▸ Bounce Mix… ▸ Format ▸ FLAC** writes a lossless master, 16 or 24 bit, for archiving or
for hosts that take FLAC. It carries the same episode info as an MP3, as Vorbis comments (`TITLE`,
`ALBUM`, `ARTIST`, `TRACKNUMBER`, `DISCNUMBER`, `DATE`, `DESCRIPTION`, `GENRE`), the chapters as
`CHAPTER001=HH:MM:SS.mmm` / `CHAPTER001NAME` / `CHAPTER001URL`, and the cover art as a front-cover
`PICTURE` block. It keeps the session's sample rate and up to 8 channels.

## Voice presets

**Track ▸ Voice Preset ▸ Standard / Broadcast / Warm / Phone/Remote / Room-heavy**, or
`track.voice_preset {preset, tracks?}`, replaces inserts A–D of the selected audio or aux tracks
with a speech chain (EQ, expander, compressor, de-esser) tuned for that voice. Standard is the New
Episode template's chain; Phone/Remote band-limits and levels a thin call; Room-heavy cuts
boxiness and gates the room harder between phrases. One undo step.

## Tighten pauses

**Edit ▸ Tighten Pauses…**, or `edit.tighten_pauses {max_pause_ms?: 1500, keep_ms?: 700,
threshold_db?: -45, tracks?, start?, end?, dry_run?}`. Finds every pause where nobody on the
speaker tracks talks (Strip Silence's detector, over all of them together) longer than
`max_pause_ms`, and shortens it to `keep_ms` with a ripple on all of those tracks at once, so the
conversation stays in sync; markers after a cut move with it. Speaker tracks default to the mono
audio tracks (the template's speakers). Preview (`dry_run`) reports what it would cut.

## Auto-ducking

**Track ▸ Auto-Duck Music…**, or `mix.auto_duck {tracks?, voices?, depth_db?: -12, attack_ms?:
300, release_ms?: 800, threshold_db?: -45, start?, end?}`. Writes volume automation on the music
and effects tracks (default: the stereo audio tracks) that dips by `depth_db` while anyone on the
speaker tracks talks, fading down over `attack_ms` before the words and back up over `release_ms`
after them. It is ordinary automation: edit or redraw it afterwards.

## Episode pre-flight check

**File ▸ Episode Pre-flight Check…**, or `session.preflight {start?, end?, target?}`. Renders the
mix once and reports:

- clipping (samples at 0 dBFS);
- integrated loudness more than 1 LU off the target (default `podcast`, -16 LUFS) and a true peak
  over its ceiling;
- more than 2 s of silence at the start or the end;
- dead air: 3 s or more of near-silence (under -50 dBFS) inside the episode, with a Go button;
- speaker balance: one speaker track (soloed, through its chain) more than 3 LU louder than another.

## Syncing remote recordings

**Track ▸ Sync Remote Recordings…**, or `track.sync_recordings {reference?, tracks?, method?:
auto|clap, max_offset_s?: 30, drift?: true, clap_window_s?: 60, dry_run?}`. For a double-ender,
where each speaker records themselves locally and the call is recorded too: the speaker tracks
(default: the mono audio tracks) move to line up with the reference, the track that hears everyone
(default: one named call, zoom, riverside or reference).

- **Voices** (`auto`) matches the tracks' loudness envelopes (1 ms steps) by cross-correlation over
  the first ten minutes, searching ±`max_offset_s`. With `drift`, it matches four stretches across
  the recording again; when a recorder's clock ran fast or slow (2 ppm to 0.1%), the track's clips
  are played that much faster or slower (clip varispeed) so the end lines up as well as the start.
- **Clap** lines up the loudest moment in the first `clap_window_s` of each track.

Material moved before the session start is trimmed off. Analyze (`dry_run`) reports the offset,
the drift in ms an hour and how well each track matched; a track that matches under 15% is left
where it is.

## Segments and ad slots

**Edit ▸ Insert Segment…**, or `segment.insert {path | source, name?, at? | marker? | ad_slots?,
ripple?: true, track?, fade_ms?: 20}`. Drops a reusable piece of audio into the episode at the
insertion point, at a marker, or at every ad slot marker. With `ripple` it opens room on every
track first (clips, automation and later markers move later; a marker exactly at the spot stays at
the start of the segment), so it suits intros, outros and sponsor reads; without, it lays the audio
over what is there, for stingers or music under talk. It goes on the **Segments** track (made when
missing) with short fades.

The window keeps a library of segment files (name, kind: Intro, Outro, Sponsor, Stinger or Other)
with the app's settings, so the same intro and sponsor reads are one click away in every episode.

## Voice cleanup

Four built-in plugins (insert menus ▸ Other, and **AudioSuite ▸ Noise Reduction** to render them
into the selected clips), all written from scratch for PodCraft:

| Plugin | Id | What it does | Parameters |
|---|---|---|---|
| Noise Reduction | `noise_reduction` | An adaptive spectral gate (1024-point STFT, 75% overlap). Each frequency bin's noise floor follows the quietest level it has had recently, so it learns the room from the pauses between words without a separate noise print; bins near the floor are turned down. Latency 1024 samples (compensated in playback and AudioSuite). | Reduction 0–40 dB, Sensitivity 0–12 dB over the floor, Learn Speed 1–10 s |
| Hum Removal | `hum_removal` | Narrow notches at 50 or 60 Hz and up to 8 harmonics. | Mains, Harmonics, Width (Q) |
| De-Click | `de_click` | Finds mouth clicks and pops as sudden high-frequency outliers and bridges each with a line between the good samples around it. Latency 32 samples. | Sensitivity 1–10 |
| De-Plosive | `de_plosive` | Turns the band below the frequency down only while it bursts over the threshold (a "p" or "b" thump); a Linkwitz-Riley split, so it is transparent when idle. | Frequency, Threshold, Max Reduction |

A steady tone or drone is indistinguishable from noise to Noise Reduction: it is meant for
speech, which keeps stopping.
