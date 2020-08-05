use crate::collision::CollisionResult;
use crate::item::Item;
use crate::particle::Particle;
use crate::player::{Player, RenderPlayer};
use crate::projectile::{Projectile, ProjectileAction};
use crate::rules::Goal;
use crate::graphics;

use canon_collision_lib::geometry::Rect;
use canon_collision_lib::entity_def::{EntityDef, ActionFrame, CollisionBoxRole, ECB, Action};
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::stage::{Stage, Surface};

use treeflection::KeyedContextVec;
use num_traits::FromPrimitive;
use rand_chacha::ChaChaRng;
use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;
use slotmap::{DenseSlotMap, SparseSecondaryMap, new_key_type};

use std::collections::HashSet;
use std::f32::consts::PI;

new_key_type! { pub struct EntityKey; }
pub type Entities = DenseSlotMap<EntityKey, Entity>;
pub type DebugEntities = SparseSecondaryMap<EntityKey, DebugEntity>;

#[derive(Clone, Serialize, Deserialize)]
pub enum EntityType {
    Player (Player),
    Projectile (Projectile),
    Item (Item),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Entity {
    pub ty: EntityType,
    // TODO: Already split into struct + enum and ill probs need it anyway so ill leave it as is.
    // I can probably create another sub struct which I pass into the ty.step for processing.
    // if none of that works out, lets just squish into a single enum again.
}

impl Entity {
    pub fn is_hogging_ledge(&self, check_platform_i: usize, face_right: bool) -> bool {
        match &self.ty {
            EntityType::Player (player) => player.is_hogging_ledge(check_platform_i, face_right),
            _ => false,
        }
    }

    pub fn face_right(&self) -> bool {
        match &self.ty {
            EntityType::Player (player) => player.face_right,
            EntityType::Item   (item)   => item.face_right,
            EntityType::Projectile (projectile) => {
                let angle = projectile.angle % (PI * 2.0); // TODO: does this handle negative numbers?
                let face_left = angle > PI / 2.0 && angle < PI * 3.0 / 2.0;
                !face_left
            }
        }
    }

    pub fn bps_xy(&self, context: &StepContext) -> (f32, f32) {
        self.public_bps_xy(&context.entities, &context.entity_defs, &context.surfaces)
    }

    pub fn public_bps_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        match &self.ty {
            EntityType::Player     (player)     => player.public_bps_xy(entities, entity_defs, surfaces),
            EntityType::Item       (item)       => item.public_bps_xy(entities, entity_defs, surfaces),
            EntityType::Projectile (projectile) => (projectile.x, projectile.y)
        }
    }

    pub fn physics_step(&mut self, context: &mut StepContext, game_frame: usize, goal: Goal) {
        match &mut self.ty {
            EntityType::Player     (player) => player.physics_step(context, game_frame, goal),
            EntityType::Item       (item)   => item.physics_step(context),
            EntityType::Projectile (_)      => { }
        }
    }

    pub fn step_collision(&mut self, context: &mut StepContext, col_results: &[CollisionResult]) {
        match &mut self.ty {
            EntityType::Player     (player)     => player.step_collision(context, col_results),
            EntityType::Item       (item)       => item.step_collision(context, col_results),
            EntityType::Projectile (projectile) => projectile.step_collision(context, col_results),
        }
    }

    pub fn action_hitlag_step(&mut self, context: &mut StepContext) {
        match &mut self.ty {
            EntityType::Player     (player)     => player.action_hitlag_step(context),
            EntityType::Item       (item)       => item.action_hitlag_step(context),
            EntityType::Projectile (projectile) => projectile.action_hitlag_step(context),
        }
    }

