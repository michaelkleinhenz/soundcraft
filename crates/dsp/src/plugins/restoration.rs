//! Voice restoration for podcasts (clean-room): Hum Removal (mains notches), Noise Reduction
//! (adaptive spectral gate), De-Click (outlier repair) and De-Plosive (dynamic low-band cut).

use crate::biquad::{Biquad, FilterType};
use crate::params::{Params, choice, lin, param_plumbing};
use crate::util::{frames_in, sane_channels, sane_sr};
use crate::{Category, ParamInfo, Plugin, PluginInfo, Unit, db_to_gain};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

// ---- Hum Removal ----------------------------------------------------------------------------

static HUM_PARAMS: [ParamInfo; 3] = [
    choice("mains", "Mains", &["50 Hz", "60 Hz"], 0),
    lin("harmonics", "Harmonics", 1.0, 8.0, 4.0, Unit::None),
    lin("q", "Width (Q)", 5.0, 60.0, 30.0, Unit::None),
];

pub static HUM_INFO: PluginInfo = PluginInfo {
    id: "hum_removal",
    name: "Hum Removal",
    short_name: "Hum",
    category: Category::Other,
    params: &HUM_PARAMS,
    is_instrument: false,
    audiosuite: true,
};

/// The most notches per channel.
const MAX_HARMONICS: usize = 8;

/// Narrow notches at the mains frequency and its harmonics.
pub struct HumRemoval {
    p: Params,
    sr: f32,
    /// Per channel, one notch per harmonic.
    notches: Vec<[Biquad; MAX_HARMONICS]>,
    active: usize,
}

impl Default for HumRemoval {
    fn default() -> Self {
        Self::new()
    }
}

impl HumRemoval {
    pub fn new() -> Self {
        Self { p: Params::new(&HUM_PARAMS), sr: 48_000.0, notches: Vec::new(), active: 0 }
    }

    fn update(&mut self) {
        let f0 = if self.p.choice(0) == 1 { 60.0 } else { 50.0 };
        let want = (self.p.v(1).round() as usize).clamp(1, MAX_HARMONICS);
        // Harmonics at or past Nyquist are left out.
        self.active = (1..=want).take_while(|k| (*k as f32) * f0 < self.sr * 0.45).count();
        let q = self.p.v(2);
        for ch in &mut self.notches {
            for (k, n) in ch.iter_mut().enumerate().take(self.active) {
                n.set(FilterType::Notch, f0 * (k + 1) as f32, q, 0.0, self.sr);
            }
        }
    }
}

impl Plugin for HumRemoval {
    param_plumbing!(HUM_INFO);

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sane_sr(sample_rate);
        self.notches = vec![[Biquad::default(); MAX_HARMONICS]; sane_channels(channels)];
        self.update();
    }

    fn reset(&mut self) {
        self.notches.iter_mut().flat_map(|c| c.iter_mut()).for_each(Biquad::reset);
    }

    fn process(&mut self, io: &mut [Vec<f32>], frames: usize) {
        let ch = self.notches.len().min(io.len());
        let frames = frames_in(io, frames, ch);
        let active = self.active;
        for (b, n) in io.iter_mut().zip(self.notches.iter_mut()) {
            for x in b.iter_mut().take(frames) {
                let mut y = if x.is_finite() { *x } else { 0.0 };
                for f in n.iter_mut().take(active) {
                    y = f.process_sample(y);
                }
                *x = y;
            }
        }
    }
}

// ---- Noise Reduction ------------------------------------------------------------------------

static NR_PARAMS: [ParamInfo; 3] = [
    lin("reduction", "Reduction", 0.0, 40.0, 12.0, Unit::Db),
    lin("sensitivity", "Sensitivity", 0.0, 12.0, 6.0, Unit::Db),
    lin("learn", "Learn Speed", 1.0, 10.0, 4.0, Unit::Seconds),
];

pub static NR_INFO: PluginInfo = PluginInfo {
    id: "noise_reduction",
    name: "Noise Reduction",
    short_name: "Denoise",
    category: Category::Other,
    params: &NR_PARAMS,
    is_instrument: false,
    audiosuite: true,
};

