//! Mapa de artistas: lista editable para elegir a quién nombrar en cada type beat.
//! Se guarda en la carpeta de datos de la app; arranca con el mapa investigado (seed/artistas.json).

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

const SEED: &str = include_str!("../seed/artistas.json");

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Opportunity {
    /// 3 = buena oportunidad, 2 = se puede, 1 = saturado o sin demanda.
    pub level: u8,
    pub label: String,
    pub why: String,
    /// Segundos desde 1970.
    pub measured: u64,
    pub recent30: u32,
    pub big_share: f32,
    pub demand: bool,
    pub median_views: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    /// MX | US | otro
    pub country: String,
    pub city: String,
    /// rnb | rap | trap | otro
    pub group: String,
    pub style: String,
    pub vibe: String,
    pub bpm_min: u32,
    pub bpm_max: u32,
    pub level: String,
    /// Pedidos de la landing que lo mencionan, y de qué país.
    pub requests: u32,
    pub requests_mx: u32,
    pub requests_us: u32,
    /// Lo sumó la lectura de Notion o está sin revisar.
    pub new: bool,
    pub opp: Option<Opportunity>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Request {
    pub date: String,
    pub country: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Store {
    pub artists: Vec<Artist>,
    /// Nombres quitados a mano: la lectura de Notion no los vuelve a sumar.
    pub discarded: Vec<Artist>,
    /// Pedidos con descripción que no nombran a ningún artista del mapa.
    pub unmatched: Vec<Request>,
    pub synced: u64,
    pub total_requests: u32,
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("artistas.json")
}

pub fn load(data_dir: &Path) -> Store {
    if let Some(s) = std::fs::read_to_string(path(data_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        return s;
    }
    #[derive(Deserialize)]
    struct Seed {
        artists: Vec<Artist>,
    }
    let seed: Seed = serde_json::from_str(SEED).expect("mapa inicial inválido");
    Store { artists: seed.artists, ..Default::default() }
}

pub fn save(data_dir: &Path, s: &Store) -> Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let tmp = path(data_dir).with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(tmp, path(data_dir))?;
    Ok(())
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in fold(s).chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Minúsculas y sin acentos, para comparar nombres.
pub fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// ¿El texto nombra al artista? Compara palabras completas, sin acentos ni mayúsculas.
pub fn mentions(text: &str, a: &Artist) -> bool {
    let t = format!(" {} ", fold(text).replace(|c: char| !c.is_alphanumeric(), " "));
    std::iter::once(&a.name).chain(a.aliases.iter()).any(|n| {
        let n = fold(n).replace(|c: char| !c.is_alphanumeric(), " ");
        let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
        n.len() >= 2 && t.contains(&format!(" {n} "))
    })
}

/// País por el teléfono: +52 México, +1 EE. UU.; 10 dígitos sin código se toman como México.
pub fn country_from_phone(contact: &str) -> String {
    let d: String = contact.chars().filter(|c| c.is_ascii_digit()).collect();
    let plus = contact.trim_start().starts_with('+');
    if d.starts_with("52") && (plus || d.len() >= 12) {
        "MX".into()
    } else if d.starts_with('1') && d.len() == 11 {
        "US".into()
    } else if d.len() == 10 && !plus {
        "MX".into()
    } else if d.is_empty() {
        "otro".into()
    } else {
        "otro".into()
    }
}

// ---------- Notion ----------

fn plain(prop: &Value) -> String {
    let arr = prop.get("rich_text").or_else(|| prop.get("title")).and_then(|v| v.as_array());
    arr.map(|a| a.iter().filter_map(|t| t["plain_text"].as_str()).collect::<String>()).unwrap_or_default()
}

/// Lee todos los pedidos de la base de Notion.
pub async fn fetch_requests(token: &str, database_id: &str) -> Result<Vec<Request>> {
    let http = reqwest::Client::new();
    let id = database_id.trim().replace('-', "");
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut body = json!({ "page_size": 100 });
        if let Some(c) = &cursor {
            body["start_cursor"] = json!(c);
        }
        let resp = http
            .post(format!("https://api.notion.com/v1/databases/{id}/query"))
            .bearer_auth(token.trim())
            .header("Notion-Version", "2022-06-28")
            .json(&body)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .context("No se pudo conectar con Notion")?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or_default();
        if !status.is_success() {
            let msg = v["message"].as_str().unwrap_or("error desconocido");
            if status.as_u16() == 404 || status.as_u16() == 403 {
                bail!("Notion no deja leer la base. Revisá que la integración esté conectada a \"Contactos de la landing\" ({msg})");
            }
            if status.as_u16() == 401 {
                bail!("La clave de Notion no es válida. Revisala en Ajustes.");
            }
            bail!("Notion respondió: {msg}");
        }
        for page in v["results"].as_array().cloned().unwrap_or_default() {
            let p = &page["properties"];
            out.push(Request {
                date: page["created_time"].as_str().unwrap_or("").chars().take(10).collect(),
                country: country_from_phone(&plain(&p["Contacto"])),
                text: plain(&p["Descripción"]).trim().to_string(),
            });
        }
        if v["has_more"].as_bool() == Some(true) {
            cursor = v["next_cursor"].as_str().map(String::from);
        } else {
            break;
        }
    }
    Ok(out)
}

/// Cruza los pedidos con el mapa: cuenta menciones por artista y guarda los pedidos sin artista reconocido.
pub fn apply_requests(store: &mut Store, requests: &[Request], now: u64) {
    for a in store.artists.iter_mut() {
        a.requests = 0;
        a.requests_mx = 0;
        a.requests_us = 0;
    }
    store.unmatched.clear();
    for r in requests {
        if r.text.is_empty() {
            continue;
        }
        let mut hit = false;
        for a in store.artists.iter_mut() {
            if mentions(&r.text, a) {
                hit = true;
                a.requests += 1;
                match r.country.as_str() {
                    "MX" => a.requests_mx += 1,
                    "US" => a.requests_us += 1,
                    _ => {}
                }
            }
        }
        // Un pedido que solo nombra a alguien que quitaste no vuelve como "sin artista".
        let discarded = store.discarded.iter().any(|a| mentions(&r.text, a));
        if !hit && !discarded {
            store.unmatched.push(r.clone());
        }
    }
    store.total_requests = requests.len() as u32;
    store.synced = now;
}

// ---------- YouTube: oportunidad ----------

async fn get_json(http: &reqwest::Client, url: &str, query: &[(&str, String)]) -> Result<Value> {
    let resp = http.get(url).query(query).timeout(Duration::from_secs(20)).send().await?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        let reason = v.pointer("/error/errors/0/reason").and_then(|r| r.as_str()).unwrap_or("");
        if reason == "quotaExceeded" || reason == "dailyLimitExceeded" {
            bail!("CUOTA: se terminó la cuota diaria de búsquedas de YouTube. Mañana se puede seguir.");
        }
        let msg = v.pointer("/error/message").and_then(|m| m.as_str()).unwrap_or("error desconocido");
        bail!("YouTube respondió: {msg}");
    }
    Ok(v)
}

/// ¿El autocompletado de YouTube sugiere "<artista> type beat"? Es la señal de que la gente lo busca.
async fn suggested(http: &reqwest::Client, artist: &str) -> bool {
    let q = format!("{} type", fold(artist));
    let r = http
        .get("https://suggestqueries.google.com/complete/search")
        .query(&[("client", "firefox"), ("ds", "yt"), ("q", q.as_str())])
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    let Ok(r) = r else { return false };
    let Ok(v) = r.json::<Value>().await else { return false };
    let want = format!("{} type beat", fold(artist));
    v.get(1).and_then(|s| s.as_array()).map(|s| s.iter().filter_map(|x| x.as_str()).any(|x| fold(x).starts_with(&want))).unwrap_or(false)
}

/// Mide la oportunidad de un artista: demanda (autocompletado y vistas recientes) contra competencia
/// (subidas del último mes y canales grandes entre los primeros resultados). Gasta ~200 unidades de cuota.
pub async fn measure(api_key: &str, artist: &str, now: u64) -> Result<Opportunity> {
    let key = api_key.trim();
    if key.is_empty() {
        bail!("Falta la clave de búsqueda de YouTube en Ajustes");
    }
    let http = reqwest::Client::new();
    let q = format!("{artist} type beat");
    let base = "https://www.googleapis.com/youtube/v3";
    let since = rfc3339(now.saturating_sub(30 * 86400));

    let top = get_json(&http, &format!("{base}/search"), &[("part", "snippet".into()), ("type", "video".into()), ("q", q.clone()), ("maxResults", "20".into()), ("key", key.into())]).await?;
    let recent = get_json(&http, &format!("{base}/search"), &[("part", "id".into()), ("type", "video".into()), ("q", q.clone()), ("maxResults", "50".into()), ("publishedAfter", since), ("key", key.into())]).await?;

    let items = top["items"].as_array().cloned().unwrap_or_default();
    let video_ids: Vec<String> = items.iter().filter_map(|i| i.pointer("/id/videoId").and_then(|v| v.as_str()).map(String::from)).collect();
    let channel_ids: Vec<String> = items.iter().take(10).filter_map(|i| i.pointer("/snippet/channelId").and_then(|v| v.as_str()).map(String::from)).collect();
    let recent30 = recent["items"].as_array().map(|a| a.len()).unwrap_or(0) as u32
        + if recent["nextPageToken"].is_string() { 50 } else { 0 };

    // Vistas de los videos de los últimos 90 días entre los primeros 20.
    let mut views_recent: Vec<u64> = Vec::new();
    if !video_ids.is_empty() {
        let v = get_json(&http, &format!("{base}/videos"), &[("part", "statistics,snippet".into()), ("id", video_ids.join(",")), ("key", key.into())]).await?;
        let limit = rfc3339(now.saturating_sub(90 * 86400));
        for it in v["items"].as_array().cloned().unwrap_or_default() {
            let published = it.pointer("/snippet/publishedAt").and_then(|p| p.as_str()).unwrap_or("");
            if published >= limit.as_str() {
                views_recent.push(it.pointer("/statistics/viewCount").and_then(|x| x.as_str()).and_then(|x| x.parse().ok()).unwrap_or(0));
            }
        }
    }
    views_recent.sort_unstable();
    let median_views = views_recent.get(views_recent.len() / 2).copied().unwrap_or(0);

    let mut big = 0usize;
    if !channel_ids.is_empty() {
        let mut uniq = channel_ids.clone();
        uniq.sort();
        uniq.dedup();
        let c = get_json(&http, &format!("{base}/channels"), &[("part", "statistics".into()), ("id", uniq.join(",")), ("key", key.into())]).await?;
        let big_ids: Vec<String> = c["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter(|ch| ch.pointer("/statistics/subscriberCount").and_then(|x| x.as_str()).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) >= 50_000)
            .filter_map(|ch| ch["id"].as_str().map(String::from))
            .collect();
        big = channel_ids.iter().filter(|id| big_ids.contains(id)).count();
    }
    let demand = suggested(&http, artist).await || median_views >= 500;

    Ok(score(demand, recent30, big, channel_ids.len(), median_views, now))
}

pub fn score(demand: bool, recent30: u32, big: usize, top: usize, median_views: u64, now: u64) -> Opportunity {
    let big_share = if top == 0 { 0.0 } else { big as f32 / top as f32 };
    let recent_txt = if recent30 >= 100 { "más de 100".to_string() } else { recent30.to_string() };
    let (level, label, why) = if !demand && recent30 < 10 {
        (1, "Sin demanda", "Casi nadie lo busca todavía: no aparece en el autocompletado y los videos recientes tienen pocas vistas.".to_string())
    } else if big_share >= 0.6 || recent30 >= 100 {
        (1, "Saturado", format!("En el último mes subieron {recent_txt} videos y {big} de los {top} primeros son canales de más de 50 mil suscriptores."))
    } else if demand && recent30 <= 40 && big_share <= 0.3 {
        (3, "Buena oportunidad", format!("Se busca, en el último mes subieron {recent_txt} videos y los recientes llegan a unas {median_views} vistas."))
    } else {
        (2, "Se puede", format!("Hay demanda, pero en el último mes subieron {recent_txt} videos. Combinalo con otro artista."))
    };
    Opportunity { level, label: label.into(), why, measured: now, recent30, big_share, demand, median_views }
}

/// Segundos desde 1970 a "AAAA-MM-DDTHH:MM:SSZ" (UTC), sin depender de otra librería.
pub fn rfc3339(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Algoritmo de días civiles (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

pub fn find_mut<'a>(s: &'a mut Store, id: &str) -> Result<&'a mut Artist> {
    s.artists.iter_mut().find(|a| a.id == id).ok_or_else(|| anyhow!("No se encontró ese artista"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seed_y_menciones() {
        let s = load(Path::new("/nonexistent-dir-test"));
        assert!(s.artists.len() > 50);
        let nsqk = s.artists.iter().find(|a| a.id == "nsqk").unwrap();
        assert!(mentions("me gusta el estilo de sqk y feid", nsqk));
        assert!(!mentions("squeak", nsqk));
        let arc = s.artists.iter().find(|a| a.name == "Arcángel").unwrap();
        assert!(mentions("Arcángel, Endo y lele y el alfa", arc));
        assert!(mentions("arcangel", arc));
    }
    #[test]
    fn paises_y_fechas() {
        assert_eq!(country_from_phone("+52 55 1234 5678"), "MX");
        assert_eq!(country_from_phone("16191234567"), "US");
        assert_eq!(country_from_phone("6461234567"), "MX");
        assert_eq!(country_from_phone("+55 71 9999 9999"), "otro");
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_795_219_200), "2026-11-21T00:00:00Z");
    }
    #[test]
    fn puntaje() {
        assert_eq!(score(true, 18, 1, 10, 2000, 0).level, 3);
        assert_eq!(score(true, 140, 2, 10, 2000, 0).level, 1);
        assert_eq!(score(true, 60, 4, 10, 500, 0).level, 2);
        assert_eq!(score(false, 3, 0, 10, 20, 0).level, 1);
    }
    #[test]
    fn cruce_de_pedidos() {
        let mut s = load(Path::new("/nonexistent-dir-test"));
        let reqs = vec![
            Request { date: "2026-11-20".into(), country: "MX".into(), text: "Busco beat tipo Gera mx".into() },
            Request { date: "2026-11-19".into(), country: "US".into(), text: "algo como Nadie Conocido".into() },
            Request { date: "2026-11-18".into(), country: "MX".into(), text: "".into() },
        ];
        apply_requests(&mut s, &reqs, 1);
        let g = s.artists.iter().find(|a| a.id == "gera-mx").unwrap();
        assert_eq!((g.requests, g.requests_mx), (1, 1));
        assert_eq!(s.unmatched.len(), 1);
        assert_eq!(s.total_requests, 3);
    }
}
