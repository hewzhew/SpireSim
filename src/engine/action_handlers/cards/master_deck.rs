use crate::content::{cards::CardId, relics::RelicId};
use crate::runtime::combat::{CombatState, MasterDeckAcquisition, MetaChange};
use crate::state::deck::{
    context::DeckContext,
    manager::{DeckAction, DeckManager},
};

/// AddCardToDeckAction / ShowCardAndObtainEffect settle during combat.
/// Only the permanent card is added; the battle piles are unchanged.
pub fn handle_add_card_to_master_deck(
    card_id: CardId,
    source: crate::EntityId,
    state: &mut CombatState,
) {
    let (monster_slot, monster) = state
        .entities
        .monsters
        .iter()
        .enumerate()
        .find(|(_, monster)| monster.id == source)
        .expect("master-deck grant requires its actual monster producer");
    let monster_id = monster.enemy_id;
    let context = DeckContext::from_relics(&state.entities.player.relics);
    let result = DeckManager::obtain_card(
        &context,
        card_id,
        &mut state.meta.master_deck_lineage_allocator,
        0,
    );
    for action in result.actions {
        match action {
            DeckAction::PreventObtain => {}
            DeckAction::GainGold(amount) => {
                crate::engine::action_handlers::damage::handle_gain_gold(amount, state)
            }
            DeckAction::GainMaxHp(amount) => {
                crate::engine::action_handlers::powers::handle_gain_max_hp(amount, state)
            }
            DeckAction::UpdateRelicCounter(id, counter) => {
                crate::engine::action_handlers::spawning::handle_update_relic_counter(
                    id, counter, state,
                );
                if id == RelicId::Omamori && counter == 0 {
                    crate::engine::action_handlers::spawning::handle_update_relic_used_up(
                        id, true, state,
                    );
                }
            }
            DeckAction::LoseMaxHp(_) | DeckAction::ReaddCardToMasterDeck(_) => {
                panic!("card obtain emitted a removal effect");
            }
        }
    }
    if result.final_cards.is_empty() {
        return;
    }
    let mut deck = state.meta.master_deck_snapshot.to_vec();
    for card in result.final_cards {
        assert!(
            !deck.iter().any(|existing| existing.lineage == card.lineage),
            "persistent card allocator reused a live lineage"
        );
        deck.push(card.clone());
        state
            .meta
            .meta_changes
            .push(MetaChange::AddCardToMasterDeck(MasterDeckAcquisition {
                card,
                monster_id,
                monster_slot: monster_slot as u32,
            }));
    }
    crate::content::relics::du_vu_doll::refresh_counters_from_deck(
        &deck,
        &mut state.entities.player.relics,
    );
    state.meta.master_deck_snapshot = deck.into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{monsters::EnemyId, relics::RelicState};
    use crate::runtime::combat::CombatCard;

    fn mass_combat() -> CombatState {
        let mut state = crate::test_support::blank_test_combat();
        let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
        mass.id = 42;
        state.entities.monsters = vec![mass];
        state.entities.player.current_hp = 30;
        state.entities.player.max_hp = 80;
        state.entities.player.gold = 100;
        let card = CombatCard::with_lineage(
            CardId::Strike,
            state.meta.master_deck_lineage_allocator.allocate(),
        );
        state.meta.master_deck_snapshot = vec![card.clone()].into();
        state.zones.hand = vec![card];
        state
    }

    #[test]
    fn implant_obtains_permanent_card_immediately_without_changing_battle_piles() {
        let mut state = mass_combat();
        let zones = state.zones.clone();
        handle_add_card_to_master_deck(CardId::Parasite, 42, &mut state);
        assert_eq!(state.zones, zones);
        assert_eq!(state.meta.master_deck_snapshot.len(), 2);
        let [MetaChange::AddCardToMasterDeck(receipt)] = state.meta.meta_changes.as_slice() else {
            panic!("one actual acquisition");
        };
        assert_eq!(receipt.monster_id, EnemyId::WrithingMass);
        assert_eq!(receipt.monster_slot, 0);
        assert_eq!(receipt.card, state.meta.master_deck_snapshot[1]);
        assert_eq!(receipt.card.id, CardId::Parasite);
        assert!(state
            .meta
            .master_deck_lineage_allocator
            .contains_allocated(receipt.card.lineage));
    }

    #[test]
    fn implant_omamori_prevents_all_obtain_hooks_and_identity_allocation() {
        let mut state = mass_combat();
        let mut omamori = RelicState::new(RelicId::Omamori);
        omamori.counter = 1;
        state.entities.player.relics = vec![
            omamori,
            RelicState::new(RelicId::DarkstonePeriapt),
            RelicState::new(RelicId::CeramicFish),
        ];
        let allocator = state.meta.master_deck_lineage_allocator.clone();
        handle_add_card_to_master_deck(CardId::Parasite, 42, &mut state);
        assert_eq!(state.meta.master_deck_snapshot.len(), 1);
        assert!(state.meta.meta_changes.is_empty());
        assert_eq!(state.meta.master_deck_lineage_allocator, allocator);
        assert_eq!(
            (
                state.entities.player.max_hp,
                state.entities.player.current_hp,
                state.entities.player.gold
            ),
            (80, 30, 100)
        );
        assert_eq!(state.entities.player.relics[0].counter, 0);
        assert!(state.entities.player.relics[0].used_up);
    }

    #[test]
    fn implant_applies_obtain_relic_resources_and_master_deck_callbacks_in_combat() {
        let mut state = mass_combat();
        state.entities.player.relics = [
            RelicId::DarkstonePeriapt,
            RelicId::CeramicFish,
            RelicId::BloodyIdol,
            RelicId::DuVuDoll,
        ]
        .into_iter()
        .map(RelicState::new)
        .collect();
        handle_add_card_to_master_deck(CardId::Parasite, 42, &mut state);
        assert_eq!(
            (
                state.entities.player.max_hp,
                state.entities.player.current_hp,
                state.entities.player.gold
            ),
            (86, 41, 109)
        );
        assert_eq!(state.entities.player.relics[3].counter, 1);
    }

    #[test]
    fn implant_never_reuses_removed_lineages_and_branch_allocation_is_isolated() {
        let mut state = mass_combat();
        let removed = state.meta.master_deck_lineage_allocator.allocate();
        let mut branch = state.clone();
        handle_add_card_to_master_deck(CardId::Parasite, 42, &mut branch);
        assert_eq!(state.meta.master_deck_snapshot.len(), 1);
        assert_eq!(
            state
                .meta
                .master_deck_lineage_allocator
                .next_acquisition_ordinal(),
            3
        );
        assert_ne!(branch.meta.master_deck_snapshot[1].lineage, removed);
        assert_ne!(
            crate::ai::combat_state_key::combat_exact_state_key(
                &crate::state::EngineState::CombatPlayerTurn,
                &state
            ),
            crate::ai::combat_state_key::combat_exact_state_key(
                &crate::state::EngineState::CombatPlayerTurn,
                &branch
            )
        );
        handle_add_card_to_master_deck(CardId::Parasite, 42, &mut state);
        assert_eq!(state, branch);
    }
}
