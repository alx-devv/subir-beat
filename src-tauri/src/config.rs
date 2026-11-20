use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEYRING_SERVICE: &str = "subir-beat";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    // Shopify
    pub shop_domain: String,
    pub shop_client_id: String,
    pub store_url: String,
    pub vendor: String,
    pub price_premium: String,
    pub price_exclusiva: String,
    // R2
    pub r2_account_id: String,
    pub r2_access_key_id: String,
    pub r2_bucket: String,
    pub public_base: String,
    // Local
    pub output_dir: String,
    pub keep_zips: bool,
    // Web gif
    pub gif_size: u32,
    pub gif_fps: u32,
    pub gif_max_secs: u32,
    // YouTube
    pub yt_client_id: String,
    /// Nombre del canal, se completa con la primera subida.
    pub yt_channel: String,
    /// public | unlisted | scheduled
    pub yt_privacy: String,
    /// Google ya aprobó la auditoría: los videos respetan la visibilidad elegida.
    pub yt_audited: bool,
    pub yt_title: String,
    /// Título todo en minúscula.
    pub yt_title_lower: bool,
    // Mapa de artistas
    pub notion_db_id: String,
    pub yt_description: String,
    pub yt_tags: String,
    // Léeme.txt dentro de los zips
    pub downloads_url: String,
    pub readme_premium: String,
    pub readme_exclusiva: String,
}

impl Default for Settings {
    fn default() -> Self {
        let output_dir = dirs_videos().join("Beats subidos").to_string_lossy().to_string();
        Self {
            shop_domain: "pc1swd-9c.myshopify.com".into(),
            shop_client_id: String::new(),
            store_url: "https://alexgareis.com".into(),
            vendor: "Mi tienda".into(),
            price_premium: "49".into(),
            price_exclusiva: "125".into(),
            r2_account_id: String::new(),
            r2_access_key_id: String::new(),
            r2_bucket: "beats-entregables".into(),
            public_base: "https://dl.alexgareis.com".into(),
            output_dir,
            keep_zips: false,
            gif_size: 600,
            gif_fps: 15,
            gif_max_secs: 8,
            yt_client_id: String::new(),
            yt_channel: String::new(),
            yt_privacy: "public".into(),
            yt_audited: false,
            yt_title: DEFAULT_TITLE.into(),
            yt_title_lower: true,
            notion_db_id: "4b320e67-6e02-472b-9fe0-b63904794a12".into(),
            yt_description: DEFAULT_DESCRIPTION.into(),
            downloads_url: "https://alexgareis.com/pages/descargas".into(),
            readme_premium: README_PREMIUM.into(),
            readme_exclusiva: README_EXCLUSIVA.into(),
            yt_tags: "{artista} type beat, {artista} instrumental, {genero} type beat, {genero} beat, beat {mood}, instrumental {genero}, prod alex gareis, alex gareis".into(),
        }
    }
}

pub const DEFAULT_TITLE: &str = "{artista} type beat - \"{nombre}\" | {estilo}";
const OLD_TITLE: &str = "{artista} Type Beat - \"{nombre}\"";

const DEFAULT_DESCRIPTION: &str = "🎧 Licencia este beat: {link}

\"{nombre}\" · {bpm} BPM · {key}
Prod. Alex Gareis

Soy Alex, productor. Si grabas sobre este beat, mándame la canción: me encanta escuchar lo que hacen los artistas con mis beats.

💿 Licencias
• WAV Premium — ${premium} USD
• WAV + STEMS Exclusiva — ${exclusiva} USD (el beat sale de la tienda)

🎚️ Mezcla, mastering y beats a medida: {tienda}
📲 Instagram: https://instagram.com/alexgareis

⚠️ Este beat no es de uso libre. Para publicar una canción con él hace falta una licencia.

#typebeat #instrumental #{artista_tag}typebeat";

const README_PREMIUM: &str = "¡Gracias por tu compra de \"{nombre}\"! (WAV Premium)

Descarga tu licencia aquí → {descargas}
Se ingresa con el mismo email de la compra.

Esta licencia cubre hasta 500.000 reproducciones acumuladas.
Si la canción crece o se quiere el beat en exclusiva, se puede
pasar a la licencia Exclusiva: escríbeme y lo resolvemos.

Estoy a disposición para lo que haga falta :)
WhatsApp → wa.me/543436108116";

const README_EXCLUSIVA: &str = "¡Gracias por tu compra de \"{nombre}\"! (WAV + STEMS Exclusiva)

Descarga tu licencia aquí → {descargas}
Se ingresa con el mismo email de la compra.

Desde ahora este beat queda en exclusiva para tu proyecto:
ya salió de la tienda y nadie más puede comprarlo.
Los stems están en la carpeta STEMS.

Estoy a disposición para lo que haga falta :)
WhatsApp → wa.me/543436108116";

fn dirs_videos() -> PathBuf {
    if let Some(p) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(p).join("Videos");
    }
    if let Some(p) = std::env::var_os("HOME") {
        return PathBuf::from(p).join("Videos");
    }
    PathBuf::from(".")
}

pub fn settings_path(config_dir: &PathBuf) -> PathBuf {
    config_dir.join("ajustes.json")
}

pub fn load(config_dir: &PathBuf) -> Settings {
    let mut s: Settings = std::fs::read_to_string(settings_path(config_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    s.downloads_url = with_scheme(&s.downloads_url);
    if s.yt_title == OLD_TITLE {
        s.yt_title = DEFAULT_TITLE.into();
    }
    if s.yt_privacy == "scheduled" {
        s.yt_privacy = "public".into();
    }
    s
}

/// "alexgareis.com/pages/descargas" → "https://alexgareis.com/pages/descargas".
pub fn with_scheme(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() || u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else {
        format!("https://{u}")
    }
}

pub fn save(config_dir: &PathBuf, s: &Settings) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(settings_path(config_dir), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

// Secretos: guardados en el Administrador de credenciales de Windows.
pub const SECRET_SHOPIFY: &str = "shopify_client_secret";
pub const SECRET_R2: &str = "r2_secret_access_key";
pub const SECRET_YT_CLIENT: &str = "youtube_client_secret";
pub const SECRET_YT_REFRESH: &str = "youtube_refresh_token";
pub const SECRET_NOTION: &str = "notion_token";
pub const SECRET_YT_KEY: &str = "youtube_api_key";

pub fn get_secret(name: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, name)
        .ok()?
        .get_password()
        .ok()
        .filter(|s| !s.is_empty())
}

pub fn set_secret(name: &str, value: &str) -> anyhow::Result<()> {
    let e = keyring::Entry::new(KEYRING_SERVICE, name)?;
    if value.is_empty() {
        let _ = e.delete_credential();
    } else {
        e.set_password(value)?;
    }
    Ok(())
}
