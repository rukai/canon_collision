use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use hotwatch::{Event, Hotwatch};

/// Hot reloadable assets
pub struct Assets {
    path: PathBuf,
    models_reload_rx: Receiver<Reload>,
    models_reload_tx: Sender<Reload>,
    hotwatch: Hotwatch,
}

impl Assets {
    pub fn new() -> Option<Self> {
        let (models_reload_tx, models_reload_rx) = mpsc::channel();

        let current_dir = std::env::current_dir().unwrap();
        Assets::find_assets_in_parent_dirs_core(&current_dir).map(|path| Assets {
            path,
            models_reload_rx,
            models_reload_tx,
            hotwatch: Hotwatch::new().unwrap(),
        })
    }

    fn find_assets_in_parent_dirs_core(path: &Path) -> Option<PathBuf> {
        let assets_path = path.join("assets");
        match fs::metadata(&assets_path) {
            Ok(_) => Some(assets_path),
            Err(_) => Assets::find_assets_in_parent_dirs_core(path.parent()?),
        }
    }

    pub fn models_reloads(&self) -> Vec<Reload> {
        let mut reloads = vec![];

        while let Ok(reload) = self.models_reload_rx.try_recv() {
            reloads.push(reload);
        }

        reloads
    }

    /// On failure to read from disk, logs the error and returns None
    pub fn get_model(&mut self, name: &str) -> Option<Vec<u8>> {
        let path = self.path.join("models").join(format!("{}.glb", name));
        let tx = self.models_reload_tx.clone();
        let reload_name = name.to_string();
        let reload_path = path.clone();

        let result = self.hotwatch.watch(&path, move |event: Event| {
            let path = reload_path.clone();
            let name = reload_name.clone();
            if let Event::Write(_) = event {
                if let Some(data) = Assets::load_file(path) {
                    tx.send(Reload { name, data }).unwrap();
                }
            }
        });

        match result {
            Ok(_) => Assets::load_file(path),
            Err(err) => {
                error!("Failed to load or setup hotreloading for '{}'. You will need to restart the game to reattempt loading this file. error: {}", path.to_str().unwrap(), err);
                None
            }
        }
    }

    /// On failure to read from disk, logs the error and returns None
    fn load_file(path: PathBuf) -> Option<Vec<u8>> {
        std::fs::read(&path)
            .map_err(|err| {
                error!(
                    "Failed to read file '{}' because: {}",
                    path.to_str().unwrap(),
                    err
                )
            })
            .ok()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct Reload {
    pub name: String,
    pub data: Vec<u8>,
}
