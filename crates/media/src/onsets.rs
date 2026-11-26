//! Attack (note onset) detection in mono audio, for stepping and snapping to the notes.
//!
//! The method is the logarithmic filtered spectral flux (Böck, Krebs and Schedl, "Evaluating
//! the online capabilities of onset detection methods", ISMIR 2012): how much each band of a
//! log-magnitude spectrogram with logarithmically spaced bands grows from one frame to the
//! next, summed over the bands. The attacks are the peaks of that sum that stand out from its
//! recent average, picked as in SuperFlux (Böck and Widmer, DAFx 2013).
//!
//! SuperFlux also compares each band with the loudest of its neighbours in the previous frame,
//! to ignore vibrato. A piano has none, and on piano that comparison hid melodies moving by
//! semitones. Measured on the Monk reference video against the notes lit on its Synthesia
//! keyboard (`eval/attacks`): precision 0.98, recall 0.92 and F 0.95 within 50 ms, with a
//! median timing error of 3 ms. SuperFlux's own settings scored F 0.83 (recall 0.71).

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

/// Sample rate the audio must have (mono, `f32` samples from -1 to 1).
pub const SAMPLE_RATE: u32 = 24_000;
/// Analysis frames per second: attacks are placed to the nearest 10 ms.
const FRAMES_PER_SECOND: usize = 100;
const HOP: usize = SAMPLE_RATE as usize / FRAMES_PER_SECOND;
/// About 43 ms. On the reference video 512 samples scored F 0.85 and 2048 scored F 0.91.
const FRAME: usize = 1024;
const BANDS_PER_OCTAVE: f64 = 24.0;
const MIN_FREQUENCY: f64 = 30.0;
const MAX_FREQUENCY: f64 = 11_000.0;

// Peak picking, in frames of 10 ms, with SuperFlux's values. The moving average only looks
// back, so an attack's own rise does not raise the bar it must clear.
const PRE_MAX: usize = 1;
const POST_MAX: usize = 5;
const PRE_AVERAGE: usize = 15;
/// How far a peak of the onset function must rise above its moving average.
const THRESHOLD: f32 = 1.1;
/// Closest two attacks can be. The notes of a chord are never exactly together, and with
/// SuperFlux's 30 ms some chords counted twice.
const COMBINE: usize = 5;

/// An attack: when it happens, and how strong it is compared with the strong attacks of the
/// same recording (1.0 for those).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Onset {
    pub seconds: f64,
    pub strength: f32,
}

/// Finds attacks in audio fed in chunks of any size, so long recordings never sit whole in
/// memory: [`push`](Self::push) the samples, then [`finish`](Self::finish).
pub struct OnsetDetector {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    bands: Vec<Band>,
    /// Samples not analysed yet. It starts with half a frame of silence, so that frame `n`
    /// is centred on sample `n * HOP`.
    pending: Vec<f32>,
    frame: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    current: Vec<f32>,
    previous: Option<Vec<f32>>,
    /// The onset detection function: one value per frame.
    flux: Vec<f32>,
}

/// One band of the logarithmic filterbank: triangular weights over the FFT bins from `first`.
struct Band {
    first: usize,
    weights: Vec<f32>,
}

impl Default for OnsetDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl OnsetDetector {
    pub fn new() -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FRAME);
        let bands = filterbank();
        Self {
            window: (0..FRAME)
                .map(|i| {
                    let phase = std::f64::consts::TAU * i as f64 / FRAME as f64;
                    (0.5 - 0.5 * phase.cos()) as f32
                })
                .collect(),
            frame: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            current: vec![0.0; bands.len()],
            previous: None,
            pending: vec![0.0; FRAME / 2],
            flux: Vec::new(),
            bands,
            fft,
        }
    }

    /// Analyses the next samples of the recording.
    pub fn push(&mut self, samples: &[f32]) {
        self.pending.extend_from_slice(samples);
        let mut start = 0;
        while self.pending.len() - start >= FRAME {
            self.analyse(start);
            start += HOP;
        }
        self.pending.drain(..start);
    }

    /// The attacks of the whole recording, in order. The last half frame (21 ms) is not
    /// analysed: padding it with silence would turn a recording that stops abruptly into a
    /// click.
    pub fn finish(self) -> Vec<Onset> {
        let peaks = pick_peaks(&self.flux);
        let reference = strong_attack(&peaks);
        peaks
            .into_iter()
            // On synthetic piano notes the peaks fall within a frame (10 ms) of the attacks,
            // so the frame's centre is the attack's time.
            .map(|(frame, value)| Onset {
                seconds: frame as f64 / FRAMES_PER_SECOND as f64,
                strength: (value / reference).min(1.0),
            })
            .collect()
    }

    fn analyse(&mut self, start: usize) {
        let samples = &self.pending[start..start + FRAME];
        for ((input, sample), weight) in self.frame.iter_mut().zip(samples).zip(&self.window) {
            *input = sample * weight;
        }
        self.fft
            .process_with_scratch(&mut self.frame, &mut self.spectrum, &mut self.scratch)
            .expect("the buffers come from the FFT plan");
        for (value, band) in self.current.iter_mut().zip(&self.bands) {
            let magnitude: f32 = band
                .weights
                .iter()
                .zip(&self.spectrum[band.first..])
                .map(|(weight, bin)| weight * bin.norm())
                .sum();
            *value = (1.0 + magnitude).log10();
        }
        let flux = match &self.previous {
            None => 0.0,
            Some(previous) => self
                .current
                .iter()
                .zip(previous)
                .map(|(now, before)| (now - before).max(0.0))
                .sum(),
        };
        self.flux.push(flux);
        match &mut self.previous {
            Some(previous) => std::mem::swap(previous, &mut self.current),
            None => self.previous = Some(self.current.clone()),
        }
    }
}

