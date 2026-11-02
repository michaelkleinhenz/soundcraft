//! ID3v2.3 tags for MP3 files: episode text fields, cover art and chapters (the ID3v2 Chapter
//! Frame Addendum's `CHAP` and `CTOC`), which podcast apps read.

use crate::{AudioError, Result};

/// The largest cover image a tag carries (podcast directories ask for well under this).
pub const MAX_ARTWORK_BYTES: usize = 10 * 1024 * 1024;
/// A table of contents lists at most 255 chapters (its entry count is one byte).
pub const MAX_CHAPTERS: usize = 255;
/// Text fields are cut to this many characters.
const MAX_TEXT_CHARS: usize = 16_384;

/// A chapter: where it starts and ends in the file, in milliseconds, its title and an optional
/// link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Id3Chapter {
    pub start_ms: u32,
    pub end_ms: u32,
    pub title: String,
    pub url: Option<String>,
}

/// Cover art: a JPEG or PNG image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Id3Artwork {
    pub mime: &'static str,
    pub data: Vec<u8>,
}

impl Id3Artwork {
    /// Cover art from image file bytes; JPEG and PNG are recognized by their signatures.
    pub fn from_bytes(data: Vec<u8>) -> Result<Id3Artwork> {
        if data.len() > MAX_ARTWORK_BYTES {
            return Err(AudioError::TooLarge(format!("cover art is {} bytes; the most is {MAX_ARTWORK_BYTES}", data.len())));
        }
        let mime = if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else {
            return Err(AudioError::Unsupported("cover art must be a JPEG or PNG image".into()));
        };
        Ok(Id3Artwork { mime, data })
    }
}

/// The fields of a tag. Empty text fields are left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Id3Tag {
    /// `TIT2`: the episode title.
    pub title: String,
    /// `TALB`: the show.
    pub album: String,
    /// `TPE1`: the host or author.
    pub artist: String,
    /// `TRCK`: the episode number.
    pub track: Option<u32>,
    /// `TPOS`: the season number.
    pub disc: Option<u32>,
    /// `TYER`: the year.
    pub year: String,
    /// `COMM`: the description.
    pub comment: String,
    /// `TCON`: the genre ("Podcast").
    pub genre: String,
    /// `APIC`: the front cover.
    pub artwork: Option<Id3Artwork>,
    /// `CHAP` frames and a `CTOC` that lists them in order (at most [`MAX_CHAPTERS`]).
    pub chapters: Vec<Id3Chapter>,
}

