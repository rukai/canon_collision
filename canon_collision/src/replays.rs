use crate::camera::Camera;
use crate::game::{Game, GameSetup, PlayerSetup, GameState, Edit};
use crate::entity::{EntityType, Entity, DebugEntity};
use crate::rules::Rules;

use canon_collision_lib::files;
use canon_collision_lib::input::Input;
use canon_collision_lib::input::state::ControllerInput;
use canon_collision_lib::stage::{Stage, DebugStage};
use canon_collision_lib::replays_files;

use chrono::{Local, DateTime};
use generational_arena::Arena;

pub fn load_replay(name: &str) -> Result<Replay, String> {
    let replay_path = replays_files::get_replay_path(name);
    files::load_struct_bincode(&replay_path)
}

pub fn save_replay(replay: &Replay) {
    let replay_path = replays_files::get_replay_path(&format!("{}.zip", replay.timestamp.to_rfc2822())); // TODO: could still collide under strange circumstances: check and handle
    files::save_struct_bincode(&replay_path, &replay)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Replay {
    pub init_seed:                 u64,
    pub timestamp:                 DateTime<Local>,
    pub input_history:             Vec<Vec<ControllerInput>>,
    pub entity_history:            Vec<Arena<Entity>>,
    pub stage_history:             Vec<Stage>,
    pub selected_controllers:      Vec<usize>,
    pub selected_players:          Vec<PlayerSetup>,
    pub selected_ais:              Vec<usize>,
    pub selected_stage:            String,
    pub rules:                     Rules,
    pub max_history_frames:        Option<usize>,
    pub deleted_history_frames:    usize,
    pub hot_reload_current_frame:  usize,
    pub hot_reload_camera:         Camera,
    pub hot_reload_debug_entities: [DebugEntity; 9],
    pub hot_reload_debug_stage:    DebugStage,
    pub hot_reload_entities:       Arena<Entity>,
    pub hot_reload_stage:          Stage,
    pub hot_reload_as_running:     bool,
    pub hot_reload_edit:           Edit,
}

impl Replay {
    pub fn new(game: &Game, input: &Input) -> Replay {
        let mut selected_players = vec!();
        for (_, entity) in &game.entities {
            match entity.ty {
                EntityType::Player (player) => {
                    selected_players.push(
                        PlayerSetup {
                            fighter: player.fighter.clone(),
                            team:    player.team,
                        }
                    );
                }
                _ => { }
            }
        }

        let hot_reload_as_running = match game.state {
            GameState::Local => true,
            _ => false
        };

        Replay {
            init_seed:                 game.init_seed.clone(),
            timestamp:                 Local::now(),
            input_history:             input.get_history(),
            entity_history:            game.entity_history().clone(),
            stage_history:             game.stage_history.clone(),
            selected_controllers:      game.selected_controllers.clone(),
            selected_ais:              game.selected_ais.clone(),
            selected_stage:            game.selected_stage.clone(),
            rules:                     game.rules.clone(),
            max_history_frames:        game.max_history_frames,
            deleted_history_frames:    game.deleted_history_frames,
            hot_reload_current_frame:  game.current_frame,
            hot_reload_camera:         game.camera.clone(),
            hot_reload_debug_entities: game.debug_entities.clone(),
            hot_reload_debug_stage:    game.debug_stage.clone(),
            hot_reload_entities:       game.entities.clone(),
            hot_reload_stage:          game.stage.clone(),
            hot_reload_edit:           game.edit.clone(),
            hot_reload_as_running,
            selected_players,
        }
    }

    // TODO: maybe hotreloading should be its own thing seperate to replays
    // Its increasing looking like hot reloading wants to serialize EVERYTHING
    // whereas replays only wants to serialize the bits relevant to gameplay
    pub fn into_game_setup(self, hot_reload: bool) -> GameSetup {
        let state = if !hot_reload {
            GameState::ReplayForwardsFromHistory
        } else if self.hot_reload_as_running {
            GameState::Local
        } else {
            GameState::Paused
        };

        let current_frame = if hot_reload {
            self.hot_reload_current_frame
        } else {
            self.deleted_history_frames
        };

        let debug_entities = if hot_reload {
            Some(self.hot_reload_debug_entities)
        } else {
            None
        };

        let debug_stage = if hot_reload {
            Some(self.hot_reload_debug_stage)
        } else {
            None
        };

        let camera = if hot_reload {
            self.hot_reload_camera
        } else {
            Camera::new()
        };

        let hot_reload_entities = if hot_reload {
            Some(self.hot_reload_entities)
        } else {
            None
        };

        let hot_reload_stage = if hot_reload {
            Some(self.hot_reload_stage)
        } else {
            None
        };

        GameSetup {
            init_seed:              self.init_seed,
            input_history:          self.input_history,
            entity_history:         self.entity_history,
            stage_history:          self.stage_history,
            controllers:            self.selected_controllers,
            players:                self.selected_players,
            ais:                    self.selected_ais,
            stage:                  self.selected_stage,
            rules:                  self.rules,
            max_history_frames:     self.max_history_frames,
            deleted_history_frames: self.deleted_history_frames,
            edit:                   self.hot_reload_edit,
            current_frame:          current_frame,
            debug:                  false,
            camera,
            debug_entities,
            debug_stage,
            hot_reload_entities,
            hot_reload_stage,
            state,
        }
    }
}
