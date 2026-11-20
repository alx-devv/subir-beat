//! YouTube: autorización OAuth de escritorio (loopback + PKCE) y subida reanudable.

use crate::config;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const SCOPE: &str = "https://www.googleapis.com/auth/youtube.upload";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const UPLOAD_URL: &str = "https://www.googleapis.com/upload/youtube/v3/videos";
const THUMB_URL: &str = "https://www.googleapis.com/upload/youtube/v3/thumbnails/set";
/// Múltiplo de 256 KiB, como pide YouTube.
const CHUNK: u64 = 8 * 1024 * 1024;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random_token() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("sin fuente de azar");
    b64(&b)
}

pub fn is_connected() -> bool {
    config::get_secret(config::SECRET_YT_REFRESH).is_some()
}

fn credentials(client_id: &str) -> Result<(String, String)> {
    let id = client_id.trim();
    let secret = config::get_secret(config::SECRET_YT_CLIENT).unwrap_or_default();
    if id.is_empty() || secret.is_empty() {
        bail!("Falta cargar el ID y el secreto de cliente de Google en Ajustes");
    }
    Ok((id.to_string(), secret))
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
    #[serde(default)]
    refresh_token: Option<String>,
}

fn google_error(v: &Value) -> String {
    v.pointer("/error/message")
        .or_else(|| v.get("error_description"))
        .or_else(|| v.get("error"))
        .and_then(|x| x.as_str())
        .unwrap_or("error desconocido")
        .to_string()
}

/// Abre el navegador para autorizar el canal y guarda el refresh token.
pub async fn connect(client_id: &str, open: impl Fn(String) -> Result<()>) -> Result<()> {
    let (id, secret) = credentials(client_id)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random_token();
    let challenge = b64(&Sha256::digest(verifier.as_bytes()));
    let state = random_token();

    let mut url = url::Url::parse(AUTH_URL)?;
    url.query_pairs_mut()
        .append_pair("client_id", &id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    open(url.to_string())?;

    let code = tokio::time::timeout(Duration::from_secs(300), wait_for_code(&listener, &state))
        .await
        .map_err(|_| anyhow!("Pasaron 5 minutos sin autorizar. Probá de nuevo."))??;

    let resp = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;
    let ok = resp.status().is_success();
    let v: Value = resp.json().await?;
    if !ok {
        bail!("Google rechazó la autorización: {}", google_error(&v));
    }
    let t: TokenResp = serde_json::from_value(v)?;
    let refresh = t.refresh_token.ok_or_else(|| anyhow!("Google no devolvió un token de acceso permanente"))?;
    config::set_secret(config::SECRET_YT_REFRESH, &refresh)?;
    Ok(())
}

async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await?;
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req.split_whitespace().nth(1).unwrap_or("/");
        let url = url::Url::parse(&format!("http://localhost{path}"))?;
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        if q.is_empty() {
            // El navegador a veces pide /favicon.ico.
            let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
            continue;
        }
        let (title, result) = if let Some(e) = q.get("error") {
            ("No se conectó", Err(anyhow!("No se autorizó el acceso ({e})")))
        } else if q.get("state").map(String::as_str) != Some(state) {
            ("No se conectó", Err(anyhow!("La respuesta de Google no coincide con el pedido. Probá de nuevo.")))
        } else if let Some(c) = q.get("code") {
            ("Listo, ya podés volver a Subir Beat", Ok(c.clone()))
        } else {
            ("No se conectó", Err(anyhow!("Google no devolvió el código de autorización")))
        };
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>Subir Beat</title>\
             <body style=\"font:16px system-ui;background:#FAF9F7;color:#1D1C1A;display:grid;place-items:center;height:90vh\">\
             <p>{title}</p></body>"
        );
        let _ = sock
            .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes())
            .await;
        return result;
    }
}

pub async fn disconnect() -> Result<()> {
    if let Some(t) = config::get_secret(config::SECRET_YT_REFRESH) {
        let _ = reqwest::Client::new().post(REVOKE_URL).form(&[("token", t.as_str())]).send().await;
    }
    config::set_secret(config::SECRET_YT_REFRESH, "")
}

/// Cliente con token de acceso que se renueva solo.
pub struct YouTube {
    http: reqwest::Client,
    id: String,
    secret: String,
    refresh: String,
    token: String,
    expires: Instant,
}

impl YouTube {
    pub async fn new(client_id: &str, http: reqwest::Client) -> Result<Self> {
        let (id, secret) = credentials(client_id)?;
        let refresh = config::get_secret(config::SECRET_YT_REFRESH)
            .ok_or_else(|| anyhow!("YouTube no está conectado. Conectalo en Ajustes."))?;
        let mut yt = Self { http, id, secret, refresh, token: String::new(), expires: Instant::now() };
        yt.renew().await?;
        Ok(yt)
    }

