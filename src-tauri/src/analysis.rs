//! Detección de nombre, BPM y tonalidad: primero el nombre del archivo, después el audio.

use anyhow::{bail, Context, Result};
use regex::Regex;
use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;

const NOTES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLATS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

/// Rango de BPM habitual de Alex: el detector elige siempre el tempo dentro de este rango.
pub const BPM_MIN: f64 = 80.0;
pub const BPM_MAX: f64 = 140.0;

#[derive(Serialize, Default, Debug, Clone, PartialEq)]
pub struct FromName {
    pub name: Option<String>,
    pub bpm: Option<u32>,
    pub key: Option<String>,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct FromAudio {
    pub bpm: u32,
    pub key: String,
    pub alt_key: String,
    pub key_confidence: f64,
}

/// Nombre de una tonalidad tal como la guarda la tienda ("C#m / Dbm", "Am").
pub fn shop_key(pc: usize, minor: bool) -> String {
    let m = if minor { "m" } else { "" };
    if NOTES[pc].contains('#') {
        format!("{}{m} / {}{m}", NOTES[pc], FLATS[pc])
    } else {
        format!("{}{m}", NOTES[pc])
    }
}

/// "C#m", "Dbm", "Bb", "Amin", "F#major" -> (clase de altura, menor)
pub fn parse_key(s: &str) -> Option<(usize, bool)> {
    let re = Regex::new(r"(?i)^([A-G])([#b]|sharp|flat)?(m|min|minor|maj|major)?$").unwrap();
    let c = re.captures(s.trim())?;
    let letter = c[1].to_uppercase();
    let mut pc = NOTES.iter().position(|n| *n == letter)? as i32;
    match c.get(2).map(|m| m.as_str().to_lowercase()) {
        Some(a) if a == "#" || a == "sharp" => pc += 1,
        Some(a) if a == "b" || a == "flat" => pc -= 1,
        _ => {}
    }
    let minor = matches!(c.get(3).map(|m| m.as_str().to_lowercase()).as_deref(), Some("m" | "min" | "minor"));
    Some((pc.rem_euclid(12) as usize, minor))
}

/// Lee nombre, BPM y tonalidad de nombres como "@alexgareis_turn it up_94bpm_C#m.wav".
pub fn from_filename(path: &Path) -> FromName {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let prefix = Regex::new(r"(?i)^@?alexgareis[_\s-]+").unwrap();
    let rest = prefix.replace(&stem, "").to_string();

    let bpm_re = Regex::new(r"(?i)(\d{2,3})\s*bpm").unwrap();
    let mut out = FromName::default();
    let mut cut = rest.len();

    if let Some(m) = bpm_re.find(&rest) {
        let n: u32 = bpm_re.captures(m.as_str()).unwrap()[1].parse().unwrap_or(0);
        if (40..=250).contains(&n) {
            out.bpm = Some(n);
            cut = cut.min(m.start());
        }
    }

    // Tonalidad: un bloque suelto pegado al BPM o, si no hay BPM, el último bloque.
    let tokens: Vec<(usize, &str)> = Regex::new(r"[^_\s]+")
        .unwrap()
        .find_iter(&rest)
        .map(|m| (m.start(), m.as_str()))
        .collect();
    let bpm_idx = tokens.iter().position(|(_, t)| bpm_re.is_match(t));
    let candidates: Vec<usize> = match bpm_idx {
        Some(i) => [i + 1, i.wrapping_sub(1)].into_iter().filter(|&j| j < tokens.len()).collect(),
        None => tokens.len().checked_sub(1).into_iter().collect(),
    };
    for j in candidates {
        let (start, tok) = tokens[j];
        if let Some((pc, minor)) = parse_key(tok) {
            // Una sola letra al final sin BPM es ambiguo ("a"), salvo que sea mayúscula.
            if bpm_idx.is_none() && tok.len() == 1 && tok.chars().all(|c| c.is_lowercase()) {
                continue;
            }
            out.key = Some(shop_key(pc, minor));
            cut = cut.min(start);
            break;
        }
    }

    let name = rest[..cut].trim_matches(|c: char| c == '_' || c == '-' || c.is_whitespace()).replace('_', " ");
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if !name.is_empty() && (out.bpm.is_some() || out.key.is_some() || prefix.is_match(&stem)) {
        out.name = Some(name);
    }
    out
}

const SR: usize = 22050;
const FRAME: usize = 2048;
const HOP: usize = 512;

/// Decodifica hasta 60 s del audio (desde el segundo 15, para saltear intros) en mono.
async fn decode(path: &Path) -> Result<Vec<f32>> {
    let mut cmd = tokio::process::Command::new(crate::media::ffmpeg_path());
    cmd.args(["-hide_banner", "-loglevel", "error", "-ss", "15", "-t", "60", "-i"])
        .arg(path)
        .args(["-ac", "1", "-ar", &SR.to_string(), "-f", "f32le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = cmd.output().await.context("No se pudo ejecutar ffmpeg")?;
    let mut samples: Vec<f32> = out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    if samples.len() < SR * 8 {
        // Beats cortos: se analiza desde el principio.
        let mut cmd = tokio::process::Command::new(crate::media::ffmpeg_path());
        cmd.args(["-hide_banner", "-loglevel", "error", "-t", "60", "-i"])
            .arg(path)
            .args(["-ac", "1", "-ar", &SR.to_string(), "-f", "f32le", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        let out = cmd.output().await?;
        samples = out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    }
    if samples.len() < SR * 4 {
        bail!("El audio es demasiado corto para analizarlo");
    }
    Ok(samples)
}

pub async fn analyze(path: &Path) -> Result<FromAudio> {
    let samples = decode(path).await?;
    tokio::task::spawn_blocking(move || analyze_samples(&samples)).await?
}

pub fn analyze_samples(samples: &[f32]) -> Result<FromAudio> {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FRAME as f32).cos())
        .collect();
    let bins = FRAME / 2;
    let hz_per_bin = SR as f32 / FRAME as f32;

    // Clase de altura de cada bin (entre 100 Hz y 2 kHz: abajo de eso mandan el bombo y el 808) para el cromagrama.
    let pc_of_bin: Vec<Option<usize>> = (0..bins)
        .map(|k| {
            let f = k as f32 * hz_per_bin;
            if !(100.0..=2000.0).contains(&f) {
                return None;
            }
            let midi = 69.0 + 12.0 * (f / 440.0).log2();
            Some((midi.round() as i32).rem_euclid(12) as usize)
        })
        .collect();

    let mut prev = vec![0f32; bins];
    let mut onset = Vec::new();
    let mut chroma = [0f64; 12];
    let mut buf = vec![Complex::new(0f32, 0f32); FRAME];
    let mut pos = 0;
    while pos + FRAME <= samples.len() {
        for i in 0..FRAME {
            buf[i] = Complex::new(samples[pos + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        let mut flux = 0f32;
        for k in 0..bins {
            let mag = buf[k].norm();
            let lm = (1.0 + 100.0 * mag).ln();
            let d = lm - prev[k];
            if d > 0.0 {
                flux += d;
            }
            prev[k] = lm;
            if let Some(pc) = pc_of_bin[k] {
                chroma[pc] += mag.sqrt() as f64;
            }
        }
        onset.push(flux);
        pos += HOP;
    }

    let bpm = tempo(&onset);
    let (key, alt, conf) = key_from_chroma(&chroma);
    Ok(FromAudio { bpm, key, alt_key: alt, key_confidence: conf })
}

fn tempo(onset: &[f32]) -> u32 {
    // Se resta el promedio local para quedarse con los golpes.
    let n = onset.len();
    let w = 16;
    let env: Vec<f64> = (0..n)
        .map(|i| {
            let a = i.saturating_sub(w);
            let b = (i + w).min(n - 1);
            let mean = onset[a..=b].iter().map(|&x| x as f64).sum::<f64>() / (b - a + 1) as f64;
            (onset[i] as f64 - mean).max(0.0)
        })
        .collect();
    let fps = SR as f64 / HOP as f64;
    let acf = |lag: f64| -> f64 {
        if lag < 1.0 {
            return 0.0;
        }
        let l0 = lag.floor() as usize;
        let frac = lag - l0 as f64;
        let mut s = 0.0;
        for i in 0..n.saturating_sub(l0 + 1) {
            let v = env[i + l0] * (1.0 - frac) + env[i + l0 + 1] * frac;
            s += env[i] * v;
        }
        s / (n - l0) as f64
    };
    let mut best = (BPM_MIN, f64::MIN);
    let mut b = BPM_MIN;
    while b <= BPM_MAX + 1e-9 {
        let lag = 60.0 * fps / b;
        let score = acf(lag) + 0.5 * acf(2.0 * lag) + 0.25 * acf(4.0 * lag) + 0.25 * acf(lag / 2.0);
        if score > best.1 {
            best = (b, score);
        }
        b += 0.25;
    }
    best.0.round() as u32
}

fn key_from_chroma(chroma: &[f64; 12]) -> (String, String, f64) {
    // Perfiles de Krumhansl-Kessler.
    const MAJ: [f64; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
    const MIN: [f64; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
    let corr = |profile: &[f64; 12], tonic: usize| -> f64 {
        let x: Vec<f64> = (0..12).map(|i| chroma[(i + tonic) % 12]).collect();
        let mx = x.iter().sum::<f64>() / 12.0;
        let my = profile.iter().sum::<f64>() / 12.0;
        let (mut num, mut dx, mut dy) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            num += (x[i] - mx) * (profile[i] - my);
            dx += (x[i] - mx).powi(2);
            dy += (profile[i] - my).powi(2);
        }
        if dx == 0.0 { 0.0 } else { num / (dx * dy).sqrt() }
    };
    let mut scores: Vec<(f64, usize, bool)> = Vec::new();
    for t in 0..12 {
        scores.push((corr(&MAJ, t), t, false));
        scores.push((corr(&MIN, t), t, true));
    }
    scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let (s1, t, minor) = scores[0];
    let s2 = scores[1].0;
    // La alternativa que se ofrece es la relativa (comparten notas).
    let (alt_t, alt_minor) = if minor { ((t + 3) % 12, false) } else { ((t + 9) % 12, true) };
    (shop_key(t, minor), shop_key(alt_t, alt_minor), ((s1 - s2) * 10.0).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn nombres_de_archivo() {
        let f = |s: &str| from_filename(&PathBuf::from(s));
        assert_eq!(
            f("@alexgareis_turn it up_94bpm_C#m.wav"),
            FromName { name: Some("turn it up".into()), bpm: Some(94), key: Some("C#m / Dbm".into()) }
        );
        let r = f("@alexgareis_intimate-thoughts_92bpm_D.wav");
        assert_eq!((r.name.as_deref(), r.bpm, r.key.as_deref()), (Some("intimate-thoughts"), Some(92), Some("D")));
        let r = f("esperame 96 BPM Bm.wav");
        assert_eq!((r.name.as_deref(), r.bpm, r.key.as_deref()), (Some("esperame"), Some(96), Some("Bm")));
        let r = f("cheese_Dbm_93bpm.wav");
        assert_eq!((r.name.as_deref(), r.bpm, r.key.as_deref()), (Some("cheese"), Some(93), Some("C#m / Dbm")));
        let r = f("@alexgareis_tsss!.wav");
        assert_eq!((r.name.as_deref(), r.bpm, r.key), (Some("tsss!"), None, None));
        let r = f("master final.wav");
        assert_eq!(r, FromName::default());
        assert_eq!(parse_key("Bb"), Some((10, false)));
        assert_eq!(shop_key(10, false), "A# / Bb");
    }

    // Correr con: ANALYSIS_TEST_DIR=/ruta cargo test analisis -- --nocapture
    #[tokio::test]
    async fn analisis_de_audio() {
        let Ok(d) = std::env::var("ANALYSIS_TEST_DIR") else { return };
        for (file, bpm, key) in [("cs_minor_94.wav", 94, "C#m / Dbm"), ("a_minor_136.wav", 136, "Am"), ("f_major_84.wav", 84, "F")] {
            let r = analyze(&PathBuf::from(&d).join(file)).await.unwrap();
            println!("{file}: {r:?}");
            assert_eq!(r.bpm, bpm, "{file}");
            assert!(r.key == key || r.alt_key == key, "{file}: {} / {}", r.key, r.alt_key);
        }
    }
}
