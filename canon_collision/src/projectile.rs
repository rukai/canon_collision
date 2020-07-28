use crate::collision::CollisionResult;
use crate::entity::{DebugEntity, StepContext};

use canon_collision_lib::fighter::{Fighter, ActionFrame};

use treeflection::KeyedContextVec;
use num_traits::FromPrimitive;

#[repr(u64)]
#[derive(Clone, PartialEq, Debug, ToPrimitive, FromPrimitive, EnumIter, IntoStaticStr, Serialize, Deserialize)]
pub enum ProjectileAction {
    Spawn,
    Travel,
    Hit,
}

impl Default for ProjectileAction {
    fn default() -> ProjectileAction {
        ProjectileAction::Spawn
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Projectile {
    pub entity_def_key: String,
    pub action: u64,
    pub frame: i64,
    pub frame_no_restart: i64,
    pub angle: f32,
    pub speed: f32,
    pub x: f32,
    pub y: f32,
}

impl Projectile {
    pub fn get_fighter_frame<'a>(&self, fighter: &'a Fighter) -> Option<&'a ActionFrame> {
        if fighter.actions.len() > self.action as usize {
            let fighter_frames = &fighter.actions[self.action as usize].frames;
            if fighter_frames.len() > self.frame as usize {
                return Some(&fighter_frames[self.frame as usize]);
            }
        }
        None
    }

    pub fn action_hitlag_step(&mut self, context: &mut StepContext) {
        self.frame += 1;
        self.frame_step(context);
    }

    fn frame_step(&mut self, context: &mut StepContext) {
        let last_action_frame = context.fighter.actions[self.action as usize].frames.len() as i64 - 1;

        match ProjectileAction::from_u64(self.action) {
            Some(ProjectileAction::Travel) => {
                self.x += self.angle.cos() * self.speed;
                self.y += self.angle.sin() * self.speed;
            }
            _ => { }
        }

        if self.frame > last_action_frame {
            self.action_expired(context);
        }

        let blast = &context.stage.blast;
        if self.x < blast.left() || self.x > blast.right() || self.y < blast.bot() || self.y > blast.top() {
            context.delete_self = true;
        }
    }

    fn action_expired(&mut self, context: &mut StepContext) {
        match ProjectileAction::from_u64(self.action) {
            None => panic!("Custom defined action expirations have not been implemented"),

            // Idle
            Some(ProjectileAction::Spawn)    => self.set_action(context, ProjectileAction::Travel),
            Some(ProjectileAction::Travel)   => self.set_action(context, ProjectileAction::Travel),
            Some(ProjectileAction::Hit) => {
                context.delete_self = true;
            }
        }
    }

    fn set_action(&mut self, context: &mut StepContext, action: ProjectileAction) {
        let action = action as u64;
        self.frame = 0;

        if self.action != action {
            self.frame_no_restart = 0;
            self.action = action;

            self.frame_step(context);
        }
    }

    pub fn step_collision(&mut self, context: &mut StepContext, col_results: &[CollisionResult]) {
        for col_result in col_results {
            match col_result {
                &CollisionResult::Clang { .. } => {
                    self.set_action(context, ProjectileAction::Hit);
                }
                &CollisionResult::HitAtk { .. } => {
                    self.set_action(context, ProjectileAction::Hit);
                }
                &CollisionResult::HitShieldAtk { .. } => {
                    self.set_action(context, ProjectileAction::Hit);
                }
                &CollisionResult::ReflectAtk { .. } => {
                    // TODO
                    self.set_action(context, ProjectileAction::Hit);
                }
                &CollisionResult::AbsorbAtk { .. } => {
                    self.set_action(context, ProjectileAction::Hit);
                }
                _ => { }
            }
        }
    }

    pub fn debug_print(&self, fighters: &KeyedContextVec<Fighter>, debug: &DebugEntity, index: usize) -> Vec<String> {
        let mut lines = vec!();
        let fighter = &fighters[self.entity_def_key.as_ref()];
        if debug.physics {
            lines.push(format!("Entity: {}  location: {:?}  angle: {:.5}",
                index, (self.x, self.y), self.angle));
        }
        if debug.action {
            let action = ProjectileAction::from_u64(self.action).unwrap();
            let last_action_frame = fighter.actions[self.action as usize].frames.len() as u64 - 1;
            let iasa = fighter.actions[self.action as usize].iasa;

            lines.push(format!("Entity: {}  Projectile  action: {:?}  frame: {}/{}  frame no restart: {}  IASA: {}",
                index, action, self.frame, last_action_frame, self.frame_no_restart, iasa));
        }

        lines
    }
}
