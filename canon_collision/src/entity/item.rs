use crate::collision::collision_box::CollisionResult;
use crate::entity::{Entities, DebugEntity, StepContext, EntityKey};
use crate::entity::components::body::{Body, PhysicsResult, Location};

use canon_collision_lib::entity_def::{EntityDef, ActionFrame};

use cgmath::Quaternion;
use num_traits::FromPrimitive;
use treeflection::KeyedContextVec;

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

pub enum MessageItem {
    Thrown { x_vel: f32, y_vel: f32 },
    Dropped,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub owner_id: Option<usize>,
    pub entity_def_key: String,
    pub action: u64,
    pub frame: i64,
    pub frame_no_restart: i64,
    pub body: Body,
}

impl Item {
    pub fn process_message(&mut self, message: &MessageItem, context: &mut StepContext) {
        match message {
            MessageItem::Thrown { x_vel, y_vel } => {
                let (x, y) = self.bps_xy(context);
                self.body.location = Location::Airbourne { x, y };
                self.body.x_vel = *x_vel;
                self.body.y_vel = *y_vel;
                self.set_action(context, ItemAction::Thrown);
            }
            MessageItem::Dropped => {
                let (x, y) = self.bps_xy(context);
                self.body.location = Location::Airbourne { x, y };
                self.set_action(context, ItemAction::Dropped);
            }
        }
    }

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

        if let Some(action) = ItemAction::from_u64(self.action) {
            match action {
                ItemAction::Held => { }
                ItemAction::Spawn |
                ItemAction::Idle => {
                    self.body.apply_friction_strong(&context.entity_def);
                }

                ItemAction::Thrown |
                ItemAction::Fall |
                ItemAction::Dropped => {
                    self.body.y_vel += context.entity_def.gravity;
                    if self.body.y_vel < context.entity_def.terminal_vel {
                        self.body.y_vel = context.entity_def.terminal_vel;
                    }
                }
            }
        }
    }

    pub fn grabbed(&mut self, context: &mut StepContext, grabbed_by_key: EntityKey, grabbed_by_id: Option<usize>) {
        self.set_action(context, ItemAction::Held);
        self.body.location = Location::ItemHeldByPlayer (grabbed_by_key);
        self.owner_id = grabbed_by_id;
    }

    pub fn physics_step(&mut self, context: &mut StepContext) {
        let fighter_frame = &context.entity_def.actions[self.action as usize].frames[self.frame as usize];
        match self.body.physics_step(context, fighter_frame) {
            Some(PhysicsResult::Fall) => {
                self.set_action(context, ItemAction::Fall);
            }
            Some(PhysicsResult::Land) => {
                self.set_action(context, ItemAction::Idle);
            }
            Some(PhysicsResult::OutOfBounds) => {
                context.delete_self = true;
            }
            _ => { }
        }
    }

    pub fn bps_xy(&self, context: &StepContext) -> (f32, f32) {
        let action_frame = self.get_entity_frame(&context.entity_defs[self.entity_def_key.as_ref()]);
        self.body.public_bps_xy(&context.entities, &context.entity_defs, action_frame, &context.surfaces)
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
            lines.push(self.body.debug_string(i));
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

    pub fn held_render_angle(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>) -> Option<Quaternion<f32>> {
        match self.body.location {
            Location::ItemHeldByPlayer (player_i) => {
                entities.get(player_i)
                    .and_then(|player| player.get_entity_frame(&entity_defs[player.entity_def_key()]))
                    .and_then(|action_frame| action_frame.item_hold.as_ref())
                    .and_then(|item_hold| Some(Quaternion::new(
                        item_hold.quaternion_x,
                        item_hold.quaternion_y,
                        item_hold.quaternion_z,
                        item_hold.quaternion_rotation
                    )))
            }
            _ => None
        }
    }
}
