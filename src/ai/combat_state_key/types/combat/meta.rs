use crate::runtime::combat::MasterDeckAcquisition;
use crate::runtime::combat::{CardLineageId, MasterDeckSnapshot};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatMetaKey {
    pub(crate) ascension_level: u8,
    pub(crate) player_class: String,
    pub(crate) is_boss_fight: bool,
    pub(crate) is_elite_fight: bool,
    pub(crate) master_deck_snapshot: CombatMasterDeckKey,
    pub(crate) next_master_deck_ordinal: u32,
    pub(crate) meta_changes: Vec<CombatMetaChangeKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct CombatMasterDeckKey(MasterDeckSnapshot);

impl CombatMasterDeckKey {
    pub(crate) fn new(snapshot: &MasterDeckSnapshot) -> Self {
        Self(snapshot.clone())
    }
}

impl Hash for CombatMasterDeckKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.structural_hash());
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatMetaChangeKey {
    AddCardToMasterDeck(MasterDeckAcquisition),
    ModifyCardMisc {
        card_lineage: CardLineageId,
        amount: i32,
    },
    UpgradeMasterDeckCard {
        card_lineage: CardLineageId,
    },
}
