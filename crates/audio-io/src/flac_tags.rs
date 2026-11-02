//! The episode tag as FLAC metadata blocks: a VORBIS_COMMENT (title, album, artist, track and
//! disc numbers, date, description, genre and chapters in the `CHAPTERnnn` convention) and a
//! front-cover PICTURE.

use crate::id3::{Id3Chapter, Id3Tag};
use crate::{AudioError, Result};

/// VORBIS_COMMENT.
pub(crate) const VORBIS_COMMENT: u8 = 4;
/// PICTURE.
pub(crate) const PICTURE: u8 = 6;
/// The largest metadata block body (a 24-bit length).
const MAX_BLOCK: usize = (1 << 24) - 1;
const VENDOR: &str = "PodCraft";

fn le(out: &mut Vec<u8>, n: usize) -> Result<()> {
    let n = u32::try_from(n).map_err(|_| AudioError::TooLarge("a FLAC tag field is too large".into()))?;
    out.extend_from_slice(&n.to_le_bytes());
    Ok(())
}

fn be(out: &mut Vec<u8>, n: usize) -> Result<()> {
    let n = u32::try_from(n).map_err(|_| AudioError::TooLarge("a FLAC picture is too large".into()))?;
    out.extend_from_slice(&n.to_be_bytes());
    Ok(())
}

/// `HH:MM:SS.mmm`, the `CHAPTERnnn` time format.
fn chapter_time(ms: u32) -> String {
    let (s, ms) = (ms / 1000, ms % 1000);
    format!("{:02}:{:02}:{:02}.{ms:03}", s / 3600, s / 60 % 60, s % 60)
}

fn comments(tag: &Id3Tag) -> Vec<(String, String)> {
    let mut c = Vec::new();
    let mut add = |k: &str, v: &str| {
        if !v.is_empty() {
            c.push((k.to_string(), v.to_string()));
        }
    };
    add("TITLE", &tag.title);
    add("ALBUM", &tag.album);
    add("ARTIST", &tag.artist);
    add("TRACKNUMBER", &tag.track.map(|n| n.to_string()).unwrap_or_default());
    add("DISCNUMBER", &tag.disc.map(|n| n.to_string()).unwrap_or_default());
    add("DATE", &tag.year);
    add("DESCRIPTION", &tag.comment);
    add("GENRE", &tag.genre);
    for (i, Id3Chapter { start_ms, title, url, .. }) in tag.chapters.iter().enumerate() {
        let key = format!("CHAPTER{:03}", i + 1);
        add(&key, &chapter_time(*start_ms));
        add(&format!("{key}NAME"), title);
        add(&format!("{key}URL"), url.as_deref().unwrap_or_default());
    }
    c
}