/// Triangular bands with centres 24 per octave from 30 Hz to 11 kHz. Low centres that fall on
/// the same FFT bin are merged, so the lowest bands are single bins. Each band averages its
/// bins, so wide high bands do not outweigh narrow low ones.
fn filterbank() -> Vec<Band> {
    let bin_hz = f64::from(SAMPLE_RATE) / FRAME as f64;
    let bins = FRAME / 2 + 1;
    let step = 2f64.powf(1.0 / BANDS_PER_OCTAVE);
    let mut centres: Vec<usize> = Vec::new();
    let mut frequency = MIN_FREQUENCY;
    while frequency <= MAX_FREQUENCY {
        let bin = (frequency / bin_hz).round() as usize;
        if bin < bins && centres.last() != Some(&bin) {
            centres.push(bin);
        }
        frequency *= step;
    }
    centres
        .windows(3)
        .map(|edges| {
            let (left, centre, right) = (edges[0], edges[1], edges[2]);
            let mut weights: Vec<f32> = (left..=right)
                .map(|bin| {
                    if bin <= centre {
                        (bin - left) as f32 / (centre - left) as f32
                    } else {
                        (right - bin) as f32 / (right - centre) as f32
                    }
                })
                .collect();
            let sum: f32 = weights.iter().sum();
            for weight in &mut weights {
                *weight /= sum;
            }
            Band {
                first: left,
                weights,
            }
        })
        .collect()
}

/// Frames where the onset function has a local maximum that rises far enough above its
/// recent average, at least `COMBINE` frames apart; with the function's value there.
fn pick_peaks(flux: &[f32]) -> Vec<(usize, f32)> {
    let mut peaks: Vec<(usize, f32)> = Vec::new();
    for (frame, &value) in flux.iter().enumerate() {
        let around = |before: usize, after: usize| {
            &flux[frame.saturating_sub(before)..(frame + after + 1).min(flux.len())]
        };
        if around(PRE_MAX, POST_MAX).iter().any(|&other| other > value) {
            continue;
        }
        let recent = around(PRE_AVERAGE, 0);
        let average = recent.iter().sum::<f32>() / recent.len() as f32;
        if value < average + THRESHOLD {
            continue;
        }
        if peaks
            .last()
            .is_some_and(|&(last, _)| frame - last < COMBINE)
        {
            continue;
        }
        peaks.push((frame, value));
    }
    peaks
}