impl Id3Tag {
    /// Whether the tag has nothing to write.
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
            && self.album.is_empty()
            && self.artist.is_empty()
            && self.track.is_none()
            && self.disc.is_none()
            && self.year.is_empty()
            && self.comment.is_empty()
            && self.genre.is_empty()
            && self.artwork.is_none()
            && self.chapters.is_empty()
    }

    /// The tag's bytes: an ID3v2.3 header and its frames (nothing at all for an empty tag).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.is_empty() {
            return Ok(Vec::new());
        }
        let mut frames = Vec::new();
        for (id, text) in [("TIT2", &self.title), ("TALB", &self.album), ("TPE1", &self.artist), ("TYER", &self.year), ("TCON", &self.genre)] {
            if !text.is_empty() {
                frame(&mut frames, id, &text_body(text))?;
            }
        }
        for (id, n) in [("TRCK", self.track), ("TPOS", self.disc)] {
            if let Some(n) = n {
                frame(&mut frames, id, &text_body(&n.to_string()))?;
            }
        }
        if !self.comment.is_empty() {
            let mut body = vec![1u8];
            body.extend_from_slice(b"eng");
            body.extend(utf16(""));
            body.extend_from_slice(&[0, 0]);
            body.extend(utf16(&self.comment));
            frame(&mut frames, "COMM", &body)?;
        }
        if let Some(art) = &self.artwork {
            let mut body = vec![0u8];
            body.extend_from_slice(art.mime.as_bytes());
            body.push(0);
            body.push(3); // front cover
            body.push(0); // empty description
            body.extend_from_slice(&art.data);
            frame(&mut frames, "APIC", &body)?;
        }
        let chapters = self.chapters.get(..self.chapters.len().min(MAX_CHAPTERS)).unwrap_or_default();
        if !chapters.is_empty() {
            let mut toc = Vec::new();
            toc.extend_from_slice(b"toc\0");
            toc.push(0x03); // top level, ordered
            toc.push(u8::try_from(chapters.len()).unwrap_or(u8::MAX));
            for i in 0..chapters.len() {
                toc.extend_from_slice(format!("chp{i}").as_bytes());
                toc.push(0);
            }
            frame(&mut frames, "CTOC", &toc)?;
            for (i, c) in chapters.iter().enumerate() {
                let mut body = Vec::new();
                body.extend_from_slice(format!("chp{i}").as_bytes());
                body.push(0);
                body.extend_from_slice(&c.start_ms.to_be_bytes());
                body.extend_from_slice(&c.end_ms.max(c.start_ms).to_be_bytes());
                body.extend_from_slice(&u32::MAX.to_be_bytes()); // no byte offsets
                body.extend_from_slice(&u32::MAX.to_be_bytes());
                frame(&mut body, "TIT2", &text_body(&c.title))?;
                if let Some(url) = c.url.as_deref().filter(|u| !u.is_empty() && u.is_ascii()) {
                    let mut w = vec![0u8, 0u8]; // Latin-1, empty description
                    w.extend_from_slice(url.as_bytes());
                    frame(&mut body, "WXXX", &w)?;
                }
                frame(&mut frames, "CHAP", &body)?;
            }
        }
        let size =
            u32::try_from(frames.len()).ok().filter(|n| *n < (1 << 28)).ok_or_else(|| AudioError::TooLarge("ID3 tag is over 256 MiB".into()))?;
        let mut out = Vec::with_capacity(frames.len() + 10);
        out.extend_from_slice(b"ID3\x03\x00\x00");
        out.extend_from_slice(&syncsafe(size));
        out.extend_from_slice(&frames);
        Ok(out)
    }
}

