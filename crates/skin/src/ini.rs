//! Lenient INI parsing for `pledit.txt` and `region.txt`, following Webamp's `parseIni`.
//!
//! - Section and key names are trimmed and lower-cased; a repeated section starts over.
//! - Lines whose first non-blank character is `;` are comments; lines before the first
//!   section and lines without `=` are ignored.
//! - A value ends at a second `=`, is trimmed, and loses one leading and one trailing quote.

use std::collections::HashMap;

pub type Ini = HashMap<String, HashMap<String, String>>;

pub fn parse(text: &str) -> Ini {
    let mut ini = Ini::new();
    let mut section: Option<String> = None;
    for line in text.split(['\r', '\n']) {
        let trimmed = line.trim();
        if let Some(name) = &section
            && let Some((key, value)) = property(trimmed)
        {
            ini.entry(name.clone()).or_default().insert(key, value);
        } else if trimmed.len() > 2 && trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed[1..trimmed.len() - 1].trim().to_lowercase();
            ini.insert(name.clone(), HashMap::new());
            section = Some(name);
        }
    }
    ini
}

fn property(line: &str) -> Option<(String, String)> {
    if line.starts_with(';') {
        return None;
    }
    let (key, rest) = line.split_once('=')?;
    let key = key.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    let value = rest.split('=').next().unwrap_or("").trim();
    let value = value.strip_prefix(['"', '\'']).unwrap_or(value);
    let value = value.strip_suffix(['"', '\'']).unwrap_or(value);
    Some((key, value.to_string()))
}

/// Decodes a skin text file: UTF-8 (BOM dropped) or, failing that, Latin-1.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}
