use crate::player::{Player, RenderPlayer};
use crate::rules::Goal;
use crate::simple_projectile::SimpleProjectile;
use crate::collision::CollisionResult;
use crate::particle::Particle;
use crate::graphics;

use canon_collision_lib::geometry::Rect;
use canon_collision_lib::fighter::{Fighter, ActionFrame, CollisionBoxRole, ECB};
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::stage::{Stage, Surface};

use rand_chacha::ChaChaRng;
use treeflection::{Node, NodeRunner, NodeToken, KeyedContextVec};
use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;

use std::collections::HashSet;

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum Entity {
    Player (Player),
    SimpleProjectile (SimpleProjectile)
}

impl Default for Entity {
    fn default() -> Self {
        Entity::Player(Default::default())
    }
}

impl Entity {
    pub fn is_hogging_ledge(&self, check_platform_i: usize, face_right: bool) -> bool {
        match self {
            Entity::Player (player) => player.is_hogging_ledge(check_platform_i, face_right),
            _ => false,
        }
    }

    pub fn face_right(&self) -> bool {
        match self {
            Entity::Player (player) => player.face_right,
            Entity::SimpleProjectile (projectile) => projectile.angle > 0.0 && projectile.angle < 180.0, // TODO: what is the actual range?
        }
    }

    pub fn bps_xy(&self, context: &StepContext) -> (f32, f32) {
        match self {
            Entity::Player (player) => player.bps_xy(context),
            Entity::SimpleProjectile (projectile) => (projectile.x, projectile.y)
        }
    }

    // TODO: uhhh.... surely I merge these
    pub fn public_bps_xy(&self, players: &[Entity], fighters: &KeyedContextVec<Fighter>, surfaces: &[Surface]) -> (f32, f32) {
        match self {
            Entity::Player (player) => player.public_bps_xy(players, fighters, surfaces),
            Entity::SimpleProjectile (projectile) => (projectile.x, projectile.y)
        }
    }

    pub fn physics_step(&mut self, context: &mut StepContext, player_i: usize, game_frame: usize, goal: Goal) {
        match self {
            Entity::Player (player) => player.physics_step(context, player_i, game_frame, goal),
            Entity::SimpleProjectile (_) => { }
        }
    }

    pub fn step_collision(&mut self, context: &mut StepContext, col_results: &[CollisionResult]) {
        match self {
            Entity::Player (player) => player.step_collision(context, col_results),
            Entity::SimpleProjectile (_) => { }
        }
    }

    pub fn action_hitlag_step(&mut self, context: &mut StepContext) {
        match self {
            Entity::Player (player) => player.action_hitlag_step(context),
            Entity::SimpleProjectile (_) => { }
        }
    }

    pub fn grabbing_xy(&self, players: &[Entity], fighters: &KeyedContextVec<Fighter>, surfaces: &[Surface]) -> (f32, f32) {
        match self {
            Entity::Player (player) => player.grabbing_xy(players, fighters, surfaces),
            _ => (0.0, 0.0),
        }
    }

    pub fn platform_deleted(&mut self, players: &[Entity], fighters: &KeyedContextVec<Fighter>, surfaces: &[Surface], deleted_platform_i: usize) {
        match self {
            Entity::Player (player) => player.platform_deleted(players, fighters, surfaces, deleted_platform_i),
            Entity::SimpleProjectile (_) => { }
        }
    }

    pub fn entity_def_key(&self) -> &str {
        match self {
            Entity::Player (player) => player.fighter.as_ref(),
            Entity::SimpleProjectile (projectile) => projectile.entity_def_key.as_ref(),
        }
    }

    pub fn angle(&self, fighter: &Fighter, surfaces: &[Surface]) -> f32 {
        match self {
            Entity::Player (player) => player.angle(fighter, surfaces),
            Entity::SimpleProjectile (projectile) => projectile.angle,
        }
    }

    pub fn relative_f(&self, input: f32) -> f32 {
        input * if self.face_right() { 1.0 } else { -1.0 }
    }

