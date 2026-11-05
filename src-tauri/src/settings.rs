use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// How many emoji float up per signal unless the receiver set otherwise.
pub const DEFAULT_EMOJI_COUNT: i64 = 40;

/// Everything kept on this PC only: the server knows contacts and labels,
/// never colors, a pause or how a signal looks.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub my_color: i64,
    pub sound: bool,
    pub calm: bool,
    /// How many emoji float up with a signal; 0 is the glow alone.
    pub emoji_count: i64,
    /// No signals while something is full screen or a presentation is on.
    pub quiet_when_presenting: bool,
    /// An incoming signal glows at most once per this many seconds, one of `GAPS`.
    pub min_gap: f64,
    /// "Сердечки от кого-нибудь на свете": the stream asks for signals to anyone.
    pub accept_anyone: bool,
    /// When this PC last sent a signal to anyone (ms since the epoch): one an hour.
    pub anyone_sent_at: Option<i64>,
    pub contact_colors: HashMap<String, i64>,
    /// When each contact was last sent a signal from this PC, for "Недавние".
    pub last_sent: HashMap<String, i64>,
    /// Ms since the epoch; `FOREVER` until resumed.
    pub paused_until: Option<i64>,
    pub login_item_set_up: bool,
    /// A code from `i18n::CODES`, or "" for Windows' language.
    pub language: String,
}

pub const FOREVER: i64 = i64::MAX;

/// The steps of "Свечение не чаще раза в".
pub const GAPS: [f64; 8] = [1.0, 5.0, 15.0, 30.0, 60.0, 300.0, 900.0, 3600.0];

/// The step nearest to `gap`, so the slider shows what is in effect.
pub fn nearest_gap(gap: f64) -> f64 {
    GAPS.iter().copied().min_by(|a, b| (a - gap).abs().total_cmp(&(b - gap).abs())).unwrap_or(1.0)
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            my_color: 1,
            sound: false,
            calm: false,
            emoji_count: DEFAULT_EMOJI_COUNT,
            quiet_when_presenting: true,
            min_gap: 1.0,
            accept_anyone: true,
            anyone_sent_at: None,
            contact_colors: HashMap::new(),
            last_sent: HashMap::new(),
            paused_until: None,
            login_item_set_up: false,
            language: String::new(),
        }
    }
}

impl Settings {
    pub fn load(file: &PathBuf) -> Self {
        let mut settings: Settings = std::fs::read(file).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default();
        settings.min_gap = nearest_gap(settings.min_gap);
        settings
    }

    pub fn save(&self, file: &PathBuf) {
        if let Ok(data) = serde_json::to_vec_pretty(self) {
            let tmp = file.with_extension("tmp");
            if std::fs::write(&tmp, data).is_ok() {
                let _ = std::fs::rename(&tmp, file);
            }
        }
    }
}
