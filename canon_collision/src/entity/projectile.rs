use crate::collision::collision_box::CollisionResult;
use crate::entity::{DebugEntity, StepContext, EntityKey, ActionResult};
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::EntityDef;

use treeflection::KeyedContextVec;
use num_traits::FromPrimitive;

#[repr(u64)]
#[derive(Clone, PartialEq, Debug, ToPrimitive, FromPrimitive, EnumIter, IntoStaticStr, Serialize, Deserialize)]
pub enum ProjectileAction {
    Spawn,
    Travel,
    Hit,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Projectile {
    pub owner_id: Option<usize>,
    pub angle: f32,
    pub speed: f32,
    pub x: f32,
    pub y: f32,
}

impl Projectile {
    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match ProjectileAction::from_u64(state.action) {
            Some(ProjectileAction::Travel) => {
                self.x += self.angle.cos() * self.speed;
                self.y += self.angle.sin() * self.speed;
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

    fn action_expired(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        ActionResult::set_action(match ProjectileAction::from_u64(state.action) {
            None => panic!("Custom defined action expirations have not been implemented"),

            // Idle
            Some(ProjectileAction::Spawn)    => ProjectileAction::Travel,
            Some(ProjectileAction::Travel)   => ProjectileAction::Travel,
            Some(ProjectileAction::Hit) => {
                context.delete_self = true;
                ProjectileAction::Hit
            }
        })
    }

    pub fn step_collision(&mut self, col_results: &[CollisionResult]) -> Option<ActionResult> {
        let mut set_action = None;

        for col_result in col_results {
            match col_result {
                &CollisionResult::Clang { .. } => {
                    set_action = ActionResult::set_action(ProjectileAction::Hit);
                }
                &CollisionResult::HitAtk { .. } => {
                    set_action = ActionResult::set_action(ProjectileAction::Hit);
                }
                &CollisionResult::HitShieldAtk { .. } => {
                    set_action = ActionResult::set_action(ProjectileAction::Hit);
                }
                &CollisionResult::ReflectAtk { .. } => {
                    // TODO
                    set_action = ActionResult::set_action(ProjectileAction::Hit);
                }
                &CollisionResult::AbsorbAtk { .. } => {
                    set_action = ActionResult::set_action(ProjectileAction::Hit);
                }
                _ => { }
            }
        }
        set_action
    }

    pub fn debug_print(&self, entities: &KeyedContextVec<EntityDef>, state: &ActionState, debug: &DebugEntity, i: EntityKey) -> Vec<String> {
        let mut lines = vec!();
        let entity = &entities[state.entity_def_key.as_ref()];
        if debug.physics {
            lines.push(format!("Entity: {:?}  location: {:?}  angle: {:.5}",
                i, (self.x, self.y), self.angle));
        }
        if debug.action {
            let action = ProjectileAction::from_u64(state.action).unwrap();
            let last_action_frame = entity.actions[state.action as usize].frames.len() as u64 - 1;
            let iasa = entity.actions[state.action as usize].iasa;

            lines.push(format!("Entity: {:?}  Projectile  action: {:?}  frame: {}/{}  frame no restart: {}  IASA: {}",
                i, action, state.frame, last_action_frame, state.frame_no_restart, iasa));
        }

        lines
    }
}
