#[repr(u64)]
#[derive(Clone, PartialEq, Debug, EnumString, IntoStaticStr, EnumIter, Serialize, Deserialize)]
pub enum TorielAction {
    // Specials
    DspecialGroundStart,
    DspecialAirStart,

    SspecialGroundStart,
    SspecialAirStart,

    NspecialGroundStart,
    NspecialAirStart,

    // Throws
    Uthrow,
    Dthrow,
    Fthrow,
    Bthrow,
}
