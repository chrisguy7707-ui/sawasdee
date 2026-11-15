//! Pitch (fundamental frequency) curves, for comparing the learner's tone with the native recording.
//!
//! The curve is a picture of how the voice goes up and down, in semitones around the speaker's own
//! middle pitch, so a deep and a high voice can be compared. It is shown to the learner as two lines;
//! the app does not decide "right" or "wrong" from it.

/// Samples as mono f32 in -1..1 and their sample rate.
pub struct Pcm {
    pub samples: Vec<f32>,
    pub rate: u32,
}

/// Reads a 16-bit PCM mono WAV file (what the recorder writes).
pub fn parse_wav(bytes: &[u8]) -> Option<Pcm> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let (mut pos, mut rate, mut channels, mut bits) = (12usize, 0u32, 1u16, 16u16);
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().ok()?);
            rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().ok()?);
            bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().ok()?);
        } else if id == b"data" {
            if bits != 16 || rate == 0 || channels == 0 {
                return None;
            }
            // a recorder that was stopped abruptly may leave a wrong length: trust the file size
            let end = (body + len).min(bytes.len());
            let raw = &bytes[body..end];
            let ch = channels as usize;
            let samples: Vec<f32> = raw
                .chunks_exact(2 * ch)
                .map(|f| i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0)
                .collect();
            return Some(Pcm { samples, rate });
        }
        pos = body + len + (len & 1);
    }
    None
}

const HOP_MS: f32 = 10.0;
const FRAME_MS: f32 = 40.0;
const F_MIN: f32 = 70.0;
const F_MAX: f32 = 420.0;

/// Pitch in Hz every 10 ms, `None` where the sound is not voiced (silence, noise, consonants).
pub fn f0_track(pcm: &Pcm) -> Vec<Option<f32>> {
    let rate = pcm.rate as f32;
    let frame = (rate * FRAME_MS / 1000.0) as usize;
    let hop = ((rate * HOP_MS / 1000.0) as usize).max(1);
    let tau_min = (rate / F_MAX) as usize;
    let tau_max = ((rate / F_MIN) as usize).min(frame / 2);
    let s = &pcm.samples;
    if s.len() < frame + tau_max || tau_min < 2 || tau_max <= tau_min + 2 {
        return Vec::new();
    }
    // loudness gate: a fraction of the loudest part
    let rms = |a: usize| -> f32 {
        let w = &s[a..a + frame];
        (w.iter().map(|x| x * x).sum::<f32>() / frame as f32).sqrt()
    };
    let mut starts = Vec::new();
    let mut a = 0;
    while a + frame + tau_max <= s.len() {
        starts.push(a);
        a += hop;
    }
    let levels: Vec<f32> = starts.iter().map(|&a| rms(a)).collect();
    let peak = levels.iter().cloned().fold(0.0f32, f32::max);
    let gate = (peak * 0.08).max(0.004);
    let n = frame / 2; // window of the difference function
    let mut out = Vec::with_capacity(starts.len());
    let mut d = vec![0.0f32; tau_max + 1];
    for (k, &a) in starts.iter().enumerate() {
        if levels[k] < gate {
            out.push(None);
            continue;
        }
        // YIN: difference function and its cumulative mean normalisation
        for tau in 1..=tau_max {
            let mut sum = 0.0f32;
            for j in 0..n {
                let diff = s[a + j] - s[a + j + tau];
                sum += diff * diff;
            }
            d[tau] = sum;
        }
        let mut running = 0.0f32;
        let mut norm = vec![1.0f32; tau_max + 1];
        for tau in 1..=tau_max {
            running += d[tau];
            norm[tau] = if running > 0.0 { d[tau] * tau as f32 / running } else { 1.0 };
        }
        // first dip below the threshold, then walk to its bottom
        let mut pick = None;
        let mut tau = tau_min;
        while tau < tau_max {
            if norm[tau] < 0.15 {
                while tau + 1 < tau_max && norm[tau + 1] < norm[tau] {
                    tau += 1;
                }
                pick = Some(tau);
                break;
            }
            tau += 1;
        }
        let Some(t) = pick else {
            out.push(None);
            continue;
        };
        // parabolic refinement around the minimum
        let (x0, x1, x2) = (norm[t - 1], norm[t], norm[t + 1.min(tau_max - t)]);
        let denom = x0 - 2.0 * x1 + x2;
        let shift = if denom.abs() > 1e-9 { 0.5 * (x0 - x2) / denom } else { 0.0 };
        let period = t as f32 + shift.clamp(-1.0, 1.0);
        out.push(Some(rate / period));
    }
    out
}

