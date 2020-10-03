use crate::entity::fighters::player::Player;
use crate::entity::{ActionResult, StepContext};
use crate::entity::components::action_state::ActionState;

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

    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.player.action_step(context, state)
    }
}
