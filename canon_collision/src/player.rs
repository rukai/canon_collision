use crate::collision::collision_box::CollisionResult;
use crate::graphics;
use crate::particle::{Particle, ParticleType};
use crate::results::{RawPlayerResult, DeathRecord};
use crate::rules::{Goal, Rules};
use crate::entity::{Entity, EntityType, StepContext, DebugEntity, VectorArrow, Entities, EntityKey, Message, MessageContents, MessageItem};
use crate::projectile::Projectile;
use crate::body::{Body, Location, Hitlag, PhysicsResult, HitlagResult};
use crate::item::{Item, ItemAction};

use canon_collision_lib::entity_def::*;
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::package::Package;
use canon_collision_lib::stage::{Stage, Surface};

use treeflection::KeyedContextVec;
use rand::Rng;
use num_traits::{FromPrimitive, ToPrimitive};

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

#[derive(Clone, Serialize, Deserialize)]
pub struct Player {
    pub entity_def_key:     String,
    pub id:                 usize, // unique id among players
    pub team:               usize,
    pub action:             u64, // always change through next_action
    pub set_action_called:  bool,
    pub new_action:         bool,

    // frame count values:
    // == -1 doesnt correspond to a frame in the fighter data, used for the action logic triggered directly after action state transition, must never be in this state after action_step()
    // >=  0  corresponds to a frame in the fighter data, used for the regular action logic step on each game frame
    pub frame:              i64,
    pub frame_norestart:    i64, // Used to keep track of total frames passed on states that loop

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
    pub ecb:                ECB,
    pub hitlist:            Vec<EntityKey>,
    pub hitstun:            f32,
    /// this is only used for end-game statistics so player id is fine
    pub hit_by:             Option<usize>,
    pub particles:          Vec<Particle>,
    pub aerial_dodge_frame: Option<u64>,
    pub result:             RawPlayerResult,

    // Only use for debug display
    pub frames_since_hit:  u64,
    pub hit_angle_pre_di:  Option<f32>,
    pub hit_angle_post_di: Option<f32>,
    pub stick:             Option<(f32, f32)>,
    pub c_stick:           Option<(f32, f32)>,
}

impl Player {
    pub fn new(entity_def_key: String, team: usize, id: usize, stage: &Stage, package: &Package, rules: &Rules) -> Player {
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
            // just place the player in midair even though this interacts weirdly with Action::Idle
            // which is supposed to be grounded.
            Location::Airbourne { x: 0.0, y: 0.0 }
        };

