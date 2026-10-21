use crate::runtime::combat::{CombatState, MetaChange};

use super::super::types::{CombatMasterDeckKey, CombatMetaChangeKey, CombatMetaKey};

pub(super) fn meta_key(combat: &CombatState) -> CombatMetaKey {
    let meta = &combat.meta;
    CombatMetaKey {
        ascension_level: meta.ascension_level,
        player_class: meta.player_class.clone(),
        is_boss_fight: meta.is_boss_fight,
        is_elite_fight: meta.is_elite_fight,
        master_deck_snapshot: CombatMasterDeckKey::new(&meta.master_deck_snapshot),
        next_master_deck_ordinal: meta
            .master_deck_lineage_allocator
            .next_acquisition_ordinal(),
        meta_changes: meta.meta_changes.iter().map(meta_change_key).collect(),
    }
}

fn meta_change_key(change: &MetaChange) -> CombatMetaChangeKey {
    match change {
        MetaChange::AddCardToMasterDeck(card_id) => {
            CombatMetaChangeKey::AddCardToMasterDeck(card_id.clone())
        }
        MetaChange::ModifyCardMisc {
            card_lineage,
            amount,
        } => CombatMetaChangeKey::ModifyCardMisc {
            card_lineage: *card_lineage,
            amount: *amount,
        },
        MetaChange::UpgradeMasterDeckCard { card_lineage } => {
            CombatMetaChangeKey::UpgradeMasterDeckCard {
                card_lineage: *card_lineage,
            }
        }
    }
}
