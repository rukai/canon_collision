#[repr(u64)]
#[derive(Clone, PartialEq, Debug, EnumString, IntoStaticStr, EnumIter, Serialize, Deserialize)]
pub enum TorielAction {
    DspecialGroundStart,
    DspecialAirStart,

    SspecialGroundStart,
    SspecialAirStart,

    NspecialGroundStart,
    NspecialAirStart,
}
