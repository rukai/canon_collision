#[repr(u64)]
#[derive(Clone, PartialEq, Debug, EnumString, IntoStaticStr, EnumIter, Serialize, Deserialize)]
pub enum ItemAction {
    Spawn,
    Idle,
    Fall,
    Held,
    Thrown,
    Dropped,
}
