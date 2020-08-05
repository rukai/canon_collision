use crate::collision::CollisionResult;
use crate::entity::{DebugEntity, StepContext, EntityKey};
use crate::location::Location;

use canon_collision_lib::entity_def::{EntityDef, ActionFrame};

use treeflection::KeyedContextVec;
use num_traits::FromPrimitive;

#[repr(u64)]
#[derive(Clone, PartialEq, Debug, ToPrimitive, FromPrimitive, EnumIter, IntoStaticStr, Serialize, Deserialize)]
pub enum ItemAction {
    Spawn,
    Idle,
    Fall,
    Held,
    Thrown,
    Dropped,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub owner_id: Option<usize>,
    pub entity_def_key: String,
    pub action: u64,
    pub frame: i64,
    pub frame_no_restart: i64,
    pub x_vel: f32,
    pub y_vel: f32,
    pub location: Location,
    pub face_right: bool,
}

impl Item {
    pub fn get_entity_frame<'a>(&self, entity_def: &'a EntityDef) -> Option<&'a ActionFrame> {
        if entity_def.actions.len() > self.action as usize {
            let frames = &entity_def.actions[self.action as usize].frames;
            if frames.len() > self.frame as usize {
                return Some(&frames[self.frame as usize]);
            }
        }
        None
    }

    pub fn action_hitlag_step(&mut self, context: &mut StepContext) {
        self.frame += 1;
        self.frame_step(context);
    }

    fn frame_step(&mut self, context: &mut StepContext) {
        let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as i64 - 1;

        if self.frame > last_action_frame {
            self.action_expired(context);
        }

    }

    pub fn physics_step(&mut self, context: &mut StepContext) {
        // TODO: Surely I can store all this logic in a shared Location enum
        match self.location.clone() {
            Location::Airbourne { x, y } => {
                let new_x = x + self.x_vel;
                let new_y = y + self.y_vel;
                if let Some(platform_i) = self.land_stage_collision(context, (x, y), (new_x, new_y)) {
                    let x = context.stage.surfaces[platform_i].world_x_to_plat_x(new_x);
                    //self.land(context, platform_i, x);
                } else {
                    self.location = Location::Airbourne { x: new_x, y: new_y };
                }
            }
            Location::Surface { platform_i, mut x } => {
                if let Some(platform) = context.stage.surfaces.get(platform_i) {
                    x += self.x_vel * platform.floor_angle().unwrap_or_default().cos();
                    //self.floor_move(context, platform, platform_i, x);
                }
                else {
                    self.location = Location::Airbourne { x: 0.0, y: 0.0 };
                    self.set_action(context, ItemAction::Fall);
                }
            }
            _ => { }
        }

        let blast = &context.stage.blast;
        let (x, y) = self.bps_xy(context);
        if x < blast.left() || x > blast.right() || y < blast.bot() || y > blast.top() {
            context.delete_self = true;
        }
    }

    fn action_expired(&mut self, context: &mut StepContext) {
        match ItemAction::from_u64(self.action) {
            None => panic!("Custom defined action expirations have not been implemented"),

            // Idle
            Some(ItemAction::Spawn)   => self.set_action(context, ItemAction::Idle),
            Some(ItemAction::Idle)    => self.set_action(context, ItemAction::Idle),
            Some(ItemAction::Fall)    => self.set_action(context, ItemAction::Fall),
            Some(ItemAction::Held)    => self.set_action(context, ItemAction::Held),
            Some(ItemAction::Thrown)  => self.set_action(context, ItemAction::Thrown),
            Some(ItemAction::Dropped) => self.set_action(context, ItemAction::Dropped),
        }
    }

    fn set_action(&mut self, context: &mut StepContext, action: ItemAction) {
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
                    self.set_action(context, ItemAction::Fall);
                }
                &CollisionResult::HitAtk { .. } => {
                    self.set_action(context, ItemAction::Fall);
                }
                &CollisionResult::HitShieldAtk { .. } => {
                    self.set_action(context, ItemAction::Fall);
                }
                &CollisionResult::ReflectAtk { .. } => {
                    // TODO
                    self.set_action(context, ItemAction::Fall);
                }
                &CollisionResult::AbsorbAtk { .. } => {
                    self.set_action(context, ItemAction::Fall);
                }
                _ => { }
            }
        }
    }

    pub fn debug_print(&self, entities: &KeyedContextVec<EntityDef>, debug: &DebugEntity, i: EntityKey) -> Vec<String> {
        let mut lines = vec!();
        let entity = &entities[self.entity_def_key.as_ref()];
        if debug.physics {
            lines.push(format!("Entity: {:?}  location: {:?}  x_vel: {:.5}, y_vel: {:.5}",
                i, self.location, self.x_vel, self.y_vel));
        }
        if debug.action {
            let action = ItemAction::from_u64(self.action).unwrap();
            let last_action_frame = entity.actions[self.action as usize].frames.len() as u64 - 1;
            let iasa = entity.actions[self.action as usize].iasa;

            lines.push(format!("Entity: {:?}  Item  action: {:?}  frame: {}/{}  frame no restart: {}  IASA: {}",
                i, action, self.frame, last_action_frame, self.frame_no_restart, iasa));
        }

        lines
    }
}
