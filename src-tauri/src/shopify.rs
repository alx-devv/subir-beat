//! Shopify Admin API (GraphQL). Token por "client credentials" (app del Dev Dashboard).

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const API: &str = "2026-10";

pub struct Shopify {
    domain: String,
    client_id: String,
    secret: String,
    http: reqwest::Client,
    token: Mutex<Option<(String, Instant)>>,
}

impl Shopify {
    pub fn new(domain: &str, client_id: &str, secret: &str, http: reqwest::Client) -> Result<Self> {
        if domain.is_empty() || client_id.is_empty() || secret.is_empty() {
            bail!("Faltan los datos de la app de Shopify en Ajustes");
        }
        Ok(Self {
            domain: domain.trim().trim_start_matches("https://").trim_end_matches('/').into(),
            client_id: client_id.trim().into(),
            secret: secret.trim().into(),
            http,
            token: Mutex::new(None),
        })
    }

    async fn token(&self) -> Result<String> {
        let mut t = self.token.lock().await;
        if let Some((tok, until)) = t.as_ref() {
            if Instant::now() < *until {
                return Ok(tok.clone());
            }
        }
        let r = self
            .http
            .post(format!("https://{}/admin/oauth/access_token", self.domain))
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", &self.client_id),
                ("client_secret", &self.secret),
            ])
            .send()
            .await
            .context("Sin conexión con Shopify")?;
        let status = r.status();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        let tok = v["access_token"]
            .as_str()
            .ok_or_else(|| anyhow!("Shopify rechazó las credenciales ({status}). Revisá el Client ID y el secreto en Ajustes."))?
            .to_string();
        let secs = v["expires_in"].as_u64().unwrap_or(86_400).saturating_sub(300);
        *t = Some((tok.clone(), Instant::now() + Duration::from_secs(secs)));
        Ok(tok)
    }

    pub async fn gql(&self, query: &str, variables: Value) -> Result<Value> {
        let tok = self.token().await?;
        let mut last = None;
        for i in 0..4u64 {
            let r = self
                .http
                .post(format!("https://{}/admin/api/{API}/graphql.json", self.domain))
                .header("X-Shopify-Access-Token", &tok)
                .json(&json!({ "query": query, "variables": variables }))
                .send()
                .await;
            match r {
                Ok(r) if r.status().as_u16() == 429 || r.status().is_server_error() => {
                    last = Some(anyhow!("Shopify respondió {}", r.status()))
                }
                Ok(r) => {
                    let status = r.status();
                    let v: Value = r.json().await.context("Respuesta inválida de Shopify")?;
                    if let Some(errs) = v.get("errors") {
                        bail!("Shopify ({status}): {}", errs);
                    }
                    return Ok(v["data"].clone());
                }
                Err(e) => last = Some(anyhow!(e)),
            }
            tokio::time::sleep(Duration::from_secs(2 * (i + 1))).await;
        }
        Err(last.unwrap())
    }

    pub async fn test(&self) -> Result<String> {
        let d = self.gql("{ shop { name } }", json!({})).await?;
        Ok(d["shop"]["name"].as_str().unwrap_or("").to_string())
    }

    pub async fn product_id_by_handle(&self, handle: &str) -> Result<Option<String>> {
        let d = self
            .gql(
                "query($h:String!){ productByIdentifier(identifier:{handle:$h}){ id } }",
                json!({ "h": handle }),
            )
            .await?;
        Ok(d["productByIdentifier"]["id"].as_str().map(String::from))
    }

    pub async fn frontpage_collection(&self) -> Result<Option<String>> {
        let d = self
            .gql(r#"{ collectionByIdentifier(identifier:{handle:"frontpage"}){ id } }"#, json!({}))
            .await?;
        Ok(d["collectionByIdentifier"]["id"].as_str().map(String::from))
    }

    /// Sube una imagen/gif a Shopify y devuelve la URL para usar como `originalSource`.
    pub async fn staged_upload(&self, file: &Path, mime: &str) -> Result<String> {
        let filename = file.file_name().unwrap().to_string_lossy().to_string();
        let bytes = tokio::fs::read(file).await?;
        let d = self
            .gql(
                "mutation($i:[StagedUploadInput!]!){ stagedUploadsCreate(input:$i){ stagedTargets{ url resourceUrl parameters{ name value } } userErrors{ field message } } }",
                json!({ "i": [{ "resource": "IMAGE", "filename": filename, "mimeType": mime, "httpMethod": "POST", "fileSize": bytes.len().to_string() }] }),
            )
            .await?;
        user_errors(&d["stagedUploadsCreate"])?;
        let t = &d["stagedUploadsCreate"]["stagedTargets"][0];
        let url = t["url"].as_str().ok_or_else(|| anyhow!("Shopify no dio destino de subida"))?;
        let mut form = reqwest::multipart::Form::new();
        for p in t["parameters"].as_array().into_iter().flatten() {
            form = form.text(
                p["name"].as_str().unwrap_or("").to_string(),
                p["value"].as_str().unwrap_or("").to_string(),
            );
        }
        form = form.part(
            "file",
            reqwest::multipart::Part::bytes(bytes).file_name(filename).mime_str(mime)?,
        );
        let r = self.http.post(url).multipart(form).send().await?;
        if !r.status().is_success() {
            bail!("No se pudo subir la imagen a Shopify: {}", r.status());
        }
        Ok(t["resourceUrl"].as_str().unwrap_or_default().to_string())
    }

    /// Crea (o completa, si se reintenta) el producto con variantes y metacampos.
    pub async fn product_set(&self, existing_id: Option<&str>, input: Value) -> Result<(String, String)> {
        let q = "mutation($id:ProductSetIdentifiers,$p:ProductSetInput!){ productSet(identifier:$id, input:$p, synchronous:true){ product{ id handle } userErrors{ field message } } }";
        let id = existing_id.map(|i| json!({ "id": i }));
        let d = self.gql(q, json!({ "id": id, "p": input })).await?;
        user_errors(&d["productSet"])?;
        let p = &d["productSet"]["product"];
        Ok((
            p["id"].as_str().ok_or_else(|| anyhow!("Shopify no devolvió el producto"))?.into(),
            p["handle"].as_str().unwrap_or_default().into(),
        ))
    }

    /// Publica en todos los canales (Tienda online, Facebook & Instagram, POS).
    pub async fn publish_everywhere(&self, product_id: &str) -> Result<usize> {
        let d = self.gql("{ publications(first:20){ nodes{ id } } }", json!({})).await?;
        let pubs: Vec<Value> = d["publications"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["id"].as_str())
            .map(|id| json!({ "publicationId": id }))
            .collect();
        if pubs.is_empty() {
            bail!("No se encontraron canales de venta");
        }
        let d = self
            .gql(
                "mutation($id:ID!,$i:[PublicationInput!]!){ publishablePublish(id:$id, input:$i){ userErrors{ field message } } }",
                json!({ "id": product_id, "i": pubs }),
            )
            .await?;
        user_errors(&d["publishablePublish"])?;
        Ok(pubs.len())
    }

    pub async fn verify(&self, product_id: &str) -> Result<(String, bool, i64)> {
        let d = self
            .gql(
                "query($id:ID!){ product(id:$id){ status onlineStoreUrl media(first:1){ nodes{ status } } variants(first:5){ nodes{ metafield(namespace:\"custom\", key:\"link_de_descarga_de_audio\"){ value } } } } }",
                json!({ "id": product_id }),
            )
            .await?;
        let p = &d["product"];
        let links = p["variants"]["nodes"]
            .as_array()
            .map(|v| v.iter().filter(|n| n["metafield"]["value"].is_string()).count() as i64)
            .unwrap_or(0);
        Ok((
            p["status"].as_str().unwrap_or("").into(),
            p["onlineStoreUrl"].is_string(),
            links,
        ))
    }
}

fn user_errors(v: &Value) -> Result<()> {
    if let Some(errs) = v["userErrors"].as_array() {
        if !errs.is_empty() {
            let msg: Vec<String> = errs
                .iter()
                .map(|e| e["message"].as_str().unwrap_or("error").to_string())
                .collect();
            bail!("Shopify: {}", msg.join(" · "));
        }
    }
    Ok(())
}
