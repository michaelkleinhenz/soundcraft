//! Double-ender sync: lines up speaker tracks recorded on separate machines with a reference
//! recording that holds everyone (the call recording), by matching their loudness envelopes, or
//! by the clap at the start; and measures the clock drift between them over the episode.

use soundcraft_model::{Session, TrackId};
use soundcraft_time::Samples;

/// Envelope values per second: 1 ms resolution.
const ENV_RATE: f64 = 1000.0;
/// How much of a track the first match searches (the rest only refines drift).
const MATCH_SECONDS: f64 = 600.0;
/// Rendered at most this much at a time.
const RENDER_SECONDS: f64 = 30.0;
/// Below this match the result isn't trusted.
pub const MIN_CONFIDENCE: f32 = 0.15;

/// A track's loudness envelope from the timeline start to its last clip, one value per
/// millisecond, and the samples each value covers.
pub fn envelope(s: &Session, t: TrackId) -> (Vec<f32>, Samples) {
    let hop = s.sample_rate.samples(1.0 / ENV_RATE).max(1);
    let end = s.track(t).map_or(0, |tr| tr.clips().iter().filter(|c| c.is_audio() && !c.muted).map(soundcraft_model::Clip::end).max().unwrap_or(0));
    let window = (s.sample_rate.samples(RENDER_SECONDS) / hop).max(1) * hop;
    let mut env = Vec::with_capacity(usize::try_from(end / hop + 1).unwrap_or(0));
    let mut at = 0;
    while at < end {
        let to = (at + window).min(end);
        let audio = soundcraft_mix::render_clips(s, t, soundcraft_time::Range::new(at, to));
        let frames = usize::try_from(to - at).unwrap_or(0);
        let step = usize::try_from(hop).unwrap_or(1);
        let mut i = 0;
        while i < frames {
            let n = step.min(frames - i);
            let mut sum = 0.0f32;
            for ch in &audio {
                sum += ch.iter().skip(i).take(n).map(|v| if v.is_finite() { v.abs() } else { 0.0 }).sum::<f32>();
            }
            // Square-root compression, so loud and quiet microphones weigh alike.
            env.push((sum / (n * audio.len().max(1)) as f32).sqrt());
            i += n;
        }
        at = to;
    }
    (env, hop)
}

/// How a track lines up with the reference: move it by `offset` samples and play it at
/// `1 + drift` of its speed (drift in parts per one: 1e-5 is 36 ms an hour).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alignment {
    /// Samples to move the track by at the timeline start (later when positive).
    pub offset: f64,
    pub drift: f64,
    /// How well the envelopes match (0..1).
    pub confidence: f32,
}

impl Alignment {
    /// Where material now at `x` belongs.
    pub fn shift_at(&self, x: Samples) -> f64 {
        self.offset + self.drift * x as f64
    }
}

