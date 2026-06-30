use serde::{Deserialize, Serialize};
use std::path::Path;

const SETTINGS_FILE: &str = "settings.toml";

/// Default auto-archive delay in days (7 days after completion).
pub const DEFAULT_AUTO_ARCHIVE_DAYS: u32 = 7;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// `"system"`, `"light"`, or `"dark"`
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,

    /// Whether auto-archive is enabled at all.
    #[serde(default = "default_auto_archive_enabled")]
    pub auto_archive_enabled: bool,

    /// Days after completion before a Done task becomes Archived.
    /// 0 = archive immediately upon completion (when enabled).
    #[serde(default = "default_auto_archive_days")]
    pub auto_archive_days: u32,
}

fn default_theme_mode() -> String {
    "system".into()
}
fn default_auto_archive_enabled() -> bool {
    true
}
fn default_auto_archive_days() -> u32 {
    DEFAULT_AUTO_ARCHIVE_DAYS
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_mode: default_theme_mode(),
            auto_archive_enabled: default_auto_archive_enabled(),
            auto_archive_days: default_auto_archive_days(),
        }
    }
}

impl Settings {
    /// Load settings from `settings.toml` next to the executable, or return defaults.
    pub fn load() -> Self {
        let path = Path::new(SETTINGS_FILE);
        if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Persist current settings to `settings.toml`.
    pub fn save(&self) -> Result<(), String> {
        let s = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(SETTINGS_FILE, s).map_err(|e| e.to_string())
    }
}
