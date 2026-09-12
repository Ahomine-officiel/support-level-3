//! Configuration persistante (sl3_config.json).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    pub lang: String,           // "fr" | "en"
    pub host_default: String,   // addr:port par défaut
    pub sensitivity: f32,
    pub volume: f32,
    #[serde(default)]
    pub render_scale: f32,      // 0.0 = auto (DRS), sinon 0.45..1.0
}

impl Default for Config {
    fn default() -> Self {
        Config {
            name: "Tech-07".into(),
            lang: "fr".into(),
            host_default: "127.0.0.1:27070".into(),
            sensitivity: 1.0,
            volume: 0.8,
            render_scale: 0.0,
        }
    }
}

impl Config {
    pub fn load() -> Config {
        match std::fs::read_to_string("sl3_config.json") {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write("sl3_config.json", s);
        }
    }
}
