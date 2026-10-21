#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatQueuedActionKey {
    pub(crate) canonical_payload: String,
}
