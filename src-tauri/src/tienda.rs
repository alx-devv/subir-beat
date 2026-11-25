//! La tienda (alexgareis.com en Cloudflare): API privada /api/admin con la clave de la app beatstore.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

pub struct Consulta {
    pub existe: bool,
    pub premium: Option<f64>,
    pub exclusiva: Option<f64>,
}

/// Precio para los textos: 49 → "49", 49.5 → "49.50".
pub fn precio_texto(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v:.2}") }
}

pub struct Tienda {
    base: String,
    clave: String,
    http: reqwest::Client,
}

impl Tienda {
    pub fn new(store_url: &str, clave: Option<String>, http: reqwest::Client) -> Result<Self> {
        let clave = clave.filter(|c| !c.trim().is_empty()).ok_or_else(|| {
            anyhow!("Falta la clave de la tienda: cargala en Ajustes → Tienda (o tocá \"Traer de la app beatstore\").")
        })?;
        Ok(Self { base: store_url.trim_end_matches('/').to_string(), clave: clave.trim().to_string(), http })
    }

    async fn llamar(&self, metodo: reqwest::Method, ruta: &str, cuerpo: Option<Value>) -> Result<(u16, Value)> {
        let url = format!("{}/api/admin/{}", self.base, ruta);
        let mut req = self.http.request(metodo, &url).bearer_auth(&self.clave).timeout(std::time::Duration::from_secs(60));
        if let Some(b) = cuerpo {
            req = req.json(&b);
        }
        let r = req.send().await.with_context(|| format!("Sin conexión con la tienda ({})", self.base))?;
        let status = r.status().as_u16();
        let v: Value = r.json().await.unwrap_or(json!({}));
        if status == 401 {
            bail!("La clave de la tienda no es válida. Revisala en Ajustes → Tienda.");
        }
        Ok((status, v))
    }

    fn error(v: &Value, status: u16) -> anyhow::Error {
        anyhow!("{}", v.get("error").and_then(|e| e.as_str()).map(String::from).unwrap_or_else(|| format!("La tienda respondió {status}")))
    }

    /// Prueba la conexión (y que la clave sea válida).
    pub async fn probar(&self) -> Result<()> {
        let (s, v) = self.llamar(reqwest::Method::GET, "estado", None).await?;
        if s >= 300 {
            return Err(Self::error(&v, s));
        }
        Ok(())
    }

    /// ¿Ya hay un beat con esa dirección (publicado o en el borrador)? Trae también los precios de las licencias.
    pub async fn consultar(&self, slug: &str) -> Result<Consulta> {
        let (s, v) = self.llamar(reqwest::Method::GET, &format!("beats/{slug}"), None).await?;
        if s >= 300 {
            return Err(Self::error(&v, s));
        }
        let lic = v.get("licencias").and_then(|l| l.as_array()).cloned().unwrap_or_default();
        let precio = |excl: bool| {
            lic.iter()
                .find(|l| l.get("is_exclusive").and_then(|x| x.as_i64().or(x.as_bool().map(|b| b as i64))).unwrap_or(0) == excl as i64)
                .and_then(|l| l.get("price_usd").and_then(|p| p.as_f64()))
        };
        Ok(Consulta {
            existe: v.get("existe").and_then(|x| x.as_bool()).unwrap_or(false),
            premium: precio(false),
            exclusiva: precio(true),
        })
    }

    /// Da de alta el beat: queda publicado al instante y sumado al borrador. Devuelve la dirección del beat.
    pub async fn alta(&self, beat: Value, reemplazar: bool) -> Result<String> {
        let (s, v) = self.llamar(reqwest::Method::POST, "beats", Some(json!({ "beat": beat, "reemplazar": reemplazar }))).await?;
        if s >= 300 {
            return Err(Self::error(&v, s));
        }
        Ok(v.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string())
    }
}

#[cfg(test)]
mod pruebas {
    /// Necesita un servidor de prueba en 127.0.0.1:8767 (ver la sesión de desarrollo).
    #[tokio::test]
    #[ignore]
    async fn consulta_y_alta() {
        let t = super::Tienda::new("http://127.0.0.1:8767", Some("test".into()), reqwest::Client::new()).unwrap();
        let c = t.consultar("cheese").await.unwrap();
        assert!(c.existe);
        assert_eq!(c.premium, Some(49.0));
        assert_eq!(c.exclusiva, Some(150.0));
        assert_eq!(super::precio_texto(49.5), "49.50");
        let url = t.alta(serde_json::json!({ "slug": "nuevo", "title": "Nuevo" }), false).await.unwrap();
        assert_eq!(url, "https://alexgareis.com/beats/nuevo");
        let e = t.alta(serde_json::json!({ "slug": "cheese", "title": "cheese" }), false).await.unwrap_err();
        assert!(e.to_string().contains("Ya existe"), "{e}");
        let mala = super::Tienda::new("http://127.0.0.1:8767", Some("otra".into()), reqwest::Client::new()).unwrap();
        assert!(mala.probar().await.unwrap_err().to_string().contains("no es válida"));
    }
}
