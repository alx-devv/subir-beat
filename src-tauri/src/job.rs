//! El proceso completo de subir un beat, con estado guardado para poder reintentar.

use crate::config::{self, Settings};
use crate::media::{self, VisualKind};
use crate::naming;
use crate::r2::R2;
use crate::onda;
use crate::tienda::{self, Tienda};
use crate::youtube;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct BeatInput {
    pub name: String,
    pub bpm: u32,
    pub key: String,
    pub generos: Vec<String>,
    pub moods: Vec<String>,
    pub instruments: Vec<String>,
    pub artist: String,
    pub price_premium: String,
    pub price_exclusiva: String,
    pub wav: String,
    pub stems: String,
    pub mp3: String,
    pub visual: String,
    pub thumbnail: String,
    /// public | unlisted | scheduled (vacío = el de Ajustes)
    pub yt_privacy: String,
    /// Fecha programada en UTC (RFC 3339) y en segundos, para validarla.
    pub yt_publish_at: String,
    pub yt_publish_ts: u64,
    /// Si ya hay un beat con ese nombre en la tienda, actualizarlo en vez de frenar.
    pub replace: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct StepState {
    pub status: String, // pendiente | en_curso | listo | error
    pub detail: String,
    pub progress: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Job {
    pub handle: String,
    pub created: u64,
    pub input: BeatInput,
    pub steps: BTreeMap<String, StepState>,
    pub running: bool,
    pub error: String,
    pub product_id: String,
    pub product_url: String,
    pub admin_url: String,
    pub links: BTreeMap<String, String>,
    pub out_dir: String,
    pub video: String,
    pub thumbnail: String,
    pub thumbnail_note: String,
    pub web_image: String,
    pub web_image_bytes: u64,
    /// Portada (GIF original) y miniatura WebP ya subidas a R2, como direcciones de la tienda.
    pub cover_url: String,
    pub cover_thumb: String,
    pub cover_thumb_bytes: u64,
    pub yt_title: String,
    pub yt_description: String,
    pub yt_tags: String,
    /// Sesión de subida reanudable de YouTube y resultado.
    pub yt_upload_url: String,
    pub yt_video_id: String,
    pub yt_url: String,
    pub yt_thumb_ok: bool,
    /// Nota para mostrar junto al resultado (privado por auditoría, miniatura, etc.).
    pub yt_note: String,
    /// Se subió sola (true) o quedó para subir a mano en Studio (false).
    pub yt_auto: bool,
}

pub const STEPS: [&str; 6] = ["archivos", "bucket", "web", "tienda", "video", "youtube"];

pub fn jobs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("trabajos")
}

pub fn load_job(data_dir: &Path, handle: &str) -> Option<Job> {
    std::fs::read_to_string(jobs_dir(data_dir).join(format!("{handle}.json")))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn list_jobs(data_dir: &Path) -> Vec<Job> {
    let mut v: Vec<Job> = std::fs::read_dir(jobs_dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();
    v.sort_by(|a: &Job, b| b.created.cmp(&a.created));
    v
}

pub fn validate(input: &BeatInput) -> Result<()> {
    let mut faltan = Vec::new();
    if input.name.trim().is_empty() { faltan.push("nombre"); }
    if input.bpm == 0 { faltan.push("BPM"); }
    if input.key.is_empty() { faltan.push("tonalidad"); }
    if input.instruments.is_empty() { faltan.push("al menos un instrumento"); }
    if input.artist.trim().is_empty() { faltan.push("artista de referencia"); }
    if !faltan.is_empty() {
        bail!("Falta completar: {}", faltan.join(", "));
    }
    if input.instruments.len() > 3 {
        bail!("Elegí hasta 3 instrumentos");
    }
    for (label, p, dir) in [
        ("el WAV", &input.wav, false),
        ("la carpeta de stems", &input.stems, true),
        ("el mp3 con tag", &input.mp3, false),
        ("la foto o el video (multimedia)", &input.visual, false),
    ] {
        let path = Path::new(p);
        if p.is_empty() || !path.exists() || path.is_dir() != dir {
            bail!("Falta {label}");
        }
    }
    Ok(())
}

/// Al empezar un beat nuevo, además: la fecha programada tiene que estar en el futuro.
pub fn validate_new(input: &BeatInput) -> Result<()> {
    validate(input)?;
    if input.yt_privacy == "scheduled" && (input.yt_publish_at.is_empty() || input.yt_publish_ts < now() + 10 * 60) {
        bail!("La fecha programada de YouTube tiene que ser al menos 10 minutos en el futuro");
    }
    Ok(())
}

fn price_label(s: &str) -> String {
    let v: f64 = s.trim().replace(',', ".").parse().unwrap_or(0.0);
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v:.2}") }
}

fn safe_dir_name(s: &str) -> String {
    s.chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string()
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Contexto compartido mientras corre un trabajo.
struct Ctx {
    app: AppHandle,
    data_dir: PathBuf,
    job: Mutex<Job>,
}

impl Ctx {
    async fn update(&self, f: impl FnOnce(&mut Job)) {
        let snapshot = {
            let mut j = self.job.lock().await;
            f(&mut j);
            j.clone()
        };
        let dir = jobs_dir(&self.data_dir);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join(format!("{}.json", snapshot.handle)),
            serde_json::to_string_pretty(&snapshot).unwrap_or_default(),
        );
        let _ = self.app.emit("trabajo", &snapshot);
    }

    async fn step(&self, step: &str, status: &str, detail: impl Into<String>, progress: f64) {
        let (step, status, detail) = (step.to_string(), status.to_string(), detail.into());
        self.update(move |j| {
            j.steps.insert(step, StepState { status, detail, progress });
        })
        .await;
    }

    /// Avance liviano (sin escribir a disco), para barras de progreso.
    fn progress(&self, step: &str, detail: &str, progress: f64) {
        let _ = self.app.emit(
            "avance",
            json!({ "step": step, "detail": detail, "progress": progress }),
        );
    }

    async fn is_done(&self, step: &str) -> bool {
        self.job.lock().await.steps.get(step).map(|s| s.status == "listo").unwrap_or(false)
    }
}

pub fn new_job(input: BeatInput, settings: &Settings) -> Job {
    let handle = naming::handle(&input.name);
    let out_dir = PathBuf::from(&settings.output_dir).join(safe_dir_name(&input.name));
    let mut steps = BTreeMap::new();
    for s in STEPS {
        steps.insert(s.to_string(), StepState { status: "pendiente".into(), ..Default::default() });
    }
    Job {
        handle,
        created: now(),
        input,
        steps,
        out_dir: out_dir.to_string_lossy().into(),
        ..Default::default()
    }
}

/// Corre (o retoma) un trabajo. Los pasos ya completados se saltean.
pub async fn run(app: AppHandle, data_dir: PathBuf, config_dir: PathBuf, mut job: Job) -> Result<()> {
    let settings = config::load(&config_dir);
    job.running = true;
    job.error.clear();
    // Trabajos de versiones viejas (con los pasos de Shopify): quedan solo los pasos de ahora.
    job.steps.retain(|k, _| STEPS.contains(&k.as_str()));
    for st in STEPS {
        job.steps.entry(st.to_string()).or_insert_with(|| StepState { status: "pendiente".into(), ..Default::default() });
    }
    for s in job.steps.values_mut() {
        if s.status != "listo" {
            *s = StepState { status: "pendiente".into(), ..Default::default() };
        }
    }
    let ctx = Arc::new(Ctx { app, data_dir, job: Mutex::new(job) });
    ctx.update(|_| {}).await;

    let result = run_inner(ctx.clone(), settings).await;
    let err = result.as_ref().err().map(|e| format!("{e:#}"));
    ctx.update(move |j| {
        j.running = false;
        if let Some(e) = err {
            j.error = e.clone();
            for s in j.steps.values_mut() {
                if s.status == "en_curso" {
                    s.status = "error".into();
                    s.detail = e.clone();
                }
            }
        }
    })
    .await;
    result
}

async fn run_inner(ctx: Arc<Ctx>, settings: Settings) -> Result<()> {
    let (mut input, handle, out_dir) = {
        let j = ctx.job.lock().await;
        (j.input.clone(), j.handle.clone(), PathBuf::from(&j.out_dir))
    };
    validate(&input)?;
    std::fs::create_dir_all(&out_dir).context("No se pudo crear la carpeta de salida")?;

    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()?;
    let tienda = Tienda::new(&settings.store_url, config::clave_tienda(), http.clone())?;
    let r2 = R2::new(
        &settings.r2_account_id,
        &settings.r2_access_key_id,
        &config::get_secret(config::SECRET_R2).unwrap_or_default(),
        &settings.r2_bucket,
        http.clone(),
    )?;

    // Antes de tocar nada: que no exista otro beat con la misma dirección (salvo que se pida reemplazarlo).
    // De paso trae los precios publicados de las licencias, para los textos ({premium} y {exclusiva}).
    let consulta = tienda.consultar(&handle).await?;
    if consulta.existe && !input.replace && !ctx.is_done("bucket").await {
        bail!("Ya existe un beat con el nombre \"{}\" en la tienda (alexgareis.com/beats/{handle}). Cambiá el nombre del beat.", input.name);
    }
    if let (Some(p), Some(e)) = (consulta.premium, consulta.exclusiva) {
        input.price_premium = tienda::precio_texto(p);
        input.price_exclusiva = tienda::precio_texto(e);
        let (pp, pe) = (input.price_premium.clone(), input.price_exclusiva.clone());
        ctx.update(move |j| { j.input.price_premium = pp; j.input.price_exclusiva = pe; }).await;
    } else if input.price_premium.is_empty() {
        input.price_premium = settings.price_premium.clone();
        input.price_exclusiva = settings.price_exclusiva.clone();
    }

    // El Léeme manda a la página de descargas: tiene que existir. Solo hace falta antes de armar los zips.
    let url = settings.downloads_url.trim();
    if !url.is_empty() && !ctx.is_done("bucket").await && !page_exists(url).await {
        bail!("La página de descargas ({url}) no responde. Revisala en Ajustes.");
    }

    let names = naming::names(&input.name, input.bpm, &input.key);

    // El video corre en paralelo: la CPU renderiza mientras la red sube.
    let video_task = {
        let ctx = ctx.clone();
        let input = input.clone();
        let out_dir = out_dir.clone();
        tokio::spawn(async move { step_video(&ctx, &input, &out_dir).await })
    };

    let chain = async {
        step_files_and_bucket(&ctx, &settings, &r2, &input, &names, &out_dir).await?;
        step_web(&ctx, &settings, &r2, &input, &names, &out_dir).await?;
        step_tienda(&ctx, &settings, &tienda, &input, &names, &handle).await?;
        Ok::<_, anyhow::Error>(())
    };
    if let Err(e) = chain.await {
        // No tiene sentido esperar el render si falló la tienda: se corta y se retoma al reintentar.
        video_task.abort();
        ctx.update(|j| {
            if let Some(s) = j.steps.get_mut("video") {
                if s.status == "en_curso" {
                    *s = StepState { status: "pendiente".into(), ..Default::default() };
                }
            }
        })
        .await;
        return Err(e);
    }
    video_task.await.map_err(|e| anyhow!("{e}"))??;
    step_youtube(&ctx, &settings, &input, &out_dir).await?;
    Ok(())
}

// ---------- Paso: zips + bucket ----------

fn collect_stems(dir: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let p = e?.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.push((p.clone(), format!("STEMS/{rel}")));
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    if out.is_empty() {
        bail!("La carpeta de stems está vacía");
    }
    Ok(out)
}

/// Zip sin compresión (el audio no se comprime y así es mucho más rápido).
fn build_zip(dest: &Path, entries: &[(PathBuf, String)], text: Option<(&str, &[u8])>, on_bytes: &dyn Fn(u64)) -> Result<()> {
    let tmp = dest.with_extension("zip.part");
    let file = std::fs::File::create(&tmp)?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::with_capacity(4 << 20, file));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);
    let mut buf = vec![0u8; 4 << 20];
    let mut total = 0u64;
    if let Some((name, bytes)) = text {
        zip.start_file(name, opts)?;
        zip.write_all(bytes)?;
    }
    for (src, name) in entries {
        zip.start_file(name.as_str(), opts)?;
        let mut f = std::fs::File::open(src).with_context(|| format!("No se pudo abrir {}", src.display()))?;
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 { break; }
            zip.write_all(&buf[..n])?;
            total += n as u64;
            on_bytes(total);
        }
    }
    zip.finish()?.flush()?;
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// ¿La página existe? Se pide como un navegador. Solo cuenta como caída si no hay conexión,
/// si no existe (404/410) o si el servidor falla; un bloqueo antibots (403/429) no frena la subida.
pub async fn page_exists(url: &str) -> bool {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap_or_default();
    for intento in 0..3 {
        if intento > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        match client.get(url).header("Accept", "text/html,application/xhtml+xml").header("Accept-Language", "es").send().await {
            Ok(r) => {
                let s = r.status().as_u16();
                if !matches!(s, 404 | 410 | 500 | 502 | 504) {
                    return true;
                }
                if matches!(s, 404 | 410) {
                    return false;
                }
            }
            Err(_) => {}
        }
    }
    false
}

