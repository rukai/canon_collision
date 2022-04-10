use crate::files;

use std::path::PathBuf;

use serde_json;
use treeflection::{Node, NodeRunner, NodeToken};

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct Config {
    pub netplay_region: Option<String>,
    pub auto_save_replay: bool,
    pub verify_package_hashes: bool,
    pub fullscreen: bool,
}

impl Config {
    fn get_path() -> PathBuf {
        let mut path = files::get_path();
        path.push("config.json");
        path
    }

    pub fn load() -> Config {
        if let Ok(json) = files::load_json(&Config::get_path()) {
            if let Ok(config) = serde_json::from_value::<Config>(json) {
                return config;
            }
        }
        warn!(
            "{:?} is invalid or does not exist, loading default values",
            Config::get_path()
        );
        Config::default()
    }

    pub fn save(&self) {
        files::save_struct_json(&Config::get_path(), self);
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            netplay_region: None,
            auto_save_replay: false,
            verify_package_hashes: true,
            fullscreen: false,
        }
    }
}
