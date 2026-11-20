//! Render con ffmpeg: video de YouTube, gif de la web y miniatura.

use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VisualKind {
    Image,
    Gif,
    Video,
}

pub fn visual_kind(p: &Path) -> VisualKind {
    match ext(p).as_str() {
        "gif" => VisualKind::Gif,
        "mp4" | "mov" | "webm" | "mkv" | "m4v" | "avi" => VisualKind::Video,
        _ => VisualKind::Image,
    }
}

pub fn ext(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// ffmpeg viene junto al .exe; si no, se busca en el PATH.
pub fn ffmpeg_path() -> PathBuf {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(name);
            if p.exists() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

fn command() -> Command {
    let mut c = Command::new(ffmpeg_path());
    c.kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Duración en segundos, leída del encabezado que imprime ffmpeg.
pub async fn duration_secs(input: &Path) -> Result<f64> {
    let out = command()
        .arg("-hide_banner")
        .arg("-i")
        .arg(input)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("No se pudo ejecutar ffmpeg")?;
    let text = String::from_utf8_lossy(&out.stderr);
    let d = text
        .split("Duration: ")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .ok_or_else(|| anyhow!("No se pudo leer la duración de {}", input.display()))?;
    let parts: Vec<f64> = d.trim().split(':').filter_map(|x| x.parse().ok()).collect();
    if parts.len() != 3 {
        bail!("Duración inválida: {d}");
    }
    Ok(parts[0] * 3600.0 + parts[1] * 60.0 + parts[2])
}

/// Ejecuta ffmpeg informando el avance (0..1) en base a `total` segundos.
async fn run(args: Vec<String>, total: f64, mut on_progress: impl FnMut(f64)) -> Result<()> {
    let mut child = command()
        .args(["-hide_banner", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats"])
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("No se pudo ejecutar ffmpeg")?;

    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let err_task = tokio::spawn(async move {
        let mut s = String::new();
        let _ = tokio::io::AsyncReadExt::read_to_string(&mut stderr, &mut s).await;
        s
    });

    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(v) = line.strip_prefix("out_time_us=") {
            if let Ok(us) = v.trim().parse::<f64>() {
                if total > 0.0 {
                    on_progress((us / 1_000_000.0 / total).clamp(0.0, 1.0));
                }
            }
        }
    }
    let status = child.wait().await?;
    let err = err_task.await.unwrap_or_default();
    if !status.success() {
        bail!("ffmpeg falló: {}", err.lines().last().unwrap_or("error desconocido"));
    }
    on_progress(1.0);
    Ok(())
}

const SQUARE: &str = "scale=1080:1080:force_original_aspect_ratio=increase,crop=1080:1080,setsar=1";

/// Video 1920x1080: visual cuadrado de 1080 al centro, franjas negras, loop hasta el final del audio.
pub async fn render_youtube(
    visual: &Path,
    audio: &Path,
    out: &Path,
    on_progress: impl FnMut(f64),
) -> Result<()> {
    let dur = duration_secs(audio).await?;
    let kind = visual_kind(visual);
    let mut a: Vec<String> = Vec::new();
    let s = |x: &str| x.to_string();
    let pad = "pad=1920:1080:420:0:black,format=yuv420p";
    match kind {
        VisualKind::Image => {
            a.extend([s("-loop"), s("1"), s("-framerate"), s("2"), s("-i")]);
            a.push(visual.to_string_lossy().into());
            a.push(s("-i"));
            a.push(audio.to_string_lossy().into());
            a.extend([
                s("-vf"),
                format!("{SQUARE},{pad}"),
                s("-c:v"), s("libx264"), s("-preset"), s("veryfast"), s("-tune"), s("stillimage"),
                s("-crf"), s("18"), s("-r"), s("2"), s("-g"), s("4"),
            ]);
        }
        VisualKind::Gif | VisualKind::Video => {
            a.extend([s("-stream_loop"), s("-1"), s("-i")]);
            a.push(visual.to_string_lossy().into());
            a.push(s("-i"));
            a.push(audio.to_string_lossy().into());
            a.extend([
                s("-vf"),
                format!("fps=30,{SQUARE},{pad}"),
                s("-c:v"), s("libx264"), s("-preset"), s("veryfast"),
                s("-crf"), s("20"), s("-g"), s("60"),
            ]);
        }
    }
    a.extend([
        s("-map"), s("0:v:0"), s("-map"), s("1:a:0"),
        s("-c:a"), s("aac"), s("-b:a"), s("320k"), s("-ar"), s("48000"),
        s("-t"), format!("{dur:.3}"),
        s("-movflags"), s("+faststart"),
    ]);
    a.push(out.to_string_lossy().into());
    run(a, dur, on_progress).await
}

/// Gif liviano para Shopify a partir de un video (paleta calculada para el clip).
pub async fn render_web_gif(
    video: &Path,
    out: &Path,
    size: u32,
    fps: u32,
    max_secs: u32,
    on_progress: impl FnMut(f64),
) -> Result<()> {
    let dur = duration_secs(video).await.unwrap_or(max_secs as f64);
    let total = dur.min(max_secs as f64);
    let filter = format!(
        "fps={fps},scale={size}:{size}:force_original_aspect_ratio=increase:flags=lanczos,crop={size}:{size},split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle"
    );
    let a = vec![
        "-t".into(), format!("{total:.3}"),
        "-i".into(), video.to_string_lossy().into(),
        "-filter_complex".into(), filter,
        "-loop".into(), "0".into(),
        out.to_string_lossy().into(),
    ];
    run(a, total, on_progress).await
}

/// Imagen cuadrada comprimida (cuando la portada pesa demasiado).
pub async fn compress_image(input: &Path, out: &Path, size: u32) -> Result<()> {
    let a = vec![
        "-i".into(), input.to_string_lossy().into(),
        "-vf".into(), format!("scale={size}:{size}:force_original_aspect_ratio=increase,crop={size}:{size}"),
        "-q:v".into(), "3".into(), "-frames:v".into(), "1".into(),
        out.to_string_lossy().into(),
    ];
    run(a, 0.0, |_| {}).await
}

/// Miniatura de YouTube: 1280x720 y menos de 2 MB. Devuelve la ruta lista y un aviso si hubo que adaptarla.
pub async fn prepare_thumbnail(input: &Path, out_dir: &Path) -> Result<(PathBuf, Option<String>)> {
    let meta = std::fs::metadata(input)?;
    let dims = imagesize::size(input).map_err(|e| anyhow!("No se pudo leer la miniatura: {e}"))?;
    let e = ext(input);
    let ok_format = matches!(e.as_str(), "jpg" | "jpeg" | "png");
    let ok_dims = dims.width == 1280 && dims.height == 720;
    let ok_size = meta.len() < 2 * 1024 * 1024;
    let dest_name = format!("miniatura.{}", if ok_format { e.as_str() } else { "jpg" });
    if ok_format && ok_dims && ok_size {
        let dest = out_dir.join(dest_name);
        std::fs::copy(input, &dest)?;
        return Ok((dest, None));
    }
    let ratio = dims.width as f64 / dims.height.max(1) as f64;
    let mut aviso = Vec::new();
    if (ratio - 16.0 / 9.0).abs() > 0.02 {
        aviso.push(format!("no era 16:9 ({}×{}), se recortó al centro", dims.width, dims.height));
    } else if !ok_dims {
        aviso.push(format!("se llevó de {}×{} a 1280×720", dims.width, dims.height));
    }
    if !ok_size {
        aviso.push("pesaba más de 2 MB, se comprimió".into());
    }
    let dest = out_dir.join("miniatura.jpg");
    for q in [2, 4, 6, 9] {
        let a = vec![
            "-i".into(), input.to_string_lossy().into(),
            "-vf".into(), "scale=1280:720:force_original_aspect_ratio=increase:flags=lanczos,crop=1280:720".into(),
            "-q:v".into(), q.to_string(), "-frames:v".into(), "1".into(),
            dest.to_string_lossy().into(),
        ];
        run(a, 0.0, |_| {}).await?;
        if std::fs::metadata(&dest)?.len() < 2 * 1024 * 1024 {
            break;
        }
    }
    Ok((dest, if aviso.is_empty() { None } else { Some(aviso.join("; ")) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    // Correr con: MEDIA_TEST_DIR=/ruta cargo test media -- --nocapture
    #[tokio::test]
    async fn render_real() {
        let Ok(d) = std::env::var("MEDIA_TEST_DIR") else { return };
        let d = PathBuf::from(d);
        let audio = d.join("beat.mp3");
        for (v, out) in [("cover.jpg", "o_img.mp4"), ("clip.mp4", "o_vid.mp4"), ("anim.gif", "o_gif.mp4")] {
            let t = std::time::Instant::now();
            render_youtube(&d.join(v), &audio, &d.join(out), |_| {}).await.unwrap();
            println!("{out}: {:?}", t.elapsed());
        }
        render_web_gif(&d.join("clip.mp4"), &d.join("web.gif"), 600, 15, 8, |_| {}).await.unwrap();
        let (p, note) = prepare_thumbnail(&d.join("thumb.png"), &d).await.unwrap();
        println!("thumb {p:?} {note:?}");
        assert!((duration_secs(&d.join("o_vid.mp4")).await.unwrap() - 20.0).abs() < 0.2);
    }
}

/// Miniaturas candidatas de 1280x720 con el mismo armado que el video (cuadrado al centro, franjas negras).
/// Video o gif: 3 fotogramas de distintos momentos (la tanda cambia con `seed`). Imagen: una sola.
pub async fn thumb_candidates(visual: &Path, out_dir: &Path, seed: u32) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir)?;
    let filter = "scale=720:720:force_original_aspect_ratio=increase,crop=720:720,setsar=1,pad=1280:720:280:0:black";
    let stem = format!("{:x}", seed);
    let kind = visual_kind(visual);
    let times: Vec<Option<f64>> = match kind {
        VisualKind::Image => vec![None],
        _ => {
            let d = duration_secs(visual).await.unwrap_or(0.0).max(0.1);
            // Tandas: 25/50/75 %, luego 15/45/85 %, 35/60/90 %, y vuelve a empezar.
            let sets = [[0.25, 0.5, 0.75], [0.15, 0.45, 0.85], [0.35, 0.6, 0.9]];
            sets[(seed as usize) % sets.len()].iter().map(|f| Some((d * f).min(d - 0.05).max(0.0))).collect()
        }
    };
    let mut out = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let path = out_dir.join(format!("miniatura_{stem}_{i}.jpg"));
        let mut c = command();
        c.arg("-hide_banner").arg("-loglevel").arg("error").arg("-y");
        if let Some(t) = t {
            c.arg("-ss").arg(format!("{t:.2}"));
        }
        c.arg("-i").arg(visual).arg("-frames:v").arg("1").arg("-vf").arg(filter).arg("-q:v").arg("3").arg(&path);
        let st = c.stdout(Stdio::null()).stderr(Stdio::piped()).output().await.context("No se pudo ejecutar ffmpeg")?;
        if !st.status.success() || !path.exists() {
            bail!("No se pudo sacar un fotograma: {}", String::from_utf8_lossy(&st.stderr).trim());
        }
        out.push(path);
    }
    Ok(out)
}

#[cfg(test)]
mod miniaturas {
    #[tokio::test]
    #[ignore]
    async fn tres_fotogramas() {
        let out = std::env::temp_dir().join("mini_test");
        let v = super::thumb_candidates(std::path::Path::new("/tmp/tc/v.mp4"), &out, 0).await.unwrap();
        assert_eq!(v.len(), 3);
        let v2 = super::thumb_candidates(std::path::Path::new("/tmp/tc/v.mp4"), &out, 1).await.unwrap();
        assert_ne!(v[0], v2[0]);
        for p in v.iter().chain(&v2) { let (w, h) = { let d = imagesize::size(p).unwrap(); (d.width, d.height) }; assert_eq!((w, h), (1280, 720)); }
        println!("{:?}", v);
    }
}