const NR_FFT: usize = 1024;
const NR_HOP: usize = NR_FFT / 4;
const NR_BINS: usize = NR_FFT / 2 + 1;

/// One channel's STFT state.
#[derive(Clone)]
struct NrChannel {
    /// The last `NR_FFT` input samples.
    input: Vec<f32>,
    /// Overlap-add accumulator; the first `NR_HOP` samples are ready after each frame.
    output: Vec<f32>,
    /// Samples written since the last frame.
    fill: usize,
    /// Ready output samples being played out.
    ready: Vec<f32>,
    pos: usize,
    /// Per bin: the noise floor estimate (power), the smoothed power and the last gain.
    noise: Vec<f32>,
    smooth: Vec<f32>,
    gain: Vec<f32>,
    frames_seen: u32,
}

impl NrChannel {
    fn new() -> Self {
        NrChannel {
            input: vec![0.0; NR_FFT],
            output: vec![0.0; NR_FFT],
            fill: 0,
            ready: vec![0.0; NR_HOP],
            pos: 0,
            noise: vec![0.0; NR_BINS],
            smooth: vec![0.0; NR_BINS],
            gain: vec![1.0; NR_BINS],
            frames_seen: 0,
        }
    }
}

/// An adaptive spectral gate: each bin's noise floor is tracked continuously (it follows the
/// quietest level the bin has had over the last few seconds), and bins near that floor are turned
/// down by up to `reduction`. It learns from the pauses between words, so no separate noise
/// print is needed. Latency: one FFT frame.
pub struct NoiseReduction {
    p: Params,
    sr: f32,
    fft: Option<(Arc<dyn Fft<f32>>, Arc<dyn Fft<f32>>)>,
    window: Vec<f32>,
    buf: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    chans: Vec<NrChannel>,
}

impl Default for NoiseReduction {
    fn default() -> Self {
        Self::new()
    }
}

impl NoiseReduction {
    pub fn new() -> Self {
        let window = (0..NR_FFT).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / NR_FFT as f32).cos()).collect();
        Self { p: Params::new(&NR_PARAMS), sr: 48_000.0, fft: None, window, buf: Vec::new(), scratch: Vec::new(), chans: Vec::new() }
    }

    fn update(&mut self) {}

    /// Runs one STFT frame on channel `c`.
    fn frame(&mut self, c: usize) {
        let Some((fwd, inv)) = &self.fft else { return };
        let Some(ch) = self.chans.get_mut(c) else { return };
        for ((b, x), w) in self.buf.iter_mut().zip(&ch.input).zip(&self.window) {
            *b = Complex::new(x * w, 0.0);
        }
        fwd.process_with_scratch(&mut self.buf, &mut self.scratch);
        let floor = db_to_gain(-self.p.v(0));
        let over = db_to_gain(self.p.v(1)).powi(2);
        let frames_per_sec = self.sr / NR_HOP as f32;
        // The noise estimate may rise by this factor per frame: it reaches 10x over `learn` seconds.
        let rise = 10f32.powf(1.0 / (self.p.v(2).max(0.5) * frames_per_sec));
        ch.frames_seen = ch.frames_seen.saturating_add(1);
        let warm = ch.frames_seen < 8;
        for k in 0..NR_BINS {
            let Some(v) = self.buf.get(k) else { break };
            let power = v.norm_sqr();
            let (Some(sm), Some(n), Some(g)) = (ch.smooth.get_mut(k), ch.noise.get_mut(k), ch.gain.get_mut(k)) else { break };
            *sm = if warm { power } else { 0.7 * *sm + 0.3 * power };
            *n = if warm {
                (*n).max(*sm)
            } else if *sm < *n {
                *sm
            } else {
                *n * rise
            };
            // Wiener-like gain: full where the bin is well over the floor, `floor` where it is noise.
            let target = if *sm > 0.0 { (1.0 - over * *n / *sm).clamp(0.0, 1.0).max(floor) } else { floor };
            // Fast to open, slower to close, so word onsets survive and the gate doesn't flutter.
            *g = if target > *g { target } else { 0.6 * *g + 0.4 * target };
        }
        for k in 0..NR_BINS {
            let g = ch.gain.get(k).copied().unwrap_or(1.0);
            if let Some(v) = self.buf.get_mut(k) {
                *v *= g;
            }
            // Mirror for a real inverse.
            if k > 0
                && k < NR_FFT / 2
                && let (Some(src), Some(dst)) = (self.buf.get(k).copied(), NR_FFT.checked_sub(k))
                && let Some(m) = self.buf.get_mut(dst)
            {
                *m = src.conj();
            }
        }
        inv.process_with_scratch(&mut self.buf, &mut self.scratch);
        // Hann analysis + Hann synthesis at 75% overlap sums to 1.5; scale back.
        let scale = 1.0 / (NR_FFT as f32 * 1.5);
        for ((o, b), w) in ch.output.iter_mut().zip(&self.buf).zip(&self.window) {
            *o += b.re * w * scale;
        }
        ch.ready.copy_from_slice(ch.output.get(..NR_HOP).unwrap_or(&[0.0; NR_HOP]));
        ch.output.copy_within(NR_HOP.., 0);
        let len = ch.output.len();
        ch.output.get_mut(len - NR_HOP..).into_iter().flatten().for_each(|x| *x = 0.0);
        ch.pos = 0;
    }
}

