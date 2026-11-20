//! Subida a Cloudflare R2 (API S3) con partes en paralelo y verificación.

use anyhow::{anyhow, bail, Context, Result};
use futures::stream::{self, StreamExt, TryStreamExt};
use rusty_s3::actions::{
    CompleteMultipartUpload, CreateMultipartUpload, HeadObject, PutObject, UploadPart,
};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const PART: u64 = 16 * 1024 * 1024;
const PARALLEL: usize = 4;
const SIGN: Duration = Duration::from_secs(3600);

#[derive(Clone)]
pub struct R2 {
    bucket: Bucket,
    creds: Credentials,
    http: reqwest::Client,
}

impl R2 {
    pub fn new(account_id: &str, key_id: &str, secret: &str, bucket: &str, http: reqwest::Client) -> Result<Self> {
        if account_id.is_empty() || key_id.is_empty() || secret.is_empty() {
            bail!("Faltan las claves de R2 en Ajustes");
        }
        Self::with_endpoint(&format!("https://{account_id}.r2.cloudflarestorage.com"), key_id, secret, bucket, http)
    }

    pub fn with_endpoint(endpoint: &str, key_id: &str, secret: &str, bucket: &str, http: reqwest::Client) -> Result<Self> {
        let endpoint = endpoint.parse()?;
        let bucket = Bucket::new(endpoint, UrlStyle::Path, bucket.to_string(), "auto")
            .map_err(|e| anyhow!("Bucket inválido: {e}"))?;
        Ok(Self { bucket, creds: Credentials::new(key_id.to_string(), secret.to_string()), http })
    }