    pub fn grabbing_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        match &self.ty {
            EntityType::Player (player) => player.grabbing_xy(entities, entity_defs, surfaces),
            _ => (0.0, 0.0),
        }
    }

    pub fn platform_deleted(&mut self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface], deleted_platform_i: usize) {
        match &mut self.ty {
            EntityType::Player     (player) => player.platform_deleted(entities, entity_defs, surfaces, deleted_platform_i),
            EntityType::Player     (item)   => item.platform_deleted(entities, entity_defs, surfaces, deleted_platform_i),
            EntityType::Projectile (_)      => { }
        }
    }

    pub fn entity_def_key(&self) -> &str {
        match &self.ty {
            EntityType::Player     (player)     => player.entity_def_key.as_ref(),
            EntityType::Item       (item)       => item.entity_def_key.as_ref(),
            EntityType::Projectile (projectile) => projectile.entity_def_key.as_ref(),
        }
    }

    pub fn angle(&self, entity_def: &EntityDef, surfaces: &[Surface]) -> f32 {
        match &self.ty {
            EntityType::Player (player) => player.angle(entity_def, surfaces),
            EntityType::Item (item) => item.angle(entity_def, surfaces),
            EntityType::Projectile (projectile) => projectile.angle,
        }
    }

    pub fn relative_f(&self, input: f32) -> f32 {
        input * if self.face_right() { 1.0 } else { -1.0 }
    }

    pub fn get_entity_frame<'a>(&self, entity_def: &'a EntityDef) -> Option<&'a ActionFrame> {
        match &self.ty {
            EntityType::Player (player) => player.get_entity_frame(entity_def),
            EntityType::Item (item) => item.get_entity_frame(entity_def),
            EntityType::Projectile (projectile) => projectile.get_entity_frame(entity_def),
        }
    }

    pub fn relative_frame(&self, entity_def: &EntityDef, surfaces: &[Surface]) -> ActionFrame {
        let angle = self.angle(entity_def, surfaces);
        if let Some(fighter_frame) = self.get_entity_frame(entity_def) {
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

    pub fn can_hit(&self, other: &Entity) -> bool {
        self.player_id() != other.player_id()
    }


    /// The players id
    /// or owning players id
    /// or none if not owned by a player
    pub fn player_id(&self) -> Option<usize> {
        match &self.ty {
            EntityType::Player (player) => Some(player.id),
            EntityType::Item (item) => item.owner_id,
            EntityType::Projectile (projectile) => projectile.owner_id,
        }
    }

    pub fn frame(&self) -> i64 {
        match &self.ty {
            EntityType::Player (player) => player.frame,
            EntityType::Item (item) => item.frame,
            EntityType::Projectile (projectile) => projectile.frame,
        }
    }

    pub fn set_frame(&mut self, frame: i64) {
        match &mut self.ty {
            EntityType::Player (player) => player.frame = frame,
            EntityType::Item (item) => item.frame = frame,
            EntityType::Projectile (projectile) => projectile.frame = frame,
        }
    }

    pub fn action(&self) -> u64 {
        match &self.ty {
            EntityType::Player (player) => player.action,
            EntityType::Item (item) => item.action,
            EntityType::Projectile (projectile) => projectile.action,
        }
    }

    pub fn cam_area(&self, cam_max: &Rect, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> Option<Rect> {
        match &self.ty {
            EntityType::Player (player) => player.cam_area(cam_max, entities, entity_defs, surfaces),
            _ => None
        }
    }

    pub fn hitlist(&self) -> &[EntityKey] {
        match &self.ty {
            EntityType::Player (player) => &player.hitlist,
            EntityType::Item (_) => &[],
            EntityType::Projectile (_) => &[]
        }
    }

    pub fn debug_print(&self, entities: &KeyedContextVec<EntityDef>, player_input: Option<&PlayerInput>, debug: &DebugEntity, i: EntityKey) -> Vec<String> {
        match &self.ty {
            EntityType::Player     (player)     => player.debug_print(entities, player_input.unwrap(), debug, i),
            EntityType::Item       (item)       => item.debug_print(entities, debug, i),
            EntityType::Projectile (projectile) => projectile.debug_print(entities, debug, i),
        }
    }

    pub fn ecb(&self) -> ECB {
        match &self.ty {
            EntityType::Player (player) => player.ecb.clone(),
            EntityType::Item (_)  => ECB::default(),
            EntityType::Projectile (_)  => ECB::default(),
        }
    }

    pub fn team(&self) -> usize {
        match &self.ty {
            EntityType::Player (player) => player.team,
            EntityType::Item (_) => 0,
            EntityType::Projectile (_) => 0,
        }
    }

    pub fn particles(&self) -> Vec<Particle> {
        match &self.ty {
            EntityType::Player (player) => player.particles.clone(),
            EntityType::Item (_) => vec!(),
            EntityType::Projectile (_) => vec!(),
        }
    }

    pub fn render(&self, selected_colboxes: HashSet<usize>, entity_selected: bool, debug: DebugEntity, entity_i: EntityKey, entity_history: &[Entities], entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> RenderEntity {
        let fighter_color = graphics::get_team_color3(self.team());
        let entity_def = &entity_defs[self.entity_def_key()];

        let vector_arrows = if let EntityType::Player (player) = &self.ty {
            player.vector_arrows(&debug)
        } else {
            vec!()
        };

        let mut frames = vec!(self.render_frame(entities, entity_defs, surfaces));
        let range = entity_history.len().saturating_sub(10) .. entity_history.len();
        for entities in entity_history[range].iter().rev() {
            if let Some(entity) = entities.get(entity_i) {
                // handle deleted frames by just skipping it, only encountered when the editor is used.
                if entity_def.actions[entity.action() as usize].frames.len() > entity.frame() as usize {
                    frames.push(entity.render_frame(entities, entity_defs, surfaces));
                }
            }
        }

        let render_type = match &self.ty {
            EntityType::Player (player) => RenderEntityType::Player (player.render(entities, entity_defs, surfaces)),
            EntityType::Projectile (_) => RenderEntityType::Projectile,
        };

        RenderEntity {
            render_type,
            frame_data:  self.relative_frame(entity_def, surfaces),
            particles:   self.particles().clone(),
            frames,
            fighter_color,
            entity_selected,
            selected_colboxes,
            debug,
            vector_arrows,
        }
    }

    fn render_frame(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> RenderEntityFrame {
        let entity_def = &entity_defs[self.entity_def_key()];
        RenderEntityFrame {
            entity_def_key: self.entity_def_key().to_string(),
            model_name:     entity_def.name.clone(),
            bps:            self.public_bps_xy(entities, entity_defs, surfaces),
            ecb:            self.ecb(),
            frame:          self.frame() as usize,
            action:         self.action() as usize,
            face_right:     self.face_right(),
            angle:          self.angle(entity_def, surfaces),
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
    Projectile,
}

impl RenderEntityType {
    /// TODO: figure out a better spot to put this so we can access from the hurtbox generator.
    pub fn action_index_to_string(&self, action_index: usize) -> String {
        match self {
            RenderEntityType::Player (_) => {
                match Action::from_u64(action_index as u64) {
                    Some(action) => {
                        let action: &str = action.into();
                        action.to_string()
                    }
                    None => format!("{}", action_index),
                }
            }
            RenderEntityType::Projectile => {
                match ProjectileAction::from_u64(action_index as u64) {
                    Some(action) => {
                        let action: &str = action.into();
                        action.to_string()
                    }
                    None => format!("{}", action_index),
                }
            }
        }
    }
}

#[derive(Copy, Clone, Serialize, Deserialize)]
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

// TODO: Split player specific debug into a DebugPlayer stored in Player
#[derive(Copy, Clone, Default, Serialize, Deserialize)]
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
    // TODO: move into game logic, then we can keep all the debug keys together (across entity/player/other)
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
    pub entity_def_key: String,
    pub model_name:     String,
    pub bps:            (f32, f32),
    pub ecb:            ECB,
    pub frame:          usize,
    pub action:         usize,
    pub face_right:     bool,
    pub angle:          f32,
}

pub struct VectorArrow {
    pub x:     f32,
    pub y:     f32,
    pub color: [f32; 4]
}

pub struct StepContext<'a> {
    pub input:        &'a PlayerInput,
    pub entities:     &'a Entities,
    pub entity_defs:  &'a KeyedContextVec<EntityDef>,
    pub entity_def:   &'a EntityDef,
    pub stage:        &'a Stage,
    pub surfaces:     &'a [Surface],
    pub rng:          &'a mut ChaChaRng,
    pub new_entities: &'a mut Vec<Entity>,
    pub delete_self:  bool,
}
