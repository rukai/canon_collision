use crate::entity::{StepContext, ActionResult};
use crate::entity::components::body::Body;
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::toriel_oven::TorielOvenAction;

pub enum MessageTorielOven {
    KeepAlive,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TorielOven {
    pub owner_id: Option<usize>,
    /// Body needed so location can be attached to surface
    pub body: Body,
    pub keep_alive: bool,
}

impl TorielOven {
    pub fn new(owner_id: usize, body: Body) -> Self {
        TorielOven {
            body,
            owner_id: Some(owner_id),
            keep_alive: false,
        }
    }

    pub fn process_message(&mut self, message: &MessageTorielOven, _context: &mut StepContext, _state: &ActionState) -> Option<ActionResult> {
        match message {
            MessageTorielOven::KeepAlive => {
                self.keep_alive = true;
                None
            }
        }
    }

    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        let action_frames = context.entity_def.actions[state.action.as_ref()].frames.len() as i64;
        if state.frame + 1 >= action_frames {
            context.delete_self = true;
        }

        let result = match state.get_action() {
            Some(TorielOvenAction::EarlyEnd) => {
                if state.frame == 20 && self.keep_alive {
                    if context.input.b.value {
                        self.body.face_right = !self.body.face_right;
                        ActionResult::set_action_keep_frame(TorielOvenAction::AttackExtended)
                    } else {
                        ActionResult::set_action_keep_frame(TorielOvenAction::Attack)
                    }
                } else {
                    None
                }
            }
            Some(TorielOvenAction::AttackExtended) => None,
            Some(TorielOvenAction::Attack) => None,
            None => None,
        };


        self.keep_alive = false;
        result
    }
}
