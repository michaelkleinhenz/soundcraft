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
- **Music effects:** pitch, time shift, modulation and delay, lo-fi, rectifier, subharmonic,
  amp sims, the plate reverb, the signal generator and time compression/expansion, from the insert
  menus and AudioSuite. EQ, dynamics, the channel strip, the room reverb, gain, trim, DC removal,
  dither, the saturator and third-party plugins stay.
- **Studio and console extras:** VCA tracks, routing folders, Freeze/Commit, insert bypass by
  category, playlists, Narrow Mix, workspaces, window configurations, Task Manager, peripherals,
  disk allocation, collaboration, sign-in and the dashboard.

The Edit window is simpler too: the toolbar offers Shuffle and Slip (Spot and Grid stay on F3 and
F4) and the Trim, Selector, Grabber and Zoom tools (Scrubber and Pencil stay on F9 and F10), and
an empty session shows a **Start an episode** card with New Episode…, Import Recordings… and Open
Session…. Speaker tracks from the template are Large, so speech waveforms are easy to read.

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

Both go at the playhead while playing, else at the insertion point, so you can tap them while
listening back. Edit ▸ Go to Next / Previous Edit Marker (`markers.next_edit`,
`markers.previous_edit`) step through the edit markers only; delete one in Window ▸ Memory
Locations once it's fixed. Edit markers are never chapters. Outside Podcast mode the same keys
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
