//! The app's languages: the dictionaries in i18n/<lang>.json at the top of the
//! repository, keyed by the Russian text.
//!
//!     tr!("Пауза до {time}", time = "08:00")
//!
//! The language is the one chosen in Settings, or the first of Windows' display
//! languages that the app has, or English. The pages get the same table through
//! the `strings` command (lib.js).
use std::sync::RwLock;

use serde_json::{Map, Value};

/// The site's languages, in its order.
pub const CODES: [&str; 13] = ["en", "es", "pt", "de", "fr", "it", "pl", "tr", "uk", "ru", "id", "ko", "zh"];
const FALLBACK: &str = "en";

fn source(code: &str) -> &'static str {
    match code {
        "en" => include_str!("../../i18n/en.json"),
        "es" => include_str!("../../i18n/es.json"),
        "pt" => include_str!("../../i18n/pt.json"),
        "de" => include_str!("../../i18n/de.json"),
        "fr" => include_str!("../../i18n/fr.json"),
        "it" => include_str!("../../i18n/it.json"),
        "pl" => include_str!("../../i18n/pl.json"),
        "tr" => include_str!("../../i18n/tr.json"),
        "uk" => include_str!("../../i18n/uk.json"),
        "ru" => include_str!("../../i18n/ru.json"),
        "id" => include_str!("../../i18n/id.json"),
        "ko" => include_str!("../../i18n/ko.json"),
        "zh" => include_str!("../../i18n/zh.json"),
        _ => "{}",
    }
}

struct Current {
    lang: &'static str,
    table: Map<String, Value>,
    fallback: Map<String, Value>,
}

static CURRENT: RwLock<Option<Current>> = RwLock::new(None);

fn load(code: &str) -> Map<String, Value> {
    serde_json::from_str(source(code)).unwrap_or_default()
}

/// The system's languages, most preferred first, as two-letter codes.
fn system_languages() -> Vec<String> {
    sys_locale::get_locales().map(|l| l.split(['-', '_']).next().unwrap_or("").to_lowercase()).collect()
}

/// The language to show: the chosen one, else the system's, else English.
pub fn resolve(choice: &str) -> &'static str {
    if let Some(code) = CODES.iter().find(|c| **c == choice) {
        return code;
    }
    for wanted in system_languages() {
        if let Some(code) = CODES.iter().find(|c| **c == wanted) {
            return code;
        }
    }
    FALLBACK
}

/// `choice`: a code from `CODES`, or "" for the system's.
pub fn set(choice: &str) -> &'static str {
    let lang = resolve(choice);
    let fallback = if lang == "ru" || lang == FALLBACK { Map::new() } else { load(FALLBACK) };
    *CURRENT.write().unwrap() = Some(Current { lang, table: load(lang), fallback });
    lang
}

pub fn lang() -> &'static str {
    CURRENT.read().unwrap().as_ref().map_or(FALLBACK, |c| c.lang)
}

/// `text` in the current language, its {placeholders} filled from `args`.
pub fn tr(text: &str, args: &[(&str, String)]) -> String {
    let mut value = {
        let current = CURRENT.read().unwrap();
        current
            .as_ref()
            .and_then(|c| c.table.get(text).and_then(Value::as_str).or_else(|| c.fallback.get(text).and_then(Value::as_str)))
            .unwrap_or(text)
            .to_string()
    };
    for (key, arg) in args {
        value = value.replace(&format!("{{{key}}}"), arg);
    }
    value
}

/// For the pages: the language, its table and English under it.
pub fn strings() -> Value {
    let current = CURRENT.read().unwrap();
    match current.as_ref() {
        Some(c) => serde_json::json!({ "lang": c.lang, "table": c.table, "fallback": c.fallback }),
        None => serde_json::json!({ "lang": FALLBACK, "table": {}, "fallback": {} }),
    }
}

#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::i18n::tr($text, &[])
    };
    ($text:literal, $($key:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr($text, &[$((stringify!($key), $value.to_string())),+])
    };
}
