use crate::entity::components::action_state::ActionState;
use crate::entity::components::body::{Body, Location};
use crate::entity::fighters::FighterTrait;
use crate::entity::fighters::player::Player;
use crate::entity::item::Item;
use crate::entity::projectile::Projectile;
use crate::entity::toriel_fireball::TorielFireball;
use crate::entity::{Entity, EntityType, ActionResult, StepContext};

use canon_collision_lib::entity_def::player::PlayerAction;
use canon_collision_lib::entity_def::projectile::ProjectileAction;
use canon_collision_lib::entity_def::toriel::TorielAction;
use canon_collision_lib::entity_def::toriel_fireball::TorielFireballAction;
use canon_collision_lib::entity_def::item::ItemAction;

use std::f32::consts::PI;

#[derive(Clone, Serialize, Deserialize)]
pub struct Toriel {
    pub player: Player,
}

impl Toriel {
    pub fn new(player: Player) -> Toriel {
        Toriel {
            player,
        }
    }
}

impl FighterTrait for Toriel {
    fn frame_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match state.get_action() {
            // Specials
            Some(TorielAction::DspecialGroundStart) => self.d_special_ground_start_action(context, state),
            Some(TorielAction::DspecialAirStart)    => self.d_special_air_start_action(context, state),

            Some(TorielAction::SspecialGroundStart) => self.s_special_ground_start_action(context, state),
            Some(TorielAction::SspecialAirStart)    => self.s_special_air_start_action(context, state),

            Some(TorielAction::NspecialGroundStart) => self.n_special_ground_start_action(context, state),
            Some(TorielAction::NspecialAirStart)    => self.n_special_air_start_action(context, state),

            // Throws
            Some(TorielAction::Uthrow) => self.u_throw_action(context, state),
            Some(TorielAction::Dthrow) => self.d_throw_action(context, state),
            Some(TorielAction::Fthrow) => self.f_throw_action(context, state),
            Some(TorielAction::Bthrow) => self.b_throw_action(context, state),

            None => self.player.frame_step(context, state)
        }
    }

    fn action_expired(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match state.get_action() {
            // Specials
            Some(TorielAction::DspecialGroundStart) => ActionResult::set_action(PlayerAction::Idle),
            Some(TorielAction::SspecialGroundStart) => ActionResult::set_action(PlayerAction::Idle),
            Some(TorielAction::NspecialGroundStart) => ActionResult::set_action(PlayerAction::Idle),
            Some(TorielAction::DspecialAirStart)    => ActionResult::set_action(PlayerAction::Fall),
            Some(TorielAction::SspecialAirStart)    => ActionResult::set_action(PlayerAction::Fall),
            Some(TorielAction::NspecialAirStart)    => ActionResult::set_action(PlayerAction::Fall),

            // Throws
            Some(TorielAction::Uthrow) => ActionResult::set_action(PlayerAction::Idle),
            Some(TorielAction::Dthrow) => ActionResult::set_action(PlayerAction::Idle),
            Some(TorielAction::Fthrow) => ActionResult::set_action(PlayerAction::Idle),
            Some(TorielAction::Bthrow) => ActionResult::set_action(PlayerAction::Idle),

            None => self.player.action_expired(context, state),
        }
    }
}

impl Toriel {
    fn d_special_ground_start_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.player.ground_idle_action(context, state)
            .or_else(|| self.d_special_start(context, state))
    }

    fn d_special_air_start_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.player.aerial_action(context, state)
            .or_else(|| self.d_special_start(context, state))
    }

    fn d_special_start(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 5 {
            let (x, y) = self.player.bps_xy(context, state);
            let x = x + self.relative_f(10.0);
            let y = y + 10.0;
            context.new_entities.push(Entity {
                ty: EntityType::Item(
                    Item {
                        owner_id: None,
                        body: Body::new(Location::Airbourne { x, y }, true),
                    }
                ),
                state: ActionState::new(
                    "PerfectlyGenericObject.cbor".to_string(),
                    ItemAction::Fall
                ),
            });
        }
        None
    }

    fn s_special_ground_start_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.player.ground_idle_action(context, state)
            .or_else(|| self.s_special_start(context, state))
    }

    fn s_special_air_start_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.player.aerial_action(context, state)
            .or_else(|| self.s_special_start(context, state))
    }

    fn s_special_start(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 20 {
            let (x, y) = self.player.bps_xy(context, state);
            context.new_entities.push(Entity {
                ty: EntityType::Projectile(
                    Projectile {
                        owner_id: Some(self.player.id),
                        speed: 0.6,
                        angle: if self.player.body.face_right { 0.0 } else { PI },
                        x: x + self.relative_f(2.0),
                        y: y + 10.0,
                    }
                ),
                state: ActionState::new(
                    "PerfectlyGenericProjectile.cbor".to_string(),
                    ProjectileAction::Spawn
                ),
            });
        }
        None
    }

    fn n_special_ground_start_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.player.ground_idle_action(context, state)
            .or_else(|| self.n_special_start(context, state))
    }

    fn n_special_air_start_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.player.aerial_action(context, state)
            .or_else(|| self.n_special_start(context, state))
    }

    fn n_special_start(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 5 {
            let (x, y) = self.player.bps_xy(context, state);
            context.new_entities.push(Entity {
                ty: EntityType::TorielFireball(
                    TorielFireball {
                        owner_id: Some(self.player.id),
                        face_right: self.player.body.face_right,
                        x: x - self.relative_f(4.0),
                        y: y + 12.5,
                        y_vel: 2.2,
                        x_sin_counter: 0.0,
                        x_sin_origin: 0.0,
                    }
                ),
                state: ActionState::new(
                    "TorielFireball.cbor".to_string(),
                    TorielFireballAction::Spawn
                ),
            });
        }
        None
    }

    fn u_throw_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 5 {
            // TODO: lets make this a struct instead of commenting the args.
            self.player.send_thrown_message(
                context,
                85.0, // angle
                5.0,  // damage
                80.0, // bkb
                1.1   // kbg
            );
        }
        None
    }

    fn d_throw_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 5 {
            self.player.send_thrown_message(
                context,
                -90.0, // angle
                5.0,   // damage
                80.0,  // bkb
                1.1    // kbg
            );
        }
        None
    }

    fn f_throw_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 5 {
            self.player.send_thrown_message(
                context,
                30.0, // angle
                5.0,  // damage
                80.0, // bkb
                1.1   // kbg
            );
        }
        None
    }

    fn b_throw_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 5 {
            self.player.send_thrown_message(
                context,
                170.0, // angle
                5.0,   // damage
                80.0,  // bkb
                1.1    // kbg
            );
        }
        None
    }

    fn relative_f(&self, input: f32) -> f32 {
        self.player.body.relative_f(input)
    }
}
