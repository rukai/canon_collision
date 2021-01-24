use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

use kira::Value;
use kira::instance::InstanceSettings;
use kira::manager::AudioManager;
use kira::sound::SoundSettings;
use kira::sound::handle::SoundHandle;

use canon_collision_lib::entity_def::EntityDef;

// TODO: move into hitbox canon_collision_lib hitbox definition
pub enum HitBoxSFX {
    Sword,
    Punch,
    //Explode, etc...
}

pub enum SFXType {
    Walk,
    Run,
    Dash,
    Jump,
    Land,
    Die,
    Hit (HitBoxSFX),
    /// TODO: Dont know if the ergonomics and efficiency of this is a good idea.
    ///       Lets play with it a bit and throw it away if we dont like it.
    Custom {
        filename: String,
        volume: Value<f64>,
        pitch: Value<f64>
    }
}

pub struct SFX {
    sfx: HashMap<String, SoundHandle>,
}

impl SFX {
    pub fn new(manager: &mut AudioManager, path: &Path) -> Self {
        let mut sfx = HashMap::new();
        let path = path.join("sfx");
        SFX::populate_sfx(manager, &path, None, &mut sfx);
        SFX { sfx }
    }

    fn populate_sfx(manager: &mut AudioManager, root_path: &Path, search_path: Option<&PathBuf>, sfx: &mut HashMap<String, SoundHandle>) {
        let path = if let Some(search_path) = search_path {
            root_path.join(search_path)
        } else {
            root_path.to_path_buf()
        };

        for file in fs::read_dir(path).unwrap() {
            let file = file.unwrap();
            let playable_settings = SoundSettings::default();

            let sub_search_path = if let Some(search_path) = search_path {
                search_path.join(file.file_name())
            } else {
                PathBuf::from(file.file_name())
            };

            let file_type = file.file_type().unwrap();
            if file_type.is_file() {
                let id = manager.load_sound(file.path(), playable_settings).unwrap();

                let key = sub_search_path.to_str().unwrap().replace(MAIN_SEPARATOR, "/");
                if sfx.contains_key(&key) {
                    panic!("Duplicate sfx key");
                }
                sfx.insert(key, id);
            }
            else if file_type.is_dir() {
                SFX::populate_sfx(manager, root_path, Some(&sub_search_path), sfx);
            }
        }
    }

    /// TODO: How to handle rollback?
    pub fn play_sound_effect(&mut self, entity: &EntityDef, sfx: SFXType) {
        let entity_name = entity.name.replace(" ", "");

        let sfx_id = match (&entity_name, &sfx) {
            //(_, SFXType::Walk) => ["Common/walk1.ogg", "Common/walk2.ogg"].choose(&mut rand::thread_rng()).unwrap(), // TODO: This is possible
            (_, SFXType::Walk) => self.sfx.get_mut("Common/walk.ogg"),
            (_, SFXType::Run)  => self.sfx.get_mut("Common/walk.ogg"),
            (_, SFXType::Dash) => self.sfx.get_mut("Common/dash.ogg"),
            (_, SFXType::Jump) => self.sfx.get_mut("Common/jump.ogg"),
            (_, SFXType::Land) => self.sfx.get_mut("Common/land.ogg"),
            (_, SFXType::Die)  => self.sfx.get_mut("Common/die.wav"),
            (_, SFXType::Hit (HitBoxSFX::Sword)) => self.sfx.get_mut("Common/hit.wav"),
            (_, SFXType::Hit (HitBoxSFX::Punch)) => self.sfx.get_mut("Common/hit.wav"),
            (folder, SFXType::Custom { filename, .. }) => self.sfx.get_mut(&format!("{}/{}", folder, filename)),
        };

        let (volume, pitch) = match (&entity_name, sfx) {
            (_, SFXType::Walk) => (Value::Random(0.01, 0.03), Value::Random(0.95, 1.05)),
            (_, SFXType::Run)  => (Value::Random(0.03, 0.1), Value::Random(0.95, 1.05)),
            (_, SFXType::Dash) => (Value::Random(0.15, 0.2), Value::Random(0.95, 1.05)),
            (_, SFXType::Jump) => (Value::Random(0.15, 0.2), Value::Random(0.90, 1.1)),
            (_, SFXType::Land) => (Value::Random(0.05, 0.1), Value::Random(0.90, 1.1)),
            (_, SFXType::Die)  => (Value::Random(0.30, 0.4), Value::Random(0.90, 1.1)),
            (_, SFXType::Hit (HitBoxSFX::Sword)) => (Value::Random(0.15, 0.2), Value::Random(0.95, 1.05)),
            (_, SFXType::Hit (HitBoxSFX::Punch)) => (Value::Random(0.15, 0.2), Value::Random(0.90, 1.1)),
            (_, SFXType::Custom { volume, pitch, .. }) => (volume, pitch),
        };

        let instance_settings = InstanceSettings::default().volume(volume).pitch(pitch);
        sfx_id.unwrap().play(instance_settings).map_err(|x| x.to_string()).unwrap();
    }
}
