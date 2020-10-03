pub(crate) mod toriel;
pub(crate) mod player;

use toriel::Toriel;
use player::Player;
use crate::entity::{ActionResult, StepContext};
use crate::entity::components::action_state::ActionState;

#[derive(Clone, Serialize, Deserialize)]
pub enum Fighter {
    Toriel(Toriel),
}

impl Fighter {
    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match self {
            Fighter::Toriel(fighter) => fighter.action_step(context, state),
        }
    }

    pub fn get_player(&self) -> &Player {
        match self {
            Fighter::Toriel(fighter) => &fighter.player,
        }
    }

    pub fn get_player_mut(&mut self) -> &mut Player {
        match self {
            Fighter::Toriel(fighter) => &mut fighter.player,
        }
    }
}
