use crate::entity::{EntityKey, Entities, StepContext};
use crate::entity::components::action_state::{ActionState, Hitlag};

use canon_collision_lib::entity_def::{EntityDef, ActionFrame, HitBox, HurtBox, ECB};
use canon_collision_lib::geometry;
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::stage::Surface;

use treeflection::KeyedContextVec;

use std::f32::consts::PI;

// Describes the player location by offsets from other locations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Location {
    Surface { platform_i: usize, x: f32 },
    GrabbedLedge { platform_i: usize, d_x: f32, d_y: f32, logic: LedgeLogic }, // player.face_right determines which edge on the platform
    GrabbedByPlayer (EntityKey),
    ItemHeldByPlayer (EntityKey),
    Airbourne { x: f32, y: f32 },
}

pub enum PhysicsResult {
    Fall,
    Land,
    Teeter,
    LedgeGrab,
    OutOfBounds,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Body {
    pub ecb:                ECB,
    pub damage:             f32,
    pub x_vel:              f32,
    pub y_vel:              f32,
    pub kb_x_vel:           f32,
    pub kb_y_vel:           f32,
    pub kb_x_dec:           f32,
    pub kb_y_dec:           f32,
    pub location:           Location,
    pub face_right:         bool,
    pub frames_since_ledge: u64,

    // Only use for debug display
    pub frames_since_hit:  u64,
    pub hit_angle_pre_di:  Option<f32>,
    pub hit_angle_post_di: Option<f32>,
}

impl Body {
    pub fn new(location: Location, face_right: bool) -> Body {
        Body {
            ecb:                ECB::default(),
            damage:             0.0,
            x_vel:              0.0,
            y_vel:              0.0,
            kb_x_vel:           0.0,
            kb_y_vel:           0.0,
            kb_x_dec:           0.0,
            kb_y_dec:           0.0,
            frames_since_ledge: 0,
            location,
            face_right,

            // Only use for debug display
            frames_since_hit:  0,
            hit_angle_pre_di:  None,
            hit_angle_post_di: None,
        }
    }