    async fn renew(&mut self) -> Result<()> {
        let resp = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("client_id", self.id.as_str()),
                ("client_secret", self.secret.as_str()),
                ("refresh_token", self.refresh.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        let ok = resp.status().is_success();
        let v: Value = resp.json().await?;
        if !ok {
            if v.get("error").and_then(|e| e.as_str()) == Some("invalid_grant") {
                bail!("Se venció la conexión con YouTube. Volvé a conectarla en Ajustes.");
            }
            bail!("No se pudo renovar el acceso a YouTube: {}", google_error(&v));
        }
        let t: TokenResp = serde_json::from_value(v)?;
        self.token = t.access_token;
        self.expires = Instant::now() + Duration::from_secs(t.expires_in.max(120) - 60);
        Ok(())
    }

    async fn bearer(&mut self) -> Result<String> {
        if Instant::now() >= self.expires {
            self.renew().await?;
        }
        Ok(format!("Bearer {}", self.token))
    }

    /// Abre una sesión de subida y devuelve su URL (sirve una semana para retomar).
    pub async fn start_upload(&mut self, meta: &Value, size: u64) -> Result<String> {
        let auth = self.bearer().await?;
        let resp = self
            .http
            .post(format!("{UPLOAD_URL}?uploadType=resumable&part=snippet,status"))
            .header("Authorization", auth)
            .header("X-Upload-Content-Length", size)
            .header("X-Upload-Content-Type", "video/mp4")
            .json(meta)
            .send()
            .await?;
        if !resp.status().is_success() {
            let v: Value = resp.json().await.unwrap_or_default();
            bail!("YouTube no aceptó el video: {}", explain(&v));
        }
        resp.headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .map(String::from)
            .ok_or_else(|| anyhow!("YouTube no devolvió la dirección de subida"))
    }

    /// Cuántos bytes ya tiene YouTube de una sesión. `Ok(None)` si la sesión venció.
    async fn uploaded(&mut self, session: &str, size: u64) -> Result<Option<Result<u64, Value>>> {
        let auth = self.bearer().await?;
        let resp = self
            .http
            .put(session)
            .header("Authorization", auth)
            .header("Content-Range", format!("bytes */{size}"))
            .header("Content-Length", 0)
            .send()
            .await?;
        match resp.status().as_u16() {
            200 | 201 => Ok(Some(Err(resp.json().await?))),
            308 => Ok(Some(Ok(range_end(&resp)))),
            404 | 410 => Ok(None),
            s => bail!("YouTube respondió {s} al retomar la subida"),
        }
    }

    /// Sube (o continúa) el archivo. Devuelve el recurso del video.
    pub async fn upload(&mut self, session: &str, file: &Path, on_bytes: &(dyn Fn(u64, u64) + Sync)) -> Result<Option<Value>> {
        let size = std::fs::metadata(file)?.len();
        let mut offset = match self.uploaded(session, size).await? {
            None => return Ok(None),
            Some(Err(video)) => return Ok(Some(video)),
            Some(Ok(n)) => n,
        };
        let mut f = std::fs::File::open(file).with_context(|| format!("No se pudo abrir {}", file.display()))?;
        let mut fails = 0;
        loop {
            on_bytes(offset, size);
            let end = (offset + CHUNK).min(size);
            let mut chunk = vec![0u8; (end - offset) as usize];
            f.seek(SeekFrom::Start(offset))?;
            f.read_exact(&mut chunk)?;
            let auth = self.bearer().await?;
            let sent = self
                .http
                .put(session)
                .header("Authorization", auth)
                .header("Content-Type", "video/mp4")
                .header("Content-Range", format!("bytes {offset}-{}/{size}", end - 1))
                .body(chunk)
                .timeout(Duration::from_secs(300))
                .send()
                .await;
            let resp = match sent {
                Ok(r) => r,
                Err(e) => {
                    fails += 1;
                    if fails > 5 {
                        return Err(e).context("Se cortó la conexión con YouTube");
                    }
                    tokio::time::sleep(Duration::from_secs(2u64.pow(fails))).await;
                    match self.uploaded(session, size).await? {
                        None => return Ok(None),
                        Some(Err(video)) => return Ok(Some(video)),
                        Some(Ok(n)) => { offset = n; continue; }
                    }
                }
            };
            match resp.status().as_u16() {
                200 | 201 => {
                    on_bytes(size, size);
                    return Ok(Some(resp.json().await?));
                }
                308 => {
                    fails = 0;
                    offset = range_end(&resp);
                }
                401 => self.renew().await?,
                404 | 410 => return Ok(None),
                s if s >= 500 => {
                    fails += 1;
                    if fails > 5 {
                        bail!("YouTube no responde ({s}). Probá retomar en un rato.");
                    }
                    tokio::time::sleep(Duration::from_secs(2u64.pow(fails))).await;
                    match self.uploaded(session, size).await? {
                        None => return Ok(None),
                        Some(Err(video)) => return Ok(Some(video)),
                        Some(Ok(n)) => offset = n,
                    }
                }
                _ => {
                    let v: Value = resp.json().await.unwrap_or_default();
                    bail!("YouTube cortó la subida: {}", explain(&v));
                }
            }
        }
    }

    pub async fn set_thumbnail(&mut self, video_id: &str, image: &Path) -> Result<()> {
        let mime = if crate::media::ext(image) == "png" { "image/png" } else { "image/jpeg" };
        let auth = self.bearer().await?;
        let resp = self
            .http
            .post(format!("{THUMB_URL}?videoId={video_id}"))
            .header("Authorization", auth)
            .header("Content-Type", mime)
            .body(std::fs::read(image)?)
            .send()
            .await?;
        if !resp.status().is_success() {
            let v: Value = resp.json().await.unwrap_or_default();
            bail!("{}", explain(&v));
        }
        Ok(())
    }
}

fn range_end(resp: &reqwest::Response) -> u64 {
    resp.headers()
        .get("range")
        .and_then(|h| h.to_str().ok())
        .and_then(|r| r.rsplit('-').next())
        .and_then(|n| n.parse::<u64>().ok())
        .map(|n| n + 1)
        .unwrap_or(0)
}

/// Mensajes de YouTube en criollo para los casos que más pasan.
fn explain(v: &Value) -> String {
    let reason = v.pointer("/error/errors/0/reason").and_then(|r| r.as_str()).unwrap_or("");
    match reason {
        "quotaExceeded" | "uploadLimitExceeded" => "se alcanzó el límite diario de subidas de la API. Probá retomar mañana.".into(),
        "forbidden" if google_error(v).contains("thumbnail") => "el canal todavía no puede usar miniaturas personalizadas (hay que verificarlo con el teléfono en YouTube).".into(),
        "invalidTitle" => "el título no es válido (máximo 100 caracteres, sin < ni >).".into(),
        "invalidDescription" => "la descripción no es válida (máximo 5000 caracteres, sin < ni >).".into(),
        "invalidTags" => "los tags superan el límite de YouTube.".into(),
        "invalidPublishAt" => "la fecha programada no es válida: tiene que ser en el futuro.".into(),
        _ => google_error(v),
    }
}

/// YouTube no acepta < ni > en títulos y descripciones.
pub fn clean_text(s: &str, max: usize) -> String {
    s.replace(['<', '>'], "").chars().take(max).collect()
}

/// Lista de tags que entra en los 500 caracteres (los que tienen espacios cuentan las comillas).
pub fn tag_list(tags: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut len = 0;
    for t in tags.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let cost = t.chars().count() + if t.contains(' ') { 2 } else { 0 } + if out.is_empty() { 0 } else { 1 };
        if len + cost > 500 { break; }
        len += cost;
        out.push(t.replace(['<', '>'], ""));
    }
    out
}