/// A fixed number of points of the pitch shape, in semitones around the middle pitch, over the voiced
/// part of the recording. Empty if too little voiced sound was found.
pub fn shape(pcm: &Pcm, points: usize) -> Vec<f32> {
    let track = f0_track(pcm);
    // keep the span between the first and last voiced frame
    let first = track.iter().position(|v| v.is_some());
    let last = track.iter().rposition(|v| v.is_some());
    let (Some(first), Some(last)) = (first, last) else { return Vec::new() };
    let span: Vec<Option<f32>> = track[first..=last].to_vec();
    let voiced: Vec<f32> = span.iter().flatten().copied().collect();
    if voiced.len() < 8 {
        return Vec::new();
    }
    let mut sorted = voiced.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let st = |f: f32| 12.0 * (f / median).log2();
    // semitones, then a median filter to drop one-frame octave jumps
    let raw: Vec<Option<f32>> = span.iter().map(|v| v.map(st)).collect();
    let mut filtered: Vec<Option<f32>> = Vec::with_capacity(raw.len());
    for i in 0..raw.len() {
        let lo = i.saturating_sub(2);
        let hi = (i + 3).min(raw.len());
        let mut w: Vec<f32> = raw[lo..hi].iter().flatten().copied().collect();
        if raw[i].is_none() || w.len() < 3 {
            filtered.push(None);
        } else {
            w.sort_by(|a, b| a.partial_cmp(b).unwrap());
            filtered.push(Some(w[w.len() / 2]));
        }
    }
    // fill gaps by straight lines, then resample
    let known: Vec<(usize, f32)> = filtered.iter().enumerate().filter_map(|(i, v)| v.map(|x| (i, x))).collect();
    if known.len() < 6 {
        return Vec::new();
    }
    let at = |pos: f32| -> f32 {
        let p = pos.clamp(known[0].0 as f32, known[known.len() - 1].0 as f32);
        let idx = known.partition_point(|&(i, _)| (i as f32) <= p);
        if idx == 0 {
            return known[0].1;
        }
        if idx >= known.len() {
            return known[known.len() - 1].1;
        }
        let (i0, v0) = known[idx - 1];
        let (i1, v1) = known[idx];
        let t = (p - i0 as f32) / (i1 - i0) as f32;
        v0 + (v1 - v0) * t
    };
    let (a, b) = (known[0].0 as f32, known[known.len() - 1].0 as f32);
    (0..points).map(|k| at(a + (b - a) * k as f32 / (points - 1) as f32)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: impl Fn(f32) -> f32, secs: f32, rate: u32) -> Pcm {
        let n = (secs * rate as f32) as usize;
        let mut phase = 0.0f32;
        let samples = (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                phase += std::f32::consts::TAU * freq(t) / rate as f32;
                0.4 * phase.sin()
            })
            .collect();
        Pcm { samples, rate }
    }

    #[test]
    fn steady_tone_is_found() {
        let pcm = sine(|_| 200.0, 0.6, 16000);
        let t = f0_track(&pcm);
        let v: Vec<f32> = t.iter().flatten().copied().collect();
        assert!(v.len() > 30);
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        assert!((mean - 200.0).abs() < 4.0, "mean {mean}");
    }

    #[test]
    fn rising_and_falling_shapes() {
        let up = shape(&sine(|t| 150.0 + 150.0 * t, 0.7, 16000), 40);
        assert!(up.len() == 40 && up[39] - up[0] > 6.0, "up {:?}", (up.first(), up.last()));
        let down = shape(&sine(|t| 300.0 - 150.0 * t, 0.7, 16000), 40);
        assert!(down.len() == 40 && down[0] - down[39] > 5.0, "down {:?}", (down.first(), down.last()));
    }

    #[test]
    fn silence_gives_nothing() {
        let pcm = Pcm { samples: vec![0.0; 16000], rate: 16000 };
        assert!(shape(&pcm, 40).is_empty());
    }

    #[test]
    fn wav_round_trip() {
        let mut b = Vec::new();
        let data: Vec<u8> = (0..100i16).flat_map(|v| (v * 100).to_le_bytes()).collect();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&16000u32.to_le_bytes());
        b.extend_from_slice(&32000u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        let p = parse_wav(&b).unwrap();
        assert_eq!(p.rate, 16000);
        assert_eq!(p.samples.len(), 100);
    }
}

#[cfg(test)]
mod native_clips {
    use super::*;

    /// Prints the pitch shape of the bundled one-syllable words (run with --nocapture to look at it)
    /// and checks the clear cases: rising (R) words end higher than they start, falling (F) words
    /// start higher than they end.
    #[test]
    fn tone_shapes_of_native_words() {
        let words = [("สอง", 'R'), ("สาม", 'R'), ("ขวา", 'R'), ("ห้า", 'F'), ("ไม่", 'F'), ("ใช่", 'F'), ("ไป", 'M'), ("คน", 'M'), ("น้ำ", 'H'), ("หก", 'L')];
        let (mut ok, mut total) = (0, 0);
        for (w, tone) in words {
            let Some(pcm) = crate::audio::clip_pcm(w) else { continue };
            let s = shape(&pcm, 20);
            if s.is_empty() {
                println!("{w} {tone}: no pitch found");
                continue;
            }
            let (first, last) = (s[..4].iter().sum::<f32>() / 4.0, s[16..].iter().sum::<f32>() / 4.0);
            let span = s.iter().cloned().fold(f32::MIN, f32::max) - s.iter().cloned().fold(f32::MAX, f32::min);
            println!("{w} {tone}: start {first:+.1} end {last:+.1} range {span:.1}");
            match tone {
                'R' => {
                    total += 1;
                    if last > first + 1.5 {
                        ok += 1;
                    }
                }
                'F' => {
                    total += 1;
                    if first > last + 1.5 {
                        ok += 1;
                    }
                }
                _ => {}
            }
        }
        println!("clear cases matching: {ok}/{total}");
        assert!(total > 0 && ok * 3 >= total * 2, "only {ok}/{total} rising/falling words show the expected direction");
    }
}
