use crate::collision::collision_box::CollisionResult;
use crate::entity::components::action_state::ActionState;
use crate::entity::{ActionResult, DebugEntity, EntityKey, StepContext};

use canon_collision_lib::entity_def::projectile::ProjectileAction;

#[derive(Clone, Serialize, Deserialize)]
pub struct Projectile {
    pub owner_id: Option<usize>,
    pub angle: f32,
    pub speed: f32,
    pub x: f32,
    pub y: f32,
}

impl Projectile {
    pub fn action_step(
        &mut self,
        context: &mut StepContext,
        state: &ActionState,
    ) -> Option<ActionResult> {
        match state.get_action() {
            Some(ProjectileAction::Travel) => {
                self.x += self.angle.cos() * self.speed;
                self.y += self.angle.sin() * self.speed;
            }
            _ => {}
        }

        let blast = &context.stage.blast;
        if self.x < blast.left()
            || self.x > blast.right()
            || self.y < blast.bot()
            || self.y > blast.top()
        {
            context.delete_self = true;
        }

        let action_frames = context.entity_def.actions[state.action.as_ref()]
            .frames
            .len() as i64;
        if state.frame + 1 >= action_frames {
            self.action_expired(context, state)
        } else {
            None
        }
    }

    fn action_expired(
        &mut self,
        context: &mut StepContext,
        state: &ActionState,
    ) -> Option<ActionResult> {
        ActionResult::set_action(match state.get_action() {
            None => panic!("Custom defined action expirations have not been implemented"),

            // Idle
            Some(ProjectileAction::Spawn) => ProjectileAction::Travel,
            Some(ProjectileAction::Travel) => ProjectileAction::Travel,
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
                _ => {}
            }
        }
        set_action
    }

    pub fn debug_print(&self, debug: &DebugEntity, index: EntityKey) -> Vec<String> {
        let mut lines = vec![];
        if debug.physics {
            lines.push(format!(
                "Entity: {:?}  location: {:?}  angle: {:.5}",
                index,
                (self.x, self.y),
                self.angle
            ));
        }

        lines
    }
}
