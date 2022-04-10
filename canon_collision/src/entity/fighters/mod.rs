pub(crate) mod player;
pub(crate) mod toriel;

use crate::entity::components::action_state::ActionState;
use crate::entity::{ActionResult, StepContext};
use player::Player;
use toriel::Toriel;

#[derive(Clone, Serialize, Deserialize)]
pub enum Fighter {
    Toriel(Toriel),
}

pub trait FighterTrait {
    fn frame_step(
        &mut self,
        context: &mut StepContext,
        state: &ActionState,
    ) -> Option<ActionResult>;
    fn action_expired(
        &mut self,
        context: &mut StepContext,
        state: &ActionState,
    ) -> Option<ActionResult>;
}

impl Fighter {
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

    fn get_fighter_mut(&mut self) -> &mut dyn FighterTrait {
        match self {
            Fighter::Toriel(fighter) => fighter,
        }
    }

    pub fn action_step(
        &mut self,
        context: &mut StepContext,
        state: &ActionState,
    ) -> Option<ActionResult> {
        self.get_player_mut().action_step(context, state);
        self.get_fighter_mut().frame_step(context, state)
    }

    pub fn action_expired(
        &mut self,
        context: &mut StepContext,
        state: &ActionState,
    ) -> Option<ActionResult> {
        self.get_player_mut().action_expired(context, state)
    }
}
