#[repr(u64)]
#[derive(Clone, PartialEq, Debug, EnumString, IntoStaticStr, EnumIter, Serialize, Deserialize)]
pub enum TorielFireballAction {
    Spawn,
    Travel,
    Hit,
}