    /// Tamaño del objeto si existe.
    pub async fn head_size(&self, key: &str) -> Result<Option<u64>> {
        let url = HeadObject::new(&self.bucket, Some(&self.creds), key).sign(SIGN);
        let r = self.http.head(url).send().await?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !r.status().is_success() {
            bail!("R2 respondió {} al verificar {key}", r.status());
        }
        Ok(r.headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok()))
    }

    /// Sube un archivo. Si ya está con el mismo tamaño, no lo vuelve a subir.
    pub async fn upload(
        &self,
        file: &Path,
        key: &str,
        content_type: &str,
        on_bytes: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<()> {
        let size = tokio::fs::metadata(file).await?.len();
        if self.head_size(key).await? == Some(size) {
            on_bytes(size);
            return Ok(());
        }
        if size <= PART {
            let body = tokio::fs::read(file).await?;
            let url = PutObject::new(&self.bucket, Some(&self.creds), key).sign(SIGN);
            let r = self.with_retry(|| {
                self.http.put(url.clone()).header("content-type", content_type).body(body.clone()).send()
            }).await?;
            if !r.status().is_success() {
                bail!("R2 rechazó {key}: {}", r.status());
            }
            on_bytes(size);
        } else {
            self.multipart(file, key, size, content_type, Arc::new(on_bytes)).await?;
        }
        match self.head_size(key).await? {
            Some(s) if s == size => Ok(()),
            other => bail!("{key} quedó incompleto en R2 ({other:?} de {size} bytes)"),
        }
    }

    async fn multipart(
        &self,
        file: &Path,
        key: &str,
        size: u64,
        content_type: &str,
        on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
    ) -> Result<()> {
        let url = CreateMultipartUpload::new(&self.bucket, Some(&self.creds), key).sign(SIGN);
        let r = self.http.post(url).header("content-type", content_type).send().await?;
        if !r.status().is_success() {
            bail!("R2 no permitió iniciar la subida de {key}: {}", r.status());
        }
        let resp = CreateMultipartUpload::parse_response(r.text().await?)?;
        let upload_id = resp.upload_id().to_string();

        let parts = size.div_ceil(PART);
        let done = Arc::new(AtomicU64::new(0));
        let file = file.to_path_buf();
        let mut etags: Vec<(u16, String)> = stream::iter(1..=parts)
            .map(|n| {
                let (file, upload_id, done, on_bytes) =
                    (file.clone(), upload_id.clone(), done.clone(), on_bytes.clone());
                async move {
                    let offset = (n - 1) * PART;
                    let len = PART.min(size - offset) as usize;
                    let mut f = tokio::fs::File::open(&file).await?;
                    f.seek(std::io::SeekFrom::Start(offset)).await?;
                    let mut buf = vec![0u8; len];
                    f.read_exact(&mut buf).await?;
                    let url = UploadPart::new(&self.bucket, Some(&self.creds), key, n as u16, &upload_id).sign(SIGN);
                    let r = self.with_retry(|| self.http.put(url.clone()).body(buf.clone()).send()).await?;
                    if !r.status().is_success() {
                        bail!("Falló la parte {n} de {key}: {}", r.status());
                    }
                    let etag = r
                        .headers()
                        .get("etag")
                        .and_then(|v| v.to_str().ok())
                        .ok_or_else(|| anyhow!("R2 no devolvió ETag"))?
                        .to_string();
                    on_bytes(done.fetch_add(len as u64, Ordering::SeqCst) + len as u64);
                    Ok::<_, anyhow::Error>((n as u16, etag))
                }
            })
            .buffer_unordered(PARALLEL)
            .try_collect()
            .await?;
        etags.sort_by_key(|(n, _)| *n);

        let action = CompleteMultipartUpload::new(
            &self.bucket,
            Some(&self.creds),
            key,
            &upload_id,
            etags.iter().map(|(_, e)| e.as_str()),
        );
        let url = action.sign(SIGN);
        let body = action.body();
        let r = self.with_retry(|| self.http.post(url.clone()).body(body.clone()).send()).await?;
        let status = r.status();
        let text = r.text().await.unwrap_or_default();
        if !status.is_success() || text.contains("<Error>") {
            bail!("R2 no pudo cerrar la subida de {key}: {status}");
        }
        Ok(())
    }

    async fn with_retry<F, Fut>(&self, f: F) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    {
        let mut last = None;
        for i in 0..4u64 {
            match f().await {
                Ok(r) if r.status().is_server_error() => last = Some(anyhow!("R2 respondió {}", r.status())),
                Ok(r) => return Ok(r),
                Err(e) => last = Some(anyhow!(e)),
            }
            tokio::time::sleep(Duration::from_secs(2 * (i + 1))).await;
        }
        Err(last.unwrap()).context("Sin conexión con R2 después de varios intentos")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // S3_TEST_ENDPOINT=http://127.0.0.1:5055 cargo test r2 -- --nocapture
    #[tokio::test]
    async fn subida_real() {
        let Ok(ep) = std::env::var("S3_TEST_ENDPOINT") else { return };
        let http = reqwest::Client::new();
        let r2 = R2::with_endpoint(&ep, "test", "test", "beats-entregables", http.clone()).unwrap();
        let dir = std::env::temp_dir().join("r2test");
        std::fs::create_dir_all(&dir).unwrap();
        let small = dir.join("chico.mp3");
        std::fs::write(&small, vec![7u8; 300_000]).unwrap();
        let big = dir.join("grande.zip");
        std::fs::write(&big, (0..(40 * 1024 * 1024 + 123)).map(|i| (i % 251) as u8).collect::<Vec<u8>>()).unwrap();
        r2.upload(&small, "tsss!/@alexgareis_tsss!_wtag.mp3", "audio/mpeg", |_| {}).await.unwrap();
        let last = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let l2 = last.clone();
        r2.upload(&big, "intimate thoughts/@alexgareis_intimate-thoughts_exclusiva_92bpm_C#m.zip", "application/zip", move |b| { l2.store(b, std::sync::atomic::Ordering::SeqCst); }).await.unwrap();
        assert_eq!(last.load(std::sync::atomic::Ordering::SeqCst), 40 * 1024 * 1024 + 123);
        assert_eq!(r2.head_size("intimate thoughts/@alexgareis_intimate-thoughts_exclusiva_92bpm_C#m.zip").await.unwrap(), Some(40 * 1024 * 1024 + 123));
        // Segunda vez: ya está, no se vuelve a subir.
        r2.upload(&big, "intimate thoughts/@alexgareis_intimate-thoughts_exclusiva_92bpm_C#m.zip", "application/zip", |_| {}).await.unwrap();
        assert_eq!(r2.head_size("no-existe").await.unwrap(), None);
    }
}
