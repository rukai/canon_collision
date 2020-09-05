use crate::collision::collision_box::CollisionResult;
use crate::entity::{DebugEntity, StepContext, EntityKey, ActionResult};
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::EntityDef;

use num_traits::FromPrimitive;
use treeflection::KeyedContextVec;

#[repr(u64)]
#[derive(Clone, PartialEq, Debug, ToPrimitive, FromPrimitive, EnumIter, IntoStaticStr, Serialize, Deserialize)]
pub enum TorielFireballAction {
    Spawn,
    Travel,
    Hit,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TorielFireball {
    // TODO: Probably need a body to handle collision with the stage, shouldnt be too bad though.
    pub owner_id: Option<usize>,
    pub face_right: bool,
    pub x: f32,
    pub y: f32,
    pub y_vel: f32,
    pub x_sin_counter: f32,
    pub x_sin_origin: f32,
}

impl TorielFireball {
    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match TorielFireballAction::from_u64(state.action) {
            Some(TorielFireballAction::Travel) => {
                if self.y_vel < -0.2 {
                    self.x_sin_counter += 0.07;
                    self.x = self.x_sin_origin + self.relative_f(self.x_sin_counter.sin() * 6.0);
                } else {
                    self.y_vel -= 0.08;
                    self.x += self.relative_f(1.5);
                    self.x_sin_origin = self.x;
                }
                self.y += self.y_vel;
            }
            _ => { }
        }

        let blast = &context.stage.blast;
        if self.x < blast.left() || self.x > blast.right() || self.y < blast.bot() || self.y > blast.top() {
            context.delete_self = true;
        }

        let action_frames = context.entity_def.actions[state.action as usize].frames.len() as i64;
        if state.frame + 1 >= action_frames {
            self.action_expired(context, state)
        } else {
            None
        }
    }

    pub fn relative_f(&self, input: f32) -> f32 {
        input * if self.face_right { 1.0 } else { -1.0 }
    }

    fn action_expired(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        ActionResult::set_action(match TorielFireballAction::from_u64(state.action) {
            None => panic!("Custom defined action expirations have not been implemented"),

            // Idle
            Some(TorielFireballAction::Spawn)    => TorielFireballAction::Travel,
            Some(TorielFireballAction::Travel)   => TorielFireballAction::Travel,
            Some(TorielFireballAction::Hit) => {
                context.delete_self = true;
                TorielFireballAction::Hit
            }
        })
    }

    pub fn step_collision(&mut self, col_results: &[CollisionResult]) -> Option<ActionResult> {
        let mut set_action = None;

        for col_result in col_results {
            match col_result {
                &CollisionResult::Clang { .. } => {
                    set_action = ActionResult::set_action(TorielFireballAction::Hit);
                }
                &CollisionResult::HitAtk { .. } => {
                    set_action = ActionResult::set_action(TorielFireballAction::Hit);
                }
                &CollisionResult::HitShieldAtk { .. } => {
                    set_action = ActionResult::set_action(TorielFireballAction::Hit);
                }
                &CollisionResult::ReflectAtk { .. } => {
                    // TODO
                    set_action = ActionResult::set_action(TorielFireballAction::Hit);
                }
                &CollisionResult::AbsorbAtk { .. } => {
                    set_action = ActionResult::set_action(TorielFireballAction::Hit);
                }
                _ => { }
            }
        }
        set_action
    }

    pub fn debug_print(&self, entities: &KeyedContextVec<EntityDef>, state: &ActionState, debug: &DebugEntity, index: EntityKey) -> Vec<String> {
        let mut lines = vec!();
        if debug.action {
            lines.push(state.debug_string::<TorielFireballAction>(entities, index));
        }
        if debug.physics {
            //lines.push(format!("Entity: {:?}  location: {:?}  angle: {:.5}",
                //index, (self.x, self.y), self.angle));
        }

        lines
    }
}
