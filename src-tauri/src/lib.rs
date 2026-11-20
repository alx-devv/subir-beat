mod config;
mod job;
mod media;
mod naming;
mod r2;
mod shopify;

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
}

#[tauri::command]
fn get_settings(app: AppHandle) -> SettingsView {
    SettingsView {
        settings: config::load(&config_dir(&app)),
        has_shopify_secret: config::get_secret(config::SECRET_SHOPIFY).is_some(),
        has_r2_secret: config::get_secret(config::SECRET_R2).is_some(),
    }
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    settings: Settings,
    shopify_secret: Option<String>,
    r2_secret: Option<String>,
) -> Res<()> {
    config::save(&config_dir(&app), &settings).map_err(err)?;
    if let Some(s) = shopify_secret.filter(|s| !s.is_empty()) {
        config::set_secret(config::SECRET_SHOPIFY, s.trim()).map_err(err)?;
    }
    if let Some(s) = r2_secret.filter(|s| !s.is_empty()) {
        config::set_secret(config::SECRET_R2, s.trim()).map_err(err)?;
    }
    Ok(())
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
    job::validate(&input).map_err(err)?;
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
}

/// Vista previa de nombres y título mientras se completa el formulario.
#[tauri::command]
fn preview(app: AppHandle, input: BeatInput) -> Preview {
    let s = config::load(&config_dir(&app));
    let n = naming::names(&input.name, input.bpm, &input.key);
    Preview {
        handle: naming::handle(&input.name),
        folder: n.folder,
        mp3: n.mp3,
        premium: n.premium,
        exclusiva: n.exclusiva,
        title: job::fill(&s.yt_title, &input, "", &s.store_url),
    }
}

#[tauri::command]
fn start_job(app: AppHandle, input: BeatInput) -> Res<String> {
    job::validate(&input).map_err(err)?;
    let settings = config::load(&config_dir(&app));
    let j = job::new_job(input, &settings);
    if let Some(prev) = job::load_job(&data_dir(&app), &j.handle) {
        if prev.running {
            return Err("Ese beat ya se está subiendo".into());
        }
        if prev.steps.values().all(|s| s.status == "listo") {
            return Err("Ese beat ya se subió completo. Está en el historial.".into());
        }
        if !prev.product_id.is_empty() {
            return Err("Ese beat quedó a medias: retomalo desde el historial.".into());
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
            path_info
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar la app");
}
