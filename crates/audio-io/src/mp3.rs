//! MP3 (MPEG Layer III) writing through `rusty_mp3`, a permissively licensed (Apache-2.0)
//! pure-Rust encoder: an optional ID3v2 tag, then the audio, whose first frame is rewritten at the
//! end with the completed Info/Xing tag so players know the exact length (gapless, seekable).

use crate::{AudioError, Result};
use std::io::{Seek, SeekFrom, Write};

/// The sample rates Layer III encodes (MPEG-1, MPEG-2 and MPEG-2.5).
pub const SAMPLE_RATES: &[u32] = &[48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000];
/// The constant bitrates offered, in kbps (MPEG-1 values; lower rates snap to their own table).
pub const BITRATES: &[u32] = &[64, 96, 128, 160, 192, 256, 320];
/// The bitrate when none is given: transparent speech, a common podcast choice.
pub const DEFAULT_BITRATE: u32 = 128;

/// How to write an MP3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp3Options {
    /// Constant bitrate in kbps, snapped to the nearest legal value.
    pub bitrate_kbps: u32,
    /// An ID3v2 tag written before the audio (see [`crate::id3::Id3Tag::to_bytes`]); may be empty.
    pub id3: Vec<u8>,
}

impl Default for Mp3Options {
    fn default() -> Self {
        Mp3Options { bitrate_kbps: DEFAULT_BITRATE, id3: Vec::new() }
    }
}

/// Fails unless MP3 can carry `channels` channels at `sample_rate`.
pub fn check(channels: usize, sample_rate: u32) -> Result<()> {
    if !(1..=2).contains(&channels) {
        return Err(AudioError::Unsupported(format!("MP3 holds mono or stereo, not {channels} channels (fold the mix down to stereo)")));
    }
    if !SAMPLE_RATES.contains(&sample_rate) {
        return Err(AudioError::Unsupported(format!("MP3 has no {sample_rate} Hz; use a session at 48 kHz or 44.1 kHz")));
    }
    Ok(())
}

pub(crate) struct Writer<W> {
    out: W,
    enc: rusty_mp3::Mp3Encoder,
    channels: usize,
    sample_rate: u32,
    /// Where the first audio frame starts (after the ID3 tag).
    audio_start: u64,
    /// Interleaved samples of the block being pushed.
    scratch: Vec<f32>,
    failed: bool,
}

impl<W: Write + Seek> Writer<W> {
    pub(crate) fn new(mut out: W, opts: &Mp3Options, channels: usize, sample_rate: u32) -> Result<Self> {
        check(channels, sample_rate)?;
        out.write_all(&opts.id3)?;
        let audio_start = out.stream_position()?;
        let enc = rusty_mp3::Mp3Encoder::new(rusty_mp3::Mp3EncoderConfig { bitrate_kbps: opts.bitrate_kbps.clamp(8, 320), vbr_quality: None });
        Ok(Writer { out, enc, channels, sample_rate, audio_start, scratch: Vec::new(), failed: false })
    }

    pub(crate) fn write(&mut self, block: &[Vec<f32>], frames: usize) -> Result<()> {
        if self.failed {
            return Err(crate::failed_write_error());
        }
        let r = self.push(block, frames);
        self.failed = r.is_err();
        r
    }

    fn push(&mut self, block: &[Vec<f32>], frames: usize) -> Result<()> {
        if frames == 0 {
            return Ok(());
        }
        self.scratch.clear();
        self.scratch.try_reserve(frames.saturating_mul(self.channels)).map_err(|_| AudioError::TooLarge("cannot allocate an MP3 block".into()))?;
        for i in 0..frames {
            for c in 0..self.channels {
                self.scratch.push(block.get(c).and_then(|ch| ch.get(i)).copied().unwrap_or(0.0));
            }
        }
        let channels = u16::try_from(self.channels).unwrap_or(2);
        self.enc.push_pcm_f32(&self.scratch, channels, self.sample_rate).map_err(|e| AudioError::Encode(format!("MP3: {e:?}")))?;
        self.drain()
    }