impl Plugin for NoiseReduction {
    param_plumbing!(NR_INFO);

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sane_sr(sample_rate);
        let mut planner = FftPlanner::new();
        let (fwd, inv) = (planner.plan_fft_forward(NR_FFT), planner.plan_fft_inverse(NR_FFT));
        let scratch = fwd.get_inplace_scratch_len().max(inv.get_inplace_scratch_len());
        self.scratch = vec![Complex::new(0.0, 0.0); scratch];
        self.buf = vec![Complex::new(0.0, 0.0); NR_FFT];
        self.fft = Some((fwd, inv));
        self.chans = vec![NrChannel::new(); sane_channels(channels)];
    }

    fn reset(&mut self) {
        self.chans.iter_mut().for_each(|c| *c = NrChannel::new());
    }

    fn latency(&self) -> usize {
        NR_FFT
    }

    fn process(&mut self, io: &mut [Vec<f32>], frames: usize) {
        let ch = self.chans.len().min(io.len());
        let frames = frames_in(io, frames, ch);
        for c in 0..ch {
            for i in 0..frames {
                let x = io.get(c).and_then(|b| b.get(i)).copied().filter(|x| x.is_finite()).unwrap_or(0.0);
                let Some(st) = self.chans.get_mut(c) else { break };
                let y = st.ready.get(st.pos).copied().unwrap_or(0.0);
                st.pos += 1;
                st.input.copy_within(1.., 0);
                if let Some(last) = st.input.last_mut() {
                    *last = x;
                }
                st.fill += 1;
                if st.fill == NR_HOP {
                    st.fill = 0;
                    self.frame(c);
                }
                if let Some(o) = io.get_mut(c).and_then(|b| b.get_mut(i)) {
                    *o = y;
                }
            }
        }
    }
}

// ---- De-Click -------------------------------------------------------------------------------

static CLICK_PARAMS: [ParamInfo; 1] = [lin("sensitivity", "Sensitivity", 1.0, 10.0, 5.0, Unit::None)];

pub static CLICK_INFO: PluginInfo = PluginInfo {
    id: "de_click",
    name: "De-Click",
    short_name: "DeClick",
    category: Category::Other,
    params: &CLICK_PARAMS,
    is_instrument: false,
    audiosuite: true,
};

/// Samples of lookahead: a click is found before it is played and bridged over.
const CLICK_DELAY: usize = 32;
/// Samples repaired on each side of a detected click.
const CLICK_HALF: usize = 12;
const CLICK_RING: usize = 128;

#[derive(Clone)]
struct ClickChannel {
    ring: Vec<f32>,
    /// Samples taken in so far (the newest is `n - 1`).
    n: u64,
    hp: Biquad,
    /// Running mean square of the high-passed signal.
    ms: f32,
    /// The samples being bridged, first and last (absolute sample numbers).
    repair: Option<(u64, u64)>,
}

