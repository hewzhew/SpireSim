//! Reviewed delayed effects. These describe requests and installed powers,
//! not immediate draw, attack damage, or guaranteed combat outcomes.

use crate::content::cards::{CardId, CardType};
use serde::{Deserialize, Serialize};

pub const COMBUST_HP_LOSS_PER_APPLICATION_V1: i32 = 1;

/// Each application adds both amounts to one persistent power. At player turn
/// end, while monsters are not basically dead, it queues one combined HP loss
/// (Rupture eligible) before source-less THORNS damage to all enemies. Block,
/// Intangible, death and queue clearing are resolved by the existing runtime.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardEndOfTurnHpLossAndDamageV1 {
    pub damage_per_enemy_per_application: i32,
    pub hp_loss_per_application: i32,
}

pub fn card_end_of_turn_hp_loss_and_damage_v1(
    card: CardId,
    effective_magic: i32,
) -> Option<CardEndOfTurnHpLossAndDamageV1> {
    (card == CardId::Combust).then_some(CardEndOfTurnHpLossAndDamageV1 {
        damage_per_enemy_per_application: effective_magic,
        hp_loss_per_application: COMBUST_HP_LOSS_PER_APPLICATION_V1,
    })
}

/// Installs additive draw per card-exhaust event while the combat is ongoing.
/// It does not draw when played; No Draw and hand caps still apply to requests.
pub fn card_exhaust_draw_power_v1(card: CardId, effective_magic: i32) -> Option<i32> {
    (card == CardId::DarkEmbrace).then_some(effective_magic)
}

/// Installs additive Block per card-exhaust event after deployment. The power
/// itself gains no Block on play; generated Block follows ordinary turn rules.
pub fn card_exhaust_block_power_v1(card: CardId, effective_magic: i32) -> Option<i32> {
    (card == CardId::FeelNoPain).then_some(effective_magic)
}

/// A separate, non-random PutOnDeck request after the draw action. It is not
/// conditional on having drawn a card. Empty/one-card hands use runtime fallback.
/// Thinking Ahead has a different use-time gate and is not covered by this rule.
pub fn card_hand_topdeck_after_draw_v1(card: CardId) -> Option<usize> {
    (card == CardId::Warcry).then_some(1)
}

/// Increase this instance's base damage after the attack, for this combat.
pub fn card_damage_growth_on_play_v1(card: CardId, effective_magic: i32) -> Option<i32> {
    (card == CardId::Rampage).then_some(effective_magic)
}

/// Signed cost mutation on each positive player HP-loss event. Runtime applies
/// this only to instances in hand, draw and discard, using Java's cost floor.
pub fn card_cost_change_on_hp_loss_v1(card: CardId) -> Option<i32> {
    match card {
        CardId::BloodForBlood => Some(-1),
        CardId::MasterfulStab => Some(1),
        _ => None,
    }
}

/// Test the target's Vulnerable when the special attack resolves, before its
/// damage. The queued order is damage, energy, draw; this is not free draw on play.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardTargetVulnerableFollowUpV1 {
    pub energy_gain: i32,
    pub draw_cards: u32,
}

pub fn card_target_vulnerable_follow_up_v1(card: CardId) -> Option<CardTargetVulnerableFollowUpV1> {
    (card == CardId::Dropkick).then(|| CardTargetVulnerableFollowUpV1 {
        energy_gain: super::card_energy_gain_v1(card, 0, 0)
            .expect("Dropkick energy rule")
            .amount,
        draw_cards: 1,
    })
}

/// Copy one eligible hand card into hand. The single-candidate branch keeps
/// the original; selection removes it and includes a replacement in its count.
/// Both branches add the same net number of copies before hand capacity applies.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardHandCopyV1 {
    pub eligible_types: [CardType; 2],
    pub single_candidate_copies: u8,
    pub selected_candidate_replacement_copies: u8,
}

pub fn card_hand_copy_v1(card: CardId, effective_magic: i32) -> Option<CardHandCopyV1> {
    (card == CardId::DualWield).then(|| {
        let amount = effective_magic as u8;
        CardHandCopyV1 {
            eligible_types: [CardType::Attack, CardType::Power],
            single_candidate_copies: amount,
            selected_candidate_replacement_copies: amount.saturating_add(1),
        }
    })
}

/// Resolve the exhaust-pile choice and available hand space after earlier hooks.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardExhaustPileToHandV1 {
    pub count: u8,
    pub excluded_card: CardId,
}

pub fn card_exhaust_pile_to_hand_v1(card: CardId) -> Option<CardExhaustPileToHandV1> {
    (card == CardId::Exhume).then_some(CardExhaustPileToHandV1 {
        count: 1,
        excluded_card: CardId::Exhume,
    })
}

