use crate::collision::collision_box::CollisionResult;
use crate::entity::{Entities, DebugEntity, StepContext, EntityKey, ActionResult};
use crate::entity::components::body::{Body, PhysicsResult, Location};
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::EntityDef;

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
    pub body: Body,
}

impl Item {
    pub fn process_message(&mut self, message: &MessageItem, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match message {
            MessageItem::Thrown { x_vel, y_vel } => {
                let (x, y) = self.bps_xy(context, state);
                self.body.location = Location::Airbourne { x, y };
                self.body.x_vel = *x_vel;
                self.body.y_vel = *y_vel;
                ActionResult::set_action(ItemAction::Thrown)
            }
            MessageItem::Dropped => {
                let (x, y) = self.bps_xy(context, state);
                self.body.location = Location::Airbourne { x, y };
                ActionResult::set_action(ItemAction::Dropped)
            }
        }
    }

    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if let Some(action) = ItemAction::from_u64(state.action) {
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

        let action_frames = context.entity_def.actions[state.action as usize].frames.len() as i64;
        if state.frame + 1 >= action_frames {
            self.action_expired(state)
        } else {
            None
        }
    }

    pub fn grabbed(&mut self, grabbed_by_key: EntityKey, grabbed_by_id: Option<usize>) -> Option<ActionResult> {
        self.body.location = Location::ItemHeldByPlayer (grabbed_by_key);
        self.owner_id = grabbed_by_id;
        ActionResult::set_action(ItemAction::Held)
    }

    pub fn physics_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        let fighter_frame = &context.entity_def.actions[state.action as usize].frames[state.frame as usize];
        match self.body.physics_step(context, state, fighter_frame) {
            Some(PhysicsResult::Fall) => ActionResult::set_action(ItemAction::Fall),
            Some(PhysicsResult::Land) => ActionResult::set_action(ItemAction::Idle),
            Some(PhysicsResult::OutOfBounds) => {
                context.delete_self = true;
                None
            }
            _ => None
        }
    }

    pub fn bps_xy(&self, context: &StepContext, state: &ActionState) -> (f32, f32) {
        let action_frame = state.get_entity_frame(&context.entity_defs[state.entity_def_key.as_ref()]);
        self.body.public_bps_xy(&context.entities, &context.entity_defs, action_frame, &context.surfaces, state)
    }

    fn action_expired(&mut self, state: &ActionState) -> Option<ActionResult> {
        ActionResult::set_action(match ItemAction::from_u64(state.action) {
            None => panic!("Custom defined action expirations have not been implemented"),

            // Idle
            Some(ItemAction::Spawn)   => ItemAction::Idle,
            Some(ItemAction::Idle)    => ItemAction::Idle,
            Some(ItemAction::Fall)    => ItemAction::Fall,
            Some(ItemAction::Held)    => ItemAction::Held,
            Some(ItemAction::Thrown)  => ItemAction::Thrown,
            Some(ItemAction::Dropped) => ItemAction::Dropped,
        })
    }

    pub fn step_collision(&mut self, col_results: &[CollisionResult]) -> Option<ActionResult> {
        let mut set_action = None;

        for col_result in col_results {
            match col_result {
                &CollisionResult::Clang { .. } => {
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::HitAtk { .. } => {
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::HitShieldAtk { .. } => {
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::ReflectAtk { .. } => {
                    // TODO
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::AbsorbAtk { .. } => {
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                _ => { }
            }
        }
        set_action
    }

    pub fn debug_print(&self, entities: &KeyedContextVec<EntityDef>, state: &ActionState, debug: &DebugEntity, index: EntityKey) -> Vec<String> {
        let mut lines = vec!();
        if debug.action {
            lines.push(state.debug_string::<ItemAction>(entities, index));
        }
        if debug.physics {
            lines.push(self.body.debug_string(index));
        }

        lines
    }

    pub fn held_render_angle(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>) -> Option<Quaternion<f32>> {
        match self.body.location {
            Location::ItemHeldByPlayer (player_i) => {
                entities.get(player_i)
                    .and_then(|player| player.get_entity_frame(&entity_defs[player.state.entity_def_key.as_ref()]))
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