    fn drain(&mut self) -> Result<()> {
        while let Ok(packet) = self.enc.next_packet() {
            self.out.write_all(&packet)?;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<W> {
        if self.failed {
            return Err(crate::failed_write_error());
        }
        if self.out.stream_position()? == self.audio_start {
            // Nothing pushed yet: fix the format with an empty push so the file is a valid stream.
            let channels = u16::try_from(self.channels).unwrap_or(2);
            self.enc.push_pcm_f32(&[], channels, self.sample_rate).map_err(|e| AudioError::Encode(format!("MP3: {e:?}")))?;
        }
        self.enc.finish();
        self.drain()?;
        if let Some(tag) = self.enc.lame_tag_frame() {
            self.out.seek(SeekFrom::Start(self.audio_start))?;
            self.out.write_all(tag)?;
            self.out.seek(SeekFrom::End(0))?;
        }
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn tone(frames: usize, sr: u32) -> Vec<f32> {
        (0..frames).map(|i| 0.4 * (std::f32::consts::TAU * 440.0 * i as f32 / sr as f32).sin()).collect()
    }

    #[test]
    fn an_mp3_decodes_back_to_its_length_and_level() {
        let sr = 44_100;
        let frames = sr as usize * 2;
        let t = tone(frames, sr);
        let mut w = Writer::new(Cursor::new(Vec::new()), &Mp3Options::default(), 2, sr).unwrap();
        for chunk in (0..frames).step_by(1000) {
            let n = 1000.min(frames - chunk);
            let block = vec![t[chunk..chunk + n].to_vec(), t[chunk..chunk + n].to_vec()];
            w.write(&block, n).unwrap();
        }
        let bytes = w.finish().unwrap().into_inner();
        let (info, buf) = crate::decode(&bytes, Some("mp3")).unwrap();
        assert_eq!(info.format, crate::FileFormat::Mp3);
        assert_eq!(buf.channels.len(), 2);
        assert_eq!(buf.sample_rate, sr);
        // The Info tag trims the encoder delay and padding: the length comes back exactly.
        assert_eq!(buf.frames(), frames, "gapless length");
        let peak = buf.peak();
        assert!((0.3..0.5).contains(&peak), "peak {peak}");
        // About 128 kbps.
        let kbps = bytes.len() as f64 * 8.0 / 2.0 / 1000.0;
        assert!((110.0..150.0).contains(&kbps), "{kbps} kbps");
    }

    #[test]
    fn the_id3_tag_comes_first_and_the_audio_still_decodes() {
        let tag = crate::id3::Id3Tag { title: "Ep 1".into(), ..Default::default() }.to_bytes().unwrap();
        let opts = Mp3Options { bitrate_kbps: 64, id3: tag.clone() };
        let mut w = Writer::new(Cursor::new(Vec::new()), &opts, 1, 48_000).unwrap();
        w.write(&[tone(48_000, 48_000)], 48_000).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert!(bytes.starts_with(&tag));
        let (_, buf) = crate::decode(&bytes, Some("mp3")).unwrap();
        assert_eq!(buf.channels.len(), 1);
        assert_eq!(buf.frames(), 48_000);
    }

    #[test]
    fn unsupported_layouts_and_rates_are_refused() {
        assert!(matches!(Writer::new(Cursor::new(Vec::new()), &Mp3Options::default(), 6, 48_000), Err(AudioError::Unsupported(_))));
        assert!(matches!(Writer::new(Cursor::new(Vec::new()), &Mp3Options::default(), 2, 96_000), Err(AudioError::Unsupported(_))));
    }

    #[test]
    fn an_empty_mp3_is_still_a_valid_stream() {
        let w = Writer::new(Cursor::new(Vec::new()), &Mp3Options::default(), 2, 48_000).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let (_, buf) = crate::decode(&bytes, Some("mp3")).unwrap();
        assert_eq!(buf.frames(), 0);
    }
}