/// Metadatos del video para la API.
pub fn metadata(title: &str, description: &str, tags: &str, privacy: &str, publish_at: &str) -> Value {
    let mut status = json!({ "selfDeclaredMadeForKids": false, "embeddable": true, "license": "youtube" });
    match privacy {
        "scheduled" => {
            status["privacyStatus"] = json!("private");
            status["publishAt"] = json!(publish_at);
        }
        "unlisted" => status["privacyStatus"] = json!("unlisted"),
        "private" => status["privacyStatus"] = json!("private"),
        _ => status["privacyStatus"] = json!("public"),
    }
    json!({
        "snippet": {
            "title": clean_text(title, 100),
            "description": clean_text(description, 5000),
            "tags": tag_list(tags),
            "categoryId": "10",
            "defaultLanguage": "es",
            "defaultAudioLanguage": "es"
        },
        "status": status
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tags_y_metadatos() {
        let many = (0..80).map(|i| format!("tag numero {i}")).collect::<Vec<_>>().join(", ");
        let l = tag_list(&many);
        let total: usize = l.iter().map(|t| t.len() + 2).sum::<usize>() + l.len() - 1;
        assert!(total <= 500 && l.len() > 20);
        let m = metadata("a <b>", "x", "uno, dos tres", "scheduled", "2026-11-22T21:00:00Z");
        assert_eq!(m["snippet"]["title"], "a b");
        assert_eq!(m["status"]["privacyStatus"], "private");
        assert_eq!(m["status"]["publishAt"], "2026-11-22T21:00:00Z");
        assert_eq!(m["snippet"]["tags"][1], "dos tres");
        assert_eq!(metadata("a", "b", "", "unlisted", "")["status"]["privacyStatus"], "unlisted");
    }
}

#[cfg(test)]
mod mock {
    use super::*;
    /// Necesita el servidor de prueba: python3 yt.py 8765 <bytes>
    #[tokio::test]
    #[ignore]
    async fn subida_reanudable() {
        let f = std::env::temp_dir().join("yt_mock.bin");
        std::fs::write(&f, vec![7u8; 20 * 1024 * 1024 + 123]).unwrap();
        let mut yt = YouTube {
            http: reqwest::Client::new(), id: "i".into(), secret: "s".into(), refresh: "r".into(),
            token: "T".into(), expires: Instant::now() + Duration::from_secs(3600),
        };
        let seen = std::sync::Mutex::new(Vec::new());
        let v = yt.upload("http://127.0.0.1:8765/s", &f, &|d, t| seen.lock().unwrap().push((d, t))).await.unwrap().unwrap();
        assert_eq!(v["id"], "abc123");
        println!("{:?}", seen.lock().unwrap());
    }
}