/// What a strong attack of this recording measures: the 90th percentile of the peaks.
fn strong_attack(peaks: &[(usize, f32)]) -> f32 {
    let mut values: Vec<f32> = peaks.iter().map(|&(_, value)| value).collect();
    if values.is_empty() {
        return 1.0;
    }
    values.sort_by(f32::total_cmp);
    values[(values.len() - 1) * 9 / 10].max(f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = SAMPLE_RATE as f64;

    /// A piano-like note: twelve decaying harmonics (the higher ones fading faster, as on a
    /// piano) after a 2 ms attack, plus the short noise burst of the hammer.
    fn add_note(signal: &mut [f32], start: f64, frequency: f64, level: f64, decay: f64) {
        let first = (start * RATE) as usize;
        let mut seed = (frequency * 1000.0) as u32 ^ first as u32;
        for (offset, sample) in signal[first..].iter_mut().enumerate() {
            let t = offset as f64 / RATE;
            let tone: f64 = (1..=12)
                .map(|harmonic| {
                    let h = f64::from(harmonic);
                    let envelope = (-t * h.sqrt() / decay).exp();
                    envelope * (std::f64::consts::TAU * frequency * h * t).sin() / h.powf(0.8)
                })
                .sum();
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (f64::from(seed) / f64::from(u32::MAX) - 0.5) * (-t / 0.004).exp();
            *sample += (level * (t / 0.002).min(1.0) * (0.3 * tone + 0.5 * noise)) as f32;
        }
    }

    fn detect(signal: &[f32]) -> Vec<Onset> {
        let mut detector = OnsetDetector::new();
        // Odd chunk sizes, as a pipe delivers them.
        for chunk in signal.chunks(4_093) {
            detector.push(chunk);
        }
        detector.finish()
    }

    /// Notes at known times: a melody over a sustained bass, soft notes, a chord and fast
    /// repeats. (Steps of a semitone are measured on real piano by `eval/attacks`: two pure
    /// synthetic tones a semitone apart beat far more regularly than piano strings do.)
    fn performance() -> (Vec<f32>, Vec<f64>) {
        let mut signal = vec![0.0f32; (8.0 * RATE) as usize];
        // A tiny noise floor, as any recording has.
        let mut seed = 1u32;
        for sample in &mut signal {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *sample += (f64::from(seed) / f64::from(u32::MAX) - 0.5) as f32 * 2e-4;
        }
        let notes = [
            (0.50, 110.0, 1.0, 2.5), // bass, ringing for seconds
            (0.50, 440.0, 0.8, 0.8), // with a melody note
            (1.10, 494.0, 0.5, 0.8),
            (1.70, 554.0, 0.15, 0.8), // soft, a tone above a ringing note
            (2.30, 587.0, 0.6, 0.8),
            (2.30, 740.0, 0.6, 0.8), // a chord: one attack
            (2.30, 880.0, 0.6, 0.8),
            (3.00, 659.0, 0.7, 0.4),
            (3.15, 659.0, 0.7, 0.4), // fast repeats, 150 ms apart
            (3.30, 659.0, 0.7, 0.4),
            (4.00, 196.0, 0.9, 2.0),
            (4.60, 1047.0, 0.3, 0.5),
            (5.30, 831.0, 0.08, 0.6), // very soft, while two notes ring
            (6.10, 330.0, 0.8, 1.0),
            (6.70, 262.0, 0.6, 1.0),
        ];
        let mut times = Vec::new();
        for &(start, frequency, level, decay) in &notes {
            add_note(&mut signal, start, frequency, level, decay);
            if times.last() != Some(&start) {
                times.push(start);
            }
        }
        (signal, times)
    }

    #[test]
    fn finds_every_attack_of_a_performance_on_time() {
        let (signal, expected) = performance();
        let found = detect(&signal);
        let times: Vec<f64> = found.iter().map(|onset| onset.seconds).collect();
        assert_eq!(times.len(), expected.len(), "found {times:?}");
        for (time, wanted) in times.iter().zip(&expected) {
            assert!(
                (time - wanted).abs() <= 0.02,
                "{time} for {wanted}: {times:?}"
            );
        }
    }

    #[test]
    fn strengths_follow_the_loudness() {
        let (signal, expected) = performance();
        let found = detect(&signal);
        assert_eq!(found.len(), expected.len());
        let strength_at = |time: f64| {
            found
                .iter()
                .find(|onset| (onset.seconds - time).abs() <= 0.02)
                .unwrap()
                .strength
        };
        assert!(
            strength_at(0.5) > strength_at(1.7),
            "loud bass and melody over a soft note"
        );
        assert!(
            found
                .iter()
                .all(|onset| onset.strength > 0.0 && onset.strength <= 1.0)
        );
    }

    #[test]
    fn silence_and_steady_tones_have_no_attacks_after_they_start() {
        assert!(detect(&vec![0.0; 3 * SAMPLE_RATE as usize]).is_empty());

        let mut tone = vec![0.0f32; 3 * SAMPLE_RATE as usize];
        add_note(&mut tone, 0.5, 440.0, 0.8, 1e9); // never decays
        let found = detect(&tone);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].seconds - 0.5).abs() <= 0.02);
    }

    #[test]
    fn filterbank_bands_average_their_bins() {
        let bands = filterbank();
        assert!(bands.len() > 100, "{} bands", bands.len());
        for band in &bands {
            let sum: f32 = band.weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5);
            assert!(band.first + band.weights.len() <= FRAME / 2 + 1);
        }
    }
}
