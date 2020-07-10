use chrono::{Local, DateTime};

use canon_collision_lib::files;
use canon_collision_lib::input::Input;
use canon_collision_lib::input::state::ControllerInput;
use canon_collision_lib::stage::Stage;
use crate::game::{Game, GameSetup, PlayerSetup, GameState};
use crate::player::Player;
use crate::rules::Rules;
use canon_collision_lib::replays_files;

pub fn load_replay(name: &str) -> Result<Replay, String> {
    let replay_path = replays_files::get_replay_path(name);
    files::load_struct_bincode(replay_path)
}

pub fn save_replay(replay: &Replay) {
    let replay_path = replays_files::get_replay_path(&format!("{}.zip", replay.timestamp.to_rfc2822())); // TODO: could still collide under strange circumstances: check and handle
    files::save_struct_bincode(replay_path, &replay)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Replay {
    pub init_seed:            u64,
    pub timestamp:            DateTime<Local>,
    pub input_history:        Vec<Vec<ControllerInput>>,
    pub player_history:       Vec<Vec<Player>>,
    pub stage_history:        Vec<Stage>,
    pub selected_controllers: Vec<usize>,
    pub selected_players:     Vec<PlayerSetup>,
    pub selected_ais:         Vec<usize>,
    pub selected_stage:       String,
    pub rules:                Rules,
}

impl Replay {
    pub fn new(game: &Game, input: &Input) -> Replay {
        let selected_players = game.players.iter().map(|x| PlayerSetup {
            fighter: x.fighter.clone(),
            team:    x.team,
        }).collect();

        Replay {
            init_seed:            game.init_seed.clone(),
            timestamp:            Local::now(),
            input_history:        input.get_history(),
            player_history:       game.player_history.clone(),
            stage_history:        game.stage_history.clone(),
            selected_controllers: game.selected_controllers.clone(),
            selected_ais:         game.selected_ais.clone(),
            selected_stage:       game.selected_stage.clone(),
            rules:                game.rules.clone(),
            selected_players
        }
    }

    pub fn into_game_setup(self, debug: bool, start_at_last_frame: bool) -> GameSetup {
        let state = if start_at_last_frame {
            GameState::Local
        } else {
            GameState::ReplayForwardsFromHistory
        };
        GameSetup {
            init_seed:      self.init_seed,
            input_history:  self.input_history,
            player_history: self.player_history,
            stage_history:  self.stage_history,
            controllers:    self.selected_controllers,
            players:        self.selected_players,
            ais:            self.selected_ais,
            stage:          self.selected_stage,
            rules:          self.rules,
            state,
            start_at_last_frame,
            debug,
        }
    }
}
