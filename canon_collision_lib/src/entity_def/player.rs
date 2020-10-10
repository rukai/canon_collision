#[repr(u64)]
#[derive(Clone, PartialEq, Debug, EnumString, IntoStaticStr, EnumIter, Serialize, Deserialize)]
pub enum PlayerAction {
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
    LedgeIdleChain, // LedgeIdle when another fighter is holding onto this fighter

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

    // Grabs
    Grab,
    DashGrab,
    GrabbingIdle,
    GrabbingEnd,
    GrabbedIdleAir,
    GrabbedIdle,
    GrabbedEnd,

    // Throws
    Uthrow,
    Dthrow,
    Fthrow,
    Bthrow,

    // Items
    ItemGrab,
    ItemEat,
    ItemThrowU,
    ItemThrowD,
    ItemThrowF,
    ItemThrowB,
    ItemThrowAirU,
    ItemThrowAirD,
    ItemThrowAirF,
    ItemThrowAirB,

    // Getup attacks
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

    // Specials
    UspecialGroundStart,
    UspecialAirStart,

    DspecialGroundStart,
    DspecialAirStart,

    SspecialGroundStart,
    SspecialAirStart,

    NspecialGroundStart,
    NspecialAirStart,

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

impl Default for PlayerAction {
    fn default() -> PlayerAction {
        PlayerAction::Spawn
    }
}

impl PlayerAction {
    pub fn is_air_attack(&self) -> bool {
        match self {
            &PlayerAction::Fair | &PlayerAction::Bair |
            &PlayerAction::Uair | &PlayerAction::Dair |
            &PlayerAction::Nair
              => true,
            _ => false
        }
    }

    pub fn is_attack_land(&self) -> bool {
        match self {
            &PlayerAction::FairLand | &PlayerAction::BairLand |
            &PlayerAction::UairLand | &PlayerAction::DairLand |
            &PlayerAction::NairLand
              => true,
            _ => false
        }
    }

    pub fn is_land(&self) -> bool {
        match self {
            &PlayerAction::FairLand | &PlayerAction::BairLand |
            &PlayerAction::UairLand | &PlayerAction::DairLand |
            &PlayerAction::NairLand | &PlayerAction::SpecialLand |
            &PlayerAction::Land
              => true,
            _ => false
        }
    }
}

