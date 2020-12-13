use crate::collision::collision_box::CollisionResult;
use crate::graphics;
use crate::particle::{Particle, ParticleType};
use crate::results::{RawPlayerResult, DeathRecord};
use crate::rules::{Goal, Rules};
use crate::entity::item::{MessageItem, Item};
use crate::entity::{Entity, EntityType, StepContext, DebugEntity, VectorArrow, Entities, EntityKey, Message, MessageContents, ActionResult};
use crate::entity::components::body::{Body, Location, PhysicsResult};
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::{EntityDef, HitStun, Shield, HitBox, HurtBox, HitboxEffect};
use canon_collision_lib::entity_def::player::PlayerAction;
use canon_collision_lib::entity_def::item::ItemAction;
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::package::Package;
use canon_collision_lib::stage::{Stage, Surface};

use treeflection::KeyedContextVec;
use rand::Rng;

use std::f32;
use std::f32::consts::PI;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LockTimer {
    Active (u64),
    Locked (u64),
    Free
}

impl LockTimer {
    pub fn is_active(&self) -> bool {
        match self {
            &LockTimer::Active (_) => true,
            _                      => false
        }
    }
}

pub enum MessagePlayer {
    Thrown { angle: f32, damage: f32, bkb: f32, kbg: f32, entity_atk_i: EntityKey },
    #[allow(dead_code)]
    Released,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Player {
    pub id:                 usize, // unique id among players
    pub team:               usize,
    pub body:               Body,
    pub stocks:             Option<u64>,
    pub ledge_idle_timer:   u64,
    pub fastfalled:         bool,
    pub air_jumps_left:     u64,
    pub jumpsquat_button:   bool,
    pub shield_hp:          f32,
    pub shield_analog:      f32,
    pub shield_offset_x:    f32,
    pub shield_offset_y:    f32,
    pub stun_timer:         u64,
    pub shield_stun_timer:  u64,
    pub parry_timer:        u64,
    pub tech_timer:         LockTimer,
    pub lcancel_timer:      u64,
    pub land_frame_skip:    u8,
    pub hitstun:            f32,
    /// this is only used for end-game statistics so player id is fine
    pub hit_by:             Option<usize>,
    pub particles:          Vec<Particle>,
    pub aerial_dodge_frame: Option<u64>,
    pub result:             RawPlayerResult,

    // Only use for debug display
    pub stick:   Option<(f32, f32)>,
    pub c_stick: Option<(f32, f32)>,
}

impl Player {
    pub fn new(entity_def_key: &str, team: usize, id: usize, stage: &Stage, package: &Package, rules: &Rules) -> Player {
        // get the spawn point
        let spawn = if stage.spawn_points.len() == 0 {
            None
        } else {
            Some(stage.spawn_points[id % stage.spawn_points.len()].clone())
        };

        let location = if let Some(spawn) = &spawn {
            // find the floor directly beneath the player
            struct FoundFloor {
                surface_i: usize,
                world_y:   f32, // The y coordinate of the point on the floor corresponding to the spawnpoints x coordinate.
            }
            let mut found_floor = None;
            for (surface_i, surface) in stage.surfaces.iter().enumerate() {
                let spawn_x_in_bounds = surface.world_x_in_bounds(spawn.x);
                let world_y = surface.world_x_to_world_y(spawn.x);
                let above_plat = world_y <= spawn.y;
                let closest = found_floor.as_ref().map(|x: &FoundFloor| x.world_y < world_y).unwrap_or(true);

                if surface.floor.is_some() && spawn_x_in_bounds && above_plat && closest {
                    found_floor = Some(FoundFloor { surface_i, world_y });
                }
            }

            if let Some(floor) = found_floor {
                let surface = &stage.surfaces[floor.surface_i];
                // place the player on the platform
                Location::Surface { platform_i: floor.surface_i, x: surface.world_x_to_plat_x(spawn.x) }
            } else {
                // no platform just make them airbourne at the exact spawnpoint
                Location::Airbourne { x: spawn.x, y: spawn.y }
            }
        } else {
            // There are no spawn points, we could attempt to find a floor surface and put the
            // player there, however there might still be no floor, so to keep this rare case simple we
            // just place the player in midair even though this interacts weirdly with PlayerAction::Idle
            // which is supposed to be grounded.
            Location::Airbourne { x: 0.0, y: 0.0 }
        };

        Player {
            stocks:             rules.stock_count,
            ledge_idle_timer:   0,
            fastfalled:         false,
            air_jumps_left:     package.entities[entity_def_key.as_ref()].fighter().map(|x| x.air_jumps).unwrap_or(1),
            jumpsquat_button:   false,
            shield_hp:          package.entities[entity_def_key.as_ref()].shield.as_ref().map_or(60.0, |x| x.hp_max),
            shield_analog:      0.0,
            shield_offset_x:    0.0,
            shield_offset_y:    0.0,
            stun_timer:         0,
            shield_stun_timer:  0,
            parry_timer:        0,
            tech_timer:         LockTimer::Free,
            lcancel_timer:      0,
            land_frame_skip:    0,
            hitstun:            0.0,
            hit_by:             None,
            particles:          vec!(),
            aerial_dodge_frame: None,
            result:             RawPlayerResult::default(),
            body: Body::new(
                location,
                spawn.map(|x| x.face_right).unwrap_or(false)
            ),
            id,
            team,

            // Only use for debug display
            stick:   None,
            c_stick: None,
        }
    }

    pub fn bps_xy(&self, context: &StepContext, state: &ActionState) -> (f32, f32) {
        let action_frame = state.get_entity_frame(&context.entity_defs[state.entity_def_key.as_ref()]);
        self.body.public_bps_xy(&context.entities, &context.entity_defs, action_frame, &context.surfaces, state)
    }

