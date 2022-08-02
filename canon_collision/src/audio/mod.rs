use std::fs;
use std::path::PathBuf;

use audiotags::Tag;
use kira::instance::handle::InstanceHandle;
use kira::instance::{InstanceSettings, StopInstanceSettings};
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::SoundSettings;
use rand::seq::IteratorRandom;

use treeflection::{Node, NodeRunner, NodeToken};

use canon_collision_lib::assets::Assets;
use canon_collision_lib::entity_def::EntityDef;

pub mod sfx;

use sfx::{Sfx, SfxType};

pub struct Audio {
    manager: AudioManager,
    path: PathBuf,
    bgm: Option<InstanceHandle>,
    sfx: Sfx,
}

impl Audio {
    pub fn new(assets: Assets) -> Self {
        let mut manager = AudioManager::new(AudioManagerSettings::default()).unwrap();
        let path = assets.path().join("audio");
        let sfx = Sfx::new(&mut manager, &path);

        Audio {
            manager,
            path,
            sfx,
            bgm: None,
        }
    }

    pub fn play_sound_effect(&mut self, entity: &EntityDef, sfx: SfxType) {
        self.sfx.play_sound_effect(entity, sfx);
    }

    /// Folders can contain music organized by stage/menu or fighter
    /// TODO:
    ///     If I need to specify per song looping metadata then add some kind of foo.json for a foo.mp3.
    ///     OR just throw the metadata into the mp3 metadata.
    pub fn play_bgm(&mut self, folder: &str) -> BGMMetadata {
        self.play_bgm_inner(folder).unwrap_or_else(|x| BGMMetadata {
            title: format!("Failed to play song from: {}", folder),
            artist: Some(x),
            album: None,
        })
    }

    fn play_bgm_inner(&mut self, folder: &str) -> Result<BGMMetadata, String> {
        let folder = folder.replace(' ', "");
        let read_dir =
            fs::read_dir(self.path.join("music").join(&folder)).map_err(|x| x.to_string())?;
        let chosen_file = read_dir
            .filter_map(|x| x.ok())
            .filter(
                |x| {
                    !x.file_name()
                        .to_str()
                        .unwrap_or_default()
                        .to_lowercase()
                        .ends_with(".json")
                }, // TODO: If we have config files here we will need to filter them out like this. Otherwise delete this line.
            )
            .choose(&mut rand::thread_rng())
            .ok_or("No files in folder")?;

        let basic_loop = SoundSettings::default().default_loop_start(0.0);
        let mut new_sound = self
            .manager
            .load_sound(chosen_file.path(), basic_loop)
            .map_err(|x| x.to_string())?;

        if let Some(mut instance_id) = self.bgm.take() {
            instance_id.stop(StopInstanceSettings::default()).unwrap();
        }

        self.bgm = Some(
            new_sound
                .play(InstanceSettings::default())
                .map_err(|x| x.to_string())?,
        );

        let tag = Tag::new().read_from_path(chosen_file.path()).unwrap();

        let title = if let Some(title) = tag.title() {
            title.to_string()
        } else {
            chosen_file
                .file_name()
                .to_str()
                .unwrap_or_default()
                .to_string()
        };
        let artist = tag
            .artist()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty());
        let album = tag
            .album_title()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty());

        Ok(BGMMetadata {
            title,
            artist,
            album,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Node)]
pub struct BGMMetadata {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
}