impl ClickChannel {
    fn at(&self, abs: u64) -> f32 {
        self.ring.get((abs % CLICK_RING as u64) as usize).copied().unwrap_or(0.0)
    }
}

/// Finds mouth clicks and pops as sudden outliers in the high frequencies (far above their
/// running level) and bridges each one with a straight line between the good samples around it.
pub struct DeClick {
    p: Params,
    sr: f32,
    chans: Vec<ClickChannel>,
}

impl Default for DeClick {
    fn default() -> Self {
        Self::new()
    }
}

impl DeClick {
    pub fn new() -> Self {
        Self { p: Params::new(&CLICK_PARAMS), sr: 48_000.0, chans: Vec::new() }
    }

    fn update(&mut self) {}
}

impl Plugin for DeClick {
    param_plumbing!(CLICK_INFO);

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sane_sr(sample_rate);
        let hp = Biquad::design(FilterType::HighPass, 4000.0f32.min(self.sr * 0.4), 0.707, 0.0, self.sr);
        self.chans = vec![ClickChannel { ring: vec![0.0; CLICK_RING], n: 0, hp, ms: 0.0, repair: None }; sane_channels(channels)];
    }

    fn reset(&mut self) {
        for c in &mut self.chans {
            c.ring.iter_mut().for_each(|x| *x = 0.0);
            c.n = 0;
            c.hp.reset();
            c.ms = 0.0;
            c.repair = None;
        }
    }

    fn latency(&self) -> usize {
        CLICK_DELAY
    }

    fn process(&mut self, io: &mut [Vec<f32>], frames: usize) {
        let ch = self.chans.len().min(io.len());
        let frames = frames_in(io, frames, ch);
        // Sensitivity 1..10 maps to a threshold of 12..3 times the running RMS.
        let k = 13.0 - self.p.v(0);
        let k2 = k * k;
        let (half, delay) = (CLICK_HALF as u64, CLICK_DELAY as u64);
        for (b, c) in io.iter_mut().zip(self.chans.iter_mut()) {
            for x in b.iter_mut().take(frames) {
                let input = if x.is_finite() { *x } else { 0.0 };
                let newest = c.n;
                if let Some(slot) = c.ring.get_mut((newest % CLICK_RING as u64) as usize) {
                    *slot = input;
                }
                c.n += 1;
                let e = c.hp.process_sample(input);
                let e2 = e * e;
                // Detection waits until the running level has settled (the first 1024 samples).
                if newest >= 1024 && e2 > k2 * c.ms {
                    let (a, z) = (newest.saturating_sub(half), newest + half);
                    c.repair = Some(match c.repair {
                        Some((s, end)) if a <= end + 1 => (s, end.max(z)),
                        _ => (a, z),
                    });
                }
                // A click barely moves the running level; a lasting rise is followed within ~100 samples.
                c.ms = (0.999 * c.ms + 0.001 * e2.min(k2 * c.ms.max(1e-10))).max(1e-10);
                let Some(out) = newest.checked_sub(delay) else {
                    *x = 0.0;
                    continue;
                };
                let mut y = c.at(out);
                if let Some((a, z)) = c.repair {
                    if out >= a && out <= z {
                        // A line from the good sample before the click to the first good one after it.
                        let before = if a > 0 { c.at(a - 1) } else { 0.0 };
                        let last = (z + 1).min(newest);
                        let after = c.at(last);
                        let t = (out + 1 - a) as f32 / (last + 1 - a).max(1) as f32;
                        y = before + (after - before) * t;
                    }
                    if out >= z {
                        c.repair = None;
                    }
                }
                *x = y;
            }
        }
    }
}

// ---- De-Plosive -----------------------------------------------------------------------------

static PLOSIVE_PARAMS: [ParamInfo; 3] = [
    lin("freq", "Frequency", 80.0, 300.0, 150.0, Unit::Hz),
    lin("threshold", "Threshold", -40.0, 0.0, -20.0, Unit::Db),
    lin("reduction", "Max Reduction", 0.0, 30.0, 18.0, Unit::Db),
];

