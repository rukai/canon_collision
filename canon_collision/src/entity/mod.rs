pub(crate) mod components;
pub(crate) mod fighters;
pub(crate) mod item;
pub(crate) mod projectile;
pub(crate) mod toriel_fireball;
pub(crate) mod toriel_oven;

use std::collections::HashSet;
use std::f32::consts::PI;

use components::action_state::{ActionState, Hitlag};
use components::body::Body;
use fighters::player::{MessagePlayer, Player, RenderPlayer};
use fighters::Fighter;
use item::{Item, MessageItem};
use projectile::Projectile;
use toriel_fireball::TorielFireball;
use toriel_oven::{MessageTorielOven, TorielOven};

use crate::audio::sfx::{HitBoxSfx, SfxType};
use crate::audio::Audio;
use crate::collision::collision_box::CollisionResult;
use crate::graphics;
use crate::particle::Particle;
use crate::rules::Goal;

use canon_collision_lib::entity_def::{ActionFrame, CollisionBoxRole, EntityDef, ECB};
use canon_collision_lib::geometry::Rect;
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::stage::{Stage, Surface};

use cgmath::{Quaternion, Rad, Rotation3};
use rand_chacha::ChaChaRng;
use slotmap::{new_key_type, DenseSlotMap, SparseSecondaryMap};
use treeflection::KeyedContextVec;

new_key_type! { pub struct EntityKey; }
pub type Entities = DenseSlotMap<EntityKey, Entity>;
pub type DebugEntities = SparseSecondaryMap<EntityKey, DebugEntity>;

#[derive(Clone, Serialize, Deserialize)]
pub enum EntityType {
    Fighter(Fighter),
    Projectile(Projectile),
    Item(Item),
    TorielFireball(TorielFireball),
    TorielOven(TorielOven),
}