    pub fn get_fighter_frame<'a>(&self, fighter: &'a Fighter) -> Option<&'a ActionFrame> {
        match self {
            Entity::Player (player) => player.get_fighter_frame(fighter),
            Entity::SimpleProjectile (projectile) => projectile.get_fighter_frame(fighter),
        }
    }

    pub fn relative_frame(&self, fighter: &Fighter, surfaces: &[Surface]) -> ActionFrame {
        let angle = self.angle(fighter, surfaces);
        if let Some(fighter_frame) = self.get_fighter_frame(fighter) {
            let mut fighter_frame = fighter_frame.clone();

            // fix hitboxes
            for colbox in fighter_frame.colboxes.iter_mut() {
                let (raw_x, y) = colbox.point;
                let x = self.relative_f(raw_x);
                let angled_x = x * angle.cos() - y * angle.sin();
                let angled_y = x * angle.sin() + y * angle.cos();
                colbox.point = (angled_x, angled_y);
                if let &mut CollisionBoxRole::Hit (ref mut hitbox) = &mut colbox.role {
                    if !self.face_right() {
                        hitbox.angle = 180.0 - hitbox.angle
                    };
                }
            }

            fighter_frame
        } else {
            ActionFrame::default()
        }
    }

    pub fn frame(&self) -> i64 {
        match self {
            Entity::Player (player) => player.frame,
            Entity::SimpleProjectile (projectile) => projectile.frame,
        }
    }

    pub fn set_frame(&mut self, frame: i64) {
        match self {
            Entity::Player (player) => player.frame = frame,
            Entity::SimpleProjectile (projectile) => projectile.frame = frame,
        }
    }

    pub fn action(&self) -> u64 {
        match self {
            Entity::Player (player) => player.action,
            Entity::SimpleProjectile (projectile) => projectile.action,
        }
    }

    pub fn cam_area(&self, cam_max: &Rect, players: &[Entity], fighters: &KeyedContextVec<Fighter>, surfaces: &[Surface]) -> Option<Rect> {
        match self {
            Entity::Player (player) => player.cam_area(cam_max, players, fighters, surfaces),
            Entity::SimpleProjectile (_) => None
        }
    }

    pub fn hitlist(&self) -> &[usize] {
        match self {
            Entity::Player (player) => &player.hitlist,
            Entity::SimpleProjectile (_) => &[]
        }
    }

    pub fn debug_print(&self, fighters: &KeyedContextVec<Fighter>, player_input: Option<&PlayerInput>, debug: &DebugEntity, index: usize) -> Vec<String> {
        match self {
            Entity::Player (player) => player.debug_print(fighters, player_input.unwrap(), debug, index),
            _ => vec!()
        }
    }

    pub fn ecb(&self) -> ECB {
        match self {
            Entity::Player (player)      => player.ecb.clone(),
            Entity::SimpleProjectile (_) => ECB::default(),
        }
    }

    pub fn team(&self) -> usize {
        match self {
            Entity::Player (player) => player.team,
            Entity::SimpleProjectile (_) => 0,
        }
    }

    pub fn particles(&self) -> Vec<Particle> {
        match self {
            Entity::Player (player) => player.particles.clone(),
            Entity::SimpleProjectile (_) => vec!(),
        }
    }

    pub fn render(&self, selected_colboxes: HashSet<usize>, entity_selected: bool, debug: DebugEntity, entity_i: usize, entity_history: &[Vec<Entity>], entities: &[Entity], fighters: &KeyedContextVec<Fighter>, surfaces: &[Surface]) -> RenderEntity {
        let fighter_color = graphics::get_team_color3(self.team());
        let fighter = &fighters[self.entity_def_key()];

        let vector_arrows = if let Entity::Player (player) = self {
            player.vector_arrows(&debug)
        } else {
            vec!()
        };

        let mut frames = vec!(self.render_frame(entities, fighters, surfaces));
        let range = entity_history.len().saturating_sub(10) .. entity_history.len();
        for entities in entity_history[range].iter().rev() {
            // TODO: Uh oh ... we cant rely on indexes remaining valid anymore, we need a way to map indices back to previous frames.
            if let Some(entity) = entities.get(entity_i) {
                // handle deleted frames by just skipping it, only encountered when the editor is used.
                if fighter.actions[entity.action() as usize].frames.len() > entity.frame() as usize {
                    frames.push(entity.render_frame(entities, fighters, surfaces));
                }
            }
        }

        let render_type = match self {
            Entity::Player (player) => RenderEntityType::Player (player.render(entities, fighters, surfaces)),
            Entity::SimpleProjectile (_) => RenderEntityType::Generic,
        };

        RenderEntity {
            render_type,
            frame_data:  self.relative_frame(fighter, surfaces),
            particles:   self.particles().clone(),
            frames,
            fighter_color,
            entity_selected,
            selected_colboxes,
            debug,
            vector_arrows,
        }
    }

    fn render_frame(&self, players: &[Entity], fighters: &KeyedContextVec<Fighter>, surfaces: &[Surface]) -> RenderEntityFrame {
        let fighter = &fighters[self.entity_def_key()];
        RenderEntityFrame {
            fighter:     self.entity_def_key().to_string(),
            model_name:  fighter.name.clone(),
            bps:         self.public_bps_xy(players, fighters, surfaces),
            ecb:         self.ecb(),
            frame:       self.frame() as usize,
            action:      self.action() as usize,
            face_right:  self.face_right(),
            angle:       self.angle(fighter, surfaces),
        }
    }
}

pub struct RenderEntity {
    pub render_type:       RenderEntityType,
    pub debug:             DebugEntity,
    /// Gauranteed to have at least one value (the current frame), and can have up to and including 10 values
    pub frames:            Vec<RenderEntityFrame>,
    pub frame_data:        ActionFrame,
    pub fighter_color:     [f32; 3],
    pub entity_selected:   bool,
    pub selected_colboxes: HashSet<usize>,
    pub vector_arrows:     Vec<VectorArrow>,
    pub particles:         Vec<Particle>,
}