/// A 28-bit size as four 7-bit bytes.
fn syncsafe(n: u32) -> [u8; 4] {
    [((n >> 21) & 0x7F) as u8, ((n >> 14) & 0x7F) as u8, ((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8]
}

/// UTF-16 with a byte-order mark (ID3v2.3's Unicode encoding), little-endian.
fn utf16(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for u in text.chars().take(MAX_TEXT_CHARS).collect::<String>().encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// A text frame's body: the encoding byte (1, UTF-16) and the text.
fn text_body(text: &str) -> Vec<u8> {
    let mut body = vec![1u8];
    body.extend(utf16(text));
    body
}

/// Appends an ID3v2.3 frame: four-letter id, big-endian size, two flag bytes, body.
fn frame(out: &mut Vec<u8>, id: &str, body: &[u8]) -> Result<()> {
    let size = u32::try_from(body.len()).map_err(|_| AudioError::TooLarge(format!("ID3 {id} frame is too large")))?;
    out.extend_from_slice(id.as_bytes());
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(body);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frames of a tag: (id, body).
    fn frames(tag: &[u8]) -> Vec<(String, Vec<u8>)> {
        assert_eq!(&tag[..6], b"ID3\x03\x00\x00");
        let size = tag[6..10].iter().fold(0usize, |n, b| (n << 7) | usize::from(*b));
        assert_eq!(size, tag.len() - 10);
        let mut out = Vec::new();
        let mut at = 10;
        while at < tag.len() {
            let id = String::from_utf8(tag[at..at + 4].to_vec()).unwrap();
            let len = u32::from_be_bytes(tag[at + 4..at + 8].try_into().unwrap()) as usize;
            out.push((id, tag[at + 10..at + 10 + len].to_vec()));
            at += 10 + len;
        }
        out
    }

    fn text(body: &[u8]) -> String {
        assert_eq!(&body[..3], &[1, 0xFF, 0xFE]);
        let units: Vec<u16> = body[3..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn an_empty_tag_writes_nothing() {
        assert!(Id3Tag::default().to_bytes().unwrap().is_empty());
    }

    #[test]
    fn text_fields_numbers_and_chapters_are_written() {
        let tag = Id3Tag {
            title: "Episode 12: Grüße".into(),
            album: "The Show".into(),
            track: Some(12),
            disc: Some(2),
            genre: "Podcast".into(),
            comment: "About things".into(),
            chapters: vec![
                Id3Chapter { start_ms: 0, end_ms: 60_000, title: "Intro".into(), url: None },
                Id3Chapter { start_ms: 60_000, end_ms: 90_500, title: "News".into(), url: Some("https://example.com".into()) },
            ],
            ..Id3Tag::default()
        };
        let f = frames(&tag.to_bytes().unwrap());
        let get = |id: &str| f.iter().find(|(i, _)| i == id).map(|(_, b)| b.clone()).unwrap();
        assert_eq!(text(&get("TIT2")), "Episode 12: Grüße");
        assert_eq!(text(&get("TALB")), "The Show");
        assert_eq!(text(&get("TRCK")), "12");
        assert_eq!(text(&get("TPOS")), "2");
        assert_eq!(text(&get("TCON")), "Podcast");
        assert!(get("COMM").starts_with(&[1, b'e', b'n', b'g']));
        let toc = get("CTOC");
        assert_eq!(&toc[..6], b"toc\0\x03\x02");
        assert_eq!(&toc[6..], b"chp0\0chp1\0");
        let chaps: Vec<&Vec<u8>> = f.iter().filter(|(i, _)| i == "CHAP").map(|(_, b)| b).collect();
        assert_eq!(chaps.len(), 2);
        let c = chaps[1];
        assert_eq!(&c[..5], b"chp1\0");
        assert_eq!(u32::from_be_bytes(c[5..9].try_into().unwrap()), 60_000);
        assert_eq!(u32::from_be_bytes(c[9..13].try_into().unwrap()), 90_500);
        assert_eq!(&c[13..21], &[0xFF; 8]);
        // Sub-frames: TIT2 then WXXX.
        let sub = &c[21..];
        assert_eq!(&sub[..4], b"TIT2");
        let tit_len = u32::from_be_bytes(sub[4..8].try_into().unwrap()) as usize;
        assert_eq!(text(&sub[10..10 + tit_len]), "News");
        let wxxx = &sub[10 + tit_len..];
        assert_eq!(&wxxx[..4], b"WXXX");
        assert!(wxxx.ends_with(b"\0\0https://example.com"));
    }

    #[test]
    fn artwork_must_be_a_jpeg_or_png_of_reasonable_size() {
        assert_eq!(Id3Artwork::from_bytes(vec![0xFF, 0xD8, 0xFF, 0xE0]).unwrap().mime, "image/jpeg");
        assert_eq!(Id3Artwork::from_bytes(b"\x89PNG\r\n\x1a\n....".to_vec()).unwrap().mime, "image/png");
        assert!(matches!(Id3Artwork::from_bytes(b"GIF89a".to_vec()), Err(AudioError::Unsupported(_))));
        assert!(matches!(Id3Artwork::from_bytes(vec![0xFF; MAX_ARTWORK_BYTES + 1]), Err(AudioError::TooLarge(_))));
        let art = Id3Artwork::from_bytes(vec![0xFF, 0xD8, 0xFF, 0x01]).unwrap();
        let tag = Id3Tag { artwork: Some(art), ..Id3Tag::default() }.to_bytes().unwrap();
        let f = frames(&tag);
        assert_eq!(f[0].0, "APIC");
        assert_eq!(&f[0].1, &[&[0u8][..], b"image/jpeg\0\x03\0", &[0xFF, 0xD8, 0xFF, 0x01]].concat());
    }

    #[test]
    fn the_table_of_contents_holds_at_most_255_chapters() {
        let chapters = (0..300u32).map(|i| Id3Chapter { start_ms: i * 1000, end_ms: i * 1000 + 1000, title: format!("C{i}"), url: None }).collect();
        let f = frames(&Id3Tag { chapters, ..Id3Tag::default() }.to_bytes().unwrap());
        assert_eq!(f.iter().filter(|(i, _)| i == "CHAP").count(), MAX_CHAPTERS);
        let toc = &f.iter().find(|(i, _)| i == "CTOC").unwrap().1;
        assert_eq!(toc[5], 255);
    }
}
