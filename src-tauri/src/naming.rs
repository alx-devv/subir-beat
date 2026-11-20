//! Nomenclatura del bucket y de Shopify.

/// "C# / Db" -> "C#", "Am" -> "Am"
pub fn key_short(key: &str) -> String {
    key.split('/').next().unwrap_or(key).trim().to_string()
}

/// Nombre del beat dentro de los archivos: espacios -> guiones.
pub fn file_name_part(name: &str) -> String {
    name.trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Handle de Shopify: minúsculas, solo letras/números separados por guiones.
pub fn handle(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.trim().to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

pub struct Names {
    pub folder: String,
    pub mp3: String,
    pub premium: String,
    pub exclusiva: String,
    pub wav_inside: String,
}

pub fn names(name: &str, bpm: u32, key: &str) -> Names {
    let n = file_name_part(name);
    let k = key_short(key);
    Names {
        folder: name.trim().to_string(),
        mp3: format!("@alexgareis_{n}_wtag.mp3"),
        premium: format!("@alexgareis_{n}_premium_{bpm}bpm_{k}.zip"),
        exclusiva: format!("@alexgareis_{n}_exclusiva_{bpm}bpm_{k}.zip"),
        wav_inside: format!("@alexgareis_{n}_{bpm}bpm_{k}.wav"),
    }
}

/// URL pública en dl.alexgareis.com con cada segmento codificado.
pub fn public_url(base: &str, folder: &str, file: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const SEG: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');
    format!(
        "{}/{}/{}",
        base.trim_end_matches('/'),
        utf8_percent_encode(folder, SEG),
        utf8_percent_encode(file, SEG)
    )
}

pub fn object_key(folder: &str, file: &str) -> String {
    format!("{folder}/{file}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nombres() {
        let n = names("intimate thoughts", 92, "D");
        assert_eq!(n.premium, "@alexgareis_intimate-thoughts_premium_92bpm_D.zip");
        assert_eq!(n.mp3, "@alexgareis_intimate-thoughts_wtag.mp3");
        let n = names("tsss!", 90, "C#m / Dbm");
        assert_eq!(n.exclusiva, "@alexgareis_tsss!_exclusiva_90bpm_C#m.zip");
        assert_eq!(handle("tsss!"), "tsss");
        assert_eq!(handle("You're All Mine"), "you-re-all-mine");
        assert_eq!(
            public_url("https://dl.alexgareis.com", "tsss!", "@alexgareis_tsss!_wtag.mp3"),
            "https://dl.alexgareis.com/tsss%21/%40alexgareis_tsss%21_wtag.mp3"
        );
    }
}