    pub fn is_platform(&self) -> bool {
        if let &Location::Surface { .. } = &self.location {
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn is_ledge(&self) -> bool {
        if let &Location::GrabbedLedge { .. } = &self.location {
            true
        } else {
            false
        }
    }

    pub fn is_grabbed(&self) -> bool {
        if let &Location::GrabbedByPlayer (_) = &self.location {
            true
        } else {
            false
        }
    }

    pub fn is_item_held(&self) -> bool {
        if let &Location::ItemHeldByPlayer (_) = &self.location {
            true
        } else {
            false
        }
    }

    pub fn is_airbourne(&self) -> bool {
        if let &Location::Airbourne { .. } = &self.location {
            true
        } else {
            false
        }
    }

    pub fn is_hogging_ledge(&self, check_platform_i: usize, face_right: bool) -> bool {
        if let &Location::GrabbedLedge { platform_i, ref logic, .. } = &self.location {
            if let &LedgeLogic::Hog = logic {
                return self.face_right == face_right && check_platform_i == platform_i;
            }
        }
        false
    }

    fn bps_xy(&self, context: &mut StepContext, action_frame: Option<&ActionFrame>, state: &ActionState) -> (f32, f32) {
        self.public_bps_xy(context.entities, context.entity_defs, action_frame, context.surfaces, state)
    }

    pub fn public_bps_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, action_frame: Option<&ActionFrame>, surfaces: &[Surface], state: &ActionState) -> (f32, f32) {
        let bps_xy = match self.location {
            Location::Surface { platform_i, x } => {
                if let Some(platform) = surfaces.get(platform_i) {
                    platform.plat_x_to_world_p(x)
                } else {
                    (0.0, 0.0)
                }
            }
            Location::GrabbedLedge { platform_i, d_x, d_y, .. } => {
                if let Some(platform) = surfaces.get(platform_i) {
                    let (ledge_x, ledge_y) = if self.face_right {
                        platform.left_ledge()
                    } else {
                        platform.right_ledge()
                    };
                    (ledge_x + self.relative_f(d_x), ledge_y + d_y)
                } else {
                    (0.0, 0.0)
                }
            }
            Location::GrabbedByPlayer (entity_i) => {
                if let Some(player) = entities.get(entity_i) {
                    if let Some(entity_frame) = action_frame {
                        let (grabbing_x, grabbing_y) = player.grabbing_xy(entities, entity_defs, surfaces);
                        let grabbed_x = self.relative_f(entity_frame.grabbed_x);
                        let grabbed_y = entity_frame.grabbed_y;
                        (grabbing_x - grabbed_x, grabbing_y - grabbed_y)
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                }
            }
            Location::ItemHeldByPlayer (entity_i) => {
                if let Some(player) = entities.get(entity_i) {
                    if let Some(action_frame) = player.get_entity_frame(&entity_defs[player.state.entity_def_key.as_ref()]) {
                        let (x, y) = player.public_bps_xy(entities, entity_defs, surfaces);
                        if let Some(item_hold) = action_frame.item_hold.as_ref() {
                            (
                                x + player.relative_f(item_hold.translation_x),
                                y + item_hold.translation_y,
                            )
                        } else {
                            (x, y)
                        }
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                }
            }
            Location::Airbourne { x, y } => {
                (x, y)
            }
        };

        match &state.hitlag {
            &Hitlag::Launch { wobble_x, .. } => {
                (bps_xy.0 + wobble_x, bps_xy.1)
            }
            _ => {
                bps_xy
            }
        }
    }

    /// only used for rendering
    pub fn public_bps_xyz(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, action_frame: Option<&ActionFrame>, surfaces: &[Surface], state: &ActionState) -> (f32, f32, f32) {
        let z = match self.location {
            Location::ItemHeldByPlayer (entity_i) => {
                if let Some(player) = entities.get(entity_i) {
                    player.get_entity_frame(&entity_defs[player.state.entity_def_key.as_ref()])
                        .and_then(|action_frame| action_frame.item_hold.as_ref())
                        .map(|item_hold| player.relative_f(item_hold.translation_z))
                        .unwrap_or(0.0)
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };
        let (x, y) = self.public_bps_xy(entities, entity_defs, action_frame, surfaces, state);
        (x, y, z)
    }

    pub fn angle(&self, action_frame: &ActionFrame, surfaces: &[Surface]) -> f32 {
         match self.location {
            Location::Surface { platform_i, .. } if action_frame.use_platform_angle => {
                surfaces.get(platform_i).map_or(0.0, |x| x.floor_angle().unwrap_or_default())
            }
            _ => 0.0,
        }
    }

    pub fn physics_step(&mut self, context: &mut StepContext, state: &ActionState, action_frame: &ActionFrame) -> Option<PhysicsResult> {
        if let Hitlag::None = state.hitlag {
            if self.kb_x_vel.abs() > 0.0 {
                let vel_dir = self.kb_x_vel.signum();
                if self.is_airbourne() {
                    self.kb_x_vel -= self.kb_x_dec;
                } else {
                    self.kb_x_vel -= vel_dir * context.entity_def.friction;
                }
                if vel_dir != self.kb_x_vel.signum() {
                    self.kb_x_vel = 0.0;
                }
            }

            if self.kb_y_vel.abs() > 0.0 {
                if self.is_airbourne() {
                    let vel_dir = self.kb_y_vel.signum();
                    self.kb_y_vel -= self.kb_y_dec;
                    if vel_dir != self.kb_y_vel.signum() {
                        self.kb_y_vel = 0.0;
                    }
                }
                else {
                    self.kb_y_vel = 0.0;
                }
            }

            let x_vel = self.x_vel + self.kb_x_vel;
            let y_vel = self.y_vel + self.kb_y_vel;

            if self.is_ledge() {
                self.frames_since_ledge = 0;
            }
            self.frames_since_ledge += 1;

            // update position
            let result = match self.location.clone() {
                Location::Airbourne { x, y } => {
                    let new_x = x + x_vel;
                    let new_y = y + y_vel;
                    if let Some(platform_i) = self.land_stage_collision(context, action_frame, (x, y), (new_x, new_y)) {
                        self.y_vel = 0.0;
                        self.kb_y_vel = 0.0;

                        let x = context.stage.surfaces[platform_i].world_x_to_plat_x(new_x);
                        self.location = Location::Surface { platform_i, x };
                        Some(PhysicsResult::Land)
                    } else {
                        self.location = Location::Airbourne { x: new_x, y: new_y };
                        None
                    }
                }
                Location::Surface { platform_i, mut x } => {
                    if let Some(platform) = context.stage.surfaces.get(platform_i) {
                        x += x_vel * platform.floor_angle().unwrap_or_default().cos();
                        self.floor_move(context, state, action_frame, platform, platform_i, x)
                    }
                    else {
                        self.location = Location::Airbourne { x: 0.0, y: 0.0 };
                        Some(PhysicsResult::Fall)
                    }
                }
                _ => None
            };
            match result {
                Some(_) => result,
                None => self.secondary_checks(context, state, action_frame)
            }
        } else {
            None
        }
    }

    fn secondary_checks(&mut self, context: &mut StepContext, state: &ActionState, action_frame: &ActionFrame) -> Option<PhysicsResult> {
        let blast = &context.stage.blast;
        let (x, y) = self.bps_xy(context, Some(action_frame), state);
        if x < blast.left() || x > blast.right() || y < blast.bot() || y > blast.top() {
            Some(PhysicsResult::OutOfBounds)
        } else {
            // ledge grabs
            if self.frames_since_ledge >= 30 && self.y_vel < 0.0 && context.input.stick_y.value > -0.5 {
                if let Some(ref ledge_grab_box) = action_frame.ledge_grab_box {
                    self.check_ledge_grab(context, ledge_grab_box)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    fn check_ledge_grab(&mut self, context: &mut StepContext, ledge_grab_box: &Rect) -> Option<PhysicsResult> {
        for (platform_i, platform) in context.surfaces.iter().enumerate() {
            let left_grab  = platform.left_grab()  && self.check_ledge_collision(ledge_grab_box, platform.left_ledge())  && context.entities.iter().all(|(_, x)| !x.is_hogging_ledge(platform_i, true));
            let right_grab = platform.right_grab() && self.check_ledge_collision(ledge_grab_box, platform.right_ledge()) && context.entities.iter().all(|(_, x)| !x.is_hogging_ledge(platform_i, false));

            // If both left and right ledges are in range then keep the same direction.
            // This prevents always facing left or right on small surfaces.
            if left_grab && !right_grab {
                self.face_right = true;
            }
            else if !left_grab && right_grab {
                self.face_right = false;
            }

            if left_grab || right_grab {
                self.x_vel = 0.0;
                self.y_vel = 0.0;
                let d_x = context.entity_def.ledge_grab_x;
                let d_y = context.entity_def.ledge_grab_y;
                self.location = Location::GrabbedLedge { platform_i, d_x, d_y, logic: LedgeLogic::Hog };
                return Some(PhysicsResult::LedgeGrab);
            }
        }

        None
    }

    fn check_ledge_collision(&self, ledge_grab_box: &Rect, ledge: (f32, f32)) -> bool {
        if let Location::Airbourne { x: p_x, y: p_y } = self.location {
            let b_x1 = self.relative_f(ledge_grab_box.x1).min(self.relative_f(ledge_grab_box.x2));
            let b_y1 =                 ledge_grab_box.y1.min(ledge_grab_box.y2);

            let b_x2 = self.relative_f(ledge_grab_box.x1).max(self.relative_f(ledge_grab_box.x2));
            let b_y2 =                 ledge_grab_box.y1.max(ledge_grab_box.y2);

            let (l_x, l_y) = ledge;

            l_x > p_x + b_x1 && l_x < p_x + b_x2 &&
            l_y > p_y + b_y1 && l_y < p_y + b_y2
        } else {
            false
        }
    }


    /// returns the index platform that the player will land on
    pub fn land_stage_collision(&mut self, context: &mut StepContext, action_frame: &ActionFrame, old_p: (f32, f32), new_p: (f32, f32)) -> Option<usize> {
        if new_p.1 > old_p.1 {
            return None
        }

        for (surface_i, surface) in context.stage.surfaces.iter().enumerate() {
            if !self.pass_through_platform(context, action_frame, surface) &&
                surface.floor.is_some() &&
                geometry::segments_intersect(old_p, new_p, surface.p1(), surface.p2())
            {
                return Some(surface_i)
            }
        }
        None
    }

    fn pass_through_platform(&self, context: &mut StepContext, action_frame: &ActionFrame, platform: &Surface) -> bool {
        platform.is_pass_through() && action_frame.pass_through && context.input[0].stick_y <= -0.56
    }

    fn floor_move(&mut self, context: &mut StepContext, state: &ActionState, action_frame: &ActionFrame, platform: &Surface, platform_i: usize, x: f32) -> Option<PhysicsResult> {
        let connected_floors = context.stage.connected_floors(platform_i);
        if platform.plat_x_in_bounds(x) {
            self.location = Location::Surface { platform_i, x };
            self.secondary_checks(context, state, action_frame)
        }
        else if x < 0.0 && connected_floors.left_i.is_some() {
            let new_platform_i = connected_floors.left_i.unwrap();
            let new_platform = &context.stage.surfaces[new_platform_i];
            let world_x = platform.plat_x_to_world_x(x);
            let x = new_platform.world_x_to_plat_x(world_x);
            self.floor_move(context, state, action_frame, new_platform, new_platform_i, x)
        }
        else if x > 0.0 && connected_floors.right_i.is_some() {
            let new_platform_i = connected_floors.right_i.unwrap();
            let new_platform = &context.stage.surfaces[new_platform_i];
            let world_x = platform.plat_x_to_world_x(x);
            let x = new_platform.world_x_to_plat_x(world_x);
            self.floor_move(context, state, action_frame, new_platform, new_platform_i, x)
        }
        else if !action_frame.ledge_cancel {
            self.location = Location::Surface { platform_i, x: platform.plat_x_clamp(x) };
            None
        }
        else if self.face_right && x < 0.0 || !self.face_right && x >= 0.0 || // facing away from the ledge
          self.relative_f(context.input.stick_x.value) > 0.6
        {
            // set max velocity
            if self.x_vel.abs() > context.entity_def.air_x_term_vel {
                self.x_vel = self.x_vel.signum() * context.entity_def.air_x_term_vel;
            }

            // force set past platform
            let x_offset = if x > 0.0 { 0.000001 } else { -0.000001 }; // just being cautious, probably dont need this
            let (air_x, air_y) = platform.plat_x_to_world_p(x + x_offset);
            self.location = Location::Airbourne { x: air_x, y: air_y };
            Some(PhysicsResult::Fall)
        }
        else {
            self.x_vel = 0.0;
            self.location = Location::Surface { platform_i, x: platform.plat_x_clamp(x) };
            Some(PhysicsResult::Teeter)
        }
    }

    pub fn apply_friction_weak(&mut self, fighter: &EntityDef) {
        if self.x_vel > 0.0 {
            self.x_vel -= fighter.friction;
            if self.x_vel < 0.0 {
                self.x_vel = 0.0;
            }
        }
        else {
            self.x_vel += fighter.friction;
            if self.x_vel > 0.0 {
                self.x_vel = 0.0;
            }
        }
    }

    pub fn apply_friction_strong(&mut self, fighter: &EntityDef) {
        if self.x_vel > 0.0 {
            self.x_vel -= fighter.friction * if self.x_vel > fighter.walk_max_vel { 2.0 } else { 1.0 };
            if self.x_vel < 0.0 {
                self.x_vel = 0.0;
            }
        }
        else {
            self.x_vel += fighter.friction * if self.x_vel < -fighter.walk_max_vel { 2.0 } else { 1.0 };
            if self.x_vel > 0.0 {
                self.x_vel = 0.0;
            }
        }
    }

    pub fn launch(&mut self, context: &mut StepContext, state: &ActionState, action_frame: Option<&ActionFrame>, hitbox: &HitBox, hurtbox: &HurtBox, entity_atk_i: EntityKey, kb_vel_mult: f32) -> f32 {
        let entity_atk = &context.entities[entity_atk_i];

        let damage_done = hitbox.damage * hurtbox.damage_mult; // TODO: apply staling
        self.damage += damage_done;

        let damage_launch = 0.05 * (hitbox.damage * (damage_done + self.damage.floor())) + (damage_done + self.damage) * 0.1;
        let weight = 2.0 - (2.0 * context.entity_def.weight) / (1.0 + context.entity_def.weight);
        let kbg = hitbox.kbg + hurtbox.kbg_add;
        let bkb = hitbox.bkb + hurtbox.bkb_add;

        let kb_vel = (bkb + kbg * (damage_launch * weight * 1.4 + 18.0)).min(2500.0) * kb_vel_mult;

        if !self.is_grabbed() || kb_vel > 50.0 {
            let (x, y) = self.bps_xy(context, action_frame, state);
            self.location = Location::Airbourne { x, y };
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
        let behind_entity_atk = self.bps_xy(context, action_frame, state).0 < entity_atk.bps_xy(context).0 && entity_atk.face_right() ||
                                self.bps_xy(context, action_frame, state).0 > entity_atk.bps_xy(context).0 && !entity_atk.face_right();
        let angle = if hitbox.enable_reverse_hit && behind_entity_atk { PI - angle_rad } else { angle_rad };

        // debug data
        self.hit_angle_pre_di = Some(angle);
        self.hit_angle_post_di = None;
        self.frames_since_hit = 0;

        let angle = if (kb_vel >= 80.0 || self.is_airbourne() || (angle != 0.0 && angle != PI)) // can di
            && !(context.input[0].stick_x == 0.0 && context.input[0].stick_y == 0.0) // not deadzone
        {
            Body::di(context.input, angle)
        } else {
            angle
        };

        // launch velocity
        let (sin, cos) = angle.sin_cos();
        self.x_vel = 0.0;
        self.y_vel = 0.0;
        self.kb_x_vel = cos * kb_vel * 0.03;
        self.kb_y_vel = sin * kb_vel * 0.03;
        self.kb_x_dec = cos * 0.051;
        self.kb_y_dec = sin * 0.051;
        self.hit_angle_post_di = Some(angle);

        if self.kb_y_vel == 0.0 {
            if kb_vel >= 80.0 {
                let (x, y) = self.bps_xy(context, action_frame, state);
                self.location = Location::Airbourne { x, y: y + 0.0001 };
            }
        }
        else if self.kb_y_vel > 0.0 {
            let (x, y) = self.bps_xy(context, action_frame, state);
            self.location = Location::Airbourne { x, y };
        }
        // TODO: determine from angle (current logic falls over when reverse hit is disabled)
        self.face_right = self.bps_xy(context, action_frame, state).0 < entity_atk.bps_xy(context).0;

        kb_vel
    }

    /// 0 < angle < 2pi
    fn di(input: &PlayerInput, angle: f32) -> f32 {
        let range = 18f32.to_radians();
        let x = input[0].stick_x;
        let y = input[0].stick_y;

        let di_angle = y.atan2(x);                                                 // -pi  <= di_angle     <= pi
        let pos_di_angle = di_angle + if di_angle < 0.0 { PI * 2.0 } else { 0.0 }; // 0    <= pos_di_angle <= 2pi
        let angle_diff = angle - pos_di_angle;                                     // -2pi <= angle_diff   <= 2pi

        let offset_distance = (angle_diff).sin() * (x * x + y * y).sqrt();                 // -1     <= offset_distance <= 1
        let offset = offset_distance.signum() * offset_distance * offset_distance * range; // -range <= offset          <= range
        angle - offset
    }


    pub fn relative_f(&self, input: f32) -> f32 {
        input * if self.face_right { 1.0 } else { -1.0 }
    }

    pub fn debug_string(&self, index: EntityKey) -> String {
        format!("Entity: {:?}  location: {:?}  x_vel: {:.5}  y_vel: {:.5}  kb_x_vel: {:.5}  kb_y_vel: {:.5}",
            index, self.location, self.x_vel, self.y_vel, self.kb_x_vel, self.kb_y_vel)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LedgeLogic {
    Hog,
    Share,
    Trump
}
