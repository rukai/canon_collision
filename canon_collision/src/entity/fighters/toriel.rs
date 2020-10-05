use crate::entity::fighters::player::Player;
use crate::entity::fighters::FighterTrait;
use crate::entity::{ActionResult, StepContext};
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::toriel::TorielAction;
use canon_collision_lib::entity_def::player::PlayerAction;

#[derive(Clone, Serialize, Deserialize)]
pub struct Toriel {
    pub player: Player,
}

impl Toriel {
    pub fn new(player: Player) -> Toriel {
        Toriel {
            player,
        }
    }
}

impl FighterTrait for Toriel {
    fn frame_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match state.get_action() {
            Some(TorielAction::NspecialStart) => None,
            None => self.player.frame_step(context, state)
        }
    }

    fn action_expired(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match state.get_action() {
            Some(TorielAction::NspecialStart) => ActionResult::set_action(PlayerAction::Idle),
            None => self.player.action_expired(context, state),
        }
    }
}
