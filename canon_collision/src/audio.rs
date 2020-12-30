use std::path::PathBuf;
use std::fs;

use audiotags::Tag;
use kira::instance::{InstanceId, InstanceSettings, StopInstanceSettings};
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::playable::PlayableSettings;
use kira::Value;
use rand::seq::IteratorRandom;
use rand;
use treeflection::{Node, NodeRunner, NodeToken};

use canon_collision_lib::assets::Assets;
use canon_collision_lib::entity_def::EntityDef;

pub struct Audio {
    manager: AudioManager,
    path:    PathBuf,
    bgm:     Option<InstanceId>,
}

impl Audio {
    pub fn new(assets: Assets) -> Self {
        let manager = AudioManager::new(AudioManagerSettings::default()).unwrap();
        let path = assets.path().join("audio");

        Audio {
            manager,
            path,
            bgm: None,
        }
    }

    /// TODO: Load all sounds effects on startup
    /// TODO: Random sfx selection from a pool?
    /// TODO: How to handle rollback?
    /// TODO: I could probably add a foo.txt for a foo.mp3 that contains a relative path to another mp3 file
    pub fn play_sound_effect(&mut self, entity: &EntityDef, sfx_name: &str, volume: Value<f64>, pitch: Value<f64>) {
        self.play_sound_effect_inner(entity, sfx_name, volume, pitch).unwrap();
    }

    pub fn play_sound_effect_inner(&mut self, entity: &EntityDef, sfx_name: &str, volume: Value<f64>, pitch: Value<f64>) -> Result<(), String> {
        let folder = entity.name.replace(" ", "");
        let path = self.path.join("sfx").join(&folder).join(sfx_name);

        let playable_settings = PlayableSettings::default();
        let new_sound = self.manager.load_sound(&path, playable_settings).map_err(|x| format!("Failed to load {:?}. {}", path, x))?;

        let instance_settings = InstanceSettings::default().volume(volume).pitch(pitch);
        self.manager.play(new_sound, instance_settings).map_err(|x| x.to_string())?;
        Ok(())
    }

    /// Folders can contain music organized by stage/menu or fighter
    /// TODO:
    ///     If I need to specify per song looping metadata then add some kind of foo.json for a foo.mp3.
    ///     OR just throw the metadata into the mp3 metadata.
    pub fn play_bgm(&mut self, folder: &str) -> BGMMetadata {
        self.play_bgm_inner(folder).unwrap_or_else(|x|
            BGMMetadata {
                title: format!("Failed to play song from: {}", folder),
                artist: Some(x),
                album: None,
            }
        )
    }

    fn play_bgm_inner(&mut self, folder: &str) -> Result<BGMMetadata, String> {
        let folder = folder.replace(" ", "");
        let read_dir = fs::read_dir(self.path.join("music").join(&folder)).map_err(|x| x.to_string())?;
        let chosen_file = read_dir
            .filter_map(|x| x.ok())
            .filter(|x|
                !x.file_name().to_str().unwrap_or_default().to_lowercase().ends_with(".json") // TODO: If we have config files here we will need to filter them out like this. Otherwise delete this line.
            ).choose(&mut rand::thread_rng())
            .ok_or("No files in folder")?;

        let basic_loop = PlayableSettings::default().default_loop_start(0.0);
        let new_sound = self.manager.load_sound(chosen_file.path(), basic_loop).map_err(|x| x.to_string())?;

        if let Some(instance_id) = self.bgm.take() {
            self.manager.stop_instance(instance_id, StopInstanceSettings::default()).unwrap();
        }

        self.bgm = Some(self.manager.play(new_sound, InstanceSettings::default()).map_err(|x| x.to_string())?);

        let tag = Tag::new().read_from_path(chosen_file.path()).unwrap();

        let title = if let Some(title) = tag.title() {
            title.to_string()
        } else {
            chosen_file.file_name().to_str().unwrap_or_default().to_string()
        };
        let artist = tag
            .artist()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty());
        let album = tag
            .album_title()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty());

        Ok(BGMMetadata { title, artist, album })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Node)]
pub struct BGMMetadata {
    pub title:  String,
    pub artist: Option<String>,
    pub album:  Option<String>,
}