    pub fn public_bps_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface], state: &ActionState) -> (f32, f32) {
        let action_frame = state.get_entity_frame(&entity_defs[state.entity_def_key.as_ref()]);
        self.body.public_bps_xy(entities, entity_defs, action_frame, surfaces, state)
    }

    pub fn grabbing_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface], state: &ActionState) -> (f32, f32) {
        let (x, y) = self.public_bps_xy(entities, entity_defs, surfaces, state);
        if let Some(entity_frame) = state.get_entity_frame(&entity_defs[state.entity_def_key.as_ref()]) {
            (x + self.relative_f(entity_frame.grabbing_x), y + entity_frame.grabbing_y)
        } else {
            (x, y)
        }
    }

    pub fn is_shielding(&self, state: &ActionState) -> bool {
        match state.get_action() {
            Some(PlayerAction::Shield) |
            Some(PlayerAction::ShieldOn) |
            Some(PlayerAction::ShieldOff) |
            Some(PlayerAction::PowerShield)
              => true,
            _ => false
        }
    }

    pub fn set_airbourne(&mut self, context: &StepContext, state: &ActionState) {
        let (x, y) = self.bps_xy(context, state);
        self.fastfalled = false;
        self.body.location = Location::Airbourne { x, y };
    }

    pub fn platform_deleted(&mut self, entities: &Entities, fighters: &KeyedContextVec<EntityDef>, surfaces: &[Surface], deleted_platform_i: usize, state: &ActionState) -> Option<ActionResult> {
        let fall = match &mut self.body.location {
            &mut Location::Surface     { ref mut platform_i, .. } |
            &mut Location::GrabbedLedge { ref mut platform_i, .. } => {
                if *platform_i == deleted_platform_i {
                    true
                } else if *platform_i > deleted_platform_i {
                    *platform_i -= 1;
                    false
                } else { false }
            }
            _ => { false }
        };

        if fall {
            //manually perform self.set_airbourne(context, state);
            let (x, y) = self.public_bps_xy(entities, fighters, surfaces, state);
            self.body.location = Location::Airbourne { x, y };
            self.fastfalled = false;

            ActionResult::set_action(PlayerAction::Fall) // TODO: use miss step state ^.^
        } else {
            None
        }
    }

    fn launch(&mut self, context: &mut StepContext, state: &ActionState, hitbox: &HitBox, hurtbox: &HurtBox, entity_atk_i: EntityKey) -> Option<ActionResult> {
        self.hit_by = context.entities.get(entity_atk_i).and_then(|x| x.player_id());
        let kb_vel_mult = if let Some(PlayerAction::Crouch) = state.get_action() {
            0.67
        } else {
            1.0
        };

        let action_frame = state.get_entity_frame(&context.entity_defs[state.entity_def_key.as_ref()]);
        let kb_vel = self.body.launch(context, state, action_frame, hitbox, hurtbox, entity_atk_i, kb_vel_mult);

        if let Location::Airbourne { .. } = self.body.location {
            self.fastfalled = false;
            self.hitstun = match hitbox.hitstun {
                HitStun::FramesTimesKnockback (frames) => { frames * kb_vel }
                HitStun::Frames               (frames) => { frames as f32 }
            };
        }

        if kb_vel > 80.0 {
            ActionResult::set_action(PlayerAction::DamageFly)
        } else {
            ActionResult::set_action(PlayerAction::Damage)
        }
    }

    pub fn step_collision(&mut self, context: &mut StepContext, state: &ActionState, col_results: &[CollisionResult]) -> Option<ActionResult> {
        // TODO: Maybe we should provide a single col_result at a time so that we can handle all ActionResults.
        //       Ah! or maybe we should filter out the collisions that can should override other
        //       collisions, consistently giving priority to a specific type of collision.
        let mut set_action = None;
        for col_result in col_results {
            match col_result {
                &CollisionResult::HitAtk { ref hitbox, ref point, .. } => {
                    self.hit_particles(point.clone(), hitbox);
                }
                &CollisionResult::HitDef { ref hitbox, ref hurtbox, entity_atk_i } => {
                    set_action = self.launch(context, state, hitbox, hurtbox, entity_atk_i);
                }
                &CollisionResult::HitShieldAtk { ref hitbox, ref power_shield, entity_defend_i } => {
                    let entity_def = &context.entities[entity_defend_i];
                    if let Some(player_def) = &entity_def.ty.get_player() {
                        if let &Some(ref power_shield) = power_shield {
                            if let (Some(PlayerAction::PowerShield), &Some(ref stun)) = (state.get_action(), &power_shield.enemy_stun) {
                                if stun.window > entity_def.state.frame as u64 {
                                    self.stun_timer = stun.duration;
                                }
                            }
                        }

                        let x_diff = self.bps_xy(context, state).0 - player_def.bps_xy(context, state).0;
                        let vel = hitbox.damage.floor() * (player_def.shield_analog - 0.3) * 0.1 + 0.02;
                        if self.body.is_platform() {
                            self.body.x_vel += vel * x_diff.signum();
                        }
                    }
                }
                &CollisionResult::HitShieldDef { ref hitbox, ref power_shield, entity_atk_i } => {
                    if let &Some(ref power_shield) = power_shield {
                        if let (Some(PlayerAction::PowerShield), &Some(ref parry)) = (state.get_action(), &power_shield.parry) {
                            if parry.window > state.frame as u64 {
                                self.parry_timer = parry.duration;
                            }
                        }
                    }

                    if self.parry_timer == 0 {
                        self.shield_hp -= hitbox.shield_damage;
                        if self.shield_hp <= 0.0 {
                            continue;
                        }
                    }

                    let analog_mult = 1.0 - (self.shield_analog - 0.3) / 0.7;
                    let vel_mult = if self.parry_timer > 0 { 1.0 } else { 0.6 };
                    let x_diff = self.bps_xy(context, state).0 - context.entities[entity_atk_i].bps_xy(context).0;
                    let vel = (hitbox.damage.floor() * (0.195 * analog_mult + 0.09) + 0.4) * vel_mult;
                    self.body.x_vel = vel.min(2.0) * x_diff.signum();
                    self.shield_stun_timer = (hitbox.damage.floor() * (analog_mult + 0.3) * 0.975 + 2.0) as u64;
                }
                &CollisionResult::GrabAtk (_entity_defend_i) => {
                    set_action = ActionResult::set_action(PlayerAction::GrabbingIdle)
                }
                &CollisionResult::GrabDef (entity_atk_i) => {
                    self.body.face_right = !context.entities[entity_atk_i].face_right();
                    self.body.location = Location::GrabbedByPlayer(entity_atk_i);
                    set_action = ActionResult::set_action(PlayerAction::GrabbedIdle)
                }
                _ => { }
            }
        }
        set_action
    }

    /*
     *  Begin action section
     */

    pub fn action_step(&mut self, context: &mut StepContext, state: &ActionState) {
        self.knockback_particles(context, state);

        // TODO: Gankra plz ... https://github.com/rust-lang/rust/issues/43244
        let mut new_particles = vec!();
        for mut particle in self.particles.drain(..) {
            if !particle.step() {
                new_particles.push(particle);
            }
        }
        self.particles = new_particles;

        if !self.is_shielding(state) {
            if let Some(ref shield) = context.entity_def.shield {
                self.shield_hp = shield.hp_max.min(self.shield_hp + shield.hp_regen);
            }
        }

        if self.parry_timer > 0 {
            self.parry_timer -= 1;
        }

        if self.shield_stun_timer > 0 {
            self.shield_stun_timer -= 1;
        }

        if self.lcancel_timer > 0 {
            self.lcancel_timer -= 1;
        }
        else if context.input.l.press || context.input.r.press || context.input[0].l_trigger > 0.165 || context.input[0].r_trigger > 0.165 ||
            context.input.z.press && !(state.frame == 0 && state.get_action::<PlayerAction>().as_ref().map_or(false, |x| x.is_air_attack())) // only register z press if its not from an attack
        {
            if let &Some(ref lcancel) = &context.entity_def.lcancel {
                self.lcancel_timer = lcancel.active_window;
            }
        }

        self.tech_timer = match (self.tech_timer.clone(), context.entity_def.tech.clone()) {
            (LockTimer::Active (timer), Some(tech)) => {
                if timer > tech.active_window {
                    LockTimer::Locked(0)
                } else {
                    LockTimer::Active(timer + 1)
                }
            }
            (LockTimer::Locked (timer), Some(tech)) => {
                if timer > tech.locked_window {
                    LockTimer::Free
                } else {
                    LockTimer::Locked(timer + 1)
                }
            }
            (LockTimer::Free, Some(_)) => {
                if context.input.l.press || context.input.r.press {
                    LockTimer::Active(0)
                } else {
                    LockTimer::Free
                }
            }
            _ => {
                LockTimer::Free
            }
        };

        if context.input[0].stick_x == 0.0 && context.input[0].stick_y == 0.0 {
            self.stick = None;
        }
        else {
            self.stick = Some((context.input[0].stick_x, context.input[0].stick_y));
        }

        if context.input[0].c_stick_x == 0.0 && context.input[0].c_stick_y == 0.0 {
            self.c_stick = None;
        }
        else {
            self.c_stick = Some((context.input[0].c_stick_x, context.input[0].c_stick_y));
        }

        let fighter_frame = &context.entity_def.actions[state.action.as_ref()].frames[state.frame as usize];

        // update ecb
        let prev_bottom = self.body.ecb.bottom;
        self.body.ecb = fighter_frame.ecb.clone();
        match state.get_action() {
            Some(PlayerAction::JumpF) | Some(PlayerAction::JumpB) | Some(PlayerAction::JumpAerialF) | Some(PlayerAction::JumpAerialB) if state.frame < 10
                => { self.body.ecb.bottom = prev_bottom }
            _   => { }
        }
    }

    pub fn frame_step(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if let Some(action) = state.get_action() {
            match action {
                PlayerAction::Spawn => None,
                PlayerAction::ReSpawn => None,
                PlayerAction::ReSpawnIdle => self.spawn_idle(context, state),

                PlayerAction::AerialFall | PlayerAction::JumpAerialF |
                PlayerAction::Fair       | PlayerAction::Bair |
                PlayerAction::Dair       | PlayerAction::Uair |
                PlayerAction::Nair       | PlayerAction::JumpAerialB |
                PlayerAction::Fall
                => self.aerial_action(context, state),

                PlayerAction::JumpF      | PlayerAction::JumpB
                => self.jump_action(context, state),

                PlayerAction::Jab       | PlayerAction::Jab2 |
                PlayerAction::Jab3      | PlayerAction::Utilt |
                PlayerAction::Ftilt     | PlayerAction::DashAttack |
                PlayerAction::Dsmash    | PlayerAction::Fsmash |
                PlayerAction::Usmash    | PlayerAction::Idle |
                PlayerAction::Grab      | PlayerAction::DashGrab |
                PlayerAction::TauntUp   | PlayerAction::TauntDown |
                PlayerAction::TauntLeft | PlayerAction::TauntRight |
                PlayerAction::CrouchEnd
                => self.ground_idle_action(context, state),

                PlayerAction::ItemThrowU | PlayerAction::ItemThrowD |
                PlayerAction::ItemThrowF | PlayerAction::ItemThrowB
                => self.item_throw_action(context, state),

                PlayerAction::ItemThrowAirU | PlayerAction::ItemThrowAirD |
                PlayerAction::ItemThrowAirF | PlayerAction::ItemThrowAirB
                => self.item_throw_air_action(context, state),

                PlayerAction::FairLand | PlayerAction::BairLand |
                PlayerAction::UairLand | PlayerAction::DairLand |
                PlayerAction::NairLand | PlayerAction::SpecialLand
                => self.attack_land_action(context, state),

                PlayerAction::Teeter |
                PlayerAction::TeeterIdle       => self.teeter_action(context, state),
                PlayerAction::Land             => self.land_action(context, state),
                PlayerAction::DamageFly        => self.damage_fly_action(context),
                PlayerAction::DamageFall       => self.damage_fall_action(context, state),
                PlayerAction::Damage           => self.damage_action(context, state),
                PlayerAction::MissedTechIdle   => self.missed_tech_action(context, state),
                PlayerAction::MissedTechStart  => self.missed_tech_start_action(context.entity_def, state),
                PlayerAction::AerialDodge      => self.aerialdodge_action(context, state),
                PlayerAction::SpecialFall      => self.specialfall_action(context),
                PlayerAction::Dtilt            => self.dtilt_action(context, state),
                PlayerAction::CrouchStart      => self.crouch_start_action(context, state),
                PlayerAction::Crouch           => self.crouch_action(context, state),
                PlayerAction::Walk             => self.walk_action(context, state),
                PlayerAction::Dash             => self.dash_action(context, state),
                PlayerAction::Run              => self.run_action(context),
                PlayerAction::RunEnd           => self.run_end_action(context, state),
                PlayerAction::TiltTurn         => self.tilt_turn_action(context, state),
                PlayerAction::SmashTurn        => self.smash_turn_action(context, state),
                PlayerAction::RunTurn          => self.run_turn_action(context, state),
                PlayerAction::LedgeIdle        => self.ledge_idle_action(context, state),
                PlayerAction::ShieldOn         => self.shield_on_action(context, state),
                PlayerAction::PowerShield      => self.shield_action(context, state),
                PlayerAction::Shield           => self.shield_action(context, state),
                PlayerAction::ShieldOff        => self.shield_off_action(context, state),
                PlayerAction::ShieldBreakFall  => self.shield_break_fall_action(context.entity_def),
                PlayerAction::ShieldBreakGetup => self.shield_break_getup_action(),
                PlayerAction::Stun             => self.stun_action(context, state),
                PlayerAction::GrabbingIdle     => self.grabbing_idle_action(context, state),
                PlayerAction::GrabbedIdle      => self.grabbed_idle_action(context, state),
                _ => None,
            }
        } else {
            None
        }
    }

    fn ledge_idle_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if
            (context.input[0].  stick_y < -0.2 && context.input[1].  stick_y >= -0.2) ||
            (context.input[0].c_stick_y < -0.2 && context.input[1].c_stick_y >= -0.2) ||
            (self.relative_f(context.input[0].  stick_x) < -0.2 && self.relative_f(context.input[1].  stick_x) >= -0.2) ||
            (self.relative_f(context.input[0].c_stick_x) < -0.2 && self.relative_f(context.input[1].c_stick_x) >= -0.2)
        {
            self.set_airbourne(context, state);
            ActionResult::set_action(PlayerAction::Fall)
        }
        else if context.input.x.press || context.input.y.press || (context.input[0].stick_y > 0.65 && context.input[1].stick_y <= 0.65) {
            if self.body.damage < 100.0 {
                ActionResult::set_action(PlayerAction::LedgeJump)
            }
            else {
                ActionResult::set_action(PlayerAction::LedgeJumpSlow)
            }
        }
        else if
            (self.relative_f(context.input[0].stick_x) > 0.2 && self.relative_f(context.input[1].stick_x) <= 0.2) ||
            (context.input[0].stick_y > 0.2 && context.input[1].stick_y <= 0.2)
        {
            if self.body.damage < 100.0 {
                ActionResult::set_action(PlayerAction::LedgeGetup)
            }
            else {
                ActionResult::set_action(PlayerAction::LedgeGetupSlow)
            }
        }
        else if context.input.a.press || context.input.b.press || (context.input[0].c_stick_y > 0.65 && context.input[1].c_stick_x <= 0.65) {
            if self.body.damage < 100.0 {
                ActionResult::set_action(PlayerAction::LedgeAttack)
            }
            else {
                ActionResult::set_action(PlayerAction::LedgeAttackSlow)
            }
        }
        else if
            context.input.l.press || context.input.r.press ||
            (context.input[0].l_trigger > 0.3 && context.input[1].l_trigger <= 0.3) || (context.input[0].r_trigger > 0.3 && context.input[1].r_trigger <= 0.3) ||
            (self.relative_f(context.input[0].c_stick_x) > 0.8 && self.relative_f(context.input[1].c_stick_x) <= 0.8)
        {
            if self.body.damage < 100.0 {
                ActionResult::set_action(PlayerAction::LedgeRoll)
            }
            else {
                ActionResult::set_action(PlayerAction::LedgeRollSlow)
            }
        }
        else if self.ledge_idle_timer > 600 {
            self.set_airbourne(context, state);
            ActionResult::set_action(PlayerAction::DamageFall)
        }
        else {
            self.ledge_idle_timer += 1;
            None
        }
    }

    fn missed_tech_start_action(&mut self, fighter: &EntityDef, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 0 {
            self.body.x_vel = 0.0;
        } else {
            self.apply_friction(fighter, state);
        }
        None
    }

    fn missed_tech_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.hitstun -= 1.0;
        if self.relative_f(context.input[0].stick_x) < -0.7 {
            ActionResult::set_action(PlayerAction::MissedTechGetupB)
        }
        else if self.relative_f(context.input[0].stick_x) > 0.7 {
            ActionResult::set_action(PlayerAction::MissedTechGetupF)
        }
        else if context.input[0].stick_y > 0.7 {
            ActionResult::set_action(PlayerAction::MissedTechGetupN)
        }
        else if context.input.a.press || context.input.b.press {
            ActionResult::set_action(PlayerAction::MissedTechAttack)
        }
        else {
            if let Some(getup_frame) = context.entity_def.missed_tech_forced_getup {
                if state.frame_no_restart > getup_frame as i64 {
                    ActionResult::set_action(PlayerAction::MissedTechGetupN)
                } else {
                    self.apply_friction(context.entity_def, state);
                    None
                }
            } else {
                self.apply_friction(context.entity_def, state);
                None
            }
        }
    }

    fn damage_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.hitstun -= 1.0;
        if self.hitstun <= 0.0 {
            if self.body.is_airbourne() {
                ActionResult::set_action(PlayerAction::Fall)
            } else {
                ActionResult::set_action(PlayerAction::Idle)
            }
        } else {
            if self.body.is_airbourne() {
                self.fall_action(context.entity_def);
            }
            else {
                self.apply_friction(context.entity_def, state);
            }
            None
        }
    }

    fn damage_fly_action(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        self.hitstun -= 1.0;
        self.fall_action(context.entity_def);
        if self.hitstun <= 0.0 {
            ActionResult::set_action(PlayerAction::DamageFall)
        } else {
            None
        }
    }

    fn damage_fall_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.interruptible(context.entity_def) {
            None
                .or_else(|| self.check_attacks_aerial(context))
                .or_else(|| self.check_special_air(context))
                .or_else(|| self.check_jump_aerial(context, state))
                .or_else(|| if
                    (context.input[0].stick_x >  0.7 && context.input[1].stick_x <  0.7) ||
                    (context.input[0].stick_x < -0.7 && context.input[1].stick_x > -0.7) ||
                    (context.input[0].stick_y >  0.7 && context.input[1].stick_y <  0.7) ||
                    (context.input[0].stick_y < -0.7 && context.input[1].stick_y > -0.7)
                    {
                        ActionResult::set_action(PlayerAction::Fall)
                    } else {
                        None
                    }
                )
        } else {
            None
        }
        .or_else(|| {
            self.fastfall_action(context);
            self.air_drift(context);
            None
        })
    }

    fn spawn_idle(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        None
            .or_else(|| self.check_attacks_aerial(context))
            .or_else(|| self.check_special_ground(context))
            .or_else(|| self.check_jump_aerial(context, state))
            .or_else(|| self.check_aerialdodge(context))
            .or_else(|| if context.input[0].stick_x.abs() > 0.2 || context.input[0].stick_y.abs() > 0.2 || state.frame_no_restart >= 1000 {
                ActionResult::set_action(PlayerAction::Fall)
            } else {
                None
            })
    }

    pub fn aerial_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.interruptible(context.entity_def) {
            None
                .or_else(|| self.check_attacks_aerial(context))
                .or_else(|| self.check_special_air(context))
                .or_else(|| self.check_jump_aerial(context, state))
                .or_else(|| self.check_aerialdodge(context))
        } else {
            None
        }
        .or_else(|| {
            self.air_drift(context);
            self.fastfall_action(context);
            None
        })
    }

    fn jump_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        None
            .or_else(|| self.check_attacks_aerial(context))
            .or_else(|| self.check_special_air(context))
            .or_else(|| self.check_jump_aerial(context, state))
            .or_else(|| self.check_aerialdodge(context))
            .or_else(|| {
                self.air_drift(context);
                self.fastfall_action(context);
                None
            })
    }

    fn air_drift(&mut self, context: &mut StepContext) {
        let term_vel = context.entity_def.air_x_term_vel * context.input[0].stick_x;
        let drift = context.input[0].stick_x.abs() >= 0.3;
        if !drift ||
           (term_vel < 0.0 && self.body.x_vel < term_vel) ||
           (term_vel > 0.0 && self.body.x_vel > term_vel) {
            if self.body.x_vel > 0.0 {
                self.body.x_vel -= context.entity_def.air_friction;
                if self.body.x_vel < 0.0 {
                    self.body.x_vel = 0.0;
                }
            }
            else if self.body.x_vel < 0.0 {
                self.body.x_vel += context.entity_def.air_friction;
                if self.body.x_vel > 0.0 {
                    self.body.x_vel = 0.0;
                }
            }
        }

        if drift {
            if (term_vel < 0.0 && self.body.x_vel > term_vel) ||
               (term_vel > 0.0 && self.body.x_vel < term_vel) {
                self.body.x_vel += context.entity_def.air_mobility_a * context.input[0].stick_x + context.entity_def.air_mobility_b * context.input[0].stick_x.signum();
            }
        }
    }

    fn tilt_turn_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        let last_action_frame = context.entity_def.actions[state.action.as_ref()].frames.len() as u64 - 1;
        if state.frame == context.entity_def.tilt_turn_flip_dir_frame as i64 ||
            (context.entity_def.tilt_turn_flip_dir_frame > last_action_frame && state.last_frame(&context.entity_def)) // ensure turn still occurs if run_turn_flip_dir_frame is invalid
        {
            self.body.face_right = !self.body.face_right;
        }

        None
            .or_else(|| self.check_dash_out_of_tilt_turn(context, state))
            .or_else(|| self.check_jump(context))
            .or_else(|| self.check_shield(context))
            .or_else(|| self.check_special_ground(context)) // TODO: No neutral special
            .or_else(|| self.check_smash(context))
            .or_else(|| self.check_attacks(context))
            .or_else(|| self.check_grab(context))
            .or_else(|| self.check_taunt(context))
            .or_else(|| {
                self.apply_friction(&context.entity_def, state);
                None
            })
    }

    fn check_dash_out_of_tilt_turn(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if context.entity_def.tilt_turn_into_dash_iasa as i64 >= state.frame && self.relative_f(context.input[0].stick_x) > 0.79 {
            if context.entity_def.tilt_turn_flip_dir_frame > context.entity_def.tilt_turn_into_dash_iasa { // ensure turn still occurs even if tilt_turn_flip_dir_frame is invalid
                self.body.face_right = !self.body.face_right
            }
            ActionResult::set_action(PlayerAction::Dash)
        } else {
            None
        }
    }

    fn smash_turn_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        None
            .or_else(|| self.check_dash_out_of_smash_turn(context, state))
            .or_else(|| self.check_jump(context))
            .or_else(|| self.check_shield(context))
            .or_else(|| self.check_special_ground(context)) // TODO: No neutral special
            .or_else(|| self.check_smash(context))
            .or_else(|| self.check_attacks(context))
            .or_else(|| self.check_grab(context))
            .or_else(|| self.check_taunt(context))
            .or_else(|| {
                self.apply_friction(&context.entity_def, state);
                None
            })
    }

    fn check_dash_out_of_smash_turn(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 0 && self.relative_f(context.input[0].stick_x) > 0.79 {
            ActionResult::set_action(PlayerAction::Dash)
        } else {
            None
        }
    }

    fn run_turn_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        let last_action_frame = context.entity_def.actions[state.action.as_ref()].frames.len() as u64 - 1;
        if state.frame == context.entity_def.run_turn_flip_dir_frame as i64 ||
            (context.entity_def.run_turn_flip_dir_frame > last_action_frame && state.last_frame(&context.entity_def)) // ensure turn still occurs if run_turn_flip_dir_frame is invalid
        {
            self.body.face_right = !self.body.face_right;
        }

        self.check_jump(context)
        .or_else(|| {
            self.apply_friction(&context.entity_def, state);
            None
        })
    }

    fn crouch_start_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.interruptible(&context.entity_def) {
            None
                .or_else(|| self.check_pass_platform(context, state))
                .or_else(|| self.check_shield(context))
                .or_else(|| self.check_special_ground(context)) // TODO: no neutral/side special
                .or_else(|| self.check_smash(context))
                .or_else(|| self.check_attacks(context))
                .or_else(|| self.check_grab(context))
                .or_else(|| self.check_taunt(context))
                .or_else(|| self.check_jump(context))
        } else {
            None
        }
        .or_else(|| {
            self.apply_friction(&context.entity_def, state);
            None
        })
    }

    fn crouch_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.interruptible(&context.entity_def) {
            None
                .or_else(|| self.check_jump(context))
                .or_else(|| self.check_shield(context))
                .or_else(|| self.check_special_ground(context)) // TODO: no neutral/side special
                .or_else(|| self.check_smash(context))
                .or_else(|| self.check_attacks(context))
                .or_else(|| self.check_grab(context))
                .or_else(|| self.check_taunt(context))
                .or_else(|| self.check_dash(context))
                .or_else(|| self.check_smash_turn(context))
                .or_else(|| self.check_tilt_turn(context))
                .or_else(||
                    if context.input.stick_y.value > -0.61 {
                        ActionResult::set_action(PlayerAction::CrouchEnd)
                    } else {
                        None
                    }
                )
        } else {
            None
        }
        .or_else(|| {
            self.apply_friction(&context.entity_def, state);
            None
        })
    }

    fn dtilt_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.interruptible(&context.entity_def) {
            None
                .or_else(|| self.check_jump(context))
                .or_else(|| self.check_shield(context))
                .or_else(|| self.check_special_ground(context)) // TODO: no neutral/side special
                .or_else(|| self.check_smash(context))
                .or_else(|| self.check_attacks(context))
                .or_else(|| self.check_grab(context))
                .or_else(|| self.check_dash(context))
                .or_else(|| self.check_smash_turn(context))
                .or_else(|| self.check_tilt_turn(context))
                .or_else(|| self.check_walk(context))
                .or_else(|| self.check_taunt(context))
        } else {
            None
        }
        .or_else(|| {
            self.apply_friction(&context.entity_def, state);
            None
        })
    }

    pub fn ground_idle_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if let Some(PlayerAction::TauntDown) = state.get_action() {
            if state.frame == 0 && context.input.b.value {
                let (x, y) = self.bps_xy(context, state);
                let x = x + 15.0;
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
        }

        if state.interruptible(&context.entity_def) {
            None
                .or_else(|| self.check_jump(context))
                .or_else(|| self.check_shield(context))
                .or_else(|| self.check_special_ground(context))
                .or_else(|| self.check_smash(context))
                .or_else(|| self.check_attacks(context))
                .or_else(|| self.check_grab(context))
                .or_else(|| self.check_taunt(context))
                .or_else(|| self.check_crouch(context, state))
                .or_else(|| self.check_dash(context))
                .or_else(|| self.check_smash_turn(context))
                .or_else(|| self.check_tilt_turn(context))
                .or_else(|| self.check_walk(context))
        } else {
            None
        }.or_else(|| {
            self.apply_friction(&context.entity_def, state);
            None
        })
    }

    fn item_throw_air_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.item_throw_action(context, state);
        self.aerial_action(context, state)
    }

    fn item_throw_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame == 4 {
            if let Some(item) = self.get_held_item(&context.entities) {
                let message_item = match state.get_action() {
                    Some(PlayerAction::ItemThrowF) | Some(PlayerAction::ItemThrowAirF) => MessageItem::Thrown { x_vel: self.relative_f(3.0),  y_vel: 0.0 },
                    Some(PlayerAction::ItemThrowB) | Some(PlayerAction::ItemThrowAirB) => MessageItem::Thrown { x_vel: self.relative_f(-3.0), y_vel: 0.0 },
                    Some(PlayerAction::ItemThrowU) | Some(PlayerAction::ItemThrowAirU) => MessageItem::Thrown { x_vel: 0.0,  y_vel: 4.0 },
                    Some(PlayerAction::ItemThrowD) | Some(PlayerAction::ItemThrowAirD) => MessageItem::Thrown { x_vel: 0.0,  y_vel: -4.0 },
                    _ => MessageItem::Dropped,
                };
                context.messages.push(Message {
                    recipient: item,
                    contents:  MessageContents::Item(message_item)
                });
            }
        }
        None
    }

    fn attack_land_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        let frame = state.frame + self.land_frame_skip as i64 + 1;

        self.land_action(context, state)
            .or_else(|| ActionResult::set_frame(frame))
    }

    fn land_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.land_particles(context, state);

        if state.interruptible(&context.entity_def) {
            None
                .or_else(|| self.check_jump(context))
                .or_else(|| self.check_shield(context))
                .or_else(|| self.check_special_ground(context))
                .or_else(|| self.check_smash(context))
                .or_else(|| self.check_attacks(context))
                .or_else(|| self.check_grab(context))
                .or_else(|| self.check_taunt(context))
                .or_else(|| self.check_dash(context))
                .or_else(|| self.check_smash_turn(context))
                .or_else(|| self.check_tilt_turn(context))
                .or_else(|| self.check_walk(context))
                .or_else(|| if state.first_interruptible(&context.entity_def) && context.input[0].stick_y < -0.5 {
                        ActionResult::set_action(PlayerAction::Crouch)
                    } else {
                        None
                    }
                )
        } else {
            None
        }.or_else(|| {
            self.apply_friction(&context.entity_def, state);
            None
        })
    }

    fn teeter_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.interruptible(&context.entity_def) {
            None
                .or_else(|| self.check_jump(context))
                .or_else(|| self.check_shield(context))
                .or_else(|| self.check_special_ground(context))
                .or_else(|| self.check_smash(context))
                .or_else(|| self.check_attacks(context))
                .or_else(|| self.check_grab(context))
                .or_else(|| self.check_taunt(context))
                .or_else(|| self.check_crouch(context, state))
                .or_else(|| self.check_dash(context))
                .or_else(|| self.check_smash_turn(context))
                .or_else(|| self.check_tilt_turn(context))
                .or_else(|| self.check_walk_teeter(context))
        } else {
            None
        }
    }

    fn walk_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if context.input[0].stick_x == 0.0 {
            ActionResult::set_action(PlayerAction::Idle)
        } else {
            None
        }
            .or_else(|| self.check_jump(context))
            .or_else(|| self.check_shield(context))
            .or_else(|| self.check_special_ground(context))
            .or_else(|| self.check_smash(context))
            .or_else(|| self.check_attacks(context))
            .or_else(|| self.check_grab(context))
            .or_else(|| self.check_crouch(context, state))
            .or_else(|| self.check_dash(context))
            .or_else(|| self.check_smash_turn(context))
            .or_else(|| self.check_tilt_turn(context))
            .or_else(|| self.check_taunt(context))
            .or_else(|| {
                let vel_max = context.entity_def.walk_max_vel * context.input[0].stick_x;

                if self.body.x_vel.abs() > vel_max.abs() {
                    self.apply_friction(&context.entity_def, state);
                }
                else {
                    let acc = (vel_max - self.body.x_vel) * (2.0/context.entity_def.walk_max_vel) * (context.entity_def.walk_init_vel + context.entity_def.walk_acc);
                    self.body.x_vel += acc;
                    if self.relative_f(self.body.x_vel) > self.relative_f(vel_max) {
                        self.body.x_vel = vel_max;
                    }
                }
                None
            })
    }

    fn dash_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.dash_particles(context, state);
        if state.frame == 1 {
            self.body.x_vel = self.relative_f(context.entity_def.dash_init_vel);
            if self.body.x_vel.abs() > context.entity_def.dash_run_term_vel {
                self.body.x_vel = self.relative_f(context.entity_def.dash_run_term_vel);
            }
        }

        if state.frame > 0 {
            if context.input[0].stick_x.abs() < 0.3 {
                self.apply_friction(&context.entity_def, state);
            }
            else {
                let vel_max = context.input[0].stick_x * context.entity_def.dash_run_term_vel;
                let acc     = context.input[0].stick_x * context.entity_def.dash_run_acc_a;

                self.body.x_vel += acc;
                if (vel_max > 0.0 && self.body.x_vel > vel_max) || (vel_max < 0.0 && self.body.x_vel < vel_max) {
                    self.apply_friction(&context.entity_def, state);
                    if (vel_max > 0.0 && self.body.x_vel < vel_max) || (vel_max < 0.0 && self.body.x_vel > vel_max) {
                        self.body.x_vel = vel_max;
                    }
                }
                else {
                    self.body.x_vel += acc;
                    if (vel_max > 0.0 && self.body.x_vel > vel_max) || (vel_max < 0.0 && self.body.x_vel < vel_max) {
                        self.body.x_vel = vel_max;
                    }
                }
            }
        }

        let run_frame = 13; // TODO: Variable per character
        let last_action_frame = context.entity_def.actions[state.action.as_ref()].frames.len() as i64 - 1;
        if (state.frame >= run_frame || (run_frame > last_action_frame && state.last_frame(&context.entity_def)))
            && self.relative_f(context.input.stick_x.value) >= 0.62
        {
            ActionResult::set_action(PlayerAction::Run)
        } else {
            None
        }
        .or_else(|| {
            let result = self.check_shield(context);
            if result.is_some() {
                self.body.x_vel *= 0.25;
            }
            result
        })
        .or_else(|| self.check_dash_grab(context))
        .or_else(|| self.check_dash_attack(context))
        .or_else(|| self.check_jump(context))
        .or_else(|| self.check_smash_turn(context))
    }

    fn run_action(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        None
            .or_else(|| self.check_jump(context))
            .or_else(|| self.check_shield(context))
            .or_else(||
                if self.relative_f(context.input.stick_x.value) <= -0.3 {
                    ActionResult::set_action(PlayerAction::RunTurn)
                } else {
                    None
                }
            )
            .or_else(||
                if self.relative_f(context.input.stick_x.value) < 0.62 {
                    ActionResult::set_action(PlayerAction::RunEnd)
                } else {
                    None
                }
            )
            .or_else(|| self.check_dash_grab(context))
            .or_else(|| self.check_dash_attack(context))
            .or_else(|| self.check_special_ground(context))
            .or_else(|| {
                let vel_max = context.input[0].stick_x * context.entity_def.dash_run_term_vel;
                let acc = (vel_max - self.body.x_vel)
                        * (context.entity_def.dash_run_acc_a + (context.entity_def.dash_run_acc_b / context.input[0].stick_x.abs()))
                        / (context.entity_def.dash_run_term_vel * 2.5);

                self.body.x_vel += acc;
                if self.relative_f(self.body.x_vel) > self.relative_f(vel_max) {
                    self.body.x_vel = vel_max;
                }
                None
            })
    }

    fn run_end_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        None
            .or_else(|| self.check_jump(context))
            .or_else(|| if state.frame > 1 {
                    self.check_crouch(context, state)
                } else {
                    None
                }
            )
            .or_else(|| if self.relative_f(context.input.stick_x.value) <= -0.3 {
                    ActionResult::set_action(PlayerAction::RunTurn)
                } else {
                    None
                }
            )
            .or_else(|| {
                self.apply_friction(context.entity_def, state);
                None
            })
    }

    fn check_aerialdodge(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.l.press || context.input.r.press {
            self.aerialdodge(context)
        } else {
            None
        }
    }

    fn aerialdodge(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        match context.input[0].stick_angle() {
            Some(angle) => {
                self.body.x_vel = angle.cos() * context.entity_def.aerialdodge_mult;
                self.body.y_vel = angle.sin() * context.entity_def.aerialdodge_mult;
            }
            None => {
                self.body.x_vel = 0.0;
                self.body.y_vel = 0.0;
            }
        }
        self.fastfalled = false;
        ActionResult::set_action(PlayerAction::AerialDodge)
    }

    fn aerialdodge_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame < context.entity_def.aerialdodge_drift_frame as i64 {
            self.body.x_vel *= 0.9;
            self.body.y_vel *= 0.9;
        }
        else {
            self.air_drift(context);
            self.fastfall_action(context);
        }
        None
    }

    fn shield_on_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.shield_shared_action(context, state)
    }

    fn shield_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.shield_shared_action(context, state)
            .or_else(|| {
                let stun_lock = self.shield_stun_timer > 0;
                if !stun_lock && context.input[0].l_trigger < 0.165 && context.input[0].r_trigger < 0.165 && !context.input[0].l && !context.input[0].r {
                    if self.parry_timer > 0 {
                        ActionResult::set_action(PlayerAction::Idle)
                    } else {
                        ActionResult::set_action(PlayerAction::ShieldOff)
                    }
                } else {
                    None
                }
            })
    }

    fn shield_off_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.shield_shared_action(context, state)
    }

    fn shield_shared_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.apply_friction(context.entity_def, state);
        if let Some(ref shield) = context.entity_def.shield {
            let stick_lock = context.entity_def.shield.as_ref().map_or(false, |x| x.stick_lock) && context.input[0].b;
            let stun_lock = self.shield_stun_timer > 0;
            let lock = stun_lock && stick_lock;

            // shield analog
            self.shield_analog = if context.input[0].l || context.input[0].r {
                1.0
            } else {
                context.input[0].l_trigger.max(context.input[0].r_trigger)
            };

            // shield offset
            let stick_x = context.input[0].stick_x;
            let stick_y = context.input[0].stick_y;
            let target_offset = (stick_x * stick_x + stick_y * stick_y).sqrt() * context.entity_def.shield.as_ref().map_or(1.0, |x| x.stick_mult);
            let target_angle = stick_y.atan2(stick_x);
            let target_x = target_angle.cos() * target_offset;
            let target_y = target_angle.sin() * target_offset;
            self.shield_offset_x += (target_x - self.shield_offset_x) / 5.0 + 0.01;
            self.shield_offset_y += (target_y - self.shield_offset_y) / 5.0 + 0.01;

            // shield hp
            self.shield_hp -= shield.hp_cost * self.shield_analog - (1.0 - self.shield_analog) / 10.0;
            let result = if self.shield_hp <= 0.0 {
                self.shield_hp = 0.0;
                self.body.kb_y_vel = shield.break_vel;
                self.body.kb_y_dec = 0.051;
                self.body.kb_x_dec = 0.0;
                self.set_airbourne(context, state);
                ActionResult::set_action(PlayerAction::ShieldBreakFall)
            } else {
                None
            };

            if !lock {
                result
                    .or_else(|| self.check_grab_shield(context))
                    .or_else(|| self.check_jump(context))
                    .or_else(|| self.check_pass_platform(context, state))
            } else {
                result
            }
        } else {
            None
        }
    }

    fn shield_break_fall_action(&mut self, entity_def: &EntityDef) -> Option<ActionResult> {
        self.fall_action(entity_def);
        None
    }

    fn shield_break_getup_action(&mut self) -> Option<ActionResult> {
        self.body.x_vel = 0.0;
        None
    }

    fn stun_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.apply_friction(context.entity_def, state);
        if self.shield_hp > 30.0 {
            self.shield_hp = 30.0;
        }

        self.stun_timer -= 1;

        // TODO: Mashout

        if self.stun_timer == 0 {
            ActionResult::set_action(PlayerAction::Idle)
        } else {
            None
        }
    }

    fn grabbing_idle_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        self.apply_friction(context.entity_def, state);
        if (self.relative_f(context.input[0].stick_x)   <= -0.66 && self.relative_f(context.input[1].stick_x)   > -0.66 && context.input[0].stick_x  .abs() > context.input[0].stick_y  .abs() - 0.1)
        || (self.relative_f(context.input[0].c_stick_x) <= -0.66 && self.relative_f(context.input[1].c_stick_x) > -0.66 && context.input[0].c_stick_x.abs() > context.input[0].c_stick_y.abs() - 0.1) {
            ActionResult::set_action(PlayerAction::Bthrow)
        }
        else if (self.relative_f(context.input[0].stick_x)   >= 0.66 && self.relative_f(context.input[1].stick_x)   < 0.66 && context.input[0].stick_x  .abs() > context.input[0].stick_y  .abs() - 0.1)
             || (self.relative_f(context.input[0].c_stick_x) >= 0.66 && self.relative_f(context.input[1].c_stick_x) < 0.66 && context.input[0].c_stick_x.abs() > context.input[0].c_stick_y.abs() - 0.1) {
            ActionResult::set_action(PlayerAction::Fthrow)
        }
        else if (context.input[0].stick_y   >= 0.66 && context.input[1].stick_y   < 0.66)
             || (context.input[0].c_stick_y >= 0.66 && context.input[1].c_stick_y < 0.66) {
            ActionResult::set_action(PlayerAction::Uthrow)
        }
        else if (context.input[0].stick_y   <= -0.66 && context.input[1].stick_y   > -0.66)
             || (context.input[0].c_stick_y <= -0.66 && context.input[1].c_stick_y > -0.66) {
            ActionResult::set_action(PlayerAction::Dthrow)
        }
        else if state.frame_no_restart > 60 { // TODO: additionally check if grabbed player is still in a grabbed state
            ActionResult::set_action(PlayerAction::GrabbingEnd)
        } else {
            None
        }
    }

    // TODO: this state should probably be split into standing and airbourne versions
    //       for now lets try to squash both cases into this one action
    fn grabbed_idle_action(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if state.frame_no_restart > 60 { // TODO: instead check if grabbing player is still in a grabbing state
            let bps_xy = self.bps_xy(context, state);
            if let Some(frame) = state.get_entity_frame(context.entity_def) {
                // ignore the x offset, we only want to check straight down.
                let bps_xy_grab_point = (bps_xy.0, bps_xy.1 + frame.grabbed_y);
                if let Some(platform_i) = self.body.land_stage_collision(context, frame, bps_xy_grab_point, bps_xy) {
                    let x = context.stage.surfaces[platform_i].world_x_to_plat_x(bps_xy.0);
                    self.body.location = Location::Surface { platform_i, x };
                    self.land(context, state);
                    ActionResult::set_action(PlayerAction::GrabbedEnd)
                }
                else {
                    self.set_airbourne(context, state);
                    ActionResult::set_action(PlayerAction::Fall)
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn shield_size(&self, shield: &Shield) -> f32 {
        let analog_size = (1.0 - self.shield_analog) * 0.6;
        let hp_size = (self.shield_hp / shield.hp_max) * shield.hp_scaling;
        let hp_size_unscaled = ((shield.hp_max - self.shield_hp) / shield.hp_max) * 2.0;

        shield.scaling * (analog_size + hp_size) + hp_size_unscaled
    }

    fn shield_pos(&self, shield: &Shield, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface], state: &ActionState) -> (f32, f32) {
        let xy = self.public_bps_xy(entities, entity_defs, surfaces, state);
        (
            xy.0 + self.shield_offset_x + self.relative_f(shield.offset_x),
            xy.1 + self.shield_offset_y + shield.offset_y
        )
    }

    fn check_crouch(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if context.input[0].stick_y < -0.77 {
            if let Some(action) = state.get_action() {
                match action {
                    PlayerAction::CrouchStart | PlayerAction::Crouch | PlayerAction::CrouchEnd => {
                        None // TODO: used to block action change do we want to restore that?
                    }
                    _ => {
                        ActionResult::set_action(PlayerAction::CrouchStart)
                    }
                }
            } else {
                None // TODO: used to block action change do we want to restore that?
            }
        } else {
            None
        }
    }

    fn check_pass_platform(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if let Location::Surface { platform_i, .. } = self.body.location {
            if let Some(platform) = context.surfaces.get(platform_i) {
                let last_action_frame = context.entity_def.actions[state.action.as_ref()].frames.len() as i64 - 1;
                let pass_frame = last_action_frame.min(4);
                if platform.is_pass_through() && state.frame == pass_frame && (context.input[0].stick_y < -0.77 || context.input[2].stick_y < -0.77) && context.input[6].stick_y > -0.36 {
                    self.set_airbourne(context, state);
                    return ActionResult::set_action(PlayerAction::PassPlatform)
                }
            }
        }
        None
    }

    fn check_walk(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input[0].stick_x.abs() > 0.3 {
            self.walk(context)
        } else {
            None
        }
    }

    fn check_walk_teeter(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input[0].stick_x.abs() > 0.6 {
            self.walk(context)
        } else {
            None
        }
    }

    fn check_dash(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if self.relative_f(context.input[0].stick_x) > 0.79 && self.relative_f(context.input[2].stick_x) < 0.3 {
            self.body.x_vel = self.relative_f(context.entity_def.dash_init_vel);
            ActionResult::set_action(PlayerAction::Dash)
        } else {
            None
        }
    }

    fn check_tilt_turn(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if self.relative_f(context.input[0].stick_x) < -0.3 {
            ActionResult::set_action(PlayerAction::TiltTurn)
        } else {
            None
        }
    }

    fn check_smash_turn(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if self.relative_f(context.input[0].stick_x) < -0.79 && self.relative_f(context.input[2].stick_x) > -0.3 {
            self.body.x_vel *= 0.25;
            self.body.face_right = !self.body.face_right;
            ActionResult::set_action(PlayerAction::SmashTurn)
        } else {
            None
        }
    }

    fn check_jump(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        match self.jump_input(&context.input) {
            JumpResult::Button => {
                self.jumpsquat_button = true;
                ActionResult::set_action(PlayerAction::JumpSquat)
            }
            JumpResult::Stick => {
                self.jumpsquat_button = false;
                ActionResult::set_action(PlayerAction::JumpSquat)
            }
            JumpResult::None => None,
        }
    }

    fn check_jump_aerial(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        if self.jump_input(&context.input).jump() && self.air_jumps_left > 0 {
            self.air_jump_particles(context, state);
            self.air_jumps_left -= 1;
            self.body.y_vel = context.entity_def.air_jump_y_vel;
            self.body.x_vel = context.entity_def.air_jump_x_vel * context.input[0].stick_x;
            self.fastfalled = false;

            if self.relative_f(context.input.stick_x.value) < -0.3 {
                ActionResult::set_action(PlayerAction::JumpAerialB)
            } else {
                ActionResult::set_action(PlayerAction::JumpAerialF)
            }
        } else {
            None
        }
    }

    fn check_attacks_aerial(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.a.press || context.input.z.press {
            if self.relative_f(context.input[0].stick_x) > 0.3 && context.input[0].stick_x.abs() > context.input[0].stick_y.abs() - 0.1 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowAirF)
                } else {
                    ActionResult::set_action(PlayerAction::Fair)
                }
            }
            else if self.relative_f(context.input[0].stick_x) < -0.3 && context.input[0].stick_x.abs() > context.input[0].stick_y.abs() - 0.1 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowAirB)
                } else {
                    ActionResult::set_action(PlayerAction::Bair)
                }
            }
            else if context.input[0].stick_y < -0.3 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowAirD)
                } else {
                    ActionResult::set_action(PlayerAction::Dair)
                }
            }
            else if context.input[0].stick_y > 0.3 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowAirU)
                } else {
                    ActionResult::set_action(PlayerAction::Uair)
                }
            }
            else if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                if let Some(item) = self.get_held_item(&context.entities) {
                    context.messages.push(Message {
                        recipient: item,
                        contents:  MessageContents::Item(MessageItem::Dropped)
                    });
                }
                None
            } else {
                ActionResult::set_action(PlayerAction::Nair)
            }
        }
        else if self.relative_f(context.input[0].c_stick_x) >= 0.3 && self.relative_f(context.input[1].c_stick_x) < 0.3 
            && context.input[0].c_stick_x.abs() > context.input[0].c_stick_y.abs() - 0.1
        {
            if self.get_held_item(&context.entities).is_some() {
                ActionResult::set_action(PlayerAction::ItemThrowAirF)
            } else {
                ActionResult::set_action(PlayerAction::Fair)
            }
        }
        else if self.relative_f(context.input[0].c_stick_x) <= -0.3 && self.relative_f(context.input[1].c_stick_x) > -0.3
            && context.input[0].c_stick_x.abs() > context.input[0].c_stick_y.abs() - 0.1
        {
            if self.get_held_item(&context.entities).is_some() {
                ActionResult::set_action(PlayerAction::ItemThrowAirB)
            } else {
                ActionResult::set_action(PlayerAction::Bair)
            }
        }
        else if context.input[0].c_stick_y < -0.3 && context.input[1].c_stick_y > -0.3 {
            if self.get_held_item(&context.entities).is_some() {
                ActionResult::set_action(PlayerAction::ItemThrowAirD)
            } else {
                ActionResult::set_action(PlayerAction::Dair)
            }
        }
        else if context.input[0].c_stick_y >= 0.3 && context.input[1].c_stick_y < 0.3 {
            if self.get_held_item(&context.entities).is_some() {
                ActionResult::set_action(PlayerAction::ItemThrowAirU)
            } else {
                ActionResult::set_action(PlayerAction::Uair)
            }
        }
        else {
            None
        }
    }

    fn check_attacks(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.a.press {
            if self.relative_f(context.input[0].stick_x) > 0.3 && context.input[0].stick_x.abs() - context.input[0].stick_y.abs() > -0.05 {
                if self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowF)
                } else {
                    ActionResult::set_action(PlayerAction::Ftilt)
                }
            }
            else if context.input[0].stick_y < -0.3 {
                if self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowD)
                } else {
                    ActionResult::set_action(PlayerAction::Dtilt)
                }
            }
            else if context.input[0].stick_y > 0.3 {
                if self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowU)
                } else {
                    ActionResult::set_action(PlayerAction::Utilt)
                }
            }
            else {
                if self.get_held_item(&context.entities).is_some() {
                    ActionResult::set_action(PlayerAction::ItemThrowF)
                } else {
                    ActionResult::set_action(PlayerAction::Jab)
                }
            }
        } else {
            None
        }
    }

    fn check_dash_attack(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.a.press {
            ActionResult::set_action(PlayerAction::DashAttack)
        } else {
            None
        }
    }

    fn check_grab_shield(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.a.press || context.input.z.press {
            ActionResult::set_action(PlayerAction::Grab)
        } else {
            None
        }
    }

    fn check_grab(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.z.press {
            ActionResult::set_action(PlayerAction::Grab)
        } else {
            None
        }
    }

    fn check_dash_grab(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.z.press {
            ActionResult::set_action(PlayerAction::DashGrab)
        } else {
            None
        }
    }

    fn check_special_ground(&mut self, context: &StepContext) -> Option<ActionResult> {
        if context.input.b.press {
            if context.input[0].stick_x.abs() > 0.3 {
                self.body.face_right = context.input.stick_x.value > 0.0;
                ActionResult::set_action(PlayerAction::SspecialGroundStart)
            }
            else if context.input[0].stick_y < -0.3 {
                ActionResult::set_action(PlayerAction::DspecialGroundStart)
            }
            else if context.input[0].stick_y > 0.3 {
                ActionResult::set_action(PlayerAction::UspecialGroundStart)
            }
            else {
                ActionResult::set_action(PlayerAction::NspecialGroundStart)
            }
        } else {
            None
        }
    }

    fn check_special_air(&mut self, context: &StepContext) -> Option<ActionResult> {
        if context.input.b.press {
            if context.input[0].stick_x.abs() > 0.3 {
                self.body.face_right = context.input.stick_x.value > 0.0;
                ActionResult::set_action(PlayerAction::SspecialAirStart)
            }
            else if context.input[0].stick_y < -0.3 {
                ActionResult::set_action(PlayerAction::DspecialAirStart)
            }
            else if context.input[0].stick_y > 0.3 {
                ActionResult::set_action(PlayerAction::UspecialAirStart)
            }
            else {
                ActionResult::set_action(PlayerAction::NspecialAirStart)
            }
        } else {
            None
        }
    }

    fn check_smash(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.a.press && 
           (context.input[0].stick_x >=  0.79 && context.input[2].stick_x < 0.3) ||
           (context.input[0].stick_x <= -0.79 && context.input[2].stick_x > -0.3) {
            self.body.face_right = context.input.stick_x.value > 0.0;
            ActionResult::set_action(PlayerAction::Fsmash)
        }
        else if context.input.a.press && context.input[0].stick_y >= 0.66 && context.input[2].stick_y < 0.3 {
            ActionResult::set_action(PlayerAction::Usmash)
        }
        else if context.input.a.press && context.input[0].stick_y <= -0.66 && context.input[2].stick_y > 0.3 {
            ActionResult::set_action(PlayerAction::Dsmash)
        }
        else if context.input[0].c_stick_x.abs() >= 0.79 && context.input[1].c_stick_x.abs() < 0.79 {
            self.body.face_right = context.input.c_stick_x.value > 0.0;
            ActionResult::set_action(PlayerAction::Fsmash)
        }
        else if context.input[0].c_stick_y >= 0.66 && context.input[1].c_stick_y < 0.66 {
            ActionResult::set_action(PlayerAction::Usmash)
        }
        else if context.input[0].c_stick_y <= -0.66 && context.input[1].c_stick_y > -0.66 {
            ActionResult::set_action(PlayerAction::Dsmash)
        }
        else {
            None
        }
    }

    fn check_taunt(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if context.input.up.press {
            ActionResult::set_action(PlayerAction::TauntUp)
        }
        else if context.input.down.press {
            ActionResult::set_action(PlayerAction::TauntDown)
        }
        else if context.input.left.press {
            ActionResult::set_action(PlayerAction::TauntLeft)
        }
        else if context.input.right.press {
            ActionResult::set_action(PlayerAction::TauntRight)
        }
        else {
            None
        }
    }

    fn check_shield(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        match (&context.entity_def.shield, &context.entity_def.power_shield) {
            (&Some(_), &Some(_)) => {
                if context.input.l.press || context.input.r.press {
                    ActionResult::set_action(PlayerAction::PowerShield)
                }
                else if context.input[0].l || context.input[0].r || context.input[0].l_trigger > 0.165 || context.input[0].r_trigger > 0.165 {
                    ActionResult::set_action(PlayerAction::ShieldOn)
                }
                else {
                    None
                }
            }
            (&None, &Some(_)) => {
                if context.input[0].l || context.input[0].r || context.input[0].l_trigger > 0.165 || context.input[0].r_trigger > 0.165 {
                    ActionResult::set_action(PlayerAction::PowerShield)
                } else {
                    None
                }
            }
            (&Some(_), &None) => {
                if context.input[0].l || context.input[0].r || context.input[0].l_trigger > 0.165 || context.input[0].r_trigger > 0.165 {
                    ActionResult::set_action(PlayerAction::ShieldOn)
                } else {
                    None
                }
            }
            (&None, &None) => None
        }
    }

    fn jump_input(&self, input: &PlayerInput) -> JumpResult {
        if input.x.press || input.y.press {
            JumpResult::Button
        }
        else if input[0].stick_y > 0.66 && input[3].stick_y < 0.2 {
            JumpResult::Stick
        }
        else {
            JumpResult::None
        }
    }

    pub fn action_expired(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        ActionResult::set_action(match state.get_action() {
            None => panic!("Unknown action expired"),

            // Idle
            Some(PlayerAction::Spawn)          => PlayerAction::Idle,
            Some(PlayerAction::ReSpawn)        => PlayerAction::ReSpawnIdle,
            Some(PlayerAction::ReSpawnIdle)    => PlayerAction::ReSpawnIdle,
            Some(PlayerAction::Idle)           => PlayerAction::Idle,
            Some(PlayerAction::Teeter)         => PlayerAction::TeeterIdle,
            Some(PlayerAction::TeeterIdle)     => PlayerAction::TeeterIdle,
            Some(PlayerAction::MissedTechIdle) => PlayerAction::MissedTechIdle,

            // crouch
            Some(PlayerAction::CrouchStart) => PlayerAction::Crouch,
            Some(PlayerAction::Crouch)      => PlayerAction::Crouch,
            Some(PlayerAction::CrouchEnd)   => PlayerAction::Idle,

            // Movement
            Some(PlayerAction::Fall)           => PlayerAction::Fall,
            Some(PlayerAction::AerialFall)     => PlayerAction::AerialFall,
            Some(PlayerAction::Land)           => PlayerAction::Idle,
            Some(PlayerAction::JumpF)          => PlayerAction::Fall,
            Some(PlayerAction::JumpB)          => PlayerAction::Fall,
            Some(PlayerAction::JumpAerialF)    => PlayerAction::AerialFall,
            Some(PlayerAction::JumpAerialB)    => PlayerAction::AerialFall,
            Some(PlayerAction::SmashTurn)      => PlayerAction::Idle,
            Some(PlayerAction::RunTurn) =>
            if self.relative_f(context.input[0].stick_x) > 0.6 {
                PlayerAction::Run
            } else {
                PlayerAction::Idle
            }
            Some(PlayerAction::TiltTurn)       => PlayerAction::Idle,
            Some(PlayerAction::Dash)           => PlayerAction::Idle,
            Some(PlayerAction::Run)            => PlayerAction::Run,
            Some(PlayerAction::RunEnd)         => PlayerAction::Idle,
            Some(PlayerAction::Walk)           => PlayerAction::Walk,
            Some(PlayerAction::PassPlatform)   => PlayerAction::AerialFall,
            Some(PlayerAction::Damage)         => PlayerAction::Damage,
            Some(PlayerAction::DamageFly)      => PlayerAction::DamageFly,
            Some(PlayerAction::DamageFall)     => PlayerAction::DamageFall,
            Some(PlayerAction::LedgeGetup)     => self.set_action_idle_from_ledge(context, state),
            Some(PlayerAction::LedgeGetupSlow) => self.set_action_idle_from_ledge(context, state),
            Some(PlayerAction::LedgeJump)      => self.set_action_fall_from_ledge_jump(context, state),
            Some(PlayerAction::LedgeJumpSlow)  => self.set_action_fall_from_ledge_jump(context, state),
            Some(PlayerAction::LedgeIdle)      => PlayerAction::LedgeIdle,
            Some(PlayerAction::LedgeIdleChain) => PlayerAction::LedgeIdleChain,
            Some(PlayerAction::LedgeGrab) => {
                self.ledge_idle_timer = 0;
                PlayerAction::LedgeIdle
            }
            Some(PlayerAction::JumpSquat) => {
                self.set_airbourne(context, state);
                if let &mut Location::Airbourne { ref mut y, .. } = &mut self.body.location {
                    *y += 0.0001;
                }

                let shorthop = if self.jumpsquat_button {
                    !context.input[0].x && !context.input[0].y
                }
                else {
                    context.input[0].stick_y < 0.67
                };

                if shorthop {
                    self.body.y_vel = context.entity_def.jump_y_init_vel_short;
                }
                else {
                    self.body.y_vel = context.entity_def.jump_y_init_vel;
                }

                self.body.x_vel = self.body.x_vel * context.entity_def.jump_x_vel_ground_mult + context.input[0].stick_x * context.entity_def.jump_x_init_vel;
                if self.body.x_vel.abs() > context.entity_def.jump_x_term_vel {
                    self.body.x_vel = context.entity_def.jump_x_term_vel * self.body.x_vel.signum();
                }

                if self.relative_f(context.input[2].stick_x) >= -0.3 {
                    PlayerAction::JumpF
                }
                else {
                    PlayerAction::JumpB
                }
            }

            // Defense
            Some(PlayerAction::PowerShield)      => if context.entity_def.shield.is_some() { PlayerAction::Shield } else { PlayerAction::Idle },
            Some(PlayerAction::ShieldOn)         => PlayerAction::Shield,
            Some(PlayerAction::Shield)           => PlayerAction::Shield,
            Some(PlayerAction::ShieldOff)        => PlayerAction::Idle,
            Some(PlayerAction::RollF)            => PlayerAction::Idle,
            Some(PlayerAction::RollB)            => PlayerAction::Idle,
            Some(PlayerAction::SpotDodge)        => PlayerAction::Idle,
            Some(PlayerAction::AerialDodge)      => PlayerAction::SpecialFall,
            Some(PlayerAction::SpecialFall)      => PlayerAction::SpecialFall,
            Some(PlayerAction::SpecialLand)      => PlayerAction::Idle,
            Some(PlayerAction::TechF)            => PlayerAction::Idle,
            Some(PlayerAction::TechN)            => PlayerAction::Idle,
            Some(PlayerAction::TechB)            => PlayerAction::Idle,
            Some(PlayerAction::MissedTechGetupF) => PlayerAction::Idle,
            Some(PlayerAction::MissedTechGetupN) => PlayerAction::Idle,
            Some(PlayerAction::MissedTechGetupB) => PlayerAction::Idle,
            Some(PlayerAction::Rebound)          => PlayerAction::Idle,
            Some(PlayerAction::LedgeRoll)        => self.set_action_idle_from_ledge(context, state),
            Some(PlayerAction::LedgeRollSlow)    => self.set_action_idle_from_ledge(context, state),

            // Vulnerable
            Some(PlayerAction::MissedTechStart)  => PlayerAction::MissedTechIdle,
            Some(PlayerAction::ShieldBreakFall)  => PlayerAction::ShieldBreakFall,
            Some(PlayerAction::Stun)             => PlayerAction::Stun,
            Some(PlayerAction::ShieldBreakGetup) => {
                self.stun_timer = 490;
                PlayerAction::Stun
            }

            // Attack
            Some(PlayerAction::Jab)              => PlayerAction::Idle,
            Some(PlayerAction::Jab2)             => PlayerAction::Idle,
            Some(PlayerAction::Jab3)             => PlayerAction::Idle,
            Some(PlayerAction::Utilt)            => PlayerAction::Idle,
            Some(PlayerAction::Dtilt)            => PlayerAction::Crouch,
            Some(PlayerAction::Ftilt)            => PlayerAction::Idle,
            Some(PlayerAction::DashAttack)       => PlayerAction::Idle,
            Some(PlayerAction::Usmash)           => PlayerAction::Idle,
            Some(PlayerAction::Dsmash)           => PlayerAction::Idle,
            Some(PlayerAction::Fsmash)           => PlayerAction::Idle,
            Some(PlayerAction::MissedTechAttack) => PlayerAction::Idle,
            Some(PlayerAction::LedgeAttack)      => self.set_action_idle_from_ledge(context, state),
            Some(PlayerAction::LedgeAttackSlow)  => self.set_action_idle_from_ledge(context, state),

            // Grab
            Some(PlayerAction::Grab)           => PlayerAction::Idle,
            Some(PlayerAction::DashGrab)       => PlayerAction::Idle,
            Some(PlayerAction::GrabbingIdle)   => PlayerAction::GrabbingIdle,
            Some(PlayerAction::GrabbingEnd)    => PlayerAction::Idle,
            Some(PlayerAction::GrabbedIdleAir) => PlayerAction::GrabbedIdleAir,
            Some(PlayerAction::GrabbedIdle)    => PlayerAction::GrabbedIdle,
            Some(PlayerAction::GrabbedEnd)     => PlayerAction::Idle,

            // Throws
            Some(PlayerAction::Uthrow) => PlayerAction::Idle,
            Some(PlayerAction::Dthrow) => PlayerAction::Idle,
            Some(PlayerAction::Fthrow) => PlayerAction::Idle,
            Some(PlayerAction::Bthrow) => PlayerAction::Idle,

            // Items
            Some(PlayerAction::ItemGrab)      => PlayerAction::Idle,
            Some(PlayerAction::ItemEat)       => PlayerAction::Idle,
            Some(PlayerAction::ItemThrowU)    => PlayerAction::Idle,
            Some(PlayerAction::ItemThrowD)    => PlayerAction::Idle,
            Some(PlayerAction::ItemThrowF)    => PlayerAction::Idle,
            Some(PlayerAction::ItemThrowB)    => PlayerAction::Idle,
            Some(PlayerAction::ItemThrowAirU) => PlayerAction::Fall,
            Some(PlayerAction::ItemThrowAirD) => PlayerAction::Fall,
            Some(PlayerAction::ItemThrowAirF) => PlayerAction::Fall,
            Some(PlayerAction::ItemThrowAirB) => PlayerAction::Fall,

            // Aerials
            Some(PlayerAction::Uair)     => PlayerAction::Fall,
            Some(PlayerAction::Dair)     => PlayerAction::Fall,
            Some(PlayerAction::Fair)     => PlayerAction::Fall,
            Some(PlayerAction::Bair)     => PlayerAction::Fall,
            Some(PlayerAction::Nair)     => PlayerAction::Fall,
            Some(PlayerAction::UairLand) => PlayerAction::Idle,
            Some(PlayerAction::DairLand) => PlayerAction::Idle,
            Some(PlayerAction::FairLand) => PlayerAction::Idle,
            Some(PlayerAction::BairLand) => PlayerAction::Idle,
            Some(PlayerAction::NairLand) => PlayerAction::Idle,

            // Specials
            Some(PlayerAction::UspecialGroundStart) => PlayerAction::Idle,
            Some(PlayerAction::DspecialGroundStart) => PlayerAction::Idle,
            Some(PlayerAction::SspecialGroundStart) => PlayerAction::Idle,
            Some(PlayerAction::NspecialGroundStart) => PlayerAction::Idle,

            Some(PlayerAction::UspecialAirStart) => PlayerAction::Fall,
            Some(PlayerAction::DspecialAirStart) => PlayerAction::Fall,
            Some(PlayerAction::SspecialAirStart) => PlayerAction::Fall,
            Some(PlayerAction::NspecialAirStart) => PlayerAction::Fall,

            // Taunts
            Some(PlayerAction::TauntUp)    => PlayerAction::Idle,
            Some(PlayerAction::TauntDown)  => PlayerAction::Idle,
            Some(PlayerAction::TauntLeft)  => PlayerAction::Idle,
            Some(PlayerAction::TauntRight) => PlayerAction::Idle,

            Some(PlayerAction::Eliminated)         => PlayerAction::Eliminated,
            Some(PlayerAction::DummyFramePreStart) => PlayerAction::Spawn,
        })
    }

    pub fn set_action_idle_from_ledge(&mut self, context: &mut StepContext, state: &ActionState) -> PlayerAction {
        if let Location::GrabbedLedge { platform_i, .. } = self.body.location {
            let platform = &context.surfaces[platform_i];
            let (world_x, _) = self.bps_xy(context, state);
            let x = platform.world_x_to_plat_x_clamp(world_x);

            self.body.location = Location::Surface { platform_i, x };
            PlayerAction::Idle
        }
        else {
            panic!("Location must be on ledge to call this function.")
        }
    }

    pub fn set_action_fall_from_ledge_jump(&mut self, context: &mut StepContext, state: &ActionState) -> PlayerAction {
        self.set_airbourne(context, state);
        PlayerAction::Fall
    }

    pub fn relative_f(&self, input: f32) -> f32 {
        self.body.relative_f(input)
    }

    fn specialfall_action(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        self.fall_action(context.entity_def);
        self.air_drift(context);
        None
    }

    fn fall_action(&mut self, entity_def: &EntityDef) {
        self.body.y_vel += entity_def.gravity;
        if self.body.y_vel < entity_def.terminal_vel {
            self.body.y_vel = entity_def.terminal_vel;
        }
    }

    fn fastfall_action(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        if !self.fastfalled {
            if context.input[0].stick_y < -0.65 && context.input[3].stick_y > -0.1 && self.body.y_vel < 0.0 {
                self.fastfalled = true;
                self.body.y_vel = context.entity_def.fastfall_terminal_vel;
            }
            else {
                self.body.y_vel += context.entity_def.gravity;
                if self.body.y_vel < context.entity_def.terminal_vel {
                    self.body.y_vel = context.entity_def.terminal_vel;
                }
            }
        }
        None
    }

    pub fn get_held_item(&self, entities: &Entities) -> Option<EntityKey> {
        for (key, entity) in entities.iter() {
            if let EntityType::Item (item) = &entity.ty {
                if let Location::ItemHeldByPlayer (player_entity_key) = item.body.location {
                    if let Some(check_entity) = entities.get(player_entity_key) {
                        if let Some(check_player) = &check_entity.ty.get_player() {
                            if check_player.id == self.id {
                                return Some(key);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    pub fn get_held_fighter(&self, entities: &Entities) -> Option<EntityKey> {
        for (key, entity) in entities.iter() {
            if let EntityType::Fighter (fighter) = &entity.ty {
                if let Location::GrabbedByPlayer (player_entity_key) = fighter.get_player().body.location {
                    if let Some(check_entity) = entities.get(player_entity_key) {
                        if let Some(check_player) = &check_entity.ty.get_player() {
                            if check_player.id == self.id {
                                return Some(key);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    pub fn item_grab(&mut self) -> Option<ActionResult> {
        // TODO: make the context available here so we can call this:
        //match PlayerAction::from_u64(self.action) {
        //    Some(PlayerAction::Jab) => ActionResult::set_action(PlayerAction::ItemGrab),
        //    _ => {}
        //}
        None
    }

    /*
     *  Begin physics section
     */

    pub fn physics_step(&mut self, context: &mut StepContext, state: &ActionState, game_frame: usize, goal: Goal) -> Option<ActionResult> {
        let fighter_frame = &context.entity_def.actions[state.action.as_ref()].frames[state.frame as usize];
        match self.body.physics_step(context, state, fighter_frame) {
            Some(PhysicsResult::Fall) => {
                self.fastfalled = false;
                ActionResult::set_action(PlayerAction::Fall)
            }
            Some(PhysicsResult::Land) => {
                self.hitstun = 0.0;
                self.land(context, state)
            }
            Some(PhysicsResult::Teeter) => {
                ActionResult::set_action(PlayerAction::Teeter)
            }
            Some(PhysicsResult::LedgeGrab) => {
                self.fastfalled = false;
                self.air_jumps_left = context.entity_def.fighter().map(|x| x.air_jumps).unwrap_or(1);
                self.hit_by = None;
                ActionResult::set_action(PlayerAction::LedgeGrab)
            }
            Some(PhysicsResult::OutOfBounds) => {
                self.die(context, game_frame, goal)
            }
            None => None,
        }
    }

    fn apply_friction(&mut self, entity: &EntityDef, state: &ActionState) {
        match state.get_action() {
            Some(PlayerAction::Idle) |
            Some(PlayerAction::Dash) |
            Some(PlayerAction::Shield) |
            Some(PlayerAction::ShieldOn) |
            Some(PlayerAction::ShieldOff) |
            Some(PlayerAction::Damage)
              => { self.body.apply_friction_weak(entity) }
            _ => { self.body.apply_friction_strong(entity) }
        }
    }

    /// Returns the Rect surrounding the player that the camera must include
    pub fn cam_area(&self, state: &ActionState, cam_max: &Rect, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> Option<Rect> {
        match state.get_action() {
            Some(PlayerAction::Eliminated) => None,
            _ => {
                let (x, y) = self.public_bps_xy(entities, entity_defs, surfaces, state);
                let mut left  = x;
                let mut right = x;
                let mut bot   = y - 5.0;
                let mut top   = y + 25.0;

                if self.body.face_right {
                    left  -= 7.0;
                    right += 40.0;
                }
                else {
                    left  -= 40.0;
                    right += 7.0;
                }

                if left < cam_max.left() {
                    let diff = left - cam_max.left();
                    left  -= diff;
                    right -= diff;
                }
                else if right > cam_max.right() {
                    let diff = right - cam_max.right();
                    left  -= diff;
                    right -= diff;
                }

                if bot < cam_max.bot() {
                    let diff = bot - cam_max.bot();
                    bot -= diff;
                    top -= diff;
                }
                else if top > cam_max.top() {
                    let diff = top - cam_max.top();
                    bot -= diff;
                    top -= diff;
                }

                Some(Rect {
                    x1: left,
                    x2: right,
                    y1: bot,
                    y2: top,
                })
            }
        }
    }

    fn land(&mut self, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        let action = state.get_action::<PlayerAction>();

        self.land_frame_skip = match action {
            Some(_) if action.as_ref().map_or(false, |x| x.is_air_attack()) && self.lcancel_timer > 0 => 1,
            Some(PlayerAction::AerialDodge) => 2,
            Some(PlayerAction::SpecialFall) => 2,
            _ => 0
        };

        self.aerial_dodge_frame = if let Some(PlayerAction::AerialDodge) = action { Some(state.frame as u64 ) } else { None };

        self.fastfalled = false;
        self.air_jumps_left = context.entity_def.fighter().map(|x| x.air_jumps).unwrap_or(1);
        self.hit_by = None;

        ActionResult::set_action(match action {
            Some(PlayerAction::Uair)            => PlayerAction::UairLand,
            Some(PlayerAction::Dair)            => PlayerAction::DairLand,
            Some(PlayerAction::Fair)            => PlayerAction::FairLand,
            Some(PlayerAction::Bair)            => PlayerAction::BairLand,
            Some(PlayerAction::Nair)            => PlayerAction::NairLand,
            Some(PlayerAction::ShieldBreakFall) => PlayerAction::ShieldBreakGetup,
            Some(PlayerAction::DamageFly) | Some(PlayerAction::DamageFall) => {
                if self.tech_timer.is_active() {
                    if self.relative_f(context.input[0].stick_x) > 0.5 {
                        PlayerAction::TechF
                    } else if self.relative_f(context.input[0].stick_x) < -0.5 {
                        PlayerAction::TechB
                    } else {
                        PlayerAction::TechN
                    }
                } else {
                    PlayerAction::MissedTechStart
                }
            }
            Some(PlayerAction::SpecialFall) | Some(PlayerAction::AerialDodge) | None => PlayerAction::SpecialLand,
            Some(_) if self.body.y_vel >= -1.0 => PlayerAction::Idle, // no impact land
            Some(_) => PlayerAction::Land
        })
    }

    fn walk(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        let walk_init_vel = self.relative_f(context.entity_def.walk_init_vel);
        if (walk_init_vel > 0.0 && self.body.x_vel < walk_init_vel) ||
           (walk_init_vel < 0.0 && self.body.x_vel > walk_init_vel) {
            self.body.x_vel += walk_init_vel;
        }
        ActionResult::set_action(PlayerAction::Walk)
    }

    fn die(&mut self, context: &mut StepContext, game_frame: usize, goal: Goal) -> Option<ActionResult> {
        self.body = if context.stage.respawn_points.len() == 0 {
            Body::new(
                Location::Airbourne { x: 0.0, y: 0.0 },
                true
            )
        } else {
            let respawn = &context.stage.respawn_points[self.id % context.stage.respawn_points.len()];
            Body::new(
                Location::Airbourne { x: respawn.x, y: respawn.y },
                respawn.face_right
            )
        };
        self.air_jumps_left = context.entity_def.fighter().map(|x| x.air_jumps).unwrap_or(1);
        self.fastfalled = false;
        self.hitstun = 0.0;


        self.result.deaths.push(DeathRecord {
            player: self.hit_by,
            frame: game_frame,
        });

        match goal {
            Goal::LastManStanding => {
                if let Some(mut stocks) = self.stocks {
                    stocks -= 1;
                    self.stocks = Some(stocks);

                    if stocks == 0 {
                        ActionResult::set_action(PlayerAction::Eliminated)
                    } else {
                        ActionResult::set_action(PlayerAction::ReSpawn)
                    }
                } else {
                    None
                }
            }
            Goal::KillDeathScore => {
                ActionResult::set_action(PlayerAction::ReSpawn)
            }
        }
    }

    pub fn debug_print(&self, player_input: &PlayerInput, debug: &DebugEntity, index: EntityKey) -> Vec<String> {
        let mut lines: Vec<String> = vec!();

        if debug.frame {
            lines.push(format!("Entity: {:?}  shield HP: {:.5}  hitstun: {:.5}  tech timer: {:?}  lcancel timer: {}",
                index, self.shield_hp, self.hitstun, self.tech_timer, self.lcancel_timer));
        }

        if debug.input {
            let stick_x   = player_input.stick_x.value;
            let stick_y   = player_input.stick_y.value;
            let c_stick_x = player_input.c_stick_x.value;
            let c_stick_y = player_input.c_stick_y.value;
            let l_trigger = player_input.l_trigger.value;
            let r_trigger = player_input.r_trigger.value;

            lines.push(format!("Entity: {:?}  VALUE  stick_x: {:.5}  stick_y: {:.5}  c_stick_x: {:.5}  c_stick_y: {:.5}  l_trigger: {:.5}  r_trigger: {:.5}",
                index, stick_x, stick_y, c_stick_x, c_stick_y, l_trigger, r_trigger));
        }

        if debug.input_diff {
            let stick_x   = player_input.stick_x.diff;
            let stick_y   = player_input.stick_y.diff;
            let c_stick_x = player_input.c_stick_x.diff;
            let c_stick_y = player_input.c_stick_y.diff;
            let l_trigger = player_input.l_trigger.diff;
            let r_trigger = player_input.r_trigger.diff;

            lines.push(format!("Entity: {:?}  DIFF   stick_x: {:.5}  stick_y: {:.5}  c_stick_x: {:.5}  c_stick_y: {:.5}  l_trigger: {:.5}  r_trigger: {:.5}",
                index, stick_x, stick_y, c_stick_x, c_stick_y, l_trigger, r_trigger));
        }

        lines
    }

    pub fn result(&self, state: &ActionState) -> RawPlayerResult {
        let mut result = self.result.clone();
        result.final_damage = Some(self.body.damage);
        result.ended_as_fighter = Some(state.entity_def_key.clone());
        result.team = self.team;
        result
    }

    pub fn hit_particles(&mut self, point: (f32, f32), hitbox: &HitBox) {
        self.particles.push(Particle {
            color:       graphics::get_team_color3(self.team),
            counter:     0,
            counter_max: 2,
            x:           point.0,
            y:           point.1,
            z:           0.0,
            angle:       hitbox.angle.to_radians(),
            p_type:      ParticleType::Hit {
                knockback: hitbox.bkb + hitbox.kbg * 70.0, // TODO: get actual knockback
                damage:    hitbox.damage, // TODO: get actual damage
            }
        });
    }

    pub fn air_jump_particles(&mut self, context: &mut StepContext, state: &ActionState) {
        let (x, y) = self.bps_xy(context, state);
        self.particles.push(Particle {
            color:       graphics::get_team_color3(self.team),
            counter:     0,
            counter_max: 40,
            x:           x,
            y:           y,
            z:           0.0,
            angle:       0.0,
            p_type:      ParticleType::AirJump
        });
    }

    pub fn knockback_particles(&mut self, context: &mut StepContext, state: &ActionState) {
        let kb_vel = (self.body.kb_x_vel * self.body.kb_x_vel + self.body.kb_y_vel * self.body.kb_y_vel).sqrt();
        let angle = self.body.kb_y_vel.atan2(self.body.kb_x_vel) + context.rng.gen_range(-0.2, 0.2);
        let vec_mult = context.rng.gen_range(0.7, 1.0);
        let (x, y) = self.bps_xy(context, state);
        let num = if self.hitstun > 0.0 {
            (kb_vel/2.0) as usize
        } else {
            0
        };

        for _ in 0..num {
            let z = context.rng.gen_range(-1.0, 1.0);
            self.particles.push(Particle {
                color:       graphics::get_team_color3(self.team),
                counter:     0,
                counter_max: 30,
                x:           x,
                y:           y + self.body.ecb.top / 2.0,
                z,
                angle:       context.rng.gen_range(0.0, 2.0 * PI),
                p_type:      ParticleType::Spark {
                    x_vel:      angle.cos() * vec_mult * -1.0,
                    y_vel:      angle.sin() * vec_mult * -1.0,
                    z_vel:      context.rng.gen_range(0.0, 0.4) * z.signum(),
                    size:       context.rng.gen_range(1.0, 3.0),
                    angle_vel:  context.rng.gen_range(0.0, 1.0),
                }
            });
        }
    }

    pub fn land_particles(&mut self, context: &mut StepContext, state: &ActionState) {
        let num = match state.frame_no_restart { // use frame_no_restart instead as it doesnt get skipped during lcancel
            1 => 3,
            2 => 1,
            3 => 4,
            4 => 2,
            5 => 3,
            6 => 2,
            _ => 0,
        };

        let (x, y) = self.bps_xy(context, state);
        let action = state.get_action::<PlayerAction>();

        let color = if
            action.map_or(false, |x| x.is_attack_land()) && self.land_frame_skip == 0 || // missed LCancel
            self.aerial_dodge_frame.map_or(false, |x| x > 0) // imperfect wavedash
        {
            [1.0, 1.0, 1.0]
        } else {
            graphics::get_team_color3(self.team)
        };

        for _ in 0..num {
            let z = context.rng.gen_range(-3.0, 3.0);
            self.particles.push(Particle {
                color,
                counter:     0,
                counter_max: 40,
                x:           x,
                y:           y,
                z,
                angle:       context.rng.gen_range(0.0, 2.0 * PI),
                p_type:      ParticleType::Spark {
                    x_vel:      context.rng.gen_range(-0.3, 0.3),
                    y_vel:      context.rng.gen_range(0.0, 0.2),
                    z_vel:      context.rng.gen_range(0.0, 0.5) * z.signum(),
                    size:       context.rng.gen_range(1.0, 3.0),
                    angle_vel:  context.rng.gen_range(0.0, 1.0),
                }
            });
        }
    }

    pub fn dash_particles(&mut self, context: &mut StepContext, state: &ActionState) {
        let num = match state.frame {
            0 => 3,
            1 => 1,
            2 => 1,
            3 => 2,
            4 => 4,
            5 => 3,
            6 => 2,
            _ => 0,
        };

        let (x, y) = self.bps_xy(context, state);
        let x_offset = self.relative_f(3.0);

        for _ in 0..num {
            let z = context.rng.gen_range(-6.0, 6.0);
            self.particles.push(Particle {
                color:       graphics::get_team_color3(self.team),
                counter:     0,
                counter_max: 40,
                x:           x + x_offset,
                y:           y,
                z,
                angle:       context.rng.gen_range(0.0, 2.0 * PI),
                p_type:      ParticleType::Spark {
                    x_vel:      if self.body.face_right { context.rng.gen_range(-0.3, 0.0) } else { context.rng.gen_range(0.0, 0.3) },
                    y_vel:      context.rng.gen_range(0.0, 0.3),
                    z_vel:      context.rng.gen_range(0.0, 0.3) * z.signum(),
                    size:       context.rng.gen_range(2.0, 4.0),
                    angle_vel:  context.rng.gen_range(0.0, 1.0),
                }
            });
        }
    }

    pub fn render(&self, entities: &Entities, fighters: &KeyedContextVec<EntityDef>, surfaces: &[Surface], state: &ActionState) -> RenderPlayer {
        let shield = if self.is_shielding(state) {
            let fighter_color = graphics::get_team_color3(self.team);
            let fighter = &fighters[state.entity_def_key.as_ref()];

            if let &Some(ref shield) = &fighter.shield {
                let c = &fighter_color;
                let m =  1.0 - self.shield_analog;
                Some(RenderShield {
                    distort: self.shield_stun_timer,
                    color:   [c[0] + (1.0 - c[0]) * m, c[1] + (1.0 - c[1]) * m, c[2] + (1.0 - c[2]) * m, 0.2 + self.shield_analog / 2.0],
                    radius:  self.shield_size(shield),
                    pos:     self.shield_pos(shield, entities, fighters, surfaces, state),
                })
            } else { None }
        } else { None };

        RenderPlayer {
            team:   self.team,
            damage: self.body.damage,
            stocks: self.stocks,
            shield,
        }
    }

    pub fn vector_arrows(&self, debug: &DebugEntity) -> Vec<VectorArrow> {
        let mut vector_arrows = vec!();

        if debug.stick_vector {
            if let Some((x, y)) = self.stick {
                vector_arrows.push(VectorArrow {
                    x,
                    y,
                    color: [0.7, 0.7, 0.7, 1.0]
                });
            }
        }
        if debug.c_stick_vector {
            if let Some((x, y)) = self.c_stick {
                vector_arrows.push(VectorArrow {
                    x,
                    y,
                    color: [1.0, 1.0, 0.0, 1.0]
                });
            }
        }
        if debug.di_vector {
            if let Some(angle) = self.body.hit_angle_pre_di {
                vector_arrows.push(VectorArrow {
                    x: angle.cos(),
                    y: angle.sin(),
                    color: [1.0, 0.0, 0.0, 1.0]
                });
            }
            if let Some(angle) = self.body.hit_angle_post_di {
                vector_arrows.push(VectorArrow {
                    x: angle.cos(),
                    y: angle.sin(),
                    color: [0.0, 1.0, 0.0, 1.0]
                });
            }
        }

        vector_arrows
    }

    pub fn process_message(&mut self, message: &MessagePlayer, context: &mut StepContext, state: &ActionState) -> Option<ActionResult> {
        match message {
            MessagePlayer::Thrown { angle, damage, bkb, kbg, entity_atk_i } => {
                let hitbox = HitBox {
                    shield_damage: 0.0,
                    damage: *damage,
                    bkb: *bkb,
                    kbg: *kbg,
                    angle: *angle,
                    hitstun: HitStun::Frames(0),
                    enable_clang: false,
                    enable_rebound: false,
                    effect: HitboxEffect::None,
                    enable_reverse_hit: false,
                };

                let hurtbox = HurtBox::default();
                self.launch(context, state, &hitbox, &hurtbox, *entity_atk_i)
            }
            MessagePlayer::Released => None,
        }
    }

    pub fn send_thrown_message(&self, context: &mut StepContext, angle: f32, damage: f32, bkb: f32, kbg: f32) {
        if let Some(recipient) = self.get_held_fighter(context.entities) {
            let angle = if !self.body.face_right {
                180.0 - angle
            } else {
                angle
            };
            let message = MessagePlayer::Thrown {
                angle,
                damage,
                bkb,
                kbg,
                entity_atk_i: context.entity_key,
            };

            context.messages.push(Message {
                recipient,
                contents:  MessageContents::Player(message)
            });
        }
    }
}

enum JumpResult {
    Button,
    Stick,
    None,
}

impl JumpResult {
    fn jump(&self) -> bool {
        match *self {
            JumpResult::Button | JumpResult::Stick => true,
            JumpResult::None => false
        }
    }
}

pub struct RenderPlayer {
    pub team:   usize,
    pub damage: f32,
    pub stocks: Option<u64>,
    pub shield: Option<RenderShield>,
}

pub struct RenderShield {
    pub distort: u64,
    pub color:   [f32; 4],
    pub radius:  f32,
    pub pos:     (f32, f32),
}