/// Lines `other` up with `reference` by their envelopes, searching `±max_offset` samples, and with
/// `drift` measures the clock drift by matching four stretches across the overlap.
pub fn align(reference: &(Vec<f32>, Samples), other: &(Vec<f32>, Samples), max_offset: Samples, drift: bool) -> Option<Alignment> {
    let ((r, hop), (o, _)) = (reference, other);
    let hop = (*hop).max(1);
    let span = |secs: f64| (secs * ENV_RATE) as usize;
    let max_lag = (max_offset / hop).max(1);
    // The other's first ten minutes (from where it has sound) against the reference around them.
    let first = o.iter().position(|v| *v > 1e-4).unwrap_or(0);
    let seg = first..(first + span(MATCH_SECONDS)).min(o.len());
    let r_from = first.saturating_sub(usize::try_from(max_lag).unwrap_or(0));
    let r_to = (seg.end + usize::try_from(max_lag).unwrap_or(0)).min(r.len());
    let base = i64::try_from(first).ok()? - i64::try_from(r_from).ok()?;
    let (lag, confidence) = soundcraft_dsp::offline::best_lag(r.get(r_from..r_to)?, o.get(seg)?, base - max_lag, base + max_lag)?;
    // A lag `k` within the slices: the other's `first` belongs at `r_from + k`.
    let lag = r_from as f64 + lag - first as f64;
    if lag.abs() > max_lag as f64 + 1.0 {
        return None;
    }
    let mut fit = Alignment { offset: lag * hop as f64, drift: 0.0, confidence };
    if !drift {
        return Some(fit);
    }
    // Local lags at four stretches, each searched ±2 s around the first match.
    let last = o.iter().rposition(|v| *v > 1e-4).unwrap_or(0);
    let len = span(60.0).min((last.saturating_sub(first)) / 4);
    if len < span(10.0) {
        return Some(fit);
    }
    let margin = span(2.0);
    let mut points: Vec<(f64, f64)> = Vec::new();
    for i in 0..4 {
        let a = first + (last - first - len) * i / 3;
        let Some(o_seg) = o.get(a..a + len) else { continue };
        let centre = a as f64 + lag;
        let from = (centre - margin as f64).max(0.0) as usize;
        let to = ((centre + (len + margin) as f64) as usize).min(r.len());
        let Some(r_seg) = r.get(from..to) else { continue };
        if let Some((k, c)) = soundcraft_dsp::offline::best_lag(r_seg, o_seg, -(margin as i64) * 2, (margin as i64) * 2)
            && c >= MIN_CONFIDENCE
        {
            points.push(((a + len / 2) as f64, from as f64 + k - a as f64));
        }
    }
    if points.len() < 2 {
        return Some(fit);
    }
    // Least squares: lag = offset + drift * position (in envelope steps).
    let n = points.len() as f64;
    let (mx, my) = (points.iter().map(|p| p.0).sum::<f64>() / n, points.iter().map(|p| p.1).sum::<f64>() / n);
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    if sxx <= 0.0 {
        return Some(fit);
    }
    let slope = sxy / sxx;
    // Under 2 ppm is noise (7 ms an hour); over 0.1% isn't clock drift.
    if slope.abs() >= 2e-6 && slope.abs() <= 1e-3 {
        fit.drift = slope;
        fit.offset = (my - slope * mx) * hop as f64;
    }
    Some(fit)
}

/// The loudest moment in the first `window` samples of `t` (the clap), if any is louder than -30 dBFS.
pub fn clap(s: &Session, t: TrackId, window: Samples) -> Option<Samples> {
    let audio = soundcraft_mix::render_clips(s, t, soundcraft_time::Range::new(0, window.max(1)));
    let mut best = (0usize, 0.0f32);
    for ch in &audio {
        for (i, v) in ch.iter().enumerate() {
            if v.is_finite() && v.abs() > best.1 {
                best = (i, v.abs());
            }
        }
    }
    (best.1 > soundcraft_dsp::db_to_gain(-30.0)).then(|| Samples::try_from(best.0).unwrap_or(0))
}

/// Moves and speed-corrects every clip of `t` by `fit`. Material moved before the timeline start
/// is trimmed off.
pub fn apply(s: &mut Session, t: TrackId, fit: &Alignment) {
    let Some(pl) = s.track_mut(t).and_then(soundcraft_model::Track::playlist_mut) else { return };
    for c in &mut pl.clips {
        let start = c.start;
        let end = c.end();
        let new_start = start + fit.shift_at(start).round() as Samples;
        let new_end = end + fit.shift_at(end).round() as Samples;
        c.start = new_start;
        if fit.drift != 0.0 && c.is_audio() {
            c.stretch = (c.stretch * (1.0 + fit.drift)).clamp(0.5, 2.0);
            c.length = (new_end - new_start).max(1);
        }
        if c.start < 0 && c.end() > 0 {
            c.trim_start_to(0);
        }
    }
    pl.clips.retain(|c| c.end() > 0);
    pl.sort();
}
