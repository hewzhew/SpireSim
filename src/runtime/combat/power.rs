use super::*;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub enum PowerPayload {
    #[default]
    None,
    Card(CombatCard),
    /// Original creature retained by a power such as Constricted.
    SourceEntity(crate::EntityId),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Power {
    pub power_type: PowerId,
    pub instance_id: Option<u32>,
    pub amount: i32,
    pub extra_data: i32,
    pub payload: PowerPayload,
    pub just_applied: bool,
}