/// Autoplay the top draw card with fixed-cost payment bypassed and a random
/// enemy target. X-cost cards capture current energy; their own actions still
/// own spending (including free-to-play exceptions). Empty draw piles first
/// shuffle discard; ordinary autoplay gates apply.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardTopDeckPlayV1 {
    pub exhaust_after_play: bool,
    pub bypass_fixed_energy_cost: bool,
    pub x_cost_uses_current_energy: bool,
}

pub fn card_top_deck_play_v1(card: CardId) -> Option<CardTopDeckPlayV1> {
    (card == CardId::Havoc).then_some(CardTopDeckPlayV1 {
        exhaust_after_play: true,
        bypass_fixed_energy_cost: true,
        x_cost_uses_current_energy: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::resolve_card_play;
    use crate::content::powers::{store, PowerId};
    use crate::runtime::action::Action;
    use crate::runtime::combat::{CombatCard, Power};

    #[test]
    fn instance_growth_copy_and_pile_access_match_production_actions() {
        let mut state = crate::test_support::blank_test_combat();
        for upgrades in [0, 1] {
            let mut card = CombatCard::for_scenario(CardId::Rampage, 10);
            card.upgrades = upgrades;
            let growth =
                card_damage_growth_on_play_v1(card.id, 5 + 3 * i32::from(upgrades)).unwrap();
            let actions = resolve_card_play(card.id, &state, &card, Some(1));
            assert!(
                matches!(actions[1].action, Action::ModifyCardDamage { amount, .. } if amount == growth)
            );

            card.id = CardId::DualWield;
            let copy = card_hand_copy_v1(card.id, 1 + i32::from(upgrades)).unwrap();
            state.zones.hand = vec![
                CombatCard::for_scenario(CardId::Strike, 11),
                CombatCard::for_scenario(CardId::Defend, 12),
            ];
            let actions = resolve_card_play(card.id, &state, &card, None);
            assert!(
                matches!(actions[0].action, Action::MakeConstructedCopyInHand { amount, .. } if amount == copy.single_candidate_copies)
            );
            state.zones.hand[1].id = CardId::Inflame;
            let actions = resolve_card_play(card.id, &state, &card, None);
            assert!(matches!(actions[0].action, Action::SuspendForHandSelect {
                filter: crate::state::HandSelectFilter::AttackOrPower,
                reason: crate::state::HandSelectReason::Copy { amount }, ..
            } if amount == copy.selected_candidate_replacement_copies));

            card.id = CardId::Exhume;
            let access = card_exhaust_pile_to_hand_v1(card.id).unwrap();
            assert_eq!(access.excluded_card, CardId::Exhume);
            let actions = resolve_card_play(card.id, &state, &card, None);
            assert!(matches!(actions[0].action, Action::SuspendForGridSelect {
                source_pile: crate::state::PileType::Exhaust,
                filter: crate::state::GridSelectFilter::NonExhume, min, max, ..
            } if min == access.count && max == access.count));

            card.id = CardId::Havoc;
            let autoplay = card_top_deck_play_v1(card.id).unwrap();
            let actions = resolve_card_play(card.id, &state, &card, None);
            assert!(
                matches!(actions[0].action, Action::PlayTopCard { exhaust, .. } if exhaust == autoplay.exhaust_after_play)
            );
        }
    }

    #[test]
    fn hp_loss_cost_changes_cover_active_piles_and_exclude_exhaust() {
        let mut state = crate::test_support::blank_test_combat();
        state.zones.hand = vec![CombatCard::for_scenario(CardId::BloodForBlood, 1)];
        state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::BloodForBlood, 2)].into();
        state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::BloodForBlood, 3)].into();
        state.zones.exhaust_pile = vec![CombatCard::for_scenario(CardId::BloodForBlood, 4)].into();
        crate::engine::action_handlers::damage::handle_lose_hp(0, 0, false, &mut state);
        assert_eq!(state.zones.hand[0].get_cost(), 4);
        for _ in 0..5 {
            crate::engine::action_handlers::damage::handle_lose_hp(0, 1, false, &mut state);
        }
        assert_eq!(state.zones.hand[0].get_cost(), 0);
        assert_eq!(state.zones.draw_pile.iter().next().unwrap().get_cost(), 0);
        assert_eq!(
            state.zones.discard_pile.iter().next().unwrap().get_cost(),
            0
        );
        assert_eq!(
            state.zones.exhaust_pile.iter().next().unwrap().get_cost(),
            4
        );
        assert_eq!(
            card_cost_change_on_hp_loss_v1(CardId::MasterfulStab),
            Some(1)
        );
    }

    #[test]
    fn follow_up_definitions_match_installed_powers_and_ordered_actions() {
        let state = crate::test_support::blank_test_combat();
        for upgrades in [0, 1] {
            let mut combust = CombatCard::for_scenario(CardId::Combust, 1);
            combust.upgrades = upgrades;
            let damage = if upgrades == 0 { 5 } else { 7 };
            let effect = card_end_of_turn_hp_loss_and_damage_v1(combust.id, damage).unwrap();
            assert_eq!(effect.damage_per_enemy_per_application, damage);
            assert_eq!(effect.hp_loss_per_application, 1);
            let actions = resolve_card_play(combust.id, &state, &combust, None);
            assert_eq!(actions.len(), 1);
            assert!(matches!(actions[0].action, Action::ApplyPower {
                power_id: PowerId::Combust, amount, ..
            } if amount == damage));

            let mut embrace = CombatCard::for_scenario(CardId::DarkEmbrace, 2);
            embrace.upgrades = upgrades;
            assert_eq!(card_exhaust_draw_power_v1(embrace.id, 1), Some(1));
            let actions = resolve_card_play(embrace.id, &state, &embrace, None);
            assert_eq!(actions.len(), 1);
            assert!(matches!(
                actions[0].action,
                Action::ApplyPower {
                    power_id: PowerId::DarkEmbrace,
                    amount: 1,
                    ..
                }
            ));

            let mut pain = CombatCard::for_scenario(CardId::FeelNoPain, 5);
            pain.upgrades = upgrades;
            let amount = 3 + i32::from(upgrades);
            assert_eq!(card_exhaust_block_power_v1(pain.id, amount), Some(amount));
            let actions = resolve_card_play(pain.id, &state, &pain, None);
            assert_eq!(actions.len(), 1);
            assert!(matches!(actions[0].action, Action::ApplyPower {
                power_id: PowerId::FeelNoPain, amount: actual, ..
            } if actual == amount));
            assert_eq!(
                crate::content::powers::ironclad::feel_no_pain::on_exhaust(0, amount).as_slice(),
                &[Action::GainBlock { target: 0, amount }]
            );

            let mut warcry = CombatCard::for_scenario(CardId::Warcry, 3);
            warcry.upgrades = upgrades;
            let actions = resolve_card_play(warcry.id, &state, &warcry, None);
            assert_eq!(actions.len(), 2);
            assert_eq!(
                actions[0].action,
                Action::DrawCards(1 + u32::from(upgrades))
            );
            assert_eq!(
                actions[1].action,
                Action::PutOnDeck {
                    amount: card_hand_topdeck_after_draw_v1(warcry.id).unwrap(),
                    random: false,
                }
            );
        }
    }

    #[test]
    fn no_draw_does_not_cancel_warcry_follow_up_selection() {
        let mut state = crate::test_support::blank_test_combat();
        state.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Bash, 3)].into();
        store::set_powers_for(
            &mut state,
            0,
            vec![Power {
                power_type: PowerId::NoDraw,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let card = CombatCard::for_scenario(CardId::Warcry, 4);
        let actions = resolve_card_play(card.id, &state, &card, None);
        let Action::DrawCards(draw) = actions[0].action else {
            panic!("draw first")
        };
        crate::engine::action_handlers::cards::handle_draw_cards(draw, &mut state);
        assert_eq!(state.zones.hand.len(), 2);
        assert_eq!(state.zones.draw_pile.len(), 1);
        let Action::PutOnDeck { amount, random } = actions[1].action else {
            panic!("topdeck follows")
        };
        crate::engine::action_handlers::cards::handle_put_on_deck(amount, random, &mut state);
        assert!(matches!(
            state.pop_next_action(),
            Some(Action::SuspendForHandSelect {
                min: 1,
                max: 1,
                can_cancel: false,
                reason: crate::state::HandSelectReason::PutOnDrawPile,
                ..
            })
        ));
    }

    #[test]
    fn other_timing_rules_remain_unreviewed() {
        assert_eq!(card_hand_topdeck_after_draw_v1(CardId::ThinkingAhead), None);
        assert_eq!(card_exhaust_draw_power_v1(CardId::Brutality, 1), None);
        assert_eq!(
            card_end_of_turn_hp_loss_and_damage_v1(CardId::Combust, 9)
                .unwrap()
                .damage_per_enemy_per_application,
            9
        );
        assert!(card_end_of_turn_hp_loss_and_damage_v1(CardId::Immolate, 5).is_none());
    }
}
