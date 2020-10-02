use crate::collision::collision_box::CollisionResult;
use crate::entity::{Entities, StepContext, EntityKey, ActionResult};
use crate::entity::components::body::{Body, PhysicsResult, Location};
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::EntityDef;
use canon_collision_lib::entity_def::item::ItemAction;

use cgmath::Quaternion;
use treeflection::KeyedContextVec;

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
        if let Some(action) = state.get_action() {
            match action {
                ItemAction::Held => { }
                ItemAction::Spawn |
                ItemAction::Idle => {
                    self.owner_id = None;
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

        let action_frames = context.entity_def.actions[state.action.as_ref()].frames.len() as i64;
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
        let fighter_frame = &context.entity_def.actions[state.action.as_ref()].frames[state.frame as usize];
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
        ActionResult::set_action(match state.get_action() {
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

    pub fn step_collision(&mut self, context: &mut StepContext, state: &ActionState, col_results: &[CollisionResult]) -> Option<ActionResult> {
        let mut set_action = None;

        for col_result in col_results {
            match col_result {
                &CollisionResult::Clang { .. } => {
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::HitAtk { .. } => {
                    // TODO: implement better bounce logic (put this logic in the Body)
                    self.body.x_vel *= -0.5;
                    self.body.y_vel *= -0.5;
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::HitDef { ref hitbox, ref hurtbox, entity_atk_i } => {
                    let action_frame = state.get_entity_frame(&context.entity_defs[state.entity_def_key.as_ref()]);
                    let kb_vel_mult = 1.0;
                    self.body.launch(context, state, action_frame, hitbox, hurtbox, entity_atk_i, kb_vel_mult);
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::HitShieldAtk { .. } => {
                    // TODO: implement better bounce logic (put this logic in the Body)
                    self.body.x_vel *= -0.5;
                    self.body.y_vel *= -0.5;
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                &CollisionResult::ReflectAtk { entity_def_i, .. } => {
                    // TODO: implement better reflect logic, maybe the reflect hitbox should have a `set_angle: Option<f32>`
                    self.owner_id = context.entities.get(entity_def_i).and_then(|x| x.player_id());
                    self.body.x_vel *= -1.0;
                    self.body.y_vel *= -1.0;
                    set_action = ActionResult::set_action(ItemAction::Fall);
                }
                _ => { }
            }
        }
        set_action
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
