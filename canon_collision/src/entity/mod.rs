pub(crate) mod components;
pub(crate) mod item;
pub(crate) mod player;
pub(crate) mod projectile;

use player::{Player, RenderPlayer, MessagePlayer};
use projectile::{Projectile, ProjectileAction};
use item::{Item, ItemAction, MessageItem};
use components::action_state::{ActionState, Hitlag};
use components::body::{Body};

use crate::collision::collision_box::CollisionResult;
use crate::graphics;
use crate::particle::Particle;
use crate::rules::Goal;

use canon_collision_lib::geometry::Rect;
use canon_collision_lib::entity_def::{EntityDef, ActionFrame, CollisionBoxRole, ECB, Action};
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::stage::{Stage, Surface};

use cgmath::{Quaternion, Rotation3, Rad};
use num_traits::{FromPrimitive, ToPrimitive};
use rand_chacha::ChaChaRng;
use slotmap::{DenseSlotMap, SparseSecondaryMap, new_key_type};
use treeflection::KeyedContextVec;
use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;

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
    pub state: ActionState,
}

impl Entity {
    pub fn process_message(&mut self, message: Message, context: &mut StepContext) {
        let action_result = match (&mut self.ty, &message.contents) { // TODO: we could very happily match the owned value once thats stabilised
            (EntityType::Item (item), MessageContents::Item (message)) => item.process_message(message, context, &self.state),
            _ => {
                error!("Message received by entity type that cannot process it");
                None
            }
        };
        self.process_action_result(action_result);
    }

    pub fn is_hogging_ledge(&self, check_platform_i: usize, face_right: bool) -> bool {
        match self.body() {
            Some(body) => body.is_hogging_ledge(check_platform_i, face_right),
            None => false,
        }
    }