impl EntityType {
    pub fn get_player(&self) -> Option<&Player> {
        match self {
            EntityType::Fighter(fighter) => Some(fighter.get_player()),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Entity {
    pub ty: EntityType,
    pub state: ActionState,
}

impl Entity {
    #[rustfmt::skip]
    pub fn process_message(&mut self, message: Message, context: &mut StepContext) {
        let action_result = match (&mut self.ty, &message.contents) { // TODO: we could very happily match the owned value once thats stabilised
            (EntityType::Item       (entity), MessageContents::Item       (message)) => entity.process_message(message, context, &self.state),
            (EntityType::Fighter    (entity), MessageContents::Player     (message)) => entity.get_player_mut().process_message(message, context, &self.state),
            (EntityType::TorielOven (entity), MessageContents::TorielOven (message)) => entity.process_message(message, context, &self.state),
            _ => {
                error!("Message received by entity type that cannot process it");
                None
            }
        };
        self.process_action_result(context, action_result);
    }

    pub fn is_hogging_ledge(&self, check_platform_i: usize, face_right: bool) -> bool {
        match self.body() {
            Some(body) => body.is_hogging_ledge(check_platform_i, face_right),
            None => false,
        }
    }

    #[rustfmt::skip]
    pub fn face_right(&self) -> bool {
        match &self.ty {
            EntityType::Fighter (fighter) => fighter.get_player().body.face_right,
            EntityType::Item    (item)    => item.body.face_right,
            EntityType::Projectile (projectile) => {
                let angle = projectile.angle % (PI * 2.0); // TODO: does this handle negative numbers?
                let face_left = angle > PI / 2.0 && angle < PI * 3.0 / 2.0;
                !face_left
            }
            EntityType::TorielFireball (_) => true,
            EntityType::TorielOven (toriel_oven) => toriel_oven.body.face_right,
        }
    }

    pub fn bps_xy(&self, context: &StepContext) -> (f32, f32) {
        self.public_bps_xy(context.entities, context.entity_defs, context.surfaces)
    }

    #[rustfmt::skip]
    pub fn public_bps_xy(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32) {
        let action_frame = self.get_entity_frame(&entity_defs[self.state.entity_def_key.as_ref()]);
        match &self.ty {
            EntityType::Fighter        (fighter)    => fighter.get_player().body.public_bps_xy(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Item           (item)       => item.body.public_bps_xy(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::TorielOven     (toriel_oven) => toriel_oven.body.public_bps_xy(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Projectile     (projectile) => (projectile.x, projectile.y),
            EntityType::TorielFireball (projectile) => (projectile.x, projectile.y),
        }
    }

    /// only used for rendering
    #[rustfmt::skip]
    pub fn public_bps_xyz(&self, entities: &Entities, entity_defs: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> (f32, f32, f32) {
        let action_frame = self.get_entity_frame(&entity_defs[self.state.entity_def_key.as_ref()]);
        match &self.ty {
            EntityType::Fighter        (fighter)     => fighter.get_player().body.public_bps_xyz(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Item           (item)        => item.body.public_bps_xyz(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::TorielOven     (toriel_oven) => toriel_oven.body.public_bps_xyz(entities, entity_defs, action_frame, surfaces, &self.state),
            EntityType::Projectile     (projectile)  => (projectile.x, projectile.y, 0.0),
            EntityType::TorielFireball (projectile)  => (projectile.x, projectile.y, 0.0),
        }
    }

    #[rustfmt::skip]
    pub fn item_grab(&mut self, context: &mut StepContext, hit_key: EntityKey, hit_id: Option<usize>) {
        let action_result = match &mut self.ty {
            EntityType::Fighter    (fighter) => fighter.get_player_mut().item_grab(),
            EntityType::Item       (item)    => item.grabbed(hit_key, hit_id),
            _                                => None
        };
        self.process_action_result(context, action_result);
    }

    #[rustfmt::skip]
    pub fn physics_step(&mut self, context: &mut StepContext, game_frame: usize, goal: Goal) {
        let action_result = match &mut self.ty {
            EntityType::Fighter    (fighter) => fighter.get_player_mut().physics_step(context, &self.state, game_frame, goal),
            EntityType::Item       (item)    => item.physics_step(context, &self.state),
            EntityType::Projectile (_)       => None,
            EntityType::TorielFireball (_)   => None,
            EntityType::TorielOven (_)       => None,
        };
        self.process_action_result(context, action_result);
    }

    #[rustfmt::skip]
    pub fn step_collision(&mut self, context: &mut StepContext, col_results: &[CollisionResult]) {
        let action_result = match &mut self.ty {
            EntityType::Fighter    (fighter)        => fighter.get_player_mut().step_collision(context, &self.state, col_results),
            EntityType::Item       (item)           => item.step_collision(context, &self.state, col_results),
            EntityType::Projectile (projectile)     => projectile.step_collision(col_results),
            EntityType::TorielFireball (projectile) => projectile.step_collision(col_results),
            EntityType::TorielOven (_) => None,
        };
        self.process_action_result(context, action_result);
        for col_result in col_results {
            match col_result {
                CollisionResult::HitAtk { entity_defend_i, ref hitbox, .. } => {
                    context.audio.play_sound_effect(context.entity_def, SfxType::Hit(HitBoxSfx::Punch));
                    self.state.hitlist.push(*entity_defend_i);
                    self.state.hitlag = Hitlag::Attack { counter: (hitbox.damage / 3.0 + 3.0) as u64 };
                }
                CollisionResult::HitShieldAtk { entity_defend_i, ref hitbox, .. } => {
                    context.audio.play_sound_effect(context.entity_def, SfxType::Hit(HitBoxSfx::Sword));
                    self.state.hitlist.push(*entity_defend_i);
                    self.state.hitlag = Hitlag::Attack { counter: (hitbox.damage / 3.0 + 3.0) as u64 };
                }
                CollisionResult::HitDef { hitbox, .. } => {
                    self.state.hitlag = Hitlag::Launch { counter: (hitbox.damage / 3.0 + 3.0) as u64, wobble_x: 0.0 };
                }
                CollisionResult::HitShieldDef { hitbox, .. } => {
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
        if !context.entity_def.actions.contains_key(&self.state.action) {
            self.state.action = context
                .entity_def
                .actions
                .index_to_key(0)
                .expect("Entity def has no actions");
        } else {
            let fighter_frames = &context.entity_def.actions[self.state.action.as_ref()].frames;
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

        self.state.hitlag.step(context.rng);
        if let Hitlag::None = self.state.hitlag {
            let main_action_result = self.action_step(context).or_else(|| {
                if self.state.last_frame(context.entity_def) {
                    self.action_expired(context)
                } else {
                    None
                }
            });
            let secondary_action_result = match main_action_result {
                Some(ActionResult::SetAction(_)) => {
                    self.process_action_result(context, main_action_result);
                    self.action_step(context)
                }
                Some(ActionResult::SetActionKeepFrame(_)) => main_action_result,
                Some(ActionResult::SetFrame(_)) => main_action_result,
                None => ActionResult::set_frame(self.state.frame + 1),
            };
            self.process_action_result(context, secondary_action_result);
        }
    }

    fn action_step(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        let fighter_frame = &context.entity_def.actions[self.state.action.as_ref()].frames
            [self.state.frame as usize];
        if fighter_frame.force_hitlist_reset {
            self.state.hitlist.clear();
        }

        match &mut self.ty {
            EntityType::Fighter(fighter) => fighter.action_step(context, &self.state),
            EntityType::Item(item) => item.action_step(context, &self.state),
            EntityType::Projectile(projectile) => projectile.action_step(context, &self.state),
            EntityType::TorielFireball(projectile) => projectile.action_step(context, &self.state),
            EntityType::TorielOven(toriel_oven) => toriel_oven.action_step(context, &self.state),
        }
    }

    fn action_expired(&mut self, context: &mut StepContext) -> Option<ActionResult> {
        match &mut self.ty {
            EntityType::Fighter(fighter) => fighter.action_expired(context, &self.state),
            _ => None,
        }
    }

    pub fn grabbing_xy(
        &self,
        entities: &Entities,
        entity_defs: &KeyedContextVec<EntityDef>,
        surfaces: &[Surface],
    ) -> (f32, f32) {
        match &self.ty {
            EntityType::Fighter(fighter) => {
                fighter
                    .get_player()
                    .grabbing_xy(entities, entity_defs, surfaces, &self.state)
            }
            _ => (0.0, 0.0),
        }
    }

    /// TODO: Wont need this anymore when we make surfaces into entities as they will be generational
    pub fn platform_deleted(
        &mut self,
        entities: &Entities,
        entity_defs: &KeyedContextVec<EntityDef>,
        surfaces: &[Surface],
        deleted_platform_i: usize,
    ) {
        let action_result = match &mut self.ty {
            EntityType::Fighter(fighter) => fighter.get_player_mut().platform_deleted(
                entities,
                entity_defs,
                surfaces,
                deleted_platform_i,
                &self.state,
            ),
            _ => None,
        };
        match action_result {
            Some(ActionResult::SetAction(action)) => {
                self.state.frame_no_restart = 0;
                self.state.frame = 0;
                self.state.action = action;
                self.state.hitlist.clear()
            }
            _ => {}
        }
    }

    pub fn frame_angle(&self, entity_def: &EntityDef, surfaces: &[Surface]) -> f32 {
        if let Some(entity_frame) = self.get_entity_frame(entity_def) {
            match &self.ty {
                EntityType::Fighter(fighter) => {
                    fighter.get_player().body.angle(entity_frame, surfaces)
                }
                EntityType::Item(item) => item.body.angle(entity_frame, surfaces),
                EntityType::TorielOven(toriel_oven) => {
                    toriel_oven.body.angle(entity_frame, surfaces)
                }
                EntityType::Projectile(projectile) => projectile.angle,
                EntityType::TorielFireball(_) => 0.0,
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
                if let &mut CollisionBoxRole::Hit(ref mut hitbox) = &mut colbox.role {
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
            EntityType::Fighter(fighter) => Some(fighter.get_player().id),
            EntityType::Item(item) => item.owner_id,
            EntityType::Projectile(projectile) => projectile.owner_id,
            EntityType::TorielFireball(projectile) => projectile.owner_id,
            EntityType::TorielOven(toriel_oven) => toriel_oven.owner_id,
        }
    }

    pub fn cam_area(
        &self,
        cam_max: &Rect,
        entities: &Entities,
        entity_defs: &KeyedContextVec<EntityDef>,
        surfaces: &[Surface],
    ) -> Option<Rect> {
        match &self.ty {
            EntityType::Fighter(fighter) => {
                fighter
                    .get_player()
                    .cam_area(&self.state, cam_max, entities, entity_defs, surfaces)
            }
            _ => None,
        }
    }

    pub fn item_grab_box(
        &self,
        entities: &Entities,
        entity_defs: &KeyedContextVec<EntityDef>,
        surfaces: &[Surface],
    ) -> Option<Rect> {
        let (x, y) = self.public_bps_xy(entities, entity_defs, surfaces);
        let entity_def = &entity_defs[self.state.entity_def_key.as_ref()];
        let frame = self.relative_frame(entity_def, surfaces);
        frame.item_grab_box.map(|rect| rect.offset(x, y))
    }

    pub fn hitlist(&self) -> &[EntityKey] {
        &self.state.hitlist
    }

    pub fn debug_print(
        &self,
        entities: &KeyedContextVec<EntityDef>,
        player_input: Option<&PlayerInput>,
        debug: &DebugEntity,
        i: EntityKey,
    ) -> Vec<String> {
        let mut lines = vec![];
        if debug.action {
            lines.push(self.state.debug_string(entities, i));
        }

        if debug.physics {
            if let Some(body) = self.body() {
                lines.push(body.debug_string(i));
            }
        }

        match &self.ty {
            EntityType::Fighter(fighter) => lines.extend_from_slice(
                &fighter
                    .get_player()
                    .debug_print(player_input.unwrap(), debug, i),
            ),
            EntityType::Projectile(projectile) => {
                lines.extend_from_slice(&projectile.debug_print(debug, i))
            }
            _ => {}
        }

        lines
    }

    pub fn body(&self) -> Option<&Body> {
        match &self.ty {
            EntityType::Fighter(fighter) => Some(&fighter.get_player().body),
            EntityType::Item(item) => Some(&item.body),
            _ => None,
        }
    }

    pub fn body_mut(&mut self) -> Option<&mut Body> {
        match &mut self.ty {
            EntityType::Fighter(fighter) => Some(&mut fighter.get_player_mut().body),
            EntityType::Item(item) => Some(&mut item.body),
            _ => None,
        }
    }

    pub fn team(&self) -> usize {
        match &self.ty {
            EntityType::Fighter(fighter) => fighter.get_player().team,
            EntityType::Item(_) => 0,
            EntityType::Projectile(_) => 0,
            EntityType::TorielFireball(_) => 0,
            EntityType::TorielOven(_) => 0,
        }
    }

    pub fn particles(&self) -> Vec<Particle> {
        match &self.ty {
            EntityType::Fighter(fighter) => fighter.get_player().particles.clone(),
            _ => vec![],
        }
    }

    pub fn render(
        &self,
        selected_colboxes: HashSet<usize>,
        entity_selected: bool,
        debug: DebugEntity,
        entity_i: EntityKey,
        entity_history: &[Entities],
        entities: &Entities,
        entity_defs: &KeyedContextVec<EntityDef>,
        surfaces: &[Surface],
    ) -> RenderEntity {
        let fighter_color = graphics::get_team_color3(self.team());
        let entity_def = &entity_defs[self.state.entity_def_key.as_ref()];

        let vector_arrows = if let Some(player) = &self.ty.get_player() {
            player.vector_arrows(&debug)
        } else {
            vec![]
        };

        let mut frames = vec![self.render_frame(entities, entity_defs, surfaces)];
        let range = entity_history.len().saturating_sub(5)..entity_history.len();
        for entities in entity_history[range].iter().rev() {
            if let Some(entity) = entities.get(entity_i) {
                // handle deleted frames by just skipping it, only encountered when the editor is used.
                if entity_def.actions[entity.state.action.as_ref()]
                    .frames
                    .len()
                    > entity.state.frame as usize
                {
                    frames.push(entity.render_frame(entities, entity_defs, surfaces));
                }
            }
        }

        let render_type = match &self.ty {
            EntityType::Fighter(fighter) => RenderEntityType::Player(fighter.get_player().render(
                entities,
                entity_defs,
                surfaces,
                &self.state,
            )),
            EntityType::Projectile(_) => RenderEntityType::Projectile,
            EntityType::TorielFireball(_) => RenderEntityType::Projectile,
            EntityType::Item(_) => RenderEntityType::Item,
            EntityType::TorielOven(_) => RenderEntityType::Projectile,
        };

        let visible = match &self.ty {
            EntityType::Item(item) => {
                !item.body.is_item_held() || item.held_render_angle(entities, entity_defs).is_some()
            }
            _ => true,
        };

        RenderEntity {
            frame_data: self.relative_frame(entity_def, surfaces),
            particles: self.particles(),
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

    fn render_frame(
        &self,
        entities: &Entities,
        entity_defs: &KeyedContextVec<EntityDef>,
        surfaces: &[Surface],
    ) -> RenderEntityFrame {
        let entity_def = &entity_defs[self.state.entity_def_key.as_ref()];
        RenderEntityFrame {
            entity_def_key: self.state.entity_def_key.clone(),
            model_name: entity_def.name.clone(),
            frame_bps: self.public_bps_xy(entities, entity_defs, surfaces),
            render_bps: self.public_bps_xyz(entities, entity_defs, surfaces),
            ecb: self.body().map(|x| x.ecb.clone()),
            frame: self.state.frame as usize,
            frame_no_restart: self.state.frame_no_restart as usize,
            action: self.state.action.clone(),
            face_right: self.face_right(),
            frame_angle: self.frame_angle(entity_def, surfaces),
            render_angle: self.render_angle(entities, entity_defs, surfaces),
        }
    }

    fn render_angle(
        &self,
        entities: &Entities,
        entity_defs: &KeyedContextVec<EntityDef>,
        surfaces: &[Surface],
    ) -> Quaternion<f32> {
        let entity_def = &entity_defs[self.state.entity_def_key.as_ref()];
        match &self.ty {
            EntityType::Item(item) => {
                if let Some(render_angle) = item.held_render_angle(entities, entity_defs) {
                    render_angle
                } else {
                    Quaternion::from_angle_z(Rad(self.frame_angle(entity_def, surfaces)))
                }
            }
            _ => Quaternion::from_angle_z(Rad(self.frame_angle(entity_def, surfaces))),
        }
    }

    fn process_action_result(
        &mut self,
        context: &mut StepContext,
        action_result: Option<ActionResult>,
    ) {
        match action_result {
            Some(ActionResult::SetAction(action)) => {
                if self.state.action != action {
                    self.state.frame_no_restart = 0;
                } else {
                    self.state.frame_no_restart += 1;
                }
                self.state.frame = 0;
                self.state.action = action;
                self.state.hitlist.clear()
            }
            Some(ActionResult::SetActionKeepFrame(action)) => {
                self.state.frame_no_restart += 1;
                self.state.action = action;
                self.state.hitlist.clear()
            }
            Some(ActionResult::SetFrame(frame)) => {
                self.state.frame = frame;
                if self.state.past_last_frame(context.entity_def) {
                    let next_action = self.action_expired(context);
                    match next_action {
                        Some(ActionResult::SetAction(_)) | None => {
                            self.process_action_result(context, next_action)
                        }
                        _ => {}
                    }
                }

                self.state.frame_no_restart += 1;
            }
            None => {}
        }
    }
}

pub struct RenderEntity {
    pub render_type: RenderEntityType,
    pub visible: bool,
    pub debug: DebugEntity,
    /// Gauranteed to have at least one value (the current frame), and can have up to and including 10 values
    pub frames: Vec<RenderEntityFrame>,
    pub frame_data: ActionFrame,
    pub fighter_color: [f32; 3],
    pub entity_selected: bool,
    pub selected_colboxes: HashSet<usize>,
    pub vector_arrows: Vec<VectorArrow>,
    pub particles: Vec<Particle>,
}

pub enum RenderEntityType {
    Player(RenderPlayer),
    Projectile,
    Item,
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
            RenderDebugType::Normal => RenderDebugType::NormalAndDebug,
            RenderDebugType::NormalAndDebug => RenderDebugType::Debug,
            RenderDebugType::Debug => RenderDebugType::DebugOnionSkin,
            RenderDebugType::DebugOnionSkin => RenderDebugType::Normal,
        };
    }

    pub fn normal(&self) -> bool {
        match self {
            RenderDebugType::Normal => true,
            RenderDebugType::NormalAndDebug => true,
            RenderDebugType::Debug => false,
            RenderDebugType::DebugOnionSkin => false,
        }
    }

    pub fn debug(&self) -> bool {
        match self {
            RenderDebugType::Normal => false,
            RenderDebugType::NormalAndDebug => true,
            RenderDebugType::Debug => true,
            RenderDebugType::DebugOnionSkin => true,
        }
    }

    pub fn onion_skin(&self) -> bool {
        match self {
            RenderDebugType::Normal => false,
            RenderDebugType::NormalAndDebug => false,
            RenderDebugType::Debug => false,
            RenderDebugType::DebugOnionSkin => true,
        }
    }
}

// TODO: Split player specific debug into a DebugPlayer stored in Player
#[derive(Copy, Clone, Default, Serialize, Deserialize)]
pub struct DebugEntity {
    pub render: RenderDebugType,
    pub physics: bool,
    pub input: bool,
    pub input_diff: bool,
    pub action: bool,
    pub frame: bool,
    pub stick_vector: bool,
    pub c_stick_vector: bool,
    pub di_vector: bool,
    pub hitbox_vectors: bool,
    pub ecb: bool,
    pub cam_area: bool,
    pub item_grab_area: bool,
}

impl DebugEntity {
    pub fn all() -> Self {
        DebugEntity {
            render: RenderDebugType::NormalAndDebug,
            physics: true,
            input: true,
            input_diff: true,
            action: true,
            frame: true,
            stick_vector: true,
            c_stick_vector: true,
            di_vector: true,
            hitbox_vectors: true,
            ecb: true,
            cam_area: true,
            item_grab_area: true,
        }
    }
}

pub struct RenderEntityFrame {
    pub entity_def_key: String,
    pub model_name: String,
    pub frame_bps: (f32, f32),
    pub render_bps: (f32, f32, f32),
    pub ecb: Option<ECB>,
    pub frame: usize,
    pub frame_no_restart: usize,
    pub action: String,
    pub face_right: bool,
    pub frame_angle: f32,
    pub render_angle: Quaternion<f32>,
}

pub struct VectorArrow {
    pub x: f32,
    pub y: f32,
    pub color: [f32; 4],
}

pub struct StepContext<'a> {
    pub entity_key: EntityKey,
    pub input: &'a PlayerInput,
    pub entities: &'a Entities,
    pub entity_defs: &'a KeyedContextVec<EntityDef>,
    pub entity_def: &'a EntityDef,
    pub stage: &'a Stage,
    pub surfaces: &'a [Surface],
    pub rng: &'a mut ChaChaRng,
    pub new_entities: &'a mut Vec<Entity>,
    pub messages: &'a mut Vec<Message>,
    pub audio: &'a mut Audio,
    pub delete_self: bool,
}

pub struct Message {
    pub recipient: EntityKey,
    pub contents: MessageContents,
}

#[allow(dead_code)]
pub enum MessageContents {
    Player(MessagePlayer),
    Item(MessageItem),
    TorielOven(MessageTorielOven),
}

#[must_use]
pub enum ActionResult {
    SetAction(String),
    SetActionKeepFrame(String),
    SetFrame(i64),
}

impl ActionResult {
    fn set_action_keep_frame<T: Into<&'static str>>(action: T) -> Option<ActionResult> {
        Some(ActionResult::SetActionKeepFrame(action.into().to_string()))
    }

    fn set_action<T: Into<&'static str>>(action: T) -> Option<ActionResult> {
        Some(ActionResult::SetAction(action.into().to_string()))
    }

    fn set_frame(action: i64) -> Option<ActionResult> {
        Some(ActionResult::SetFrame(action))
    }
}