/// The metadata blocks (type, body) that carry `tag`; none for an empty tag.
pub(crate) fn blocks(tag: &Id3Tag) -> Result<Vec<(u8, Vec<u8>)>> {
    let mut out = Vec::new();
    let fields = comments(tag);
    if !fields.is_empty() {
        let mut b = Vec::new();
        le(&mut b, VENDOR.len())?;
        b.extend_from_slice(VENDOR.as_bytes());
        le(&mut b, fields.len())?;
        for (k, v) in &fields {
            // Field names are ASCII without `=`; values are UTF-8 and may hold anything.
            le(&mut b, k.len() + 1 + v.len())?;
            b.extend_from_slice(k.as_bytes());
            b.push(b'=');
            b.extend_from_slice(v.as_bytes());
        }
        out.push((VORBIS_COMMENT, b));
    }
    if let Some(art) = &tag.artwork {
        let mut b = Vec::new();
        be(&mut b, 3)?; // front cover
        be(&mut b, art.mime.len())?;
        b.extend_from_slice(art.mime.as_bytes());
        be(&mut b, 0)?; // no description
        for _ in 0..4 {
            be(&mut b, 0)?; // width, height, depth, colours: unknown
        }
        be(&mut b, art.data.len())?;
        b.extend_from_slice(&art.data);
        out.push((PICTURE, b));
    }
    if out.iter().any(|(_, b)| b.len() > MAX_BLOCK) {
        return Err(AudioError::TooLarge("the FLAC tag is larger than a metadata block holds (16 MiB)".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id3::Id3Artwork;

    #[test]
    fn chapter_times_are_hours_minutes_seconds_and_millis() {
        assert_eq!(chapter_time(0), "00:00:00.000");
        assert_eq!(chapter_time(3_723_045), "01:02:03.045");
    }

    #[test]
    fn an_empty_tag_has_no_blocks() {
        assert!(blocks(&Id3Tag::default()).unwrap().is_empty());
    }

    #[test]
    fn comments_and_picture_are_laid_out_per_the_spec() {
        let png = [b"\x89PNG\r\n\x1a\n".as_slice(), &[0; 8]].concat();
        let tag = Id3Tag {
            title: "Pilot".into(),
            track: Some(3),
            chapters: vec![Id3Chapter { start_ms: 61_500, end_ms: 90_000, title: "Intro".into(), url: Some("https://x.org".into()) }],
            artwork: Some(Id3Artwork::from_bytes(png.clone()).unwrap()),
            ..Default::default()
        };
        let b = blocks(&tag).unwrap();
        assert_eq!(b.len(), 2);
        let (kind, body) = &b[0];
        assert_eq!(*kind, VORBIS_COMMENT);
        let text = String::from_utf8_lossy(body);
        for field in ["TITLE=Pilot", "TRACKNUMBER=3", "CHAPTER001=00:01:01.500", "CHAPTER001NAME=Intro", "CHAPTER001URL=https://x.org"] {
            assert!(text.contains(field), "{field}");
        }
        assert_eq!(u32::from_le_bytes(body[12..16].try_into().unwrap()), 5, "five comments after the vendor");
        let (kind, pic) = &b[1];
        assert_eq!(*kind, PICTURE);
        assert_eq!(&pic[0..4], &3u32.to_be_bytes());
        assert!(pic.ends_with(&png));
    }

    #[test]
    fn a_tagged_flac_still_decodes_sample_for_sample() {
        let tag = Id3Tag {
            title: "Pilot".into(),
            chapters: vec![Id3Chapter { start_ms: 0, end_ms: 500, title: "Intro".into(), url: None }],
            ..Default::default()
        };
        let samples: Vec<f32> = (0..10_000).map(|i| ((i % 200) as f32 / 100.0 - 1.0) * 0.5).collect();
        let mut w = crate::FileEncoder::new_flac(
            std::io::Cursor::new(Vec::new()),
            &crate::FlacOptions { bit_depth: crate::BitDepth::Int16, dither: false, tag },
            1,
            48_000,
        )
        .unwrap();
        w.write(std::slice::from_ref(&samples), samples.len()).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(bytes[4] & 0x80, 0, "STREAMINFO is not the last block");
        assert!(bytes.windows(11).any(|w| w == b"TITLE=Pilot"));
        let (info, buf) = crate::decode(&bytes, Some("flac")).unwrap();
        assert_eq!((info.format, buf.frames()), (crate::FileFormat::Flac, samples.len()));
        let err = buf.channels[0].iter().zip(&samples).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(err < 1e-4, "{err}");
    }

    #[test]
    fn an_empty_tagged_flac_is_still_valid() {
        let tag = Id3Tag { title: "Pilot".into(), ..Default::default() };
        let w = crate::FileEncoder::new_flac(
            std::io::Cursor::new(Vec::new()),
            &crate::FlacOptions { bit_depth: crate::BitDepth::Int24, dither: false, tag },
            2,
            44_100,
        )
        .unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let (info, buf) = crate::decode(&bytes, Some("flac")).unwrap();
        assert_eq!((info.channels, buf.frames()), (2, 0));
    }
}
