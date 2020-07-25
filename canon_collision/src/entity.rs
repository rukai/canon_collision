use crate::player::{Player, DebugPlayer};
use crate::rules::Goal;
use crate::simple_projectile::SimpleProjectile;
use crate::collision::CollisionResult;

use canon_collision_lib::geometry::Rect;
use canon_collision_lib::fighter::{Fighter, ActionFrame, CollisionBoxRole};
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::stage::{Stage, Surface};

use treeflection::{Node, NodeRunner, NodeToken, KeyedContextVec};
use rand_chacha::ChaChaRng;

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
            Entity::SimpleProjectile (_) => "unimplemented",
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

    pub fn debug_print(&self, fighters: &KeyedContextVec<Fighter>, player_input: &PlayerInput, debug: &DebugPlayer, index: usize) -> Vec<String> {
        match self {
            Entity::Player (player) => player.debug_print(fighters, player_input, debug, index),
            _ => vec!()
        }
    }
}

pub struct StepContext<'a> {
    pub input:    &'a PlayerInput,
    pub entities: &'a [Entity],
    pub fighters: &'a KeyedContextVec<Fighter>,
    pub fighter:  &'a Fighter,
    pub stage:    &'a Stage,
    pub surfaces: &'a [Surface],
    pub rng:      &'a mut ChaChaRng,
}
