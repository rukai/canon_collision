pub mod item;
pub mod player;
pub mod projectile;
pub mod toriel;
pub mod dave;
pub mod toriel_fireball;

use strum::IntoEnumIterator;
use treeflection::{Node, NodeRunner, NodeToken, KeyedContextVec, ContextVec};

use crate::files::engine_version;
use crate::geometry::Rect;

use player::PlayerAction;
use projectile::ProjectileAction;
use item::ItemAction;
use toriel::TorielAction;
use dave::DaveAction;

use toriel_fireball::TorielFireballAction;

impl Default for EntityDef {
    fn default() -> EntityDef {
        EntityDef {
            engine_version: engine_version(),

            // css render
            name:       "Base Entity".into(),
            //css_action: PlayerAction::Idle.to_u64().unwrap(),
            css_action: "".into(),
            css_scale:  1.0,

            ty: EntityDefType::default(),

            // in game attributes
            // TODO: move into EntityDefType::Fighter
            weight:                   1.0, // weight = old value / 100
            gravity:                  -0.1,
            terminal_vel:             -2.0,
            fastfall_terminal_vel:    -3.0,
            jump_y_init_vel:          3.0,
            jump_y_init_vel_short:    2.0,
            jump_x_init_vel:          1.0,
            jump_x_term_vel:          1.5,
            jump_x_vel_ground_mult:   1.0,
            air_mobility_a:           0.04,
            air_mobility_b:           0.02,
            air_x_term_vel:           1.0,
            air_friction:             0.05,
            air_jump_x_vel:           1.0,
            air_jump_y_vel:           3.0,
            walk_init_vel:            0.2,
            walk_acc:                 0.1,
            walk_max_vel:             1.0,
            slow_walk_max_vel:        1.0,
            dash_init_vel:            2.0,
            dash_run_acc_a:           0.01,
            dash_run_acc_b:           0.2,
            dash_run_term_vel:        2.0,
            friction:                 0.1,
            aerialdodge_mult:         3.0,
            aerialdodge_drift_frame:  20,
            ledge_grab_x:             -2.0,
            ledge_grab_y:             -24.0,
            forward_roll:             false,
            backward_roll:            false,
            spot_dodge:               false,
            lcancel:                  None,
            shield:                   None,
            power_shield:             None,
            tech:                     None,
            missed_tech_forced_getup: Some(200),
            run_turn_flip_dir_frame:  30,
            tilt_turn_flip_dir_frame: 5,
            tilt_turn_into_dash_iasa: 5,
            actions:                  KeyedContextVec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct EntityDef {
    pub engine_version: u64,

    // css render
    pub name:       String,
    pub css_action: String,
    pub css_scale:  f32,

    pub ty: EntityDefType,

    // in game attributes
    // TODO: move into EntityDefType::Fighter
    pub weight:                   f32,
    pub gravity:                  f32,
    pub terminal_vel:             f32,
    pub fastfall_terminal_vel:    f32,
    pub jump_y_init_vel:          f32,
    pub jump_y_init_vel_short:    f32,
    pub jump_x_init_vel:          f32,
    pub jump_x_term_vel:          f32,
    pub jump_x_vel_ground_mult:   f32,
    pub air_mobility_a:           f32,
    pub air_mobility_b:           f32,
    pub air_x_term_vel:           f32,
    pub air_friction:             f32,
    pub air_jump_x_vel:           f32,
    pub air_jump_y_vel:           f32,
    pub walk_init_vel:            f32,
    pub walk_acc:                 f32,
    pub walk_max_vel:             f32,
    pub slow_walk_max_vel:        f32,
    pub dash_init_vel:            f32,
    pub dash_run_acc_a:           f32,
    pub dash_run_acc_b:           f32,
    pub dash_run_term_vel:        f32,
    pub friction:                 f32,
    pub aerialdodge_mult:         f32,
    pub aerialdodge_drift_frame:  u64,
    pub ledge_grab_x:             f32,
    pub ledge_grab_y:             f32,
    pub forward_roll:             bool,
    pub backward_roll:            bool,
    pub spot_dodge:               bool,
    pub lcancel:                  Option<LCancel>,
    pub shield:                   Option<Shield>,
    pub power_shield:             Option<PowerShield>,
    pub tech:                     Option<Tech>,
    pub missed_tech_forced_getup: Option<u64>,
    pub run_turn_flip_dir_frame:  u64,
    pub tilt_turn_flip_dir_frame: u64,
    pub tilt_turn_into_dash_iasa: u64,
    pub actions:                  KeyedContextVec<ActionDef>,
}

impl EntityDef {
    pub fn fighter(&self) -> Option<&Fighter> {
        if let EntityDefType::Fighter (fighter) = &self.ty {
            Some(fighter)
        } else {
            None
        }
    }

    pub fn cleanup(&mut self) {
        for action_name in self.ty.get_action_names() {
            let action_name = action_name.to_string();
            if !self.actions.contains_key(&action_name) {
                self.actions.push(action_name, ActionDef::default());
            }
        }

        let expected_action_names: Vec<_> = self.ty.get_action_names().collect();
        let check_action_names: Vec<_> = self.actions.key_iter().cloned().collect();
        for action_name in check_action_names {
            if !expected_action_names.contains(&action_name.as_str()) {
                self.actions.remove_by_key(action_name.as_str());
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum EntityDefType {
    Fighter (Fighter),
    Item,
    Projectile,
    TorielFireball,
}

impl EntityDefType {
    pub fn get_action_names(&self) -> Box<dyn Iterator<Item=&'static str>> {
        match self {
            EntityDefType::Fighter (fighter) =>
                Box::new(PlayerAction::iter().map(|x| x.into()).chain(
                    fighter.ty.get_action_names()
                )),
            EntityDefType::Item           => Box::new(ItemAction          ::iter().map(|x| x.into())),
            EntityDefType::Projectile     => Box::new(ProjectileAction    ::iter().map(|x| x.into())),
            EntityDefType::TorielFireball => Box::new(TorielFireballAction::iter().map(|x| x.into())),
        }
    }
}

impl Default for EntityDefType {
    fn default() -> Self {
        EntityDefType::Projectile
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct Fighter {
    pub ty:                       FighterType,
    pub air_jumps:                u64,
}

impl Default for Fighter {
    fn default() -> Self {
        Fighter {
            ty:                       FighterType::default(),
            air_jumps:                1,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum FighterType {
    Toriel,
    Dave,
}

impl Default for FighterType {
    fn default() -> Self {
        FighterType::Toriel
    }
}

impl FighterType {
    pub fn get_action_names(&self) -> Box<dyn Iterator<Item=&'static str>> {
        match self {
            FighterType::Toriel => Box::new(TorielAction::iter().map(|x| x.into())),
            FighterType::Dave   => Box::new(DaveAction  ::iter().map(|x| x.into())),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct Tech {
    pub active_window: u64,
    pub locked_window: u64,
}

impl Default for Tech {
    fn default() -> Self {
        Tech {
            active_window: 20,
            locked_window: 20
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct LCancel {
    pub active_window: u64,
    pub frame_skip:    u8,
    pub normal_land:   bool,
}

impl Default for LCancel {
    fn default() -> Self {
        LCancel {
            active_window: 7,
            frame_skip:    1,
            normal_land:   false
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct Shield {
    pub stick_lock: bool,
    pub stick_mult: f32,
    pub offset_x:   f32,
    pub offset_y:   f32,
    pub break_vel:  f32,
    pub scaling:    f32,
    pub hp_scaling: f32,
    pub hp_max:     f32,
    pub hp_regen:   f32,
    pub hp_cost:    f32,
}

impl Default for Shield {
    fn default() -> Self {
        Shield {
            stick_lock: false,
            stick_mult: 3.0,
            offset_x:   0.0,
            offset_y:   10.0,
            break_vel:  3.0,
            scaling:    10.0,
            hp_scaling: 1.0,
            hp_max:     60.0,
            hp_regen:   0.1,
            hp_cost:    0.3,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct PowerShield {
    pub reflect_window: Option<u64>,
    pub parry:          Option<PowerShieldEffect>,
    pub enemy_stun:     Option<PowerShieldEffect>,
}

#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct PowerShieldEffect {
    pub window:   u64,
    pub duration: u64
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct ActionDef {
    /// Invariant: Must always have one or more elements
    pub frames: ContextVec<ActionFrame>,
    pub iasa:   i64,
}

impl Default for ActionDef {
    fn default() -> ActionDef {
        ActionDef {
            iasa: 0,
            frames: ContextVec::from_vec(vec!(ActionFrame::default())),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct ActionFrame {
    pub ecb:                 ECB,
    pub colboxes:            ContextVec<CollisionBox>,
    pub item_hold:           Option<ItemHold>,
    pub grabbing_x:          f32,
    pub grabbing_y:          f32,
    pub grabbed_x:           f32,
    pub grabbed_y:           f32,
    pub pass_through:        bool, // only used on aerial actions
    pub ledge_cancel:        bool, // only used on ground actions
    pub use_platform_angle:  bool, // only used on ground actions
    // TODO: pub land_cancel: bool // only used on aerial attacks
    pub ledge_grab_box:      Option<Rect>,
    pub item_grab_box:       Option<Rect>,
    pub force_hitlist_reset: bool,
}

impl Default for ActionFrame {
    fn default() -> ActionFrame {
        ActionFrame {
            ecb:                 ECB::default(),
            colboxes:            ContextVec::new(),
            item_hold:           None,
            grabbing_x:          8.0,
            grabbing_y:          11.0,
            grabbed_x:           4.0,
            grabbed_y:           11.0,
            pass_through:        true,
            ledge_cancel:        true,
            use_platform_angle:  false,
            ledge_grab_box:      None,
            item_grab_box:       None,
            force_hitlist_reset: false,
        }
    }
}

impl ActionFrame {
    pub fn get_hitboxes(&self) -> Vec<&CollisionBox> {
        self.colboxes
            .iter()
            .filter(|x|
                match x.role {
                    CollisionBoxRole::Hit (_) => true,
                    CollisionBoxRole::Grab    => true,
                    _                         => false
                }
            )
            .collect()
    }

    pub fn get_hurtboxes(&self) -> Vec<&CollisionBox> {
        self.colboxes
            .iter()
            .filter(|x|
                match x.role {
                    CollisionBoxRole::Hurt (_) => true,
                    _                          => false
                }
            )
            .collect()
    }
}

#[derive(Default, Clone, Serialize, Deserialize, Node)]
pub struct ItemHold {
    pub translation_x: f32,
    pub translation_y: f32,
    pub translation_z: f32,
    pub quaternion_x: f32,
    pub quaternion_y: f32,
    pub quaternion_z: f32,
    pub quaternion_rotation: f32,
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct ECB {
    pub left:   f32,
    pub right:  f32,
    pub top:    f32,
    pub bottom: f32,
}

impl Default for ECB {
    fn default() -> ECB {
        ECB {
            top:    16.0,
            left:   -4.0,
            right:  4.0,
            bottom: 0.0,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct CollisionBox {
    pub point:  (f32, f32),
    pub radius: f32,
    pub role:   CollisionBoxRole,
}

impl CollisionBox {
    pub fn new(point: (f32, f32)) -> CollisionBox {
        CollisionBox {
            point:  point,
            radius: 1.0,
            role:   CollisionBoxRole::default()
        }
    }
}

impl Default for CollisionBox {
    fn default() -> CollisionBox {
        CollisionBox {
            point:  (0.0, 0.0),
            radius: 3.0,
            role:   CollisionBoxRole::default()
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum CollisionBoxRole {
    Hurt (HurtBox), // a target
    Hit  (HitBox),  // a launching attack
    Grab,           // a grabbing attack
    Invincible,     // cannot receive damage or knockback.
    Reflect,        // reflects projectiles
    Absorb,         // absorb projectiles
}

impl Default for CollisionBoxRole {
    fn default() -> CollisionBoxRole {
        CollisionBoxRole::Hurt ( HurtBox::default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Node)]
pub struct HurtBox {
    pub bkb_add:     f32,
    pub kbg_add:     f32,
    pub damage_mult: f32,
}

impl Default for HurtBox {
    fn default() -> HurtBox {
        HurtBox {
            bkb_add:     0.0,
            kbg_add:     0.0,
            damage_mult: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Node)]
pub struct HitBox {
    pub shield_damage:      f32,
    pub damage:             f32,
    pub bkb:                f32, // base knockback
    pub kbg:                f32, // knockback growth = old value / 100
    pub angle:              f32,
    pub hitstun:            HitStun,
    pub enable_clang:       bool,
    pub enable_rebound:     bool,
    pub effect:             HitboxEffect,
    pub enable_reverse_hit: bool, // if the defender is behind the attacker the direction is reversed.
    //pub team_funnel_angle: Option<f32>, // degrees to +- towards nearest teammate
}

impl Default for HitBox {
    fn default() -> HitBox {
        HitBox {
            shield_damage:      0.0,
            damage:             6.0,
            bkb:                40.0,
            kbg:                1.0,
            angle:              45.0,
            enable_clang:       true,
            enable_rebound:     true,
            enable_reverse_hit: true,
            hitstun:            HitStun::default(),
            effect:             HitboxEffect::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Node)]
pub enum HitStun {
    FramesTimesKnockback (f32),
    Frames (u64)
}

impl Default for HitStun {
    fn default() -> HitStun {
        HitStun::FramesTimesKnockback(0.5)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Node)]
pub enum HitboxEffect {
    Fire,
    Electric,
    Sleep,
    Reverse,
    Stun,
    Freeze,
    None,
}

impl Default for HitboxEffect {
    fn default() -> HitboxEffect {
        HitboxEffect::None
    }
}