pub static PLOSIVE_INFO: PluginInfo = PluginInfo {
    id: "de_plosive",
    name: "De-Plosive",
    short_name: "DePlos",
    category: Category::Other,
    params: &PLOSIVE_PARAMS,
    is_instrument: false,
    audiosuite: true,
};

#[derive(Clone, Copy, Default)]
struct PlosiveChannel {
    lp: Biquad,
    lp2: Biquad,
    hp: Biquad,
    hp2: Biquad,
    env: f32,
    gain: f32,
}

/// Turns down the low band (below `freq`) only while it bursts over `threshold`: the thump of a
/// "p" or "b" into the microphone. The bands are a Linkwitz-Riley split, so inactive they sum
/// back to the input's level (an all-pass).
pub struct DePlosive {
    p: Params,
    sr: f32,
    chans: Vec<PlosiveChannel>,
}

impl Default for DePlosive {
    fn default() -> Self {
        Self::new()
    }
}

impl DePlosive {
    pub fn new() -> Self {
        Self { p: Params::new(&PLOSIVE_PARAMS), sr: 48_000.0, chans: Vec::new() }
    }

    fn update(&mut self) {
        let f = self.p.v(0);
        for c in &mut self.chans {
            for (b, kind) in [
                (&mut c.lp, FilterType::LowPass),
                (&mut c.lp2, FilterType::LowPass),
                (&mut c.hp, FilterType::HighPass),
                (&mut c.hp2, FilterType::HighPass),
            ] {
                b.set(kind, f, std::f32::consts::FRAC_1_SQRT_2, 0.0, self.sr);
            }
        }
    }
}

