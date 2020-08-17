use crate::entity::{EntityKey, Entities, StepContext};

use canon_collision_lib::entity_def::{EntityDef, ActionFrame};
use canon_collision_lib::geometry;
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::stage::Surface;

use rand_chacha::ChaChaRng;
use rand::Rng;
use treeflection::KeyedContextVec;

use std::f32::consts::PI;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Hitlag {
    Attack { counter: u64 },
    Launch { counter: u64, kb_vel: f32, angle: f32, wobble_x: f32 },
    None
}

impl Hitlag {
    pub fn decrement(&mut self) -> bool {
        let end = match self {
            &mut Hitlag::Attack { ref mut counter} |
            &mut Hitlag::Launch { ref mut counter, .. } => {
                *counter -= 1;
                *counter <= 1
            }
            &mut Hitlag::None => {
                false
            }
        };
        if end {
            *self = Hitlag::None
        }
        end
    }

    fn wobble(&mut self, rng: &mut ChaChaRng) {
        if let &mut Hitlag::Launch { ref mut wobble_x, .. } = self {
            *wobble_x = (rng.gen::<f32>() - 0.5) * 3.0;
        }
    }
}

// Describes the player location by offsets from other locations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Location {
    Surface { platform_i: usize, x: f32 },
    GrabbedLedge { platform_i: usize, d_x: f32, d_y: f32, logic: LedgeLogic }, // player.face_right determines which edge on the platform
    GrabbedByPlayer (EntityKey),
    Airbourne { x: f32, y: f32 },
}

pub enum PhysicsResult {
    Fall,
    Land,
    Teeter,
    LedgeGrab,
    OutOfBounds,
}

