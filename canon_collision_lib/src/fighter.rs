use treeflection::{Node, NodeRunner, NodeToken, ContextVec};
use strum::IntoEnumIterator;
use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashMap;

use crate::files::engine_version;

impl Default for Fighter {
    fn default() -> Fighter {
        let action_def = ActionDef {
            frames: ContextVec::from_vec(vec!(ActionFrame::default())),
            iasa:   0,
        };
        let mut actions: ContextVec<ActionDef> = ContextVec::new();
        for action in Action::iter() {
            let mut action_def_new = action_def.clone();
            action_def_new.frames[0].pass_through = match action {
                Action::Damage     | Action::DamageFly |
                Action::DamageFall | Action::AerialDodge |
                Action::Uair       | Action::Dair |
                Action::Fair       | Action::Bair |
                Action::Nair => false,
                _ => true
            };
            action_def_new.frames[0].ledge_cancel = match action {
                Action::Teeter | Action::TeeterIdle |
                Action::RollB  | Action::RollF |
                Action::TechB  | Action::TechF |
                Action::MissedTechGetupB | Action::MissedTechGetupF |
                Action::SpotDodge
                  => false,
                _ => true
            };
            action_def_new.frames[0].use_platform_angle = match action {
                Action::Dsmash | Action::Fsmash |
                Action::Dtilt  | Action::MissedTechIdle
                  => true,
                _ => false
            };
            actions.push(action_def_new);
        }

        Fighter {
            engine_version: engine_version(),

            // css render
            name:       "Base Fighter".to_string(),
            css_action: Action::Idle.to_u64().unwrap(),
            css_scale:  1.0,

            // in game attributes
            air_jumps:                1,
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
            actions:                  actions,
            u32s:                     HashMap::new(),
            f32s:                     HashMap::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct Fighter {
    pub engine_version: u64,

    // css render
    pub name:       String,
    pub css_action: u64,
    pub css_scale:  f32,

    // in game attributes
    pub air_jumps:                u64,
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
    pub u32s:                     HashMap<String, u32>,
    pub f32s:                     HashMap<String, f32>,
    pub actions:                  ContextVec<ActionDef>,
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

#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct ActionDef {
    pub frames: ContextVec<ActionFrame>,
    pub iasa:   i64,
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum VelModify {
    Set (f32),
    Add (f32),
    None,
}

impl Default for VelModify {
    fn default() -> Self {
        VelModify::None
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct ActionFrame {
    pub ecb:                 ECB,
    pub colboxes:            ContextVec<CollisionBox>,
    pub item_hold_x:         f32,
    pub item_hold_y:         f32,
    pub grab_hold_x:         f32,
    pub grab_hold_y:         f32,
    pub pass_through:        bool, // only used on aerial actions
    pub ledge_cancel:        bool, // only used on ground actions
    pub use_platform_angle:  bool, // only used on ground actions
    // TODO: pub land_cancel: bool // only used on aerial attacks
    pub ledge_grab_box:      Option<LedgeGrabBox>,
    pub force_hitlist_reset: bool,
    /// Affects the next frames velocity
    pub x_vel_modify: VelModify,
    /// Affects the next frames velocity
    pub y_vel_modify: VelModify,
    /// Does not affect the next frames velocity
    pub x_vel_temp: f32,
    /// Does not affect the next frames velocity
    pub y_vel_temp: f32,
}

impl Default for ActionFrame {
    fn default() -> ActionFrame {
        ActionFrame {
            colboxes:            ContextVec::new(),
            ecb:                 ECB::default(),
            item_hold_x:         4.0,
            item_hold_y:         11.0,
            grab_hold_x:         4.0,
            grab_hold_y:         11.0,
            x_vel_modify:        VelModify::None,
            y_vel_modify:        VelModify::None,
            x_vel_temp:          0.0,
            y_vel_temp:          0.0,
            pass_through:        true,
            ledge_cancel:        true,
            use_platform_angle:  false,
            ledge_grab_box:      None,
            force_hitlist_reset: false,
        }
    }
}

impl ActionFrame {
    pub fn get_hitboxes(&self) -> Vec<&CollisionBox> {
        let mut result: Vec<&CollisionBox> = self.colboxes.iter().collect();
        result.retain(|x| matches!(x.role, CollisionBoxRole::Hit(_)));
        result
    }

    pub fn get_hurtboxes(&self) -> Vec<&CollisionBox> {
        let mut result: Vec<&CollisionBox> = self.colboxes.iter().collect();
        result.retain(|x| matches!(x.role, CollisionBoxRole::Hurt(_)));
        result
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub struct LedgeGrabBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl Default for LedgeGrabBox {
    fn default() -> LedgeGrabBox {
        LedgeGrabBox {
            x1: 0.0,
            y1: 12.0,
            x2: 14.0,
            y2: 22.0,
        }
    }
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

#[repr(u64)]
#[derive(Clone, PartialEq, Debug, ToPrimitive, FromPrimitive, EnumIter, IntoStaticStr, Serialize, Deserialize, Node)]
pub enum Action {
    // Idle
    Spawn,
    ReSpawn,
    ReSpawnIdle,
    Idle,
    Crouch,
    LedgeIdle,
    Teeter,
    TeeterIdle,
    MissedTechIdle,

    // Movement
    Fall,
    AerialFall,
    Land,
    JumpSquat,
    JumpF,
    JumpB,
    JumpAerialF,
    JumpAerialB,
    TiltTurn,
    RunTurn,
    SmashTurn,
    Dash,
    Run,
    RunEnd,
    Walk,
    PassPlatform,
    Damage,
    DamageFly,
    DamageFall,
    LedgeGrab,
    LedgeJump,
    LedgeJumpSlow,
    LedgeGetup,
    LedgeGetupSlow,

    // Defense
    PowerShield,
    ShieldOn,
    Shield,
    ShieldOff,
    RollF,
    RollB,
    SpotDodge,
    AerialDodge,
    SpecialFall,
    SpecialLand,
    TechF,
    TechN,
    TechB,
    MissedTechGetupF,
    MissedTechGetupN,
    MissedTechGetupB,
    Rebound, // State after clang
    LedgeRoll,
    LedgeRollSlow,

    // Vulnerable
    ShieldBreakFall,
    ShieldBreakGetup,
    Stun,
    MissedTechStart,

    // Attacks
    Jab,
    Jab2,
    Jab3,
    Utilt,
    Dtilt,
    Ftilt,
    DashAttack,
    Usmash,
    Dsmash,
    Fsmash,
    Grab,
    DashGrab,
    LedgeAttack,
    LedgeAttackSlow,
    MissedTechAttack,

    // Aerials
    Uair,
    Dair,
    Fair,
    Bair,
    Nair,
    UairLand,
    DairLand,
    FairLand,
    BairLand,
    NairLand,

    // Taunts
    TauntUp,
    TauntDown,
    TauntLeft,
    TauntRight,

    // Crouch
    CrouchStart,
    CrouchEnd,

    Eliminated,
    DummyFramePreStart,
}

impl Default for Action {
    fn default() -> Action {
        Action::Spawn
    }
}

impl Action {
    pub fn is_air_attack(&self) -> bool {
        match self {
            &Action::Fair | &Action::Bair |
            &Action::Uair | &Action::Dair |
            &Action::Nair
              => true,
            _ => false
        }
    }

    pub fn is_attack_land(&self) -> bool {
        match self {
            &Action::FairLand | &Action::BairLand |
            &Action::UairLand | &Action::DairLand |
            &Action::NairLand
              => true,
            _ => false
        }
    }

    pub fn is_land(&self) -> bool {
        match self {
            &Action::FairLand | &Action::BairLand |
            &Action::UairLand | &Action::DairLand |
            &Action::NairLand | &Action::SpecialLand |
            &Action::Land
              => true,
            _ => false
        }
    }

    pub fn action_index_to_string(action_index: usize) -> String {
        match Action::from_u64(action_index as u64) {
            Some(action) => {
                let action: &str = action.into();
                action.to_string()
            }
            None => format!("{}", action_index),
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

    /// Warning: panics when not a hitbox
    pub fn hitbox_ref(&self) -> &HitBox {
        match &self.role {
            &CollisionBoxRole::Hit (ref hitbox) => hitbox,
            _ => panic!("Called hitbox_ref on a CollisionBox that is not a HitBox")
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

// TODO: Pretty sure I should delete all variants except Hit and hurt
#[derive(Clone, Serialize, Deserialize, Node)]
pub enum CollisionBoxRole {
    Hurt (HurtBox), // a target
    Hit  (HitBox),  // a launching attack
    Grab,           // a grabbing attack
    Intangible,     // cannot be interacted with rendered transparent with normal outline
    IntangibleItem, // cannot be interacted with rendered as a grey surface with no outline
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