impl Plugin for DePlosive {
    param_plumbing!(PLOSIVE_INFO);

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sane_sr(sample_rate);
        self.chans = vec![PlosiveChannel { gain: 1.0, ..Default::default() }; sane_channels(channels)];
        self.update();
    }

    fn reset(&mut self) {
        for c in &mut self.chans {
            c.lp.reset();
            c.lp2.reset();
            c.hp.reset();
            c.hp2.reset();
            c.env = 0.0;
            c.gain = 1.0;
        }
    }

    fn gain_reduction_db(&self) -> f32 {
        self.chans.iter().map(|c| crate::gain_to_db(c.gain)).fold(0.0, f32::min)
    }

    fn process(&mut self, io: &mut [Vec<f32>], frames: usize) {
        let ch = self.chans.len().min(io.len());
        let frames = frames_in(io, frames, ch);
        let thr = db_to_gain(self.p.v(1));
        let min_gain = db_to_gain(-self.p.v(2));
        let attack = (-1.0 / (0.001 * self.sr)).exp();
        let release = (-1.0 / (0.080 * self.sr)).exp();
        for (b, c) in io.iter_mut().zip(self.chans.iter_mut()) {
            for x in b.iter_mut().take(frames) {
                let input = if x.is_finite() { *x } else { 0.0 };
                // Fourth-order Linkwitz-Riley bands.
                let low = c.lp2.process_sample(c.lp.process_sample(input));
                let high = c.hp2.process_sample(c.hp.process_sample(input));
                let level = low.abs();
                let coef = if level > c.env { attack } else { release };
                c.env = coef * c.env + (1.0 - coef) * level;
                let target = if c.env > thr { (thr / c.env).max(min_gain) } else { 1.0 };
                let gcoef = if target < c.gain { attack } else { release };
                c.gain = gcoef * c.gain + (1.0 - gcoef) * target;
                *x = high + low * c.gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(f: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amp * (std::f32::consts::TAU * f * i as f32 / SR).sin()).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    fn run(p: &mut dyn Plugin, x: &[f32]) -> Vec<f32> {
        p.prepare(SR, 512, 1);
        let mut out = Vec::new();
        for chunk in x.chunks(512) {
            let mut io = vec![chunk.to_vec()];
            p.process(&mut io, chunk.len());
            out.extend_from_slice(&io[0]);
        }
        out
    }

    #[test]
    fn hum_removal_notches_mains_and_its_harmonics_but_not_speech() {
        let mut h = HumRemoval::new();
        let n = 96_000;
        for f in [50.0, 100.0, 150.0] {
            let out = run(&mut h, &sine(f, 0.5, n));
            let db = crate::gain_to_db(rms(&out[n * 3 / 4..]) / rms(&sine(f, 0.5, n)[n * 3 / 4..]));
            assert!(db < -30.0, "{f} Hz {db}");
        }
        let out = run(&mut h, &sine(1000.0, 0.5, n));
        assert!(crate::gain_to_db(rms(&out[n / 2..]) / 0.3535).abs() < 0.5);
        h.set_param("mains", 1.0);
        let out = run(&mut h, &sine(60.0, 0.5, n));
        assert!(crate::gain_to_db(rms(&out[n * 3 / 4..]) / 0.3535) < -30.0, "60 Hz mains");
    }

    #[test]
    fn noise_reduction_turns_steady_noise_down_and_keeps_a_tone() {
        let mut rng = crate::util::Rng::new(7);
        let n = 48_000 * 4;
        let noise: Vec<f32> = (0..n).map(|_| 0.02 * rng.bipolar()).collect();
        let mut nr = NoiseReduction::new();
        nr.set_param("reduction", 20.0);
        let out = run(&mut nr, &noise);
        let late = n * 3 / 4;
        let drop = crate::gain_to_db(rms(&out[late..]) / rms(&noise[late..]));
        assert!(drop < -10.0, "noise down {drop} dB");
        // Speech-like bursts well over the noise pass (after the latency) at about their level.
        let burst = |i: usize| if (i / 24_000) % 2 == 1 { 1.0 } else { 0.0 };
        let tone: Vec<f32> = sine(1000.0, 0.3, n).iter().zip(&noise).enumerate().map(|(i, (a, b))| a * burst(i) + b).collect();
        let out = run(&mut nr, &tone);
        // The last burst runs from 3.5 s to 4 s; skip its onset and the latency.
        let (a, z) = (168_000 + 4_800 + NR_FFT, n);
        let kept = crate::gain_to_db(rms(&out[a..z]) / rms(&sine(1000.0, 0.3, n)[a..z]));
        assert!(kept.abs() < 1.5, "tone kept {kept} dB");
        assert_eq!(nr.latency(), NR_FFT);
    }

    #[test]
    fn de_click_bridges_a_click_and_passes_clean_audio() {
        let n = 48_000;
        let clean = sine(300.0, 0.3, n);
        let mut clicked = clean.clone();
        for at in [10_000, 30_000] {
            clicked[at] += 0.8;
            clicked[at + 1] -= 0.6;
        }
        let mut d = DeClick::new();
        let out = run(&mut d, &clicked);
        let lat = CLICK_DELAY;
        for at in [10_000, 30_000] {
            let err = (at - 4..at + 4).map(|i| (out[i + lat] - clean[i]).abs()).fold(0.0f32, f32::max);
            assert!(err < 0.15, "click at {at} left {err}");
        }
        let out = run(&mut d, &clean);
        let err = (1000..n - 100).map(|i| (out[i + lat] - clean[i]).abs()).fold(0.0f32, f32::max);
        assert!(err < 0.02, "clean audio changed by {err}");
    }

    #[test]
    fn de_plosive_cuts_a_low_thump_and_leaves_the_voice() {
        let n = 48_000;
        let voice = sine(1000.0, 0.2, n);
        let mut thump = voice.clone();
        for (i, v) in thump.iter_mut().enumerate().skip(20_000).take(4800) {
            *v += 0.8 * (std::f32::consts::TAU * 60.0 * i as f32 / SR).sin();
        }
        let mut d = DePlosive::new();
        let out = run(&mut d, &thump);
        let before = rms(&thump[21_000..24_000]);
        let after = rms(&out[21_000..24_000]);
        assert!(crate::gain_to_db(after / before) < -6.0, "thump {} dB", crate::gain_to_db(after / before));
        let out = run(&mut d, &voice);
        assert!(crate::gain_to_db(rms(&out[10_000..]) / rms(&voice[10_000..])).abs() < 0.3);
    }
}
