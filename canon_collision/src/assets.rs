use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use hotwatch::{Hotwatch, Event};

/// Hot reloadable assets
pub struct Assets {
    models_reload_rx: Receiver<Reload>,
    //audio_reload_rx: Receiver<Reload>,
    models_reload_tx: Sender<Reload>,
    //audio_reload_tx: Arc<Sender<Reload>,
    hotwatch: Hotwatch,
}

impl Assets {
    pub fn new() -> Self {
        let (models_reload_tx, models_reload_rx) = mpsc::channel();

        Assets {
            models_reload_rx,
            models_reload_tx,
            hotwatch: Hotwatch::new().unwrap(),
        }
    }

    pub fn models_reloads(&self) -> Vec<Reload> {
        let mut reloads = vec!();

        while let Ok(reload) = self.models_reload_rx.try_recv() {
            reloads.push(reload);
        }

        reloads
    }

    /// On failure to read from disk, logs the error and returns None
    pub fn get_model(&mut self, name: &str) -> Option<Vec<u8>> {
        let filename = PathBuf::from(name);

        let tx = self.models_reload_tx.clone();
        let reload_name = name.to_string();
        let reload_filename = filename.clone();
        self.hotwatch.watch(name, move |event: Event| {
            let filename = reload_filename.clone();
            let name = reload_name.clone();
            if let Event::Write(_) = event {
                if let Some(data) = Assets::load_file(filename) {
                    tx.send(Reload { name, data }).unwrap();
                }
            }
        }).unwrap();

        Assets::load_file(filename)
    }

    /// On failure to read from disk, logs the error and returns None
    fn load_file(filename: PathBuf) -> Option<Vec<u8>> {
        let mut file = match File::open(&filename) {
            Ok(file) => file,
            Err(err) => {
                error!("Failed to open file: {} because: {}", filename.to_str().unwrap(), err);
                return None;
            }
        };

        let mut contents = Vec::<u8>::new();
        if let Err(err) = file.read_to_end(&mut contents) {
            error!("Failed to read file {} because: {}", filename.to_str().unwrap(), err);
            return None;
        };
        Some(contents)
    }
}

pub struct Reload {
    pub name: String,
    pub data: Vec<u8>,
}