/// Texto del Léeme: UTF-8 con BOM y saltos de línea de Windows, para que el Bloc de notas muestre bien los acentos.
fn readme_bytes(template: &str, input: &BeatInput, settings: &Settings) -> Vec<u8> {
    let shown = settings.downloads_url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
    let text = template
        .replace("{nombre}", input.name.trim())
        .replace("{descargas}", shown)
        .replace("\r\n", "\n")
        .replace('\n', "\r\n");
    let mut out = vec![0xEF, 0xBB, 0xBF];
    out.extend_from_slice(text.as_bytes());
    out
}

async fn step_files_and_bucket(
    ctx: &Arc<Ctx>,
    settings: &Settings,
    r2: &R2,
    input: &BeatInput,
    names: &naming::Names,
    out_dir: &Path,
) -> Result<()> {
    let zip_dir = out_dir.join("_zips");
    let premium = zip_dir.join(&names.premium);
    let exclusiva = zip_dir.join(&names.exclusiva);

    if !ctx.is_done("bucket").await {
        // Zips
        if !ctx.is_done("archivos").await || !premium.exists() || !exclusiva.exists() {
            ctx.step("archivos", "en_curso", "Armando los zips", 0.0).await;
            std::fs::create_dir_all(&zip_dir)?;
            let wav = PathBuf::from(&input.wav);
            let wav_entry = vec![(wav.clone(), names.wav_inside.clone())];
            let mut excl_entries = wav_entry.clone();
            excl_entries.extend(collect_stems(Path::new(&input.stems))?);
            let wav_size = std::fs::metadata(&wav)?.len();
            let total: u64 = wav_size + excl_entries.iter().map(|(p, _)| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum::<u64>();

            let readme_p = readme_bytes(&settings.readme_premium, input, settings);
            let readme_e = readme_bytes(&settings.readme_exclusiva, input, settings);
            let c = ctx.clone();
            let (p, e) = (premium.clone(), exclusiva.clone());
            tokio::task::spawn_blocking(move || -> Result<()> {
                let report = |done: u64| c.progress("archivos", "Armando los zips", done as f64 / total.max(1) as f64);
                build_zip(&p, &wav_entry, Some(("Léeme.txt", &readme_p)), &report)?;
                build_zip(&e, &excl_entries, Some(("Léeme.txt", &readme_e)), &|d| report(wav_size + d))?;
                Ok(())
            })
            .await??;
            ctx.step("archivos", "listo", "Zip Premium (WAV) y Exclusiva (WAV + stems)", 1.0).await;
        }

        // Subida
        ctx.step("bucket", "en_curso", "Subiendo al bucket", 0.0).await;
        let uploads = vec![
            (PathBuf::from(&input.mp3), names.mp3.clone(), "audio/mpeg", "mp3"),
            (premium.clone(), names.premium.clone(), "application/zip", "premium"),
            (exclusiva.clone(), names.exclusiva.clone(), "application/zip", "exclusiva"),
        ];
        let sizes: Vec<u64> = uploads.iter().map(|(p, ..)| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).collect();
        let total: u64 = sizes.iter().sum();
        let mut base = 0u64;
        for (i, (path, file, mime, label)) in uploads.iter().enumerate() {
            let key = naming::object_key(&names.folder, file);
            let c = ctx.clone();
            let b = base;
            let detail = format!("Subiendo {}", match *label { "mp3" => "el mp3 con tag", "premium" => "el zip Premium", _ => "el zip Exclusiva" });
            let d2 = detail.clone();
            r2.upload(path, &key, mime, move |done| c.progress("bucket", &d2, (b + done) as f64 / total.max(1) as f64))
                .await
                .with_context(|| format!("Falló la subida de {file}"))?;
            base += sizes[i];
            let url = naming::public_url(&settings.public_base, &names.folder, file);
            let label = label.to_string();
            ctx.update(move |j| { j.links.insert(label, url); }).await;
        }

        // El mp3 es público: verificamos que dl.alexgareis.com lo sirva.
        let mp3_url = naming::public_url(&settings.public_base, &names.folder, &names.mp3);
        let ok = reqwest::Client::new().head(&mp3_url).send().await.map(|r| r.status().is_success()).unwrap_or(false);
        if !ok {
            bail!("El mp3 está en el bucket pero {mp3_url} no responde. Revisá el Worker audio-proxy.");
        }
        ctx.step("bucket", "listo", "mp3 y zips en beats-entregables, verificados", 1.0).await;
        if !settings.keep_zips {
            let _ = std::fs::remove_dir_all(&zip_dir);
        }
    } else {
        ctx.step("archivos", "listo", "Zip Premium (WAV) y Exclusiva (WAV + stems)", 1.0).await;
    }
    Ok(())
}

// ---------- Paso: portada y miniatura para la tienda ----------

async fn step_web(ctx: &Arc<Ctx>, settings: &Settings, r2: &R2, input: &BeatInput, names: &naming::Names, out_dir: &Path) -> Result<()> {
    if ctx.is_done("web").await && !ctx.job.lock().await.cover_url.is_empty() {
        return Ok(());
    }
    portada_web(ctx, settings, input, out_dir).await?;
    let web_image = PathBuf::from(&ctx.job.lock().await.web_image);

    // Miniatura liviana (WebP animado de 240 px) para el catálogo, el reproductor y el carrito
    ctx.step("web", "en_curso", "Armando la miniatura del catálogo", 0.5).await;
    let mini = out_dir.join("miniatura.webp");
    media::miniatura(&web_image, &mini).await?;
    let mini_bytes = std::fs::metadata(&mini)?.len();

    // Las dos a R2, dentro de la carpeta del beat. El nombre cambia en cada subida:
    // la tienda guarda las imágenes en caché por un año, así nunca queda una vieja.
    ctx.step("web", "en_curso", "Subiendo la portada y la miniatura", 0.7).await;
    let suf = now();
    let portada = format!("portada-{suf}.{}", media::ext(&web_image));
    let miniatura = format!("portada-mini-{suf}.webp");
    r2.upload(&web_image, &naming::object_key(&names.folder, &portada), mime_for(&web_image), |_| {})
        .await
        .context("Falló la subida de la portada")?;
    r2.upload(&mini, &naming::object_key(&names.folder, &miniatura), "image/webp", |_| {})
        .await
        .context("Falló la subida de la miniatura")?;
    let store = settings.store_url.trim_end_matches('/');
    let (cu, ct) = (naming::media_url(store, &names.folder, &portada), naming::media_url(store, &names.folder, &miniatura));
    let big = ctx.job.lock().await.web_image_bytes;
    ctx.update(move |j| { j.cover_url = cu; j.cover_thumb = ct; j.cover_thumb_bytes = mini_bytes; }).await;
    ctx.step(
        "web",
        "listo",
        format!("Portada {:.1} MB y miniatura {} KB, en la tienda", big as f64 / 1_048_576.0, (mini_bytes / 1024).max(1)),
        1.0,
    )
    .await;
    Ok(())
}

/// La imagen de la portada: el GIF original (o uno generado si el visual es un video o un GIF enorme).
async fn portada_web(ctx: &Arc<Ctx>, settings: &Settings, input: &BeatInput, out_dir: &Path) -> Result<()> {
    if Path::new(&ctx.job.lock().await.web_image).exists() {
        return Ok(());
    }
    let visual = PathBuf::from(&input.visual);
    let kind = media::visual_kind(&visual);
    let size = std::fs::metadata(&visual)?.len();
    let (dest, detail) = match kind {
        VisualKind::Video => {
            ctx.step("web", "en_curso", "Creando el gif para la web", 0.0).await;
            let dest = out_dir.join("web.gif");
            let c = ctx.clone();
            media::render_web_gif(&visual, &dest, settings.gif_size, settings.gif_fps, settings.gif_max_secs, move |p| {
                c.progress("web", "Creando el gif para la web", p)
            })
            .await?;
            (dest, "Gif generado desde el video")
        }
        VisualKind::Gif if size > 12 * 1024 * 1024 => {
            ctx.step("web", "en_curso", "Achicando el gif", 0.0).await;
            let dest = out_dir.join("web.gif");
            let c = ctx.clone();
            media::render_web_gif(&visual, &dest, settings.gif_size, settings.gif_fps, settings.gif_max_secs, move |p| {
                c.progress("web", "Achicando el gif", p)
            })
            .await?;
            (dest, "Gif optimizado")
        }
        VisualKind::Image if size > 4 * 1024 * 1024 => {
            ctx.step("web", "en_curso", "Comprimiendo la portada", 0.0).await;
            let dest = out_dir.join("web.jpg");
            media::compress_image(&visual, &dest, 1200).await?;
            (dest, "Portada comprimida")
        }
        _ => {
            let dest = out_dir.join(format!("web.{}", media::ext(&visual)));
            std::fs::copy(&visual, &dest)?;
            (dest, "Se usa el visual tal cual")
        }
    };
    let bytes = std::fs::metadata(&dest)?.len();
    let detail = format!("{detail}, {:.1} MB", bytes as f64 / 1_048_576.0);
    let d = dest.to_string_lossy().to_string();
    ctx.update(move |j| { j.web_image = d; j.web_image_bytes = bytes; }).await;
    ctx.step("web", "en_curso", detail, 0.4).await;
    Ok(())
}

// ---------- Paso: alta en la tienda ----------

fn mime_for(p: &Path) -> &'static str {
    match media::ext(p).as_str() {
        "gif" => "image/gif",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
}

async fn step_tienda(
    ctx: &Arc<Ctx>,
    settings: &Settings,
    tienda: &Tienda,
    input: &BeatInput,
    names: &naming::Names,
    handle: &str,
) -> Result<()> {
    if ctx.is_done("tienda").await {
        return Ok(());
    }
    // Onda del mp3: el reproductor la dibuja al instante y el audio arranca sin bajar el archivo entero
    ctx.step("tienda", "en_curso", "Calculando la onda del mp3", 0.2).await;
    let mp3 = PathBuf::from(&input.mp3);
    let preview_url = naming::public_url(&settings.public_base, &names.folder, &names.mp3);
    let onda = tokio::task::spawn_blocking(move || onda::calcular(&mp3)).await?.ok();

    ctx.step("tienda", "en_curso", "Publicando el beat en alexgareis.com", 0.6).await;
    let (cover_url, cover_thumb) = {
        let j = ctx.job.lock().await;
        (j.cover_url.clone(), j.cover_thumb.clone())
    };
    let beat = json!({
        "slug": handle,
        "title": input.name.trim(),
        "status": "active",
        "bpm": input.bpm,
        "music_key": input.key,
        "genres": input.generos,
        "moods": input.moods,
        "instruments": input.instruments,
        "cover_url": cover_url,
        "cover_thumb": cover_thumb,
        "preview_url": preview_url,
        "file_premium": naming::object_key(&names.folder, &names.premium),
        "file_exclusive": naming::object_key(&names.folder, &names.exclusiva),
        "sort_order": 0,
        "onda": onda.map(|o| json!({ "audio": preview_url, "d": o.d, "p": o.p })),
    });
    let url = tienda.alta(beat, input.replace).await?;
    let url = if url.is_empty() { format!("{}/beats/{handle}", settings.store_url.trim_end_matches('/')) } else { url };
    let u = url.clone();
    ctx.update(move |j| { j.product_url = u; j.admin_url = String::new(); }).await;
    ctx.step("tienda", "listo", "Publicado en alexgareis.com, con miniatura y onda", 1.0).await;
    Ok(())
}

// ---------- Paso: video ----------

async fn step_video(ctx: &Arc<Ctx>, input: &BeatInput, out_dir: &Path) -> Result<()> {
    let dest = out_dir.join(format!("{} - YouTube.mp4", safe_dir_name(&input.name)));
    if ctx.is_done("video").await && dest.exists() {
        return Ok(());
    }
    ctx.step("video", "en_curso", "Renderizando 1920×1080", 0.0).await;
    let tmp = out_dir.join("render.tmp.mp4");
    let c = ctx.clone();
    let r = media::render_youtube(Path::new(&input.visual), Path::new(&input.mp3), &tmp, move |p| {
        c.progress("video", "Renderizando 1920×1080", p)
    })
    .await;
    if let Err(e) = r {
        ctx.step("video", "error", format!("{e:#}"), 0.0).await;
        return Err(e);
    }
    std::fs::rename(&tmp, &dest)?;
    let mb = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0;
    let d = dest.to_string_lossy().to_string();
    ctx.update(move |j| j.video = d).await;
    ctx.step("video", "listo", format!("Video listo, {mb:.0} MB"), 1.0).await;
    Ok(())
}

// ---------- Paso: textos y miniatura de YouTube ----------

/// Título del video según la plantilla de Ajustes (en minúscula si así está configurado).
pub fn title_for(settings: &Settings, input: &BeatInput, link: &str, store: &str) -> String {
    let t = fill(&settings.yt_title, input, link, store);
    // Sin géneros elegidos no queda un "|" colgando.
    let t = t.trim().trim_end_matches('|').trim_end().to_string();
    if settings.yt_title_lower { t.to_lowercase() } else { t }
}

pub fn fill(template: &str, input: &BeatInput, link: &str, store: &str) -> String {
    let first = |v: &Vec<String>| v.first().cloned().unwrap_or_default();
    let artist_tag: String = input
        .artist
        .split(|c| c == ',' || c == '&')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .split(" x ")
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    template
        .replace("{nombre}", input.name.trim())
        .replace("{artista_tag}", &artist_tag)
        .replace("{artista}", input.artist.trim())
        .replace("{bpm}", &input.bpm.to_string())
        .replace("{key}", &naming::key_short(&input.key))
        .replace("{generos}", &input.generos.join(", "))
        .replace("{estilo}", &input.generos.iter().take(2).cloned().collect::<Vec<_>>().join(" "))
        .replace("{genero}", &first(&input.generos))
        .replace("{mood}", &first(&input.moods).to_lowercase())
        .replace("{instrumentos}", &input.instruments.join(", "))
        .replace("{link}", link)
        .replace("{tienda}", store)
        .replace("{premium}", &price_label(&input.price_premium))
        .replace("{exclusiva}", &price_label(&input.price_exclusiva))
}

/// Tags sin repetir, sin vacíos y dentro del límite de YouTube.
pub fn clean_tags(raw: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut len = 0;
    for t in raw.split(',') {
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() || t.contains('{') || !seen.insert(t.to_lowercase()) {
            continue;
        }
        if len + t.len() + 2 > 480 { break; }
        len += t.len() + 2;
        out.push(t);
    }
    out.join(", ")
}

async fn step_youtube(ctx: &Arc<Ctx>, settings: &Settings, input: &BeatInput, out_dir: &Path) -> Result<()> {
    if ctx.is_done("youtube").await {
        return Ok(());
    }
    ctx.step("youtube", "en_curso", "Preparando textos y miniatura", 0.0).await;
    let handle = ctx.job.lock().await.handle.clone();
    let store = settings.store_url.trim_end_matches('/');
    let product_url = ctx.job.lock().await.product_url.clone();
    let link = format!("{product_url}?utm_source=youtube&utm_medium=video&utm_campaign={handle}");
    let title = youtube::clean_text(&title_for(settings, input, &link, store), 100);
    let description = youtube::clean_text(&fill(&settings.yt_description, input, &link, store), 5000);
    let tags = youtube::tag_list(&clean_tags(&fill(&settings.yt_tags, input, &link, store))).join(", ");

    let (thumb, note) = if !input.thumbnail.is_empty() && Path::new(&input.thumbnail).exists() {
        let (p, n) = media::prepare_thumbnail(Path::new(&input.thumbnail), out_dir).await?;
        (p.to_string_lossy().to_string(), n.unwrap_or_default())
    } else {
        (String::new(), "YouTube elige la miniatura del video".into())
    };
    let txt = format!("TÍTULO\n{title}\n\nDESCRIPCIÓN\n{description}\n\nTAGS\n{tags}\n");
    std::fs::write(out_dir.join("youtube.txt"), txt)?;
    {
        let (t, d, ta, th, n) = (title.clone(), description.clone(), tags.clone(), thumb.clone(), note);
        ctx.update(move |j| {
            j.yt_title = t;
            j.yt_description = d;
            j.yt_tags = ta;
            j.thumbnail = th;
            j.thumbnail_note = n;
        })
        .await;
    }

    // Sin conexión con YouTube: queda listo para subir a mano, como antes.
    if !youtube::is_connected() {
        ctx.update(|j| j.yt_auto = false).await;
        ctx.step("youtube", "listo", "Todo listo para subir en YouTube Studio", 1.0).await;
        return Ok(());
    }

    let video = PathBuf::from(&ctx.job.lock().await.video);
    let size = std::fs::metadata(&video).context("No se encontró el video renderizado")?.len();
    let mut privacy = if input.yt_privacy.is_empty() { settings.yt_privacy.clone() } else { input.yt_privacy.clone() };
    let mut late = false;
    if privacy == "scheduled" && input.yt_publish_ts < now() + 5 * 60 {
        // Se retomó después de la fecha programada: queda privado para publicarlo a mano.
        privacy = "private".into();
        late = true;
    }
    let mut yt = youtube::YouTube::new(&settings.yt_client_id, reqwest::Client::new()).await?;

    let mut result = None;
    for _ in 0..2 {
        let mut session = ctx.job.lock().await.yt_upload_url.clone();
        if session.is_empty() {
            let meta = youtube::metadata(&title, &description, &tags, &privacy, &input.yt_publish_at);
            session = yt.start_upload(&meta, size).await?;
            let s2 = session.clone();
            ctx.update(move |j| j.yt_upload_url = s2).await;
        }
        let c = ctx.clone();
        let report = move |done: u64, total: u64| {
            let mb = |b: u64| b as f64 / 1_048_576.0;
            c.progress("youtube", &format!("Subiendo video · {:.0} de {:.0} MB", mb(done), mb(total)), done as f64 / total.max(1) as f64)
        };
        match yt.upload(&session, &video, &report).await? {
            Some(v) => { result = Some(v); break; }
            // La sesión venció (dura una semana): se abre una nueva.
            None => ctx.update(|j| j.yt_upload_url.clear()).await,
        }
    }
    let v = result.ok_or_else(|| anyhow!("YouTube no aceptó la subida. Probá retomar."))?;
    let id = v["id"].as_str().unwrap_or_default().to_string();
    if id.is_empty() {
        bail!("YouTube no devolvió el video subido");
    }
    let channel = v.pointer("/snippet/channelTitle").and_then(|c| c.as_str()).unwrap_or_default().to_string();
    if !channel.is_empty() && channel != settings.yt_channel {
        let cfg = ctx.app.path().app_config_dir().unwrap_or_default();
        let mut s = config::load(&cfg);
        s.yt_channel = channel;
        let _ = config::save(&cfg, &s);
    }

    let mut notes = Vec::new();
    if late {
        notes.push("La fecha programada ya había pasado, así que el video quedó privado.".to_string());
    }
    let mut thumb_ok = false;
    if !thumb.is_empty() {
        ctx.progress("youtube", "Poniendo la miniatura", 1.0);
        match yt.set_thumbnail(&id, Path::new(&thumb)).await {
            Ok(()) => thumb_ok = true,
            Err(e) => notes.push(format!("La miniatura no se pudo poner: {e:#} Subila a mano en Studio.")),
        }
    }
    if !settings.yt_audited {
        notes.push("Mientras Google no apruebe la auditoría, YouTube deja los videos en privado. Cambiale la visibilidad desde YouTube Studio.".into());
    }
    let url = format!("https://youtu.be/{id}");
    let detail = match privacy.as_str() {
        "scheduled" => "Video subido y programado",
        "unlisted" => "Video subido como oculto",
        "private" => "Video subido como privado",
        _ => "Video subido",
    };
    let (u, i, n) = (url, id, notes.join(" "));
    ctx.update(move |j| {
        j.yt_video_id = i;
        j.yt_url = u;
        j.yt_note = n;
        j.yt_thumb_ok = thumb_ok;
        j.yt_auto = true;
        j.yt_upload_url.clear();
    })
    .await;
    ctx.step("youtube", "listo", detail, 1.0).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn textos() {
        let input = BeatInput {
            name: "esperame".into(), bpm: 96, key: "Bm".into(),
            generos: vec!["R&B".into()], moods: vec!["Nostalgic".into()],
            instruments: vec!["Pads".into()], artist: "Bryant Myers x Feid".into(),
            price_premium: "49".into(), price_exclusiva: "125".into(), ..Default::default()
        };
        let t = fill("{artista} Type Beat - \"{nombre}\" #{artista_tag}typebeat ${premium}", &input, "L", "S");
        assert_eq!(t, "Bryant Myers x Feid Type Beat - \"esperame\" #bryantmyerstypebeat $49");
        assert_eq!(clean_tags("a, A , b,, {x}, c"), "a, b, c");
        assert_eq!(price_label("49.50"), "49.50");
    }

    #[test]
    fn zip_con_leeme() {
        let dir = std::env::temp_dir().join("zip_leeme");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("a.wav");
        std::fs::write(&wav, b"RIFFdata").unwrap();
        let input = BeatInput { name: "turn it up".into(), ..Default::default() };
        let s = Settings::default();
        let readme = readme_bytes(&s.readme_exclusiva, &input, &s);
        let dest = dir.join("x.zip");
        build_zip(&dest, &[(wav, "beat.wav".into())], Some(("Léeme.txt", &readme)), &|_| {}).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let mut txt = String::new();
        std::io::Read::read_to_string(&mut z.by_name("Léeme.txt").unwrap(), &mut txt).unwrap();
        assert!(txt.starts_with('\u{feff}'));
        assert!(txt.contains("\"turn it up\"") && txt.contains("alexgareis.com/cuenta") && txt.contains("\r\n"));
        assert!(!txt.contains("{nombre}"));
        assert!(z.by_name("beat.wav").is_ok());
        println!("{txt}");
    }
}

#[cfg(test)]
mod pagina {
    /// Necesita: python3 st.py
    #[tokio::test]
    #[ignore]
    async fn estados() {
        for (c, ok) in [(200, true), (403, true), (429, true), (404, false), (502, false)] {
            assert_eq!(super::page_exists(&format!("http://127.0.0.1:8766/{c}")).await, ok, "{c}");
        }
        assert!(!super::page_exists("http://127.0.0.1:1/").await);
    }
}
