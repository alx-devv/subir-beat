//! Onda del MP3 (300 picos de 0 a 1 y la duración) para que la web la dibuje sin bajar el archivo.
//! Mismo cálculo que la app beatstore.

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone, Debug)]
pub struct Onda {
    pub d: f64,
    pub p: Vec<f32>,
}

pub fn calcular(ruta: &Path) -> Result<Onda> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    const N: usize = 300;
    let archivo = std::fs::File::open(ruta).with_context(|| format!("No se pudo abrir {}", ruta.display()))?;
    let mss = MediaSourceStream::new(Box::new(archivo), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let probado = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| anyhow!("No pude leer el MP3: {e}"))?;
    let mut formato = probado.format;
    let pista = formato.default_track().ok_or_else(|| anyhow!("El MP3 no tiene audio"))?.clone();
    let mut deco = symphonia::default::get_codecs().make(&pista.codec_params, &DecoderOptions::default()).map_err(|e| anyhow!("{e}"))?;
    let tasa = pista.codec_params.sample_rate.unwrap_or(44100) as f64;
    // Máximo absoluto por bloque chico; después se agrupa en N picos
    let mut bloques: Vec<f32> = Vec::new();
    let mut muestras_totales: u64 = 0;
    let mut actual: f32 = 0.0;
    let mut cuenta: usize = 0;
    const BLOQUE: usize = 1024;
    loop {
        let paquete = match formato.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        let decodificado = match deco.decode(&paquete) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let canales = decodificado.spec().channels.count().max(1);
        let mut buf = SampleBuffer::<f32>::new(decodificado.capacity() as u64, *decodificado.spec());
        buf.copy_interleaved_ref(decodificado);
        for marco in buf.samples().chunks(canales) {
            let v = marco.iter().fold(0f32, |m, x| m.max(x.abs()));
            if v > actual { actual = v; }
            cuenta += 1;
            muestras_totales += 1;
            if cuenta == BLOQUE { bloques.push(actual); actual = 0.0; cuenta = 0; }
        }
    }
    if cuenta > 0 { bloques.push(actual); }
    if bloques.is_empty() { bail!("El MP3 está vacío"); }
    let mut p = Vec::with_capacity(N);
    for i in 0..N {
        let a = i * bloques.len() / N;
        let b = (((i + 1) * bloques.len()) / N).max(a + 1).min(bloques.len());
        let m = bloques[a.min(bloques.len() - 1)..b].iter().fold(0f32, |m, x| m.max(*x));
        p.push((m.min(1.0) * 100.0).round() / 100.0);
    }
    let d = ((muestras_totales as f64 / tasa) * 100.0).round() / 100.0;
    Ok(Onda { d, p })
}