pub enum RenderEntityType {
    Player (RenderPlayer),
    Generic,
}

#[derive(Copy, Clone, Serialize, Deserialize, Node)]
pub enum RenderDebugType {
    Normal,
    NormalAndDebug,
    Debug,
    DebugOnionSkin,
}

impl Default for RenderDebugType {
    fn default() -> Self {
        RenderDebugType::Normal
    }
}

impl RenderDebugType {
    pub fn step(&mut self) {
        *self = match self {
            RenderDebugType::Normal         => RenderDebugType::NormalAndDebug,
            RenderDebugType::NormalAndDebug => RenderDebugType::Debug,
            RenderDebugType::Debug          => RenderDebugType::DebugOnionSkin,
            RenderDebugType::DebugOnionSkin => RenderDebugType::Normal,
        };
    }

    pub fn normal(&self) -> bool {
        match self {
            RenderDebugType::Normal         => true,
            RenderDebugType::NormalAndDebug => true,
            RenderDebugType::Debug          => false,
            RenderDebugType::DebugOnionSkin => false,
        }
    }

    pub fn debug(&self) -> bool {
        match self {
            RenderDebugType::Normal         => false,
            RenderDebugType::NormalAndDebug => true,
            RenderDebugType::Debug          => true,
            RenderDebugType::DebugOnionSkin => true,
        }
    }

    pub fn onion_skin(&self) -> bool {
        match self {
            RenderDebugType::Normal         => false,
            RenderDebugType::NormalAndDebug => false,
            RenderDebugType::Debug          => false,
            RenderDebugType::DebugOnionSkin => true,
        }
    }
}

#[derive(Copy, Clone, Default, Serialize, Deserialize, Node)]
pub struct DebugEntity {
    pub render:         RenderDebugType,
    pub physics:        bool,
    pub input:          bool,
    pub input_diff:     bool,
    pub action:         bool,
    pub frame:          bool,
    pub stick_vector:   bool,
    pub c_stick_vector: bool,
    pub di_vector:      bool,
    pub hitbox_vectors: bool,
    pub ecb:            bool,
    pub cam_area:       bool,
}

impl DebugEntity {
    pub fn step(&mut self, os_input: &WinitInputHelper) {
        if os_input.key_pressed(VirtualKeyCode::F1) {
            self.physics = !self.physics;
        }
        if os_input.key_pressed(VirtualKeyCode::F2) {
            if os_input.held_shift() {
                self.input_diff = !self.input_diff;
            }
            else {
                self.input = !self.input;
            }
        }
        if os_input.key_pressed(VirtualKeyCode::F3) {
            self.action = !self.action;
        }
        if os_input.key_pressed(VirtualKeyCode::F4) {
            self.frame = !self.frame;
        }
        if os_input.key_pressed(VirtualKeyCode::F5) {
            self.stick_vector = !self.stick_vector;
            self.c_stick_vector = !self.c_stick_vector;
        }
        if os_input.key_pressed(VirtualKeyCode::F6) {
            self.di_vector = !self.di_vector;
        }
        if os_input.key_pressed(VirtualKeyCode::F7) {
            self.hitbox_vectors = !self.hitbox_vectors;
        }
        if os_input.key_pressed(VirtualKeyCode::F8) {
            self.ecb = !self.ecb;
        }
        if os_input.key_pressed(VirtualKeyCode::F9) {
            self.render.step();
        }
        if os_input.key_pressed(VirtualKeyCode::F10) {
            self.cam_area = !self.cam_area;
        }
        if os_input.key_pressed(VirtualKeyCode::F11) {
            *self = DebugEntity::all();
        }
        if os_input.key_pressed(VirtualKeyCode::F12) {
            *self = DebugEntity::default();
        }
    }

    pub fn all() -> Self {
        DebugEntity {
            render:         RenderDebugType::NormalAndDebug,
            physics:        true,
            input:          true,
            input_diff:     true,
            action:         true,
            frame:          true,
            stick_vector:   true,
            c_stick_vector: true,
            di_vector:      true,
            hitbox_vectors: true,
            ecb:            true,
            cam_area:       true,
        }
    }
}

pub struct RenderEntityFrame {
    pub fighter:    String,
    pub model_name: String,
    pub bps:        (f32, f32),
    pub ecb:        ECB,
    pub frame:      usize,
    pub action:     usize,
    pub face_right: bool,
    pub angle:      f32,
}

pub struct VectorArrow {
    pub x:     f32,
    pub y:     f32,
    pub color: [f32; 4]
}

pub struct StepContext<'a> {
    pub input:        &'a PlayerInput,
    pub entities:     &'a [Entity],
    pub fighters:     &'a KeyedContextVec<Fighter>,
    pub fighter:      &'a Fighter,
    pub stage:        &'a Stage,
    pub surfaces:     &'a [Surface],
    pub rng:          &'a mut ChaChaRng,
    pub new_entities: &'a mut Vec<Entity>,
}