    pub fn face_right(&self) -> bool {
        match &self.ty {
            EntityType::Player (player) => player.body.face_right,
            EntityType::Item   (item)   => item.body.face_right,
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
        let action_frame = self.get_entity_frame(&entity_defs[self.entity_def_key()]);
        match &self.ty {
            EntityType::Player     (player)     => player.body.public_bps_xy(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Item       (item)       => item.body.public_bps_xy(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Projectile (projectile) => (projectile.x, projectile.y)
        }
    }

    /// only used for rendering
    pub fn public_bps_xyz(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32, f32) {
        let action_frame = self.get_entity_frame(&entity_defs[self.entity_def_key()]);
        match &self.ty {
            EntityType::Player     (player)     => player.body.public_bps_xyz(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Item       (item)       => item.body.public_bps_xyz(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Projectile (projectile) => (projectile.x, projectile.y, 0.0)
        }
    }

    pub fn item_grab(&mut self, hit_key: EntityKey, hit_id: Option<usize>) {
        let action_result = match &mut self.ty {
            EntityType::Player     (player) => player.item_grab(),
            EntityType::Item       (item)   => item.grabbed(hit_key, hit_id),
            EntityType::Projectile (_)      => None
        };
        self.process_action_result(action_result);
    }

    pub fn physics_step(&mut self, context: &mut StepContext, game_frame: usize, goal: Goal) {
        let action_result = match &mut self.ty {
            EntityType::Player     (player) => player.physics_step(context, &self.state, game_frame, goal),
            EntityType::Item       (item)   => item.physics_step(context, &self.state),
            EntityType::Projectile (_)      => None,
        };
        self.process_action_result(action_result);
    }

    pub fn step_collision(&mut self, context: &mut StepContext, col_results: &[CollisionResult]) {
        let action_result = match &mut self.ty {
            EntityType::Player     (player)     => player.step_collision(context, &self.state, col_results),
            EntityType::Item       (item)       => item.step_collision(col_results),
            EntityType::Projectile (projectile) => projectile.step_collision(col_results),
        };
        self.process_action_result(action_result);
        for col_result in col_results {
            match col_result {
                &CollisionResult::HitAtk { entity_defend_i, ref hitbox, .. } => {
                    self.state.hitlist.push(entity_defend_i);
                    self.state.hitlag = Hitlag::Attack { counter: (hitbox.damage / 3.0 + 3.0) as u64 };
                }
                &CollisionResult::HitShieldAtk { entity_defend_i, ref hitbox, .. } => {
                    self.state.hitlist.push(entity_defend_i);
                    self.state.hitlag = Hitlag::Attack { counter: (hitbox.damage / 3.0 + 3.0) as u64 };
                }
                &CollisionResult::HitDef { ref hitbox, .. } => {
                    self.state.hitlag = Hitlag::Launch { counter: (hitbox.damage / 3.0 + 3.0) as u64, wobble_x: 0.0 };
                }
                &CollisionResult::HitShieldDef { ref hitbox, .. } => {
                    self.state.hitlag = Hitlag::Attack { counter: (hitbox.damage / 3.0 + 3.0) as u64 };
                }
                _ => { }
            }
        }
    }

    pub fn action_hitlag_step(&mut self, context: &mut StepContext) {
        // If the action or frame is out of bounds jump to a valid one.
        // This is needed because we can continue from any point in a replay and replays may
        // contain actions or frames that no longer exist.
        if self.state.action as usize >= context.entity_def.actions.len() {
            self.state.action = 0;
        } else {
            let fighter_frames = &context.entity_def.actions[self.state.action as usize].frames;
            if self.state.frame as usize >= fighter_frames.len() {
                self.state.frame = 0;
            }
        }

        // The code from this point onwards can assume we are on a valid action and frame
        if let Some(body) = self.body_mut() {
            body.frames_since_hit += 1;
            if body.frames_since_hit > 60 {
                body.hit_angle_pre_di = None;
                body.hit_angle_post_di = None;
            }
        }

        self.state.hitlag.step(&mut context.rng);
        if let Hitlag::None = self.state.hitlag {
            let main_action_result = self.action_step(context);
            let secondary_action_result = if let Some(main_action_result) = main_action_result {
                self.process_action_result(Some(main_action_result));
                self.action_step(context)
            } else {
                ActionResult::set_frame(self.state.frame + 1)
            };
            self.process_action_result(secondary_action_result);
        }
    }

    fn action_step(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        let fighter_frame = &context.entity_def.actions[self.state.action as usize].frames[self.state.frame as usize];
        if fighter_frame.force_hitlist_reset {
            self.state.hitlist.clear();
        }

        match &mut self.ty {
            EntityType::Player     (player)     => player.action_step(context, &self.state),
            EntityType::Item       (item)       => item.action_step(context, &self.state),
            EntityType::Projectile (projectile) => projectile.action_step(context, &self.state),
        }
    }

    pub fn grabbing_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        match &self.ty {
            EntityType::Player (player) => player.grabbing_xy(entities, entity_defs, surfaces, &self.state),
            _ => (0.0, 0.0),
        }
    }

    /// TODO: Wont need this anymore when we make surfaces into entities as they will be generational
    pub fn platform_deleted(&mut self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface], deleted_platform_i: usize) {
        let action_result = match &mut self.ty {
            EntityType::Player (player) => player.platform_deleted(entities, entity_defs, surfaces, deleted_platform_i, &self.state),
            _ => None
        };
        self.process_action_result(action_result);
    }

    // TODO: move into ActionState
    pub fn entity_def_key(&self) -> &str {
        match &self.ty {
            EntityType::Player     (player)     => player.entity_def_key.as_ref(),
            EntityType::Item       (item)       => item.entity_def_key.as_ref(),
            EntityType::Projectile (projectile) => projectile.entity_def_key.as_ref(),
        }
    }

    pub fn frame_angle(&self, entity_def: &EntityDef, surfaces: &[Surface]) -> f32 {
        if let Some(entity_frame) = self.get_entity_frame(entity_def) {
            match &self.ty {
                EntityType::Player (player) => player.body.angle(entity_frame, surfaces),
                EntityType::Item (item) => item.body.angle(entity_frame, surfaces),
                EntityType::Projectile (projectile) => projectile.angle,
            }
        } else {
            0.0
        }
    }

    pub fn relative_f(&self, input: f32) -> f32 {
        input * if self.face_right() { 1.0 } else { -1.0 }
    }

    pub fn get_entity_frame<'a>(&self, entity_def: &'a EntityDef) -> Option<&'a ActionFrame> {
        self.state.get_entity_frame(entity_def)
    }

    pub fn relative_frame(&self, entity_def: &EntityDef, surfaces: &[Surface]) -> ActionFrame {
        let angle = self.frame_angle(entity_def, surfaces);
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

            if let Some(ref mut item_grab_box) = fighter_frame.item_grab_box {
                item_grab_box.x1 = self.relative_f(item_grab_box.x1);
                item_grab_box.x2 = self.relative_f(item_grab_box.x2);
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

    pub fn cam_area(&self, cam_max: &Rect, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> Option<Rect> {
        match &self.ty {
            EntityType::Player (player) => player.cam_area(&self.state, cam_max, entities, entity_defs, surfaces),
            _ => None
        }
    }

    pub fn item_grab_box(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> Option<Rect> {
        let (x, y) = self.public_bps_xy(entities, entity_defs, surfaces);
        let entity_def = &entity_defs[self.entity_def_key()];
        let frame = self.relative_frame(entity_def, surfaces);
        frame.item_grab_box.map(|rect| rect.offset(x, y))
    }

    pub fn hitlist(&self) -> &[EntityKey] {
        &self.state.hitlist
    }

    pub fn debug_print(&self, entities: &KeyedContextVec<EntityDef>, player_input: Option<&PlayerInput>, debug: &DebugEntity, i: EntityKey) -> Vec<String> {
        match &self.ty {
            EntityType::Player     (player)     => player.debug_print(entities, player_input.unwrap(), &self.state, debug, i),
            EntityType::Item       (item)       => item.debug_print(entities, &self.state, debug, i),
            EntityType::Projectile (projectile) => projectile.debug_print(entities, &self.state, debug, i),
        }
    }

    pub fn ecb(&self) -> ECB {
        match &self.ty {
            EntityType::Player (player) => player.ecb.clone(),
            EntityType::Item (_)  => ECB::default(),
            EntityType::Projectile (_)  => ECB::default(),
        }
    }

    pub fn body(&self) -> Option<&Body> {
        match &self.ty {
            EntityType::Player (player) => Some(&player.body),
            EntityType::Item (item)  => Some(&item.body),
            EntityType::Projectile (_)  => None
        }
    }

    pub fn body_mut(&mut self) -> Option<&mut Body> {
        match &mut self.ty {
            EntityType::Player (player) => Some(&mut player.body),
            EntityType::Item (item)  => Some(&mut item.body),
            EntityType::Projectile (_)  => None
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
            _ => vec!(),
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
        let range = entity_history.len().saturating_sub(5) .. entity_history.len();
        for entities in entity_history[range].iter().rev() {
            if let Some(entity) = entities.get(entity_i) {
                // handle deleted frames by just skipping it, only encountered when the editor is used.
                if entity_def.actions[entity.state.action as usize].frames.len() > entity.state.frame as usize {
                    frames.push(entity.render_frame(entities, entity_defs, surfaces));
                }
            }
        }

        let render_type = match &self.ty {
            EntityType::Player (player) => RenderEntityType::Player (player.render(entities, entity_defs, surfaces, &self.state)),
            EntityType::Projectile (_)  => RenderEntityType::Projectile,
            EntityType::Item (_)        => RenderEntityType::Item,
        };

        let visible = match &self.ty {
            EntityType::Item (item) => !item.body.is_item_held() || item.held_render_angle(entities, entity_defs).is_some(),
            _ => true
        };

        RenderEntity {
            frame_data:  self.relative_frame(entity_def, surfaces),
            particles:   self.particles().clone(),
            visible,
            render_type,
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
            entity_def_key:   self.entity_def_key().to_string(),
            model_name:       entity_def.name.clone(),
            frame_bps:        self.public_bps_xy(entities, entity_defs, surfaces),
            render_bps:       self.public_bps_xyz(entities, entity_defs, surfaces),
            ecb:              self.ecb(),
            frame:            self.state.frame as usize,
            frame_no_restart: self.state.frame_no_restart as usize,
            action:           self.state.action as usize,
            face_right:       self.face_right(),
            frame_angle:      self.frame_angle(entity_def, surfaces),
            render_angle:     self.render_angle(entities, entity_defs, surfaces),
        }
    }

    fn render_angle(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> Quaternion<f32> {
        let entity_def = &entity_defs[self.entity_def_key()];
        match &self.ty {
            EntityType::Item (item) => {
                if let Some(render_angle) = item.held_render_angle(entities, entity_defs) {
                    render_angle
                } else {
                    Quaternion::from_angle_z(Rad(self.frame_angle(entity_def, surfaces)))
                }
            }
            _ => Quaternion::from_angle_z(Rad(self.frame_angle(entity_def, surfaces)))
        }
    }

    fn process_action_result(&mut self, action_result: Option<ActionResult>) {
        match action_result {
            Some(ActionResult::SetAction (action)) => {
                if self.state.action != action {
                    self.state.frame_no_restart = 0;
                }
                self.state.frame = 0;
                self.state.action = action;
                self.state.hitlist.clear()
            }
            Some(ActionResult::SetFrame (frame)) => {
                self.state.frame = frame;
                self.state.frame_no_restart += 1;
            }
            None => { }
        }
    }
}

pub struct RenderEntity {
    pub render_type:       RenderEntityType,
    pub visible:           bool,
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
    Item,
}

impl RenderEntityType {
    /// TODO: figure out a better spot to put this so we can access from the hurtbox generator.
    pub fn action_index_to_string(&self, action_index: usize) -> String {
        match self {
            RenderEntityType::Player (_) =>           Action::from_u64(action_index as u64).map(|x| -> &str { x.into() }),
            RenderEntityType::Projectile => ProjectileAction::from_u64(action_index as u64).map(|x| -> &str { x.into() }),
            RenderEntityType::Item       =>       ItemAction::from_u64(action_index as u64).map(|x| -> &str { x.into() }),
        }
        .map(|x| x.to_string())
        .unwrap_or(format!("{}", action_index))
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
    pub item_grab_area: bool,
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
            if os_input.held_shift() {
                self.item_grab_area = !self.item_grab_area;
            }
            else {
                self.cam_area = !self.cam_area;
            }
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
            item_grab_area: true,
        }
    }
}

pub struct RenderEntityFrame {
    pub entity_def_key:   String,
    pub model_name:       String,
    pub frame_bps:        (f32, f32),
    pub render_bps:       (f32, f32, f32),
    pub ecb:              ECB,
    pub frame:            usize,
    pub frame_no_restart: usize,
    pub action:           usize,
    pub face_right:       bool,
    pub frame_angle:      f32,
    pub render_angle:     Quaternion<f32>,
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
    pub messages:     &'a mut Vec<Message>,
    pub delete_self:  bool,
}

pub struct Message {
    pub recipient: EntityKey,
    pub contents:  MessageContents,
}

#[allow(dead_code)]
pub enum MessageContents {
    Player (MessagePlayer),
    Item   (MessageItem),
}

#[must_use]
pub enum ActionResult {
    SetAction (u64),
    SetFrame  (i64),
}

impl ActionResult {
    fn set_action<T: ToPrimitive>(action: T) -> Option<ActionResult> {
        action.to_u64().map(|x| ActionResult::SetAction (x))
    }

    fn set_frame(action: i64) -> Option<ActionResult> {
        Some(ActionResult::SetFrame(action))
    }
}
