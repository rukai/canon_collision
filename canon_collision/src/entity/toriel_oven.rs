use crate::entity::{StepContext, ActionResult};
use crate::entity::components::body::Body;
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::toriel_oven::TorielOvenAction;

pub enum MessageTorielOven {
    Attack,
    AttackExtended,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TorielOven {
    pub owner_id: Option<usize>,
    /// Body needed so location can be attached to surface
    pub body: Body,
}

impl TorielOven {
    pub fn process_message(&mut self, message: &MessageTorielOven, _context: &mut StepContext, _state: &ActionState) -> Option<ActionResult> {
        match message {
            MessageTorielOven::Attack => {
                ActionResult::set_action_keep_frame(TorielOvenAction::Attack)
            }
            MessageTorielOven::AttackExtended => {
                self.body.face_right = !self.body.face_right;
                ActionResult::set_action_keep_frame(TorielOvenAction::AttackExtended)
            }
        }
    }

    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        let action_frames = context.entity_def.actions[state.action.as_ref()].frames.len() as i64;
        if state.frame + 1 >= action_frames {
            context.delete_self = true;
        }

        None
    }
}
