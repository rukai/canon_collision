#[repr(u64)]
#[derive(Clone, PartialEq, Debug, EnumString, IntoStaticStr, EnumIter, Serialize, Deserialize)]
pub enum TorielOvenAction {
    EarlyEnd,
    Attack,
    AttackExtended,
}
