use kira::manager::{AudioManager, AudioManagerSettings};
use kira::playable::PlayableSettings;
use kira::instance::InstanceSettings;
use kira::sound::SoundId;
use canon_collision_lib::assets::Assets;

pub struct Audio {
    manager: AudioManager,
    music:   SoundId,
}

impl Audio {
    pub fn new(assets: Assets) -> Self {
        let mut manager = AudioManager::new(AudioManagerSettings::default()).unwrap();

        let path = assets.path().join("audio").join("music");
        let basic_loop = PlayableSettings::default().default_loop_start(0.0);
        let music = manager.load_sound(path.join("stormspirit.mp3"), basic_loop).unwrap();

        Audio {
            manager,
            music,
        }
    }

    /// TODO: Hmmmm no idea what kind of api we want here...
    /// Should fighters/stages call play methods directly?
    /// Or should we pass back messages and have a step method?
    pub fn play(&mut self) {
        self.manager.play(self.music, InstanceSettings::default()).unwrap();
    }
}