pub enum HitlagResult {
    Hitlag,
    NoHitlag,
    LaunchAir { angle: f32 },
    LaunchGround { angle: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body {
    pub damage:             f32,
    pub x_vel:              f32,
    pub y_vel:              f32,
    pub kb_x_vel:           f32,
    pub kb_y_vel:           f32,
    pub kb_x_dec:           f32,
    pub kb_y_dec:           f32,
    pub hitlag:             Hitlag,
    pub location:           Location,
    pub face_right:         bool,
    pub frames_since_ledge: u64,
}

impl Body {
    pub fn new(location: Location, face_right: bool) -> Body {
        Body {
            damage:             0.0,
            x_vel:              0.0,
            y_vel:              0.0,
            kb_x_vel:           0.0,
            kb_y_vel:           0.0,
            kb_x_dec:           0.0,
            kb_y_dec:           0.0,
            hitlag:             Hitlag::None,
            frames_since_ledge: 0,
            location,
            face_right,
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

    pub fn public_bps_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, action_frame: Option<&ActionFrame>, surfaces: &[Surface]) -> (f32, f32) {
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
            Location::Airbourne { x, y } => {
                (x, y)
            }
        };

        match &self.hitlag {
            &Hitlag::Launch { wobble_x, .. } => {
                (bps_xy.0 + wobble_x, bps_xy.1)
            }
            _ => {
                bps_xy
            }
        }
    }

    pub fn angle(&self, action_frame: &ActionFrame, surfaces: &[Surface]) -> f32 {
         match self.location {
            Location::Surface { platform_i, .. } if action_frame.use_platform_angle => {
                surfaces.get(platform_i).map_or(0.0, |x| x.floor_angle().unwrap_or_default())
            }
            _ => 0.0,
        }
    }

    pub fn physics_step(&mut self, context: &mut StepContext, action_frame: &ActionFrame) -> Option<PhysicsResult> {
        if let Hitlag::None = self.hitlag {
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

            let x_vel = self.x_vel + self.kb_x_vel + self.relative_f(action_frame.x_vel_temp);
            let y_vel = self.y_vel + self.kb_y_vel + action_frame.y_vel_temp;

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
                        self.floor_move(context, action_frame, platform, platform_i, x)
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
                None => self.secondary_checks(context, action_frame)
            }
        } else {
            None
        }
    }

    fn secondary_checks(&mut self, context: &mut StepContext, action_frame: &ActionFrame) -> Option<PhysicsResult> {
        let blast = &context.stage.blast;
        let (x, y) = self.public_bps_xy(&context.entities, &context.entity_defs, Some(action_frame), &context.surfaces);
        if x < blast.left() || x > blast.right() || y < blast.bot() || y > blast.top() {
            Some(PhysicsResult::OutOfBounds)
        } else {
            // ledge grabs
            if self.frames_since_ledge >= 30 && self.y_vel < 0.0 && context.input.stick_y.value > -0.5 {
                if let Some(ref ledge_grab_box) = action_frame.ledge_grab_box {
                    self.check_ledge_grab(context, &ledge_grab_box)
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

    fn floor_move(&mut self, context: &mut StepContext, action_frame: &ActionFrame, platform: &Surface, platform_i: usize, x: f32) -> Option<PhysicsResult> {
        let connected_floors = context.stage.connected_floors(platform_i);
        if platform.plat_x_in_bounds(x) {
            self.location = Location::Surface { platform_i, x };
            self.secondary_checks(context, action_frame)
        }
        else if x < 0.0 && connected_floors.left_i.is_some() {
            let new_platform_i = connected_floors.left_i.unwrap();
            let new_platform = &context.stage.surfaces[new_platform_i];
            let world_x = platform.plat_x_to_world_x(x);
            let x = new_platform.world_x_to_plat_x(world_x);
            self.floor_move(context, action_frame, new_platform, new_platform_i, x)
        }
        else if x > 0.0 && connected_floors.right_i.is_some() {
            let new_platform_i = connected_floors.right_i.unwrap();
            let new_platform = &context.stage.surfaces[new_platform_i];
            let world_x = platform.plat_x_to_world_x(x);
            let x = new_platform.world_x_to_plat_x(world_x);
            self.floor_move(context, action_frame, new_platform, new_platform_i, x)
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

    pub fn hitlag_step(&mut self, context: &mut StepContext, action_frame: Option<&ActionFrame>) -> HitlagResult {
        match self.hitlag.clone() {
            Hitlag::Attack { .. } => {
                self.hitlag.decrement();
                HitlagResult::Hitlag
            }
            Hitlag::Launch { kb_vel, angle, .. } => {
                self.hitlag.wobble(&mut context.rng);

                if self.hitlag.decrement() {
                    self.hitlag_defend_end(context, action_frame, kb_vel, angle)
                } else {
                    HitlagResult::Hitlag
                }
            }
            Hitlag::None => HitlagResult::NoHitlag
        }
    }

    fn hitlag_defend_end(&mut self, context: &mut StepContext, action_frame: Option<&ActionFrame>, kb_vel: f32, angle: f32) -> HitlagResult {
        let angle = if (kb_vel >= 80.0 || self.is_airbourne() || (angle != 0.0 && angle != PI)) // can di
            && !(context.input[0].stick_x == 0.0 && context.input[0].stick_y == 0.0) // not deadzone
        {
            Body::di(&context.input, angle)
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

        if self.kb_y_vel == 0.0 {
            if kb_vel >= 80.0 {
                let (x, y) = self.public_bps_xy(&context.entities, &context.entity_defs, action_frame, &context.surfaces);
                self.location = Location::Airbourne { x, y: y + 0.0001 };
                HitlagResult::LaunchAir { angle }
            } else {
                HitlagResult::LaunchGround { angle }
            }
        }
        else if self.kb_y_vel > 0.0 {
            let (x, y) = self.public_bps_xy(&context.entities, &context.entity_defs, action_frame, &context.surfaces);
            self.location = Location::Airbourne { x, y };
            HitlagResult::LaunchAir { angle }
        }
        else {
            HitlagResult::LaunchGround { angle }
        }
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
        format!("Entity: {:?}  location: {:?}  x_vel: {:.5}  y_vel: {:.5}  kb_x_vel: {:.5}  kb_y_vel: {:.5}  hitlag: {:?}",
            index, self.location, self.x_vel, self.y_vel, self.kb_x_vel, self.kb_y_vel, self.hitlag)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LedgeLogic {
    Hog,
    Share,
    Trump
}