        Player {
            action:             Action::DummyFramePreStart.to_u64().unwrap(),
            set_action_called:  false,
            new_action:         false,
            frame:              0,
            frame_norestart:    0,
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
            ecb:                ECB::default(),
            hitlist:            vec!(),
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
            entity_def_key,

            // Only use for debug display
            frames_since_hit:  0,
            hit_angle_pre_di:  None,
            hit_angle_post_di: None,
            stick:             None,
            c_stick:           None,
        }
    }

    pub fn bps_xy(&self, context: &StepContext) -> (f32, f32) {
        let action_frame = self.get_entity_frame(&context.entity_defs[self.entity_def_key.as_ref()]);
        self.body.public_bps_xy(&context.entities, &context.entity_defs, action_frame, &context.surfaces)
    }

    pub fn public_bps_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        let action_frame = self.get_entity_frame(&entity_defs[self.entity_def_key.as_ref()]);
        self.body.public_bps_xy(entities, entity_defs, action_frame, surfaces)
    }

    pub fn grabbing_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        let (x, y) = self.public_bps_xy(entities, entity_defs, surfaces);
        if let Some(entity_frame) = self.get_entity_frame(&entity_defs[self.entity_def_key.as_ref()]) {
            (x + self.relative_f(entity_frame.grabbing_x), y + entity_frame.grabbing_y)
        } else {
            (x, y)
        }
    }

    pub fn is_shielding(&self) -> bool {
        match Action::from_u64(self.action) {
            Some(Action::Shield) |
            Some(Action::ShieldOn) |
            Some(Action::ShieldOff) |
            Some(Action::PowerShield)
              => true,
            _ => false
        }
    }

    pub fn set_airbourne(&mut self, context: &StepContext) {
        let (x, y) = self.bps_xy(context);
        self.fastfalled = false;
        self.body.location = Location::Airbourne { x, y };
    }

    pub fn public_set_action(&mut self, action: Action) {
        let action = action as u64;
        self.frame = 0;
        self.hitlist.clear();
        if self.action != action {
            self.frame_norestart = 0;
            self.action = action;
        }
    }

    fn set_action(&mut self, context: &mut StepContext, action: Action) {
        let action = action as u64;
        self.frame = 0;
        self.hitlist.clear();
        self.set_action_called = true;

        if self.action != action {
            self.new_action = true;
            self.frame = -1;
            self.frame_norestart = -1;
            self.action = action;

            self.frame_step(context);
            let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as i64 - 1;
            // TODO: move this assert somewhere earlier, maybe the fighter loading code?
            assert_ne!(last_action_frame, -1, "A subaction has a length of 0");
            self.frame = last_action_frame.min(self.frame + 1); // +1 instead of =0 so that frame_step can skip frames if it wants to
            self.frame_norestart += 1;
        }
    }

    fn interruptible(&self, fighter: &EntityDef) -> bool {
        self.frame >= fighter.actions[self.action as usize].iasa
    }

    fn first_interruptible(&self, fighter: &EntityDef) -> bool {
        self.frame == fighter.actions[self.action as usize].iasa
    }

    fn last_frame(&self, fighter: &EntityDef) -> bool {
        self.frame == fighter.actions[self.action as usize].frames.len() as i64 - 1
    }

    pub fn platform_deleted(&mut self, entities: &Entities, fighters: &KeyedContextVec<EntityDef>, surfaces: &[Surface], deleted_platform_i: usize) {
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
            self.public_set_action(Action::Fall); // TODO: use miss step state ^.^

            //manually perform self.set_airbourne(context);
            let (x, y) = self.public_bps_xy(entities, fighters, surfaces);
            self.body.location = Location::Airbourne { x, y };
            self.fastfalled = false;
        }
    }

    pub fn step_collision(&mut self, context: &mut StepContext, col_results: &[CollisionResult]) {
        for col_result in col_results {
            match col_result {
                &CollisionResult::HitAtk { entity_defend_i, ref hitbox, ref point } => {
                    self.hit_particles(point.clone(), hitbox);
                    self.hitlist.push(entity_defend_i);
                    self.body.hitlag = Hitlag::Some ((hitbox.damage / 3.0 + 3.0) as u64);
                }
                &CollisionResult::HitDef { ref hitbox, ref hurtbox, entity_atk_i } => {
                    let entity_atk = &context.entities[entity_atk_i];

                    let damage_done = hitbox.damage * hurtbox.damage_mult; // TODO: apply staling
                    self.body.damage += damage_done;

                    let damage_launch = 0.05 * (hitbox.damage * (damage_done + self.body.damage.floor())) + (damage_done + self.body.damage) * 0.1;
                    let weight = 2.0 - (2.0 * context.entity_def.weight) / (1.0 + context.entity_def.weight);
                    let kbg = hitbox.kbg + hurtbox.kbg_add;
                    let bkb = hitbox.bkb + hurtbox.bkb_add;

                    let mut kb_vel = (bkb + kbg * (damage_launch * weight * 1.4 + 18.0)).min(2500.0);

                    if let Some(action) = Action::from_u64(self.action) {
                        match action {
                            Action::Crouch => {
                                kb_vel *= 0.67;
                            }
                            _ => { }
                        }
                    }

                    if !self.body.is_grabbed() || kb_vel > 50.0 {
                        self.hitstun = match hitbox.hitstun {
                            HitStun::FramesTimesKnockback (frames) => { frames * kb_vel }
                            HitStun::Frames               (frames) => { frames as f32 }
                        };

                        self.set_airbourne(context);

                        if kb_vel > 80.0 {
                            self.set_action(context, Action::DamageFly);
                        }
                        else {
                            self.set_action(context, Action::Damage);
                        }
                    }

                    // handle sakurai angle
                    let angle_deg = if hitbox.angle == 361.0 {
                        if kb_vel < 32.1 {
                            0.0
                        }
                        else {
                            44.0
                        }
                    } else if hitbox.angle == 180.0 - 361.0 {
                        if kb_vel < 32.1 {
                            180.0
                        }
                        else {
                            180.0 - 44.0
                        }
                    } else {
                        hitbox.angle
                    };

                    // convert from degrees to radians
                    let angle_rad = angle_deg.to_radians() + if angle_deg < 0.0 { PI * 2.0 } else { 0.0 };

                    // handle reverse hits
                    let behind_entity_atk = self.bps_xy(context).0 < entity_atk.bps_xy(context).0 && entity_atk.face_right() ||
                                            self.bps_xy(context).0 > entity_atk.bps_xy(context).0 && !entity_atk.face_right();
                    let angle = if hitbox.enable_reverse_hit && behind_entity_atk { PI - angle_rad } else { angle_rad };

                    // debug data
                    self.hit_angle_pre_di = Some(angle);
                    self.hit_angle_post_di = None;
                    self.frames_since_hit = 0;

                    self.body.hitlag = Hitlag::Launch { counter: (hitbox.damage / 3.0 + 3.0) as u64, kb_vel, angle, wobble_x: 0.0 };
                    self.hit_by = context.entities.get(entity_atk_i).and_then(|x| x.player_id());
                    // TODO: determine from angle (current logic falls over when reverse hit is disabled)
                    self.body.face_right = self.bps_xy(context).0 < entity_atk.bps_xy(context).0;
                }
                &CollisionResult::HitShieldAtk { ref hitbox, ref power_shield, entity_defend_i } => {
                    if let EntityType::Player (player_def) = &context.entities[entity_defend_i].ty {
                        self.hitlist.push(entity_defend_i);
                        if let &Some(ref power_shield) = power_shield {
                            if let (Some(Action::PowerShield), &Some(ref stun)) = (Action::from_u64(self.action), &power_shield.enemy_stun) {
                                if stun.window > player_def.frame as u64 {
                                    self.stun_timer = stun.duration;
                                }
                            }
                        }

                        let x_diff = self.bps_xy(context).0 - player_def.bps_xy(context).0;
                        let vel = hitbox.damage.floor() * (player_def.shield_analog - 0.3) * 0.1 + 0.02;
                        if self.body.is_platform() {
                            self.body.x_vel += vel * x_diff.signum();
                        }
                        self.body.hitlag = Hitlag::Some ((hitbox.damage / 3.0 + 3.0) as u64);
                    }
                }
                &CollisionResult::HitShieldDef { ref hitbox, ref power_shield, entity_atk_i } => {
                    if let &Some(ref power_shield) = power_shield {
                        if let (Some(Action::PowerShield), &Some(ref parry)) = (Action::from_u64(self.action), &power_shield.parry) {
                            if parry.window > self.frame as u64 {
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
                    let x_diff = self.bps_xy(context).0 - context.entities[entity_atk_i].bps_xy(context).0;
                    let vel = (hitbox.damage.floor() * (0.195 * analog_mult + 0.09) + 0.4) * vel_mult;
                    self.body.x_vel = vel.min(2.0) * x_diff.signum();
                    self.shield_stun_timer = (hitbox.damage.floor() * (analog_mult + 0.3) * 0.975 + 2.0) as u64;
                    self.body.hitlag = Hitlag::Some ((hitbox.damage / 3.0 + 3.0) as u64);
                }
                &CollisionResult::GrabAtk (_entity_defend_i) => {
                    self.set_action(context, Action::GrabbingIdle);
                }
                &CollisionResult::GrabDef (entity_atk_i) => {
                    self.body.face_right = !context.entities[entity_atk_i].face_right();
                    self.body.location = Location::GrabbedByPlayer(entity_atk_i);
                    self.set_action(context, Action::GrabbedIdle);
                }
                _ => { }
            }
        }
    }

    /*
     *  Begin action section
     */

    pub fn action_hitlag_step(&mut self, context: &mut StepContext) {
        // If the action or frame is out of bounds jump to a valid one.
        // This is needed because we can continue from any point in a replay and replays may
        // contain actions or frames that no longer exist.
        if self.action as usize >= context.entity_def.actions.len() {
            self.public_set_action(Action::Idle);
        } else {
            let fighter_frames = &context.entity_def.actions[self.action as usize].frames;
            if self.frame as usize >= fighter_frames.len() {
                self.frame = 0;
            }
        }
        // The code from this point onwards can assume we are on a valid action and frame

        let action_frame = self.get_entity_frame(&context.entity_def);
        match self.body.hitlag_step(context, action_frame) {
            HitlagResult::NoHitlag => self.action_step(context),
            HitlagResult::Hitlag => { }
            HitlagResult::LaunchAir { angle } => {
                self.fastfalled = false;
                self.hit_angle_post_di = Some(angle);
            }
            HitlagResult::LaunchGround { angle } => {
                self.hit_angle_post_di = Some(angle);
            }
        }
    }

    fn action_step(&mut self, context: &mut StepContext) {
        self.knockback_particles(context);

        // TODO: Gankra plz ... https://github.com/rust-lang/rust/issues/43244
        let mut new_particles = vec!();
        for mut particle in self.particles.drain(..) {
            if !particle.step() {
                new_particles.push(particle);
            }
        }
        self.particles = new_particles;

        if !self.is_shielding() {
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
            context.input.z.press && !(self.frame == 0 && Action::from_u64(self.action).as_ref().map_or(false, |x| x.is_air_attack())) // only register z press if its not from an attack
        {
            if let &Some(ref lcancel) = &context.entity_def.lcancel {
                self.lcancel_timer = lcancel.active_window;
            }
        }

        self.frames_since_hit += 1;
        if self.frames_since_hit > 60 {
            self.hit_angle_pre_di = None;
            self.hit_angle_post_di = None;
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

        let fighter_frame = &context.entity_def.actions[self.action as usize].frames[self.frame as usize];

        // update ecb
        let prev_bottom = self.ecb.bottom;
        self.ecb = fighter_frame.ecb.clone();
        match Action::from_u64(self.action) {
            Some(Action::JumpF) | Some(Action::JumpB) | Some(Action::JumpAerialF) | Some(Action::JumpAerialB) if self.frame < 10
                => { self.ecb.bottom = prev_bottom }
            _   => { }
        }

        if fighter_frame.force_hitlist_reset {
            self.hitlist.clear();
        }

        self.set_action_called = false;
        self.new_action = false;
        self.frame_step(context);

        let action_frames = context.entity_def.actions[self.action as usize].frames.len() as i64;
        if !self.set_action_called && self.frame + 1 >= action_frames {
            // Because frames can be added/removed in the in game editor, we need to be ready to handle the frame index going out of bounds for any action automatically.
            self.action_expired(context);
        }

        if !self.set_action_called { // action_expired() can call set_action()
            self.frame += 1;
        }
        if !self.new_action {
            self.frame_norestart += 1;
        }
    }

    fn frame_step(&mut self, context: &mut StepContext) {
        if let Some(action) = Action::from_u64(self.action) {
            match action {
                Action::Spawn => { }
                Action::ReSpawn => { }
                Action::ReSpawnIdle => self.spawn_idle(context),

                Action::AerialFall | Action::JumpAerialF |
                Action::Fair       | Action::Bair |
                Action::Dair       | Action::Uair |
                Action::Nair       | Action::JumpAerialB |
                Action::Fall
                => self.aerial_action(context),

                Action::JumpF      | Action::JumpB
                => self.jump_action(context),

                Action::Jab       | Action::Jab2 |
                Action::Jab3      | Action::Utilt |
                Action::Ftilt     | Action::DashAttack |
                Action::Dsmash    | Action::Fsmash |
                Action::Usmash    | Action::Idle |
                Action::Grab      | Action::DashGrab |
                Action::TauntUp   | Action::TauntDown |
                Action::TauntLeft | Action::TauntRight |
                Action::CrouchEnd
                => self.ground_idle_action(context),

                Action::ItemThrowU    | Action::ItemThrowD |
                Action::ItemThrowF    | Action::ItemThrowB |
                Action::ItemThrowAirU | Action::ItemThrowAirD |
                Action::ItemThrowAirF | Action::ItemThrowAirB
                => self.item_throw_action(context),

                Action::FairLand | Action::BairLand |
                Action::UairLand | Action::DairLand |
                Action::NairLand | Action::SpecialLand
                => self.attack_land_action(context),

                Action::Teeter |
                Action::TeeterIdle       => self.teeter_action(context),
                Action::Land             => self.land_action(context),
                Action::DamageFly        => self.damage_fly_action(context),
                Action::DamageFall       => self.damage_fall_action(context),
                Action::Damage           => self.damage_action(context),
                Action::MissedTechIdle   => self.missed_tech_action(context),
                Action::MissedTechStart  => self.missed_tech_start_action(context.entity_def),
                Action::AerialDodge      => self.aerialdodge_action(context),
                Action::SpecialFall      => self.specialfall_action(context),
                Action::Dtilt            => self.dtilt_action(context),
                Action::CrouchStart      => self.crouch_start_action(context),
                Action::Crouch           => self.crouch_action(context),
                Action::Walk             => self.walk_action(context),
                Action::Dash             => self.dash_action(context),
                Action::Run              => self.run_action(context),
                Action::RunEnd           => self.run_end_action(context),
                Action::TiltTurn         => self.tilt_turn_action(context),
                Action::SmashTurn        => self.smash_turn_action(context),
                Action::RunTurn          => self.run_turn_action(context),
                Action::LedgeIdle        => self.ledge_idle_action(context),
                Action::ShieldOn         => self.shield_on_action(context),
                Action::PowerShield      => self.power_shield_action(context),
                Action::Shield           => self.shield_action(context),
                Action::ShieldOff        => self.shield_off_action(context),
                Action::ShieldBreakFall  => self.shield_break_fall_action(context.entity_def),
                Action::ShieldBreakGetup => self.shield_break_getup_action(),
                Action::Stun             => self.stun_action(context),
                Action::GrabbingIdle     => self.grabbing_idle_action(context),
                Action::GrabbedIdle      => self.grabbed_idle_action(context),
                _ => { }
            }
        }
    }

    fn ledge_idle_action(&mut self, context: &mut StepContext) {
        if
            (context.input[0].  stick_y < -0.2 && context.input[1].  stick_y >= -0.2) ||
            (context.input[0].c_stick_y < -0.2 && context.input[1].c_stick_y >= -0.2) ||
            (self.relative_f(context.input[0].  stick_x) < -0.2 && self.relative_f(context.input[1].  stick_x) >= -0.2) ||
            (self.relative_f(context.input[0].c_stick_x) < -0.2 && self.relative_f(context.input[1].c_stick_x) >= -0.2)
        {
            self.set_airbourne(context);
            self.set_action(context, Action::Fall);
        }
        else if context.input.x.press || context.input.y.press || (context.input[0].stick_y > 0.65 && context.input[1].stick_y <= 0.65) {
            if self.body.damage < 100.0 {
                self.set_action(context, Action::LedgeJump);
            }
            else {
                self.set_action(context, Action::LedgeJumpSlow);
            }
        }
        else if
            (self.relative_f(context.input[0].stick_x) > 0.2 && self.relative_f(context.input[1].stick_x) <= 0.2) ||
            (context.input[0].stick_y > 0.2 && context.input[1].stick_y <= 0.2)
        {
            if self.body.damage < 100.0 {
                self.set_action(context, Action::LedgeGetup);
            }
            else {
                self.set_action(context, Action::LedgeGetupSlow);
            }
        }
        else if context.input.a.press || context.input.b.press || (context.input[0].c_stick_y > 0.65 && context.input[1].c_stick_x <= 0.65) {
            if self.body.damage < 100.0 {
                self.set_action(context, Action::LedgeAttack);
            }
            else {
                self.set_action(context, Action::LedgeAttackSlow);
            }
        }
        else if
            context.input.l.press || context.input.r.press ||
            (context.input[0].l_trigger > 0.3 && context.input[1].l_trigger <= 0.3) || (context.input[0].r_trigger > 0.3 && context.input[1].r_trigger <= 0.3) ||
            (self.relative_f(context.input[0].c_stick_x) > 0.8 && self.relative_f(context.input[1].c_stick_x) <= 0.8)
        {
            if self.body.damage < 100.0 {
                self.set_action(context, Action::LedgeRoll);
            }
            else {
                self.set_action(context, Action::LedgeRollSlow);
            }
        }
        else if self.ledge_idle_timer > 600 {
            self.set_airbourne(context);
            self.set_action(context, Action::DamageFall);
        }
        self.ledge_idle_timer += 1;
    }

    fn missed_tech_start_action(&mut self, fighter: &EntityDef) {
        if self.frame == -1 {
            self.apply_friction(fighter);
        } else {
            self.body.x_vel = 0.0;
        }
    }

    fn missed_tech_action(&mut self, context: &mut StepContext) {
        if self.relative_f(context.input[0].stick_x) < -0.7 {
            self.set_action(context, Action::MissedTechGetupB);
        }
        else if self.relative_f(context.input[0].stick_x) > 0.7 {
            self.set_action(context, Action::MissedTechGetupF);
        }
        else if self.relative_f(context.input[0].stick_x) > 0.7 {
            self.set_action(context, Action::MissedTechGetupF);
        }
        else if context.input[0].stick_y > 0.7 {
            self.set_action(context, Action::MissedTechGetupN);
        }
        else if context.input.a.press || context.input.b.press {
            self.set_action(context, Action::MissedTechAttack);
        }
        else {
            if let Some(getup_frame) = context.entity_def.missed_tech_forced_getup {
                if self.frame_norestart > getup_frame as i64 {
                    self.set_action(context, Action::MissedTechGetupN);
                }
            }

            self.hitstun -= 1.0;
            self.apply_friction(context.entity_def);
        }
    }

    fn damage_action(&mut self, context: &mut StepContext) {
        self.hitstun -= 1.0;
        if self.hitstun <= 0.0 {
            if self.body.is_airbourne() {
                self.set_action(context, Action::Fall);
            }
            else {
                self.set_action(context, Action::Idle);
            }
        }
        else {
            if self.body.is_airbourne() {
                self.fall_action(context.entity_def);
            }
            else {
                self.apply_friction(context.entity_def);
            }
        }
    }

    fn damage_fly_action(&mut self, context: &mut StepContext) {
        self.hitstun -= 1.0;
        self.fall_action(context.entity_def);
        if self.hitstun <= 0.0 {
            self.set_action(context, Action::DamageFall);
        }
    }

    fn damage_fall_action(&mut self, context: &mut StepContext) {
        if self.interruptible(context.entity_def) {
            if self.check_attacks_aerial(context) { }
            else if context.input.b.press {
                // special attack
            }
            else if self.check_jump_aerial(context) { }
            else if
                (context.input[0].stick_x >  0.7 && context.input[1].stick_x <  0.7) ||
                (context.input[0].stick_x < -0.7 && context.input[1].stick_x > -0.7) ||
                (context.input[0].stick_y >  0.7 && context.input[1].stick_y <  0.7) ||
                (context.input[0].stick_y < -0.7 && context.input[1].stick_y > -0.7)
            {
                self.set_action(context, Action::Fall);
            }
            else {
                self.fastfall_action(context);
                self.air_drift(context);
            }
        }
        else {
            self.fastfall_action(context);
            self.air_drift(context);
        }
    }

    fn spawn_idle(&mut self, context: &mut StepContext) {
        if self.check_attacks_aerial(context) { }
        else if context.input.b.press {
            // special attack
        }
        else if self.check_jump_aerial(context) { }
        else if context.input.l.press || context.input.r.press {
            self.aerialdodge(context);
        }
        else if context.input[0].stick_x.abs() > 0.2 || context.input[0].stick_y.abs() > 0.2 {
            self.set_action(context, Action::Fall);
        }
        else if self.frame_norestart >= 1000 {
            self.set_action(context, Action::Fall);
        }
    }

    fn aerial_action(&mut self, context: &mut StepContext) {
        if self.interruptible(context.entity_def) {
            if self.check_attacks_aerial(context) { }
            else if context.input.b.press {
                // special attack
            }
            else if self.check_jump_aerial(context) { }
            else if context.input.l.press || context.input.r.press {
                self.aerialdodge(context);
            }
            else {
                self.air_drift(context);
                self.fastfall_action(context);
            }
        }
        else {
            self.air_drift(context);
            self.fastfall_action(context);
        }
    }

    fn jump_action(&mut self, context: &mut StepContext) {
        if self.check_attacks_aerial(context) { }
        else if context.input.b.press {
            // special attack
        }
        else if self.check_jump_aerial(context) { }
        else if context.input.l.press || context.input.r.press {
            self.aerialdodge(context);
        }
        else if self.frame >= 0 {
            self.air_drift(context);
            self.fastfall_action(context);
        }
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

    fn tilt_turn_action(&mut self, context: &mut StepContext) {
        let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as u64 - 1;
        if self.frame == context.entity_def.tilt_turn_flip_dir_frame as i64 ||
            (context.entity_def.tilt_turn_flip_dir_frame > last_action_frame && self.last_frame(&context.entity_def)) // ensure turn still occurs if run_turn_flip_dir_frame is invalid
        {
            self.body.face_right = !self.body.face_right;
        }

        if context.entity_def.tilt_turn_into_dash_iasa as i64 >= self.frame && self.relative_f(context.input[0].stick_x) > 0.79 {
            if context.entity_def.tilt_turn_flip_dir_frame > context.entity_def.tilt_turn_into_dash_iasa { // ensure turn still occurs even if tilt_turn_flip_dir_frame is invalid
                self.body.face_right = !self.body.face_right
            }
            self.set_action(context, Action::Dash);
        }
        else if self.check_jump(context) { }
        else if self.check_shield(context) { }
        else if self.check_special(context) { } // TODO: No neutral special
        else if self.check_smash(context) { }
        else if self.check_attacks(context) { }
        else if self.check_grab(context) { }
        else if self.check_taunt(context) { }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn smash_turn_action(&mut self, context: &mut StepContext) {
        if self.frame == 0 && self.relative_f(context.input[0].stick_x) > 0.79 {
            self.set_action(context, Action::Dash);
        }
        else if self.check_jump(context) { }
        else if self.check_shield(context) { }
        else if self.check_special(context) { } // TODO: No neutral special
        else if self.check_smash(context) { }
        else if self.check_attacks(context) { }
        else if self.check_grab(context) { }
        else if self.check_taunt(context) { }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn run_turn_action(&mut self, context: &mut StepContext) {
        let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as u64 - 1;
        if self.frame == context.entity_def.run_turn_flip_dir_frame as i64 ||
            (context.entity_def.run_turn_flip_dir_frame > last_action_frame && self.last_frame(&context.entity_def)) // ensure turn still occurs if run_turn_flip_dir_frame is invalid
        {
            self.body.face_right = !self.body.face_right;
        }

        if self.check_jump(context) { }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn crouch_start_action(&mut self, context: &mut StepContext) {
        if self.interruptible(&context.entity_def) {
            if self.check_pass_platform(context) { }
            else if self.check_shield(context) { }
            else if self.check_special(context) { } // TODO: no neutral/side special
            else if self.check_smash(context) { }
            else if self.check_attacks(context) { }
            else if self.check_grab(context) { }
            else if self.check_taunt(context) { }
            else if self.check_jump(context) { }
            else {
                self.apply_friction(&context.entity_def);
            }
        }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn crouch_action(&mut self, context: &mut StepContext) {
        if self.interruptible(&context.entity_def) {
            if context.input.stick_y.value > -0.61 { self.set_action(context, Action::CrouchEnd); }
            if self.check_jump(context) { }
            else if self.check_shield(context) { }
            else if self.check_special(context) { } // TODO: no neutral/side special
            else if self.check_smash(context) { }
            else if self.check_attacks(context) { }
            else if self.check_grab(context) { }
            else if self.check_taunt(context) { }
            else if self.check_dash(context) { }
            else if self.check_smash_turn(context) { }
            else if self.check_tilt_turn(context) { }
            else {
                self.apply_friction(&context.entity_def);
            }
        }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn dtilt_action(&mut self, context: &mut StepContext) {
        if self.interruptible(&context.entity_def) {
            if self.check_jump(context) { }
            else if self.check_shield(context) { }
            else if self.check_special(context) { } // TODO: no neutral/side special
            else if self.check_smash(context) { }
            else if self.check_attacks(context) { }
            else if self.check_grab(context) { }
            else if self.check_dash(context) { }
            else if self.check_smash_turn(context) { }
            else if self.check_tilt_turn(context) { }
            else if self.check_walk(context) { }
            else if self.check_taunt(context) { }
            else {
                self.apply_friction(&context.entity_def);
            }
        }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn ground_idle_action(&mut self, context: &mut StepContext) {
        if let Some(Action::Jab) = Action::from_u64(self.action) {
            if self.frame == 5 {
                let (x, y) = self.bps_xy(context);
                context.new_entities.push(Entity {
                    ty: EntityType::Projectile(
                        Projectile {
                            owner_id: Some(self.id),
                            entity_def_key: "PerfectlyGenericProjectile.cbor".to_string(),
                            action: 0,
                            frame: 0,
                            frame_no_restart: 0,
                            speed: 0.6,
                            angle: if self.body.face_right { 0.0 } else { PI },
                            x: x + self.relative_f(10.0),
                            y: y + 10.0,
                        }
                    )
                });
            }
        }

        if let Some(Action::TauntRight) = Action::from_u64(self.action) {
            if self.frame == 0 {
                let (x, y) = self.bps_xy(context);
                let x = x + 15.0;
                let y = y + 10.0;
                context.new_entities.push(Entity {
                    ty: EntityType::Item(
                        Item {
                            owner_id: None,
                            entity_def_key: "PerfectlyGenericObject.cbor".to_string(),
                            action: ItemAction::Fall as u64,
                            frame: 0,
                            frame_no_restart: 0,
                            body: Body::new(Location::Airbourne { x, y }, true),
                        }
                    )
                });
            }
        }

        if self.interruptible(&context.entity_def) {
            if self.check_jump(context) { }
            else if self.check_shield(context) { }
            else if self.check_special(context) { }
            else if self.check_smash(context) { }
            else if self.check_attacks(context) { }
            else if self.check_grab(context) { }
            else if self.check_taunt(context) { }
            else if self.check_crouch(context) { }
            else if self.check_dash(context) { }
            else if self.check_smash_turn(context) { }
            else if self.check_tilt_turn(context) { }
            else if self.check_walk(context) { }
            else {
                self.apply_friction(&context.entity_def);
            }
        }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn item_throw_action(&mut self, context: &mut StepContext) {
        if self.frame == 4 {
            if let Some(item) = self.get_held_item(&context.entities) {
                context.messages.push(Message {
                    recipient: item,
                    contents:  MessageContents::Item(MessageItem::Dropped)
                });
            }
        }
    }

    fn attack_land_action(&mut self, context: &mut StepContext) {
        let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as i64 - 1;
        self.frame = last_action_frame.min(self.frame + self.land_frame_skip as i64);
        self.land_particles(context);

        if self.interruptible(&context.entity_def) {
            if self.check_jump(context) { }
            else if self.check_shield(context) { }
            else if self.check_special(context) { }
            else if self.check_smash(context) { }
            else if self.check_attacks(context) { }
            else if self.check_grab(context) { }
            else if self.check_taunt(context) { }
            else if self.check_dash(context) { }
            else if self.check_smash_turn(context) { }
            else if self.check_tilt_turn(context) { }
            else if self.check_walk(context) { }
            else if self.first_interruptible(&context.entity_def) && context.input[0].stick_y < -0.5 {
                self.set_action(context, Action::Crouch);
            }
            else {
                self.apply_friction(&context.entity_def);
            }
        }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn land_action(&mut self, context: &mut StepContext) {
        self.land_particles(context);

        if self.interruptible(&context.entity_def) {
            if self.check_jump(context) { }
            else if self.check_shield(context) { }
            else if self.check_special(context) { }
            else if self.check_smash(context) { }
            else if self.check_attacks(context) { }
            else if self.check_grab(context) { }
            else if self.check_taunt(context) { }
            else if self.check_dash(context) { }
            else if self.check_smash_turn(context) { }
            else if self.check_tilt_turn(context) { }
            else if self.check_walk(context) { }
            else if self.first_interruptible(&context.entity_def) && context.input[0].stick_y < -0.5 {
                self.set_action(context, Action::Crouch);
            }
            else {
                self.apply_friction(&context.entity_def);
            }
        }
        else {
            self.apply_friction(&context.entity_def);
        }
    }

    fn teeter_action(&mut self, context: &mut StepContext) {
        if self.interruptible(&context.entity_def) {
            if self.check_jump(context) { }
            else if self.check_shield(context) { }
            else if self.check_special(context) { }
            else if self.check_smash(context) { }
            else if self.check_attacks(context) { }
            else if self.check_grab(context) { }
            else if self.check_taunt(context) { }
            else if self.check_crouch(context) { }
            else if self.check_dash(context) { }
            else if self.check_smash_turn(context) { }
            else if self.check_tilt_turn(context) { }
            else if self.check_walk_teeter(context) { }
        }
    }

    fn walk_action(&mut self, context: &mut StepContext) {
        if context.input[0].stick_x == 0.0 {
            self.set_action(context, Action::Idle);
        }
        else if self.check_jump(context) { }
        else if self.check_shield(context) { }
        else if self.check_special(context) { }
        else if self.check_smash(context) { }
        else if self.check_attacks(context) { }
        else if self.check_grab(context) { }
        else if self.check_crouch(context) { }
        else if self.check_dash(context) { }
        else if self.check_smash_turn(context) { }
        else if self.check_tilt_turn(context) { }
        else if self.check_taunt(context) { }
        else {
            let vel_max = context.entity_def.walk_max_vel * context.input[0].stick_x;

            if self.body.x_vel.abs() > vel_max.abs() {
                self.apply_friction(&context.entity_def);
            }
            else {
                let acc = (vel_max - self.body.x_vel) * (2.0/context.entity_def.walk_max_vel) * (context.entity_def.walk_init_vel + context.entity_def.walk_acc);
                self.body.x_vel += acc;
                if self.relative_f(self.body.x_vel) > self.relative_f(vel_max) {
                    self.body.x_vel = vel_max;
                }
            }
        }
    }

    fn dash_action(&mut self, context: &mut StepContext) {
        self.dash_particles(context);
        if self.frame == 1 {
            self.body.x_vel = self.relative_f(context.entity_def.dash_init_vel);
            if self.body.x_vel.abs() > context.entity_def.dash_run_term_vel {
                self.body.x_vel = self.relative_f(context.entity_def.dash_run_term_vel);
            }
        }

        if self.frame > 0 {
            if context.input[0].stick_x.abs() < 0.3 {
                self.apply_friction(&context.entity_def);
            }
            else {
                let vel_max = context.input[0].stick_x * context.entity_def.dash_run_term_vel;
                let acc     = context.input[0].stick_x * context.entity_def.dash_run_acc_a;

                self.body.x_vel += acc;
                if (vel_max > 0.0 && self.body.x_vel > vel_max) || (vel_max < 0.0 && self.body.x_vel < vel_max) {
                    self.apply_friction(&context.entity_def);
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
        let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as i64 - 1;
        if (self.frame >= run_frame || (run_frame > last_action_frame && self.last_frame(&context.entity_def)))
            && self.relative_f(context.input.stick_x.value) >= 0.62
        {
            self.set_action(context, Action::Run);
        }

        if self.check_shield(context) {
            self.body.x_vel *= 0.25;
        }
        else if context.input.z.press {
            self.set_action(context, Action::DashGrab);
        }
        else if context.input.a.press {
            self.set_action(context, Action::DashAttack);
        }
        else if self.check_jump(context) { }
        else if self.check_smash_turn(context) {
            self.body.x_vel *= 0.25
        }
    }

    fn run_action(&mut self, context: &mut StepContext) {
        if self.check_jump(context) { }
        else if self.check_shield(context) { }
        else if self.relative_f(context.input.stick_x.value) <= -0.3 {
            self.set_action(context, Action::RunTurn);
        }
        else if self.relative_f(context.input.stick_x.value) < 0.62 {
            self.set_action(context, Action::RunEnd);
        }
        else if context.input.a.press {
            self.set_action(context, Action::DashAttack);
        }
        else if context.input.z.press {
            self.set_action(context, Action::DashGrab);
        }
        else {
            let vel_max = context.input[0].stick_x * context.entity_def.dash_run_term_vel;
            let acc = (vel_max - self.body.x_vel)
                    * (context.entity_def.dash_run_acc_a + (context.entity_def.dash_run_acc_b / context.input[0].stick_x.abs()))
                    / (context.entity_def.dash_run_term_vel * 2.5);

            self.body.x_vel += acc;
            if self.relative_f(self.body.x_vel) > self.relative_f(vel_max) {
                self.body.x_vel = vel_max;
            }
        }
    }

    fn run_end_action(&mut self, context: &mut StepContext) {
        if self.check_jump(context) { }
        else if self.frame > 1 && self.check_crouch(context) { }
        else if self.relative_f(context.input.stick_x.value) <= -0.3 {
            self.set_action(context, Action::RunTurn);
        }
        else {
            self.apply_friction(context.entity_def);
        }
    }

    fn aerialdodge(&mut self, context: &mut StepContext) {
        self.set_action(context, Action::AerialDodge);
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
    }

    fn aerialdodge_action(&mut self, context: &mut StepContext) {
        if self.frame < context.entity_def.aerialdodge_drift_frame as i64 {
            self.body.x_vel *= 0.9;
            self.body.y_vel *= 0.9;
        }
        else {
            self.air_drift(context);
            self.fastfall_action(context);
        }
    }

    fn shield_on_action(&mut self, context: &mut StepContext) {
        let stick_lock = context.entity_def.shield.as_ref().map_or(false, |x| x.stick_lock) && context.input[0].b;
        let stun_lock = self.shield_stun_timer > 0;
        let lock = stun_lock && stick_lock;
        let power_shield_len = context.entity_def.actions[Action::PowerShield as usize].frames.len();

        if !lock && self.check_grab_shield(context) { }
        else if !lock && self.check_jump(context) { }
        else if !lock && self.check_pass_platform(context) { }
        else if context.entity_def.power_shield.is_some() && self.frame == 0 && (context.input.l.press || context.input.r.press) {
            // allow the first frame to transition to power shield so that powershield input is more consistent
            self.action = Action::PowerShield as u64;
            self.frame = if power_shield_len >= 2 { 1 } else { 0 }; // change self.frame so that a powershield isnt laggier than a normal shield

            self.apply_friction(context.entity_def);
            self.shield_shared_action(context);
        }
        else {
            self.apply_friction(context.entity_def);
            self.shield_shared_action(context);
        }
    }

    fn shield_action(&mut self, context: &mut StepContext) {
        let stick_lock = context.entity_def.shield.as_ref().map_or(false, |x| x.stick_lock) && context.input[0].b;
        let stun_lock = self.shield_stun_timer > 0;
        let lock = stun_lock && stick_lock;

        if !lock && self.check_grab_shield(context) { }
        else if !lock && self.check_jump(context) { }
        else if !lock && self.check_pass_platform(context) { }
        else if !stun_lock && context.input[0].l_trigger < 0.165 && context.input[0].r_trigger < 0.165 && !context.input[0].l && !context.input[0].r {
            if self.parry_timer > 0 {
                self.set_action(context, Action::Idle);
            } else {
                self.set_action(context, Action::ShieldOff);
            }

            self.apply_friction(context.entity_def);
            self.shield_shared_action(context);
        }
        else {
            self.apply_friction(context.entity_def);
            self.shield_shared_action(context);
        }
    }

    fn shield_off_action(&mut self, context: &mut StepContext) {
        let stick_lock = context.entity_def.shield.as_ref().map_or(false, |x| x.stick_lock) && context.input[0].b;
        let stun_lock = self.shield_stun_timer > 0;
        let lock = stun_lock && stick_lock;

        if !lock && self.check_grab_shield(context) { }
        else if !lock && self.check_jump(context) { }
        else if !lock && self.check_pass_platform(context) { }
        else {
            self.apply_friction(context.entity_def);
            self.shield_shared_action(context);
        }
    }

    fn power_shield_action(&mut self, context: &mut StepContext) {
        let stick_lock = context.entity_def.shield.as_ref().map_or(false, |x| x.stick_lock) && context.input[0].b;
        let stun_lock = self.shield_stun_timer > 0;
        let lock = stun_lock && stick_lock;

        match (&context.entity_def.shield, &context.entity_def.power_shield) {
            (&Some(_), &Some(_)) => {
                if !lock && self.check_grab_shield(context) { }
                else if !lock && self.check_jump(context) { }
                else if !lock && self.check_pass_platform(context) { }
                self.shield_shared_action(context);
            }
            _ => {
                self.apply_friction(context.entity_def);
            }
        }
    }

    fn shield_shared_action(&mut self, context: &mut StepContext) {
        self.apply_friction(context.entity_def);
        if let Some(ref shield) = context.entity_def.shield {
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
            if self.shield_hp <= 0.0 {
                self.set_action(context, Action::ShieldBreakFall);
                self.shield_hp = 0.0;
                self.body.kb_y_vel = shield.break_vel;
                self.body.kb_y_dec = 0.051;
                self.body.kb_x_dec = 0.0;
                self.set_airbourne(context);
            }
        }
    }

    fn shield_break_fall_action(&mut self, entity_def: &EntityDef) {
        self.fall_action(entity_def);
    }

    fn shield_break_getup_action(&mut self) {
        self.body.x_vel = 0.0;
    }

    fn stun_action(&mut self, context: &mut StepContext) {
        self.apply_friction(context.entity_def);
        if self.shield_hp > 30.0 {
            self.shield_hp = 30.0;
        }

        self.stun_timer -= 1;

        // TODO: Mashout

        if self.stun_timer <= 0 {
            self.set_action(context, Action::Idle);
        }
    }

    fn grabbing_idle_action(&mut self, context: &mut StepContext) {
        self.apply_friction(context.entity_def);

        if self.frame_norestart > 60 { // TODO: additionally check if grabbed player is still in a grabbed state
            self.set_action(context, Action::GrabbingEnd);
        }
    }

    // TODO: this state should probably be split into standing and airbourne versions
    //       for now lets try to squash both cases into this one action
    fn grabbed_idle_action(&mut self, context: &mut StepContext) {
        if self.frame_norestart > 60 { // TODO: instead check if grabbing player is still in a grabbing state
            let bps_xy = self.bps_xy(context);
            if let Some(frame) = self.get_entity_frame(context.entity_def) {
                // ignore the x offset, we only want to check straight down.
                let bps_xy_grab_point = (bps_xy.0, bps_xy.1 + frame.grabbed_y);
                if let Some(platform_i) = self.body.land_stage_collision(context, frame, bps_xy_grab_point, bps_xy) {
                    let x = context.stage.surfaces[platform_i].world_x_to_plat_x(bps_xy.0);
                    self.body.location = Location::Surface { platform_i, x };
                    self.land(context);
                    self.set_action(context, Action::GrabbedEnd);
                }
                else {
                    self.set_airbourne(context);
                    self.set_action(context, Action::Fall);
                }
            }
        }
    }

    pub fn shield_size(&self, shield: &Shield) -> f32 {
        let analog_size = (1.0 - self.shield_analog) * 0.6;
        let hp_size = (self.shield_hp / shield.hp_max) * shield.hp_scaling;
        let hp_size_unscaled = ((shield.hp_max - self.shield_hp) / shield.hp_max) * 2.0;

        shield.scaling * (analog_size + hp_size) + hp_size_unscaled
    }

    fn shield_pos(&self, shield: &Shield, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        let xy = self.public_bps_xy(entities, entity_defs, surfaces);
        (
            xy.0 + self.shield_offset_x + self.relative_f(shield.offset_x),
            xy.1 + self.shield_offset_y + shield.offset_y
        )
    }

    fn check_crouch(&mut self, context: &mut StepContext) -> bool {
        if context.input[0].stick_y < -0.77 {
            if let Some(action) = Action::from_u64(self.action) {
                match action {
                    Action::CrouchStart | Action::Crouch | Action::CrouchEnd => {
                    }
                    _ => {
                        self.set_action(context, Action::CrouchStart);
                    }
                }
            }
            true
        }
        else {
            false
        }
    }

    fn check_pass_platform(&mut self, context: &mut StepContext) -> bool {
        if let Location::Surface { platform_i, .. } = self.body.location {
            if let Some(platform) = context.surfaces.get(platform_i) {
                let last_action_frame = context.entity_def.actions[self.action as usize].frames.len() as i64 - 1;
                let pass_frame = last_action_frame.min(4);
                if platform.is_pass_through() && self.frame == pass_frame && (context.input[0].stick_y < -0.77 || context.input[2].stick_y < -0.77) && context.input[6].stick_y > -0.36 {
                    self.set_action(context, Action::PassPlatform);
                    self.set_airbourne(context);
                    return true;
                }
            }
        }
        false
    }

    fn check_walk(&mut self, context: &mut StepContext) -> bool {
        if context.input[0].stick_x.abs() > 0.3 {
            self.walk(context);
            true
        }
        else {
            false
        }
    }

    fn check_walk_teeter(&mut self, context: &mut StepContext) -> bool {
        if context.input[0].stick_x.abs() > 0.6 {
            self.walk(context);
            true
        }
        else {
            false
        }
    }

    fn check_dash(&mut self, context: &mut StepContext) -> bool {
        if self.relative_f(context.input[0].stick_x) > 0.79 && self.relative_f(context.input[2].stick_x) < 0.3 {
            self.dash(context);
            true
        }
        else {
            false
        }
    }

    fn check_tilt_turn(&mut self, context: &mut StepContext) -> bool {
        let turn = self.relative_f(context.input[0].stick_x) < -0.3;
        if turn {
            self.set_action(context, Action::TiltTurn);
        }
        turn
    }

    fn check_smash_turn(&mut self, context: &mut StepContext) -> bool {
        let turn = self.relative_f(context.input[0].stick_x) < -0.79 && self.relative_f(context.input[2].stick_x) > -0.3;
        if turn {
            self.set_action(context, Action::SmashTurn);
            self.body.face_right = !self.body.face_right;
        }
        turn
    }

    fn check_jump(&mut self, context: &mut StepContext) -> bool {
        match self.jump_input(&context.input) {
            JumpResult::Button => {
                self.jumpsquat_button = true;
                self.set_action(context, Action::JumpSquat);
                true
            }
            JumpResult::Stick => {
                self.jumpsquat_button = false;
                self.set_action(context, Action::JumpSquat);
                true
            }
            JumpResult::None => {
                false
            }
        }
    }

    fn check_jump_aerial(&mut self, context: &mut StepContext) -> bool {
        if self.jump_input(&context.input).jump() && self.air_jumps_left > 0 {
            self.air_jump_particles(context);
            self.air_jumps_left -= 1;
            self.body.y_vel = context.entity_def.air_jump_y_vel;
            self.body.x_vel = context.entity_def.air_jump_x_vel * context.input[0].stick_x;
            self.fastfalled = false;

            if self.relative_f(context.input.stick_x.value) < -0.3 {
                self.set_action(context, Action::JumpAerialB);
            }
            else {
                self.set_action(context, Action::JumpAerialF);
            }
            true
        } else {
            false
        }
    }

    fn check_attacks_aerial(&mut self, context: &mut StepContext) -> bool {
        if context.input.a.press || context.input.z.press {
            if self.relative_f(context.input[0].stick_x) > 0.3 && context.input[0].stick_x.abs() > context.input[0].stick_y.abs() - 0.1 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowAirU);
                }
                else {
                    self.set_action(context, Action::Fair);
                }
            }
            else if self.relative_f(context.input[0].stick_x) < -0.3 && context.input[0].stick_x.abs() > context.input[0].stick_y.abs() - 0.1 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowAirU);
                }
                else {
                    self.set_action(context, Action::Bair);
                }
            }
            else if context.input[0].stick_y < -0.3 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowAirU);
                }
                else {
                    self.set_action(context, Action::Dair);
                }
            }
            else if context.input[0].stick_y > 0.3 {
                if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowAirU);
                }
                else {
                    self.set_action(context, Action::Uair);
                }
            }
            else if context.input.z.press && self.get_held_item(&context.entities).is_some() {
                if let Some(item) = self.get_held_item(&context.entities) {
                    context.messages.push(Message {
                        recipient: item,
                        contents:  MessageContents::Item(MessageItem::Dropped)
                    });
                }
            }
            else {
                self.set_action(context, Action::Nair);
            }
            true
        }
        else if self.relative_f(context.input[0].c_stick_x) >= 0.3 && self.relative_f(context.input[1].c_stick_x) < 0.3 
            && context.input[0].c_stick_x.abs() > context.input[0].c_stick_y.abs() - 0.1
        {
            if self.get_held_item(&context.entities).is_some() {
                self.set_action(context, Action::ItemThrowAirF);
            }
            else {
                self.set_action(context, Action::Fair);
            }
            true
        }
        else if self.relative_f(context.input[0].c_stick_x) <= -0.3 && self.relative_f(context.input[1].c_stick_x) > -0.3
            && context.input[0].c_stick_x.abs() > context.input[0].c_stick_y.abs() - 0.1
        {
            if self.get_held_item(&context.entities).is_some() {
                self.set_action(context, Action::ItemThrowAirB);
            }
            else {
                self.set_action(context, Action::Bair);
            }
            true
        }
        else if context.input[0].c_stick_y < -0.3 && context.input[1].c_stick_y > -0.3 {
            if self.get_held_item(&context.entities).is_some() {
                self.set_action(context, Action::ItemThrowAirD);
            }
            else {
                self.set_action(context, Action::Dair);
            }
            true
        }
        else if context.input[0].c_stick_y >= 0.3 && context.input[1].c_stick_y < 0.3 {
            if self.get_held_item(&context.entities).is_some() {
                self.set_action(context, Action::ItemThrowAirU);
            }
            else {
                self.set_action(context, Action::Uair);
            }
            true
        }
        else {
            false
        }
    }

    fn check_attacks(&mut self, context: &mut StepContext) -> bool {
        if context.input.a.press {
            if self.relative_f(context.input[0].stick_x) > 0.3 && context.input[0].stick_x.abs() - context.input[0].stick_y.abs() > -0.05 {
                if self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowF);
                }
                else {
                    self.set_action(context, Action::Ftilt);
                }
            }
            else if context.input[0].stick_y < -0.3 {
                if self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowD);
                }
                else {
                    self.set_action(context, Action::Dtilt);
                }
            }
            else if context.input[0].stick_y > 0.3 {
                if self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowU);
                }
                else {
                    self.set_action(context, Action::Utilt);
                }
            }
            else {
                if self.get_held_item(&context.entities).is_some() {
                    self.set_action(context, Action::ItemThrowF);
                }
                else {
                    self.set_action(context, Action::Jab);
                }
            }
            true
        }
        else {
            false
        }
    }

    fn check_grab_shield(&mut self, context: &mut StepContext) -> bool {
        if context.input.a.press || context.input.z.press {
            self.set_action(context, Action::Grab);
            true
        } else {
            false
        }
    }

    fn check_grab(&mut self, context: &mut StepContext) -> bool {
        if context.input.z.press {
            self.set_action(context, Action::Grab);
            true
        } else {
            false
        }
    }

    fn check_special(&mut self, context: &StepContext) -> bool {
        if context.input.b.press {
            // special attack
            true
        }
        else {
            false
        }
    }

    fn check_smash(&mut self, context: &mut StepContext) -> bool {
        if context.input.a.press {
            if (context.input[0].stick_x >=  0.79 && context.input[2].stick_x < 0.3) ||
               (context.input[0].stick_x <= -0.79 && context.input[2].stick_x > 0.3) {
                self.body.face_right = context.input.c_stick_x.value > 0.0;
                self.set_action(context, Action::Fsmash);
                return true;
            }
            else if context.input[0].stick_y >= 0.66 && context.input[2].stick_y < 0.3 {
                self.set_action(context, Action::Usmash);
                return true;
            }
            else if context.input[0].stick_y <= -0.66 && context.input[2].stick_y > 0.3 {
                self.set_action(context, Action::Dsmash);
                return true;
            }
        }
        else if context.input[0].c_stick_x.abs() >= 0.79 && context.input[1].c_stick_x.abs() < 0.79 {
            self.body.face_right = context.input.c_stick_x.value > 0.0;
            self.set_action(context, Action::Fsmash);
            return true;
        }
        else if context.input[0].c_stick_y >= 0.66 && context.input[1].c_stick_y < 0.66 {
            self.set_action(context, Action::Usmash);
            return true;
        }
        else if context.input[0].c_stick_y <= -0.66 && context.input[1].c_stick_y > -0.66 {
            self.set_action(context, Action::Dsmash);
            return true;
        }
        false
    }

    fn check_taunt(&mut self, context: &mut StepContext) -> bool {
        if context.input.up.press {
            self.set_action(context, Action::TauntUp);
            true
        }
        else if context.input.down.press {
            self.set_action(context, Action::TauntDown);
            true
        }
        else if context.input.left.press {
            self.set_action(context, Action::TauntLeft);
            true
        }
        else if context.input.right.press {
            self.set_action(context, Action::TauntRight);
            true
        }
        else {
            false
        }
    }

    fn check_shield(&mut self, context: &mut StepContext) -> bool {
        match (&context.entity_def.shield, &context.entity_def.power_shield) {
            (&Some(_), &Some(_)) => {
                if context.input.l.press || context.input.r.press {
                    self.set_action(context, Action::PowerShield);
                    true
                } else if context.input[0].l || context.input[0].r || context.input[0].l_trigger > 0.165 || context.input[0].r_trigger > 0.165 {
                    self.set_action(context, Action::ShieldOn);
                    true
                } else {
                    false
                }
            }
            (&None, &Some(_)) => {
                if context.input[0].l || context.input[0].r || context.input[0].l_trigger > 0.165 || context.input[0].r_trigger > 0.165 {
                    self.set_action(context, Action::PowerShield);
                    true
                } else {
                    false
                }
            }
            (&Some(_), &None) => {
                if context.input[0].l || context.input[0].r || context.input[0].l_trigger > 0.165 || context.input[0].r_trigger > 0.165 {
                    self.set_action(context, Action::ShieldOn);
                    true
                } else {
                    false
                }
            }
            (&None, &None) => false
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

    fn action_expired(&mut self, context: &mut StepContext) {
        match Action::from_u64(self.action) {
            None => panic!("Custom defined action expirations have not been implemented"),

            // Idle
            Some(Action::Spawn)          => self.set_action(context, Action::Idle),
            Some(Action::ReSpawn)        => self.set_action(context, Action::ReSpawnIdle),
            Some(Action::ReSpawnIdle)    => self.set_action(context, Action::ReSpawnIdle),
            Some(Action::Idle)           => self.set_action(context, Action::Idle),
            Some(Action::Teeter)         => self.set_action(context, Action::TeeterIdle),
            Some(Action::TeeterIdle)     => self.set_action(context, Action::TeeterIdle),
            Some(Action::MissedTechIdle) => self.set_action(context, Action::MissedTechIdle),

            // crouch
            Some(Action::CrouchStart) => self.set_action(context, Action::Crouch),
            Some(Action::Crouch)      => self.set_action(context, Action::Crouch),
            Some(Action::CrouchEnd)   => self.set_action(context, Action::Idle),

            // Movement
            Some(Action::Fall)           => self.set_action(context, Action::Fall),
            Some(Action::AerialFall)     => self.set_action(context, Action::AerialFall),
            Some(Action::Land)           => self.set_action(context, Action::Idle),
            Some(Action::JumpF)          => self.set_action(context, Action::Fall),
            Some(Action::JumpB)          => self.set_action(context, Action::Fall),
            Some(Action::JumpAerialF)    => self.set_action(context, Action::AerialFall),
            Some(Action::JumpAerialB)    => self.set_action(context, Action::AerialFall),
            Some(Action::SmashTurn)      => self.set_action(context, Action::Idle),
            Some(Action::RunTurn)        => {
                if self.relative_f(context.input[0].stick_x) > 0.6 {
                    self.set_action(context, Action::Run);
                }
                else {
                    self.set_action(context, Action::Idle);
                }
            }
            Some(Action::TiltTurn)       => self.set_action(context, Action::Idle),
            Some(Action::Dash)           => self.set_action(context, Action::Idle),
            Some(Action::Run)            => self.set_action(context, Action::Run),
            Some(Action::RunEnd)         => self.set_action(context, Action::Idle),
            Some(Action::Walk)           => self.set_action(context, Action::Walk),
            Some(Action::PassPlatform)   => self.set_action(context, Action::AerialFall),
            Some(Action::Damage)         => self.set_action(context, Action::Damage),
            Some(Action::DamageFly)      => self.set_action(context, Action::DamageFly),
            Some(Action::DamageFall)     => self.set_action(context, Action::DamageFall),
            Some(Action::LedgeGetup)     => self.set_action_idle_from_ledge(context),
            Some(Action::LedgeGetupSlow) => self.set_action_idle_from_ledge(context),
            Some(Action::LedgeJump)      => self.set_action_fall_from_ledge_jump(context),
            Some(Action::LedgeJumpSlow)  => self.set_action_fall_from_ledge_jump(context),
            Some(Action::LedgeIdle)      => self.set_action(context, Action::LedgeIdle),
            Some(Action::LedgeIdleChain) => self.set_action(context, Action::LedgeIdleChain),
            Some(Action::LedgeGrab) => {
                self.ledge_idle_timer = 0;
                self.set_action(context, Action::LedgeIdle);
            }
            Some(Action::JumpSquat) => {
                self.set_airbourne(context);
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
                    self.set_action(context, Action::JumpF);
                }
                else {
                    self.set_action(context, Action::JumpB);
                }
            }

            // Defense
            Some(Action::PowerShield)      => if context.entity_def.shield.is_some() { self.set_action(context,Action::Shield) } else { self.set_action(context,Action::Idle) },
            Some(Action::ShieldOn)         => self.set_action(context, Action::Shield),
            Some(Action::Shield)           => self.set_action(context, Action::Shield),
            Some(Action::ShieldOff)        => self.set_action(context, Action::Idle),
            Some(Action::RollF)            => self.set_action(context, Action::Idle),
            Some(Action::RollB)            => self.set_action(context, Action::Idle),
            Some(Action::SpotDodge)        => self.set_action(context, Action::Idle),
            Some(Action::AerialDodge)      => self.set_action(context, Action::SpecialFall),
            Some(Action::SpecialFall)      => self.set_action(context, Action::SpecialFall),
            Some(Action::SpecialLand)      => self.set_action(context, Action::Idle),
            Some(Action::TechF)            => self.set_action(context, Action::Idle),
            Some(Action::TechN)            => self.set_action(context, Action::Idle),
            Some(Action::TechB)            => self.set_action(context, Action::Idle),
            Some(Action::MissedTechGetupF) => self.set_action(context, Action::Idle),
            Some(Action::MissedTechGetupN) => self.set_action(context, Action::Idle),
            Some(Action::MissedTechGetupB) => self.set_action(context, Action::Idle),
            Some(Action::Rebound)          => self.set_action(context, Action::Idle),
            Some(Action::LedgeRoll)        => self.set_action_idle_from_ledge(context),
            Some(Action::LedgeRollSlow)    => self.set_action_idle_from_ledge(context),

            // Vulnerable
            Some(Action::MissedTechStart)  => self.set_action(context, Action::MissedTechIdle),
            Some(Action::ShieldBreakFall)  => self.set_action(context, Action::ShieldBreakFall),
            Some(Action::Stun)             => self.set_action(context, Action::Stun),
            Some(Action::ShieldBreakGetup) => {
                self.stun_timer = 490;
                self.set_action(context, Action::Stun);
            }

            // Attack
            Some(Action::Jab)              => self.set_action(context, Action::Idle),
            Some(Action::Jab2)             => self.set_action(context, Action::Idle),
            Some(Action::Jab3)             => self.set_action(context, Action::Idle),
            Some(Action::Utilt)            => self.set_action(context, Action::Idle),
            Some(Action::Dtilt)            => self.set_action(context, Action::Crouch),
            Some(Action::Ftilt)            => self.set_action(context, Action::Idle),
            Some(Action::DashAttack)       => self.set_action(context, Action::Idle),
            Some(Action::Usmash)           => self.set_action(context, Action::Idle),
            Some(Action::Dsmash)           => self.set_action(context, Action::Idle),
            Some(Action::Fsmash)           => self.set_action(context, Action::Idle),
            Some(Action::MissedTechAttack) => self.set_action(context, Action::Idle),
            Some(Action::LedgeAttack)      => self.set_action_idle_from_ledge(context),
            Some(Action::LedgeAttackSlow)  => self.set_action_idle_from_ledge(context),

            // Grab
            Some(Action::Grab)           => self.set_action(context, Action::Idle),
            Some(Action::DashGrab)       => self.set_action(context, Action::Idle),
            Some(Action::GrabbingIdle)   => self.set_action(context, Action::GrabbingIdle),
            Some(Action::GrabbingEnd)    => self.set_action(context, Action::Idle),
            Some(Action::GrabbedIdleAir) => self.set_action(context, Action::GrabbedIdleAir),
            Some(Action::GrabbedIdle)    => self.set_action(context, Action::GrabbedIdle),
            Some(Action::GrabbedEnd)     => self.set_action(context, Action::Idle),

            // Throws
            Some(Action::Uthrow) => self.set_action(context, Action::Idle),
            Some(Action::Dthrow) => self.set_action(context, Action::Idle),
            Some(Action::Fthrow) => self.set_action(context, Action::Idle),
            Some(Action::Bthrow) => self.set_action(context, Action::Idle),

            // Items
            Some(Action::ItemGrab)      => self.set_action(context, Action::Idle),
            Some(Action::ItemEat)       => self.set_action(context, Action::Idle),
            Some(Action::ItemThrowU)    => self.set_action(context, Action::Idle),
            Some(Action::ItemThrowD)    => self.set_action(context, Action::Idle),
            Some(Action::ItemThrowF)    => self.set_action(context, Action::Idle),
            Some(Action::ItemThrowB)    => self.set_action(context, Action::Idle),
            Some(Action::ItemThrowAirU) => self.set_action(context, Action::Fall),
            Some(Action::ItemThrowAirD) => self.set_action(context, Action::Fall),
            Some(Action::ItemThrowAirF) => self.set_action(context, Action::Fall),
            Some(Action::ItemThrowAirB) => self.set_action(context, Action::Fall),

            // Aerials
            Some(Action::Uair)     => self.set_action(context, Action::Fall),
            Some(Action::Dair)     => self.set_action(context, Action::Fall),
            Some(Action::Fair)     => self.set_action(context, Action::Fall),
            Some(Action::Bair)     => self.set_action(context, Action::Fall),
            Some(Action::Nair)     => self.set_action(context, Action::Fall),
            Some(Action::UairLand) => self.set_action(context, Action::Idle),
            Some(Action::DairLand) => self.set_action(context, Action::Idle),
            Some(Action::FairLand) => self.set_action(context, Action::Idle),
            Some(Action::BairLand) => self.set_action(context, Action::Idle),
            Some(Action::NairLand) => self.set_action(context, Action::Idle),

            // Taunts
            Some(Action::TauntUp)    => self.set_action(context, Action::Idle),
            Some(Action::TauntDown)  => self.set_action(context, Action::Idle),
            Some(Action::TauntLeft)  => self.set_action(context, Action::Idle),
            Some(Action::TauntRight) => self.set_action(context, Action::Idle),

            Some(Action::Eliminated)         => self.set_action(context, Action::Eliminated),
            Some(Action::DummyFramePreStart) => self.set_action(context, Action::Spawn),
        };
    }

    pub fn set_action_idle_from_ledge(&mut self, context: &mut StepContext) {
        if let Location::GrabbedLedge { platform_i, .. } = self.body.location {
            let platform = &context.surfaces[platform_i];
            let (world_x, _) = self.bps_xy(context);
            let x = platform.world_x_to_plat_x_clamp(world_x);

            self.body.location = Location::Surface { platform_i, x };
            self.set_action(context, Action::Idle);
        }
        else {
            panic!("Location must be on ledge to call this function.")
        }
    }

    pub fn set_action_fall_from_ledge_jump(&mut self, context: &mut StepContext) {
        self.set_airbourne(context);
        self.set_action(context, Action::Fall);
    }

    pub fn relative_f(&self, input: f32) -> f32 {
        self.body.relative_f(input)
    }

    /// Helper function to safely get the current entity frame
    /// Figuring out whether we need to use this helper is kind of messy:
    ///
    /// Anything hit by the rendering logic can have the indexes out of whack.
    /// This has to be the case so that replays can remain as accurate as possible.
    ///
    /// However the action_hitlag_step logic will correct any invalid indexes
    /// So anything hit by the action_hitlag_step logic doesnt need to use this helper
    /// however its not harmful either.
    pub fn get_entity_frame<'a>(&self, entity_def: &'a EntityDef) -> Option<&'a ActionFrame> {
        if entity_def.actions.len() > self.action as usize {
            let entity_frames = &entity_def.actions[self.action as usize].frames;
            if entity_frames.len() > self.frame as usize {
                return Some(&entity_frames[self.frame as usize]);
            }
        }
        None
    }

    fn specialfall_action(&mut self, context: &mut StepContext) {
        self.fall_action(context.entity_def);
        self.air_drift(context);
    }

    fn fall_action(&mut self, entity_def: &EntityDef) {
        self.body.y_vel += entity_def.gravity;
        if self.body.y_vel < entity_def.terminal_vel {
            self.body.y_vel = entity_def.terminal_vel;
        }
    }

    fn fastfall_action(&mut self, context: &mut StepContext) {
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
    }

    pub fn get_held_item(&self, entities: &Entities) -> Option<EntityKey> {
        for (key, entity) in entities.iter() {
            if let EntityType::Item (item) = &entity.ty {
                if let Location::GrabbedByPlayer (player_entity_key) = item.body.location {
                    if let Some(player) = entities.get(player_entity_key) {
                        if let EntityType::Player (player) = &player.ty {
                            if player.id == self.id {
                                return Some(key);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    pub fn item_grab(&mut self) {
        // TODO: make the context available here so we can call this:
        //match Action::from_u64(self.action) {
        //    Some(Action::Jab) => self.set_action(context, Action::ItemGrab),
        //    _ => {}
        //}
    }

    /*
     *  Begin physics section
     */

    pub fn physics_step(&mut self, context: &mut StepContext, game_frame: usize, goal: Goal) {
        let fighter_frame = &context.entity_def.actions[self.action as usize].frames[self.frame as usize];
        match self.body.physics_step(context, fighter_frame) {
            Some(PhysicsResult::Fall) => {
                self.fastfalled = false;
                self.set_action(context, Action::Fall);
            }
            Some(PhysicsResult::Land) => {
                self.hitstun = 0.0;
                self.land(context);
            }
            Some(PhysicsResult::Teeter) => {
                self.set_action(context, Action::Teeter);
            }
            Some(PhysicsResult::LedgeGrab) => {
                self.fastfalled = false;
                self.air_jumps_left = context.entity_def.fighter().map(|x| x.air_jumps).unwrap_or(1);
                self.hit_by = None;
                self.set_action(context, Action::LedgeGrab);
            }
            Some(PhysicsResult::OutOfBounds) => {
                self.die(context, game_frame, goal);
            }
            None => { }
        }
    }

    fn apply_friction(&mut self, fighter: &EntityDef) {
        match Action::from_u64(self.action) {
            Some(Action::Idle) |
            Some(Action::Dash) |
            Some(Action::Shield) |
            Some(Action::ShieldOn) |
            Some(Action::ShieldOff) |
            Some(Action::Damage)
              => { self.body.apply_friction_weak(fighter) }
            _ => { self.body.apply_friction_strong(fighter) }
        }
    }

    /// Returns the Rect surrounding the player that the camera must include
    pub fn cam_area(&self, cam_max: &Rect, entities: &Entities, fighters: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> Option<Rect> {
        match Action::from_u64(self.action) {
            Some(Action::Eliminated) => None,
            _ => {
                let (x, y) = self.public_bps_xy(entities, fighters, surfaces);
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

    fn land(&mut self, context: &mut StepContext) {
        let action = Action::from_u64(self.action);

        self.land_frame_skip = match action {
            Some(_) if action.as_ref().map_or(false, |x| x.is_air_attack()) && self.lcancel_timer > 0 => 1,
            Some(Action::AerialDodge) => 2,
            Some(Action::SpecialFall) => 2,
            _ => 0
        };

        self.aerial_dodge_frame = if let Some(Action::AerialDodge) = action { Some(self.frame as u64 ) } else { None };

        match action {
            Some(Action::Uair)            => self.set_action(context, Action::UairLand),
            Some(Action::Dair)            => self.set_action(context, Action::DairLand),
            Some(Action::Fair)            => self.set_action(context, Action::FairLand),
            Some(Action::Bair)            => self.set_action(context, Action::BairLand),
            Some(Action::Nair)            => self.set_action(context, Action::NairLand),
            Some(Action::ShieldBreakFall) => self.set_action(context, Action::ShieldBreakGetup),
            Some(Action::DamageFly) | Some(Action::DamageFall) => {
                if self.tech_timer.is_active() {
                    if self.relative_f(context.input[0].stick_x) > 0.5 {
                        self.set_action(context, Action::TechF);
                    }
                    else if self.relative_f(context.input[0].stick_x) < -0.5 {
                        self.set_action(context, Action::TechB);
                    }
                    else {
                        self.set_action(context, Action::TechN);
                    }
                }
                else {
                    self.set_action(context, Action::MissedTechStart);
                }
            }
            Some(Action::SpecialFall) |
            Some(Action::AerialDodge) |
            None => self.set_action(context, Action::SpecialLand),
            Some(_) if self.body.y_vel >= -1.0 => { self.set_action(context, Action::Idle) }, // no impact land
            Some(_) => self.set_action(context, Action::Land)
        }

        self.fastfalled = false;
        self.air_jumps_left = context.entity_def.fighter().map(|x| x.air_jumps).unwrap_or(1);
        self.hit_by = None;
    }

    fn walk(&mut self, context: &mut StepContext) {
        let walk_init_vel = self.relative_f(context.entity_def.walk_init_vel);
        if (walk_init_vel > 0.0 && self.body.x_vel < walk_init_vel) ||
           (walk_init_vel < 0.0 && self.body.x_vel > walk_init_vel) {
            self.body.x_vel += walk_init_vel;
        }
        self.set_action(context, Action::Walk);
    }

    fn dash(&mut self, context: &mut StepContext) {
        self.body.x_vel = self.relative_f(context.entity_def.dash_init_vel);
        self.set_action(context, Action::Dash);
    }

    fn die(&mut self, context: &mut StepContext, game_frame: usize, goal: Goal) {
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
                        self.set_action(context, Action::Eliminated);
                    }
                    else {
                        self.set_action(context, Action::ReSpawn);
                    }
                }
            }
            Goal::KillDeathScore => {
                self.set_action(context, Action::ReSpawn);
            }
        }
    }

    pub fn debug_print(&self, fighters: &KeyedContextVec<EntityDef>, player_input: &PlayerInput, debug: &DebugEntity, index: EntityKey) -> Vec<String> {
        let fighter = &fighters[self.entity_def_key.as_ref()];
        let mut lines: Vec<String> = vec!();
        if debug.physics {
            lines.push(self.body.debug_string(index));
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

        if debug.action {
            let action = Action::from_u64(self.action).unwrap();
            let last_action_frame = fighter.actions[self.action as usize].frames.len() as u64 - 1;
            let iasa = fighter.actions[self.action as usize].iasa;

            lines.push(format!("Entity: {:?}  Fighter  action: {:?}  frame: {}/{}  frame no restart: {}  IASA: {}",
                index, action, self.frame, last_action_frame, self.frame_norestart, iasa));
        }

        if debug.frame {
            lines.push(format!("Entity: {:?}  shield HP: {:.5}  hitstun: {:.5}  tech timer: {:?}  lcancel timer: {}",
                index, self.shield_hp, self.hitstun, self.tech_timer, self.lcancel_timer));
        }
        lines
    }

    pub fn result(&self) -> RawPlayerResult {
        let mut result = self.result.clone();
        result.final_damage = Some(self.body.damage);
        result.ended_as_fighter = Some(self.entity_def_key.clone());
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

    pub fn air_jump_particles(&mut self, context: &mut StepContext) {
        let (x, y) = self.bps_xy(context);
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

    pub fn knockback_particles(&mut self, context: &mut StepContext) {
        let kb_vel = (self.body.kb_x_vel * self.body.kb_x_vel + self.body.kb_y_vel * self.body.kb_y_vel).sqrt();
        let angle = self.body.kb_y_vel.atan2(self.body.kb_x_vel) + context.rng.gen_range(-0.2, 0.2);
        let vec_mult = context.rng.gen_range(0.7, 1.0);
        let (x, y) = self.bps_xy(context);
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
                y:           y + self.ecb.top / 2.0,
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

    pub fn land_particles(&mut self, context: &mut StepContext) {
        let num = match self.frame_norestart { // use frame_norestart instead as it doesnt get skipped during lcancel
            1 => 3,
            2 => 1,
            3 => 4,
            4 => 2,
            5 => 3,
            6 => 2,
            _ => 0,
        };

        let (x, y) = self.bps_xy(context);
        let action = Action::from_u64(self.action);

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

    pub fn dash_particles(&mut self, context: &mut StepContext) {
        let num = match self.frame {
            0 => 3,
            1 => 1,
            2 => 1,
            3 => 2,
            4 => 4,
            5 => 3,
            6 => 2,
            _ => 0,
        };

        let (x, y) = self.bps_xy(context);
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

    pub fn render(&self, entities: &Entities, fighters: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> RenderPlayer {
        let shield = if self.is_shielding() {
            let fighter_color = graphics::get_team_color3(self.team);
            let fighter = &fighters[self.entity_def_key.as_ref()];

            if let &Some(ref shield) = &fighter.shield {
                let c = &fighter_color;
                let m =  1.0 - self.shield_analog;
                Some(RenderShield {
                    distort: self.shield_stun_timer,
                    color:   [c[0] + (1.0 - c[0]) * m, c[1] + (1.0 - c[1]) * m, c[2] + (1.0 - c[2]) * m, 0.2 + self.shield_analog / 2.0],
                    radius:  self.shield_size(shield),
                    pos:     self.shield_pos(shield, entities, fighters, surfaces),
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
            if let Some(angle) = self.hit_angle_pre_di {
                vector_arrows.push(VectorArrow {
                    x: angle.cos(),
                    y: angle.sin(),
                    color: [1.0, 0.0, 0.0, 1.0]
                });
            }
            if let Some(angle) = self.hit_angle_post_di {
                vector_arrows.push(VectorArrow {
                    x: angle.cos(),
                    y: angle.sin(),
                    color: [0.0, 1.0, 0.0, 1.0]
                });
            }
        }

        vector_arrows
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
