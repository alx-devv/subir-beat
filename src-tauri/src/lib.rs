mod analysis;
mod config;
mod job;
mod media;
mod naming;
mod r2;
mod shopify;
mod youtube;

use config::Settings;
use job::{BeatInput, Job};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn config_dir(app: &AppHandle) -> PathBuf {
    app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[derive(Serialize)]
struct SettingsView {
    settings: Settings,
    has_shopify_secret: bool,
    has_r2_secret: bool,
    has_yt_secret: bool,
    yt_connected: bool,
    version: String,
}

#[tauri::command]
fn get_settings(app: AppHandle) -> SettingsView {
    SettingsView {
        settings: config::load(&config_dir(&app)),
        has_shopify_secret: config::get_secret(config::SECRET_SHOPIFY).is_some(),
        has_r2_secret: config::get_secret(config::SECRET_R2).is_some(),
        has_yt_secret: config::get_secret(config::SECRET_YT_CLIENT).is_some(),
        yt_connected: youtube::is_connected(),
        version: app.package_info().version.to_string(),
    }
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    settings: Settings,
    shopify_secret: Option<String>,
    r2_secret: Option<String>,
    yt_secret: Option<String>,
) -> Res<()> {
    config::save(&config_dir(&app), &settings).map_err(err)?;
    if let Some(s) = shopify_secret.filter(|s| !s.is_empty()) {
        config::set_secret(config::SECRET_SHOPIFY, s.trim()).map_err(err)?;
    }
    if let Some(s) = r2_secret.filter(|s| !s.is_empty()) {
        config::set_secret(config::SECRET_R2, s.trim()).map_err(err)?;
    }
    if let Some(s) = yt_secret.filter(|s| !s.is_empty()) {
        config::set_secret(config::SECRET_YT_CLIENT, s.trim()).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
async fn yt_connect(app: AppHandle) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    let s = config::load(&config_dir(&app));
    let a = app.clone();
    youtube::connect(&s.yt_client_id, move |url| {
        a.opener().open_url(url, None::<&str>).map_err(|e| anyhow::anyhow!("No se pudo abrir el navegador: {e}"))
    })
    .await
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn yt_disconnect(app: AppHandle) -> Res<()> {
    youtube::disconnect().await.map_err(err)?;
    let c = config_dir(&app);
    let mut s = config::load(&c);
    s.yt_channel.clear();
    config::save(&c, &s).map_err(err)
}

/// Sube a YouTube un beat que quedó para subir a mano (por ejemplo, de antes de conectar el canal).
#[tauri::command]
fn upload_youtube(app: AppHandle, handle: String) -> Res<()> {
    if !youtube::is_connected() {
        return Err("Primero conectá YouTube en Ajustes".into());
    }
    let mut j = job::load_job(&data_dir(&app), &handle).ok_or("No se encontró el trabajo")?;
    if j.running {
        return Err("Ese beat se está subiendo".into());
    }
    if let Some(st) = j.steps.get_mut("youtube") {
        *st = job::StepState { status: "pendiente".into(), ..Default::default() };
    }
    spawn(app, j);
    Ok(())
}

#[derive(Serialize)]
struct UpdateInfo {
    version: String,
    notes: String,
}

#[tauri::command]
async fn check_update(app: AppHandle) -> Res<Option<UpdateInfo>> {
    use tauri_plugin_updater::UpdaterExt;
    let u = app.updater().map_err(err)?.check().await.map_err(|e| format!("No se pudo buscar actualizaciones: {e}"))?;
    Ok(u.map(|u| UpdateInfo { version: u.version.clone(), notes: u.body.clone().unwrap_or_default() }))
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Res<()> {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;
    let data = data_dir(&app);
    if job::list_jobs(&data).iter().any(|j| j.running) {
        return Err("Hay un beat subiéndose. Esperá a que termine para actualizar.".into());
    }
    let u = app
        .updater()
        .map_err(err)?
        .check()
        .await
        .map_err(err)?
        .ok_or("Ya tenés la última versión")?;
    let a = app.clone();
    let mut got = 0usize;
    u.download_and_install(
        move |chunk, total| {
            got += chunk;
            let p = total.map(|t| got as f64 / t.max(1) as f64).unwrap_or(0.0);
            let _ = a.emit("actualizacion", p);
        },
        || {},
    )
    .await
    .map_err(|e| format!("No se pudo instalar la actualización: {e}"))?;
    app.restart();
}

#[derive(Serialize)]
struct Check {
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
struct Checks {
    shopify: Check,
    r2: Check,
    ffmpeg: Check,
}

#[tauri::command]
async fn test_connections(app: AppHandle) -> Checks {
    let s = config::load(&config_dir(&app));
    let http = reqwest::Client::new();

    let shopify = match shopify::Shopify::new(
        &s.shop_domain,
        &s.shop_client_id,
        &config::get_secret(config::SECRET_SHOPIFY).unwrap_or_default(),
        http.clone(),
    ) {
        Ok(sh) => match sh.test().await {
            Ok(name) => Check { ok: true, detail: format!("Conectado a {name}") },
            Err(e) => Check { ok: false, detail: format!("{e:#}") },
        },
        Err(e) => Check { ok: false, detail: e.to_string() },
    };

    let r2 = match r2::R2::new(
        &s.r2_account_id,
        &s.r2_access_key_id,
        &config::get_secret(config::SECRET_R2).unwrap_or_default(),
        &s.r2_bucket,
        http,
    ) {
        Ok(b) => match b.head_size("__prueba_de_conexion__").await {
            Ok(_) => Check { ok: true, detail: format!("Acceso a {}", s.r2_bucket) },
            Err(e) => Check { ok: false, detail: format!("{e:#}") },
        },
        Err(e) => Check { ok: false, detail: e.to_string() },
    };

    let ffmpeg = match tokio::process::Command::new(media::ffmpeg_path()).arg("-version").output().await {
        Ok(o) if o.status.success() => Check { ok: true, detail: "ffmpeg listo".into() },
        _ => Check { ok: false, detail: "No se encontró ffmpeg".into() },
    };

    Checks { shopify, r2, ffmpeg }
}

#[tauri::command]
fn validate_input(input: BeatInput) -> Res<String> {
    job::validate_new(&input).map_err(err)?;
    Ok(naming::handle(&input.name))
}

#[derive(Serialize)]
struct Preview {
    handle: String,
    folder: String,
    mp3: String,
    premium: String,
    exclusiva: String,
    title: String,
    description: String,
    tags: String,
    product_url: String,
}

/// Vista previa de nombres y título mientras se completa el formulario.
#[tauri::command]
fn preview(app: AppHandle, input: BeatInput) -> Preview {
    let s = config::load(&config_dir(&app));
    let n = naming::names(&input.name, input.bpm, &input.key);
    let handle = naming::handle(&input.name);
    let product_url = format!("{}/products/{}", s.store_url.trim_end_matches('/'), handle);
    let store = s.store_url.trim_end_matches('/');
    Preview {
        folder: n.folder,
        mp3: n.mp3,
        premium: n.premium,
        exclusiva: n.exclusiva,
        title: job::fill(&s.yt_title, &input, &product_url, store),
        description: job::fill(&s.yt_description, &input, &product_url, store),
        tags: job::clean_tags(&job::fill(&s.yt_tags, &input, &product_url, store)),
        product_url,
        handle,
    }
}

#[derive(Serialize)]
struct Preflight {
    /// Ya hay un producto con ese nombre en la tienda.
    exists: bool,
    product_url: String,
}

/// Todo lo que puede fallar se revisa antes de empezar, para no enterarse en el último paso.
#[tauri::command]
async fn preflight(app: AppHandle, input: BeatInput) -> Res<Preflight> {
    job::validate_new(&input).map_err(err)?;
    let settings = config::load(&config_dir(&app));
    let handle = naming::handle(&input.name);
    let http = reqwest::Client::new();
    let shop = shopify::Shopify::new(
        &settings.shop_domain,
        &settings.shop_client_id,
        &config::get_secret(config::SECRET_SHOPIFY).unwrap_or_default(),
        http,
    )
    .map_err(err)?;
    let exists = shop
        .product_id_by_handle(&handle)
        .await
        .map_err(|e| format!("No se pudo conectar con Shopify: {e:#}. Revisá Ajustes → Probar conexiones."))?
        .is_some();
    if settings.r2_account_id.trim().is_empty() || config::get_secret(config::SECRET_R2).is_none() {
        return Err("Falta configurar el bucket (Cloudflare R2) en Ajustes".into());
    }
    let url = settings.downloads_url.trim();
    if !url.is_empty() && !job::page_exists(url).await {
        return Err(format!("La página de descargas ({url}) no responde. Revisala en Ajustes."));
    }
    let store = settings.store_url.trim_end_matches('/');
    Ok(Preflight { exists, product_url: format!("{store}/products/{handle}") })
}

/// Fotogramas para elegir la miniatura de YouTube.
#[tauri::command]
async fn thumb_candidates(app: AppHandle, visual: String, seed: u32) -> Res<Vec<String>> {
    let dir = app.path().app_cache_dir().map_err(err)?.join("miniaturas");
    // Las tandas viejas no hacen falta.
    if seed == 0 {
        let _ = std::fs::remove_dir_all(&dir);
    }
    media::thumb_candidates(std::path::Path::new(&visual), &dir, seed)
        .await
        .map(|v| v.into_iter().map(|p| p.to_string_lossy().to_string()).collect())
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn start_job(app: AppHandle, input: BeatInput) -> Res<String> {
    job::validate_new(&input).map_err(err)?;
    let settings = config::load(&config_dir(&app));
    let j = job::new_job(input, &settings);
    if let Some(prev) = job::load_job(&data_dir(&app), &j.handle) {
        if prev.running {
            return Err("Ese beat ya se está subiendo".into());
        }
        // Si quedó uno a medias con el mismo nombre y no se pidió reemplazar, se retoma desde ahí.
        if !prev.product_id.is_empty() && !j.input.replace && !prev.steps.values().all(|s| s.status == "listo") {
            return Err("Ese beat quedó a medias: abrilo en \"Sin terminar\" y tocá Retomar.".into());
        }
    }
    spawn(app, j.clone());
    Ok(j.handle)
}

#[tauri::command]
fn retry_job(app: AppHandle, handle: String) -> Res<()> {
    let j = job::load_job(&data_dir(&app), &handle).ok_or("No se encontró el trabajo")?;
    spawn(app, j);
    Ok(())
}

fn spawn(app: AppHandle, j: Job) {
    let (d, c) = (data_dir(&app), config_dir(&app));
    tauri::async_runtime::spawn(async move {
        let _ = job::run(app, d, c, j).await;
    });
}

#[derive(Serialize)]
struct Detected {
    from_name: analysis::FromName,
    audio: Option<analysis::FromAudio>,
    audio_error: Option<String>,
}

/// Lee nombre, BPM y tonalidad del nombre del WAV y analiza el audio igual, para comparar.
#[tauri::command]
async fn detect_wav(path: String) -> Detected {
    let p = PathBuf::from(&path);
    let from_name = analysis::from_filename(&p);
    match analysis::analyze(&p).await {
        Ok(a) => Detected { from_name, audio: Some(a), audio_error: None },
        Err(e) => Detected { from_name, audio: None, audio_error: Some(format!("{e:#}")) },
    }
}

/// Duración en segundos (para la vista previa de YouTube).
#[tauri::command]
async fn media_duration(path: String) -> Option<f64> {
    media::duration_secs(std::path::Path::new(&path)).await.ok()
}

#[derive(Serialize)]
struct PathInfo {
    path: String,
    is_dir: bool,
    ext: String,
    size: u64,
    name: String,
    width: u32,
    height: u32,
    wav_count: usize,
}

/// Datos de lo que se arrastra a la ventana, para ubicarlo en el lugar correcto.
#[tauri::command]
fn path_info(paths: Vec<String>) -> Vec<PathInfo> {
    paths
        .into_iter()
        .map(|p| {
            let path = std::path::Path::new(&p);
            let meta = std::fs::metadata(path).ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let (width, height) = imagesize::size(path).map(|s| (s.width as u32, s.height as u32)).unwrap_or((0, 0));
            let wav_count = if is_dir {
                std::fs::read_dir(path)
                    .map(|d| d.flatten().filter(|e| media::ext(&e.path()) == "wav").count())
                    .unwrap_or(0)
            } else {
                0
            };
            PathInfo {
                ext: media::ext(path),
                size: meta.map(|m| m.len()).unwrap_or(0),
                name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                path: p,
                is_dir,
                width,
                height,
                wav_count,
            }
        })
        .collect()
}

#[tauri::command]
fn list_jobs(app: AppHandle) -> Vec<Job> {
    job::list_jobs(&data_dir(&app))
}

#[tauri::command]
fn forget_job(app: AppHandle, handle: String) -> Res<()> {
    let p = job::jobs_dir(&data_dir(&app)).join(format!("{handle}.json"));
    std::fs::remove_file(p).map_err(err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Si la app se cerró en medio de una subida, el trabajo queda para retomar.
            let d = data_dir(app.handle());
            for mut j in job::list_jobs(&d) {
                if j.running {
                    j.running = false;
                    j.error = "La app se cerró durante la subida. Podés retomarla.".into();
                    let _ = std::fs::write(
                        job::jobs_dir(&d).join(format!("{}.json", j.handle)),
                        serde_json::to_string_pretty(&j).unwrap_or_default(),
                    );
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            test_connections,
            validate_input,
            preview,
            start_job,
            retry_job,
            list_jobs,
            forget_job,
            path_info,
            detect_wav,
            media_duration,
            yt_connect,
            yt_disconnect,
            upload_youtube,
            check_update,
            install_update,
            preflight,
            thumb_candidates
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar la app");
}
