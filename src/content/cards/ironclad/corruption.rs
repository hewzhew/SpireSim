use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Corruption,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Corruption,
        name: "Corruption",
        card_type: CardType::Power,
        rarity: CardRarity::Rare,
        cost: 3,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn corruption_play(state: &CombatState, _card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    // In Java, the card checks `if (!powerExists)` directly in `use()`.
    if state.get_power(0, crate::content::powers::PowerId::Corruption) == 0 {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::Corruption,
                amount: -1,
            },
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}

/// Mimics the `ApplyPowerAction` constructor: reduce skill costs in the four
/// Java card groups before executing the queued power application.
pub fn corruption_on_apply(state: &mut CombatState) {
    let is_skill = |id| {
        crate::content::cards::get_card_definition(id).card_type
            == crate::content::cards::CardType::Skill
    };
    for c in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.draw_pile.iter_mut())
        .chain(state.zones.discard_pile.iter_mut())
        .chain(state.zones.exhaust_pile.iter_mut())
    {
        if is_skill(c.id) {
            c.modify_cost_for_combat_java(-9);
        }
    }
}

/// Mimics `CorruptionPower.onCardDraw(AbstractCard)`: Reduces cost for turn for newly drawn skills
/// (E.g. effectively overriding Snecko Eye's previous draw cost adjustments).
pub fn corruption_on_card_draw(_state: &CombatState, card: &mut CombatCard) {
    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type == crate::content::cards::CardType::Skill {
        card.set_cost_for_turn_java(0);
    }
}

/// The Corruption branch of AbstractPlayer.onCardDrawOrDiscard. This runs
/// after each card movement, on the cards that are still in hand.
pub fn corruption_on_draw_or_discard(state: &mut CombatState) {
    if crate::content::powers::store::has_power(
        state,
        0,
        crate::content::powers::PowerId::Corruption,
    ) {
        corruption_refresh_hand_costs(&mut state.zones.hand);
    }
}

pub(crate) fn corruption_refresh_hand_costs<'a>(
    cards: impl IntoIterator<Item = &'a mut CombatCard>,
) {
    for card in cards {
        if crate::content::cards::get_card_definition(card.id).card_type == CardType::Skill
            && card.cost_for_turn_java() != 0
        {
            card.modify_cost_for_combat_java(-9);
        }
    }
}

/// Mimics `CorruptionPower.onUseCard(AbstractCard, UseCardAction)`: Forces skills to exhaust.
pub fn corruption_on_use_card(state: &CombatState, card: &CombatCard, exhaust_override: &mut bool) {
    // Only active when the player actually has the Corruption power
    let has_corruption = state.entities.power_db.get(&0).is_some_and(|powers| {
        powers
            .iter()
            .any(|p| p.power_type == crate::content::powers::PowerId::Corruption)
    });
    if !has_corruption {
        return;
    }
    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type == crate::content::cards::CardType::Skill {
        *exhaust_override = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::action_handlers::cards;
    use crate::runtime::combat::{Power, PowerPayload};

    #[test]
    fn corruption_refresh_follows_draw_discard_and_exhaust_but_preserves_already_free_skills() {
        for active in [false, true] {
            for event in ["draw", "discard", "exhaust"] {
                let mut state = crate::test_support::blank_test_combat();
                if active {
                    crate::content::powers::store::set_powers_for(
                        &mut state,
                        0,
                        vec![Power {
                            power_type: crate::content::powers::PowerId::Corruption,
                            instance_id: None,
                            amount: -1,
                            extra_data: 0,
                            payload: PowerPayload::None,
                            just_applied: false,
                        }],
                    );
                }
                let paid = CombatCard::for_scenario(CardId::InfernalBlade, 1);
                let mut free = CombatCard::for_scenario(CardId::Shockwave, 2);
                free.set_cost_for_turn_java(0);
                let attack = CombatCard::for_scenario(CardId::Strike, 3);
                let lineage = attack.lineage;
                state.zones.hand = vec![paid, free, attack];
                match event {
                    "draw" => {
                        state.zones.draw_pile =
                            vec![CombatCard::for_scenario(CardId::Strike, 4)].into();
                        cards::handle_draw_cards(1, &mut state);
                    }
                    "discard" => cards::handle_discard_card(lineage, &mut state),
                    "exhaust" => cards::handle_exhaust_card(
                        lineage,
                        crate::state::PileType::Hand,
                        &mut state,
                    ),
                    _ => unreachable!(),
                }
                assert_eq!(
                    state.zones.hand[0].combat_cost_without_turn_override_java(),
                    if active { 0 } else { 1 },
                    "{event}"
                );
                assert_eq!(
                    state.zones.hand[1].combat_cost_without_turn_override_java(),
                    2,
                    "{event}"
                );
                assert_eq!(state.zones.hand[1].cost_for_turn_java(), 0, "{event}");
            }
        }
    }
}
