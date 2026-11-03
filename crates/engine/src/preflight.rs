//! Speech analysis for the podcast tools: where people talk (from Strip Silence's detector), and
//! the mix measurements the episode pre-flight check reports.

use crate::{EngineError, Result};
use soundcraft_model::{ChannelFormat, Session, TrackId, TrackKind};
use soundcraft_time::{Range, Samples};
use std::sync::atomic::AtomicBool;

/// The speaker tracks when none are named: active mono audio tracks with clips (the New Episode
/// template's speakers are mono; music and effects beds are stereo).
pub fn speaker_tracks(s: &Session) -> Vec<TrackId> {
    s.tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio && t.format == ChannelFormat::Mono && !t.inactive && !t.clips().is_empty())
        .map(|t| t.id)
        .collect()
}

/// Where anyone on `tracks` speaks inside `r`: the union of their non-silent ranges (louder than
/// `threshold_db`, ignoring dips shorter than `min_gap`), sorted and merged.
pub fn speech(s: &Session, tracks: &[TrackId], r: Range, threshold_db: f32, min_gap: Samples) -> Result<Vec<Range>> {
    let stop = AtomicBool::new(false);
    let mut all = Vec::new();
    for t in tracks {
        all.extend(crate::io::strip_silence_ranges(s, *t, r, threshold_db, min_gap, (0, 0), &stop)?);
    }
    Ok(merge(all))
}

/// `ranges` sorted, with overlapping or touching ones joined.
pub fn merge(mut ranges: Vec<Range>) -> Vec<Range> {
    ranges.retain(|x| !x.is_empty());
    ranges.sort_by_key(|x| x.start);
    let mut out: Vec<Range> = Vec::new();
    for x in ranges {
        match out.last_mut() {
            Some(last) if x.start <= last.end => last.end = last.end.max(x.end),
            _ => out.push(x),
        }
    }
    out
}

/// The gaps between `speech` ranges inside `r`, longer than `min` (leading and trailing silence
/// excluded).
pub fn pauses(speech: &[Range], min: Samples) -> Vec<Range> {
    speech
        .windows(2)
        .filter_map(|w| match w {
            [a, b] if b.start - a.end > min => Some(Range::new(a.end, b.start)),
            _ => None,
        })
        .collect()
}

/// What the pre-flight check measures on a rendered mix.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MixStats {
    pub lufs: f32,
    pub true_peak_db: f32,
    pub peak_db: f32,
    /// Samples at or over full scale (0 dBFS), counted per channel.
    pub clipped: u64,
    /// Silence at the start and the end of the range, in samples.
    pub lead_silence: Samples,
    pub tail_silence: Samples,
    /// Stretches of near-silence inside the mix (dead air), longest first.
    pub dead_air: Vec<Range>,
}

/// Mix level below which a 100 ms window counts as silence.
pub const SILENCE_DB: f32 = -50.0;

/// Renders the main mix of `s` over `r` (folded to stereo) and measures it.
pub fn analyze_mix(s: &Session, r: Range, dead_air_min: Samples) -> Result<MixStats> {
    if r.is_empty() {
        return Err(EngineError::BadParams("session.preflight".into(), "nothing to check: the session is empty".into()));
    }
    let (_, channels, fold) = crate::io::bounce_layout(s, None, true);
    let mut meter = soundcraft_dsp::meter::LoudnessMeter::new(s.sample_rate.as_f64() as f32, channels);
    let mut render = soundcraft_mix::RangeRender::new(s, None, r, 1024);
    let window = usize::try_from(s.sample_rate.samples(0.1)).unwrap_or(4800).max(1);
    let (mut peak, mut clipped) = (0.0f32, 0u64);
    let (mut acc, mut acc_n, mut at) = (0.0f64, 0usize, r.start);
    let mut windows: Vec<(Samples, bool)> = Vec::new();
    let mut block = Vec::new();
    while let Some((chunk, n)) = render.next_block(s) {
        crate::io::fill(&mut block, chunk, n, fold, 1.0);
        meter.process(&block, n);
        for i in 0..n {
            let mut sq = 0.0f64;
            for ch in &block {
                let v = ch.get(i).copied().unwrap_or(0.0);
                let a = v.abs();
                peak = peak.max(a);
                if a >= 1.0 {
                    clipped += 1;
                }
                sq += f64::from(v) * f64::from(v);
            }
            acc += sq / block.len().max(1) as f64;
            acc_n += 1;
            if acc_n == window {
                let db = soundcraft_dsp::gain_to_db((acc / acc_n as f64).sqrt() as f32);
                windows.push((at, db < SILENCE_DB));
                at += acc_n as Samples;
                acc = 0.0;
                acc_n = 0;
            }
        }
    }
    if acc_n > 0 {
        let db = soundcraft_dsp::gain_to_db((acc / acc_n as f64).sqrt() as f32);
        windows.push((at, db < SILENCE_DB));
    }
    let win = window as Samples;
    let lead = windows.iter().take_while(|w| w.1).count() as Samples * win;
    let tail = (windows.iter().rev().take_while(|w| w.1).count() as Samples * win).min(r.len());
    let mut dead_air = Vec::new();
    let mut run: Option<Samples> = None;
    let body_end = r.end - tail;
    for (start, silent) in &windows {
        match (silent, run) {
            (true, None) => run = Some(*start),
            (false, Some(a)) => {
                if a > r.start + lead && *start - a >= dead_air_min {
                    dead_air.push(Range::new(a, *start));
                }
                run = None;
            }
            _ => {}
        }
        if *start >= body_end {
            break;
        }
    }
    dead_air.sort_by_key(|x| std::cmp::Reverse(x.len()));
    Ok(MixStats {
        lufs: meter.integrated_lufs(),
        true_peak_db: meter.true_peak_db(),
        peak_db: soundcraft_dsp::gain_to_db(peak),
        clipped,
        lead_silence: lead.min(r.len()),
        tail_silence: tail,
        dead_air,
    })
}

/// The integrated loudness of track `t` on its own (soloed, through its effects), over `r`.
pub fn track_loudness(s: &Session, t: TrackId, r: Range) -> f32 {
    let mut solo = s.clone();
    for tr in &mut solo.tracks {
        tr.mixer.solo = tr.id == t;
    }
    let (_, channels, fold) = crate::io::bounce_layout(&solo, None, true);
    let mut meter = soundcraft_dsp::meter::LoudnessMeter::new(s.sample_rate.as_f64() as f32, channels);
    let mut render = soundcraft_mix::RangeRender::new(&solo, None, r, 1024);
    let mut block = Vec::new();
    while let Some((chunk, n)) = render.next_block(&solo) {
        crate::io::fill(&mut block, chunk, n, fold, 1.0);
        meter.process(&block, n);
    }
    meter.integrated_lufs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_merge_and_pauses_skip_the_short_ones() {
        let m = merge(vec![Range::new(10, 20), Range::new(0, 5), Range::new(18, 30), Range::new(100, 120), Range::new(40, 40)]);
        assert_eq!(m, vec![Range::new(0, 5), Range::new(10, 30), Range::new(100, 120)]);
        assert_eq!(pauses(&m, 10), vec![Range::new(30, 100)]);
    }
}
