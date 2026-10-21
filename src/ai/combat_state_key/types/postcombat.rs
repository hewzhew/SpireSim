use super::*;
use crate::runtime::combat::CardLineageId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct StablePostcombatPlayerKey {
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub relics: String,
    pub energy_master: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct StableMetaKey {
    pub player_class: String,
    pub ascension_level: u8,
    pub is_boss_fight: bool,
    pub is_elite_fight: bool,
    pub next_master_deck_ordinal: u32,
    pub master_deck_snapshot: CombatMasterDeckKey,
    pub meta_changes: Vec<StableMetaChangeKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum StableMetaChangeKey {
    AddCardToMasterDeck(crate::runtime::combat::MasterDeckAcquisition),
    ModifyCardMisc {
        card_lineage: CardLineageId,
        amount: i32,
    },
    UpgradeMasterDeckCard {
        card_lineage: CardLineageId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct StablePostcombatRuntimeKey {
    pub pending_rewards: Vec<StableRewardItemKey>,
    pub combat_mugged: bool,
    pub combat_smoked: bool,
}

impl StablePostcombatPlayerKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "hp:{}:max_hp:{}:gold:{}:relics:{}:energy_master:{}",
            self.current_hp, self.max_hp, self.gold, self.relics, self.energy_master,
        )
    }
}

impl StableMetaKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "class:{}:asc:{}:boss:{}:elite:{}:next_master:{}:deck:{}:changes:{}",
            self.player_class,
            self.ascension_level,
            self.is_boss_fight,
            self.is_elite_fight,
            self.next_master_deck_ordinal,
            serde_json::to_string(&self.master_deck_snapshot).expect("master deck key serializes"),
            join_diagnostic_strings(&self.meta_changes),
        )
    }
}

impl StableMetaChangeKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        match self {
            StableMetaChangeKey::AddCardToMasterDeck(card) => format!(
                "add_master:{}",
                serde_json::to_string(card).expect("acquisition serializes")
            ),
            StableMetaChangeKey::ModifyCardMisc {
                card_lineage,
                amount,
            } => {
                format!("modify_misc:{card_lineage}:{amount}")
            }
            StableMetaChangeKey::UpgradeMasterDeckCard { card_lineage } => {
                format!("upgrade_master:{card_lineage}")
            }
        }
    }
}

impl StablePostcombatRuntimeKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "pending_rewards:{}:mugged:{}:smoked:{}",
            join_diagnostic_strings(&self.pending_rewards),
            self.combat_mugged,
            self.combat_smoked,
        )
    }
}
