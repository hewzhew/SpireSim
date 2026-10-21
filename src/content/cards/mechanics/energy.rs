//! Reviewed energy requests, separate from play cost and actual turn energy.

use serde::{Deserialize, Serialize};

use crate::content::cards::CardId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardEnergyDeliveryV1 {
    OnPlay,
    EachPlayerTurnStart,
    NextPlayerEnergyRecharge,
    OnExhaust,
    IfTargetVulnerableAtResolution,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardEnergyGainV1 {
    /// Gross request, not gain minus the energy spent to play the card.
    pub amount: i32,
    pub delivery: CardEnergyDeliveryV1,
}

/// Runtime callers pass the evaluated magic number; definition readers pass
/// the upgrade-adjusted definition. Missing means unreviewed, never no energy.
/// Dynamic multiplication, X, discard and orb-dependent gains are not covered.
pub fn card_energy_gain_v1(
    card: CardId,
    upgrades: u8,
    effective_magic: i32,
) -> Option<CardEnergyGainV1> {
    use CardEnergyDeliveryV1::*;
    use CardId::*;
    let (amount, delivery) = match card {
        Offering | SeeingRed => (2, OnPlay),
        Bloodletting | Turbo => (effective_magic, OnPlay),
        Adrenaline => (1 + i32::from(upgrades > 0), OnPlay),
        Berserk => (1, EachPlayerTurnStart),
        Outmaneuver => (effective_magic, NextPlayerEnergyRecharge),
        Sentinel => (2 + i32::from(upgrades > 0), OnExhaust),
        Dropkick => (1, IfTargetVulnerableAtResolution),
        _ => return None,
    };
    Some(CardEnergyGainV1 { amount, delivery })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::{get_card_definition, resolve_card_on_exhaust, resolve_card_play};
    use crate::content::powers::{store, PowerId};
    use crate::runtime::action::Action;
    use crate::runtime::combat::{CombatCard, Power};

    #[test]
    fn reviewed_energy_matches_play_and_trigger_actions_without_flattening_timing() {
        use CardEnergyDeliveryV1::*;
        for (id, base, upgraded, delivery) in [
            (CardId::Offering, 2, 2, OnPlay),
            (CardId::SeeingRed, 2, 2, OnPlay),
            (CardId::Bloodletting, 2, 3, OnPlay),
            (CardId::Turbo, 2, 3, OnPlay),
            (CardId::Adrenaline, 1, 2, OnPlay),
            (CardId::Berserk, 1, 1, EachPlayerTurnStart),
            (CardId::Outmaneuver, 2, 3, NextPlayerEnergyRecharge),
            (CardId::Sentinel, 2, 3, OnExhaust),
            (CardId::Dropkick, 1, 1, IfTargetVulnerableAtResolution),
        ] {
            for (upgrades, amount) in [(0, base), (1, upgraded)] {
                let mut state = crate::test_support::blank_test_combat();
                let mut card = CombatCard::for_scenario(id, 1);
                card.upgrades = upgrades;
                let definition = get_card_definition(id);
                let magic =
                    definition.base_magic + definition.upgrade_magic * i32::from(upgrades > 0);
                assert_eq!(
                    card_energy_gain_v1(id, upgrades, magic),
                    Some(CardEnergyGainV1 { amount, delivery }),
                    "{id:?}+{upgrades}"
                );
                let actions = resolve_card_play(id, &state, &card, Some(1));
                let immediate: Vec<_> = actions
                    .iter()
                    .filter_map(|info| match info.action {
                        Action::GainEnergy { amount } => Some(amount),
                        _ => None,
                    })
                    .collect();
                if delivery == OnPlay {
                    assert_eq!(immediate, vec![amount]);
                    continue;
                }
                assert!(
                    immediate.is_empty(),
                    "{id:?} cannot provide immediate energy"
                );
                match delivery {
                    OnExhaust => assert!(resolve_card_on_exhaust(&card, &state)
                        .iter()
                        .any(|info| info.action == Action::GainEnergy { amount })),
                    EachPlayerTurnStart | NextPlayerEnergyRecharge => {
                        let power_id = if delivery == EachPlayerTurnStart {
                            PowerId::Berserk
                        } else {
                            PowerId::Energized
                        };
                        assert!(actions.iter().any(|info| matches!(info.action,
                            Action::ApplyPower { power_id: id, amount: n, .. }
                                if id == power_id && n == amount)));
                        if delivery == EachPlayerTurnStart {
                            assert_eq!(
                                crate::content::powers::resolve_power_at_turn_start(
                                    power_id, &mut state, 0, amount
                                )
                                .as_slice(),
                                &[Action::GainEnergy { amount }]
                            );
                        } else {
                            store::set_powers_for(
                                &mut state,
                                0,
                                vec![Power {
                                    power_type: power_id,
                                    instance_id: None,
                                    amount,
                                    extra_data: 0,
                                    payload: Default::default(),
                                    just_applied: false,
                                }],
                            );
                            let before = state.turn.energy;
                            crate::engine::action_handlers::powers::apply_player_turn_energy_recharge_hooks(&mut state);
                            assert_eq!(i32::from(state.turn.energy), i32::from(before) + amount);
                            assert_eq!(
                                state.pop_next_action(),
                                Some(Action::RemovePower {
                                    target: 0,
                                    power_id,
                                })
                            );
                        }
                    }
                    IfTargetVulnerableAtResolution => assert!(actions
                        .iter()
                        .any(|info| matches!(info.action, Action::DropkickDamageAndEffect { .. }))),
                    OnPlay => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn unresolved_energy_and_dynamic_amounts_are_not_replaced_with_constants() {
        for id in [
            CardId::DoubleEnergy,
            CardId::Fission,
            CardId::Concentrate,
            CardId::Strike,
        ] {
            assert!(card_energy_gain_v1(id, 0, 2).is_none());
        }
        for id in [CardId::Bloodletting, CardId::Turbo, CardId::Outmaneuver] {
            assert_eq!(card_energy_gain_v1(id, 0, 7).unwrap().amount, 7);
        }
        assert_eq!(
            card_energy_gain_v1(CardId::Adrenaline, 0, 7)
                .unwrap()
                .amount,
            1
        );
        assert_eq!(
            card_energy_gain_v1(CardId::Offering, 1, 7).unwrap().amount,
            2
        );
    }

    #[test]
    fn dropkick_energy_uses_vulnerable_at_resolution_and_keeps_damage_first() {
        // Build the pending action before changing Vulnerable. Neither the
        // projection nor use-time state may freeze the conditional reward.
        for (at_use, at_resolution) in [(false, true), (true, false)] {
            let mut state = crate::test_support::blank_test_combat();
            let vulnerable = || Power {
                power_type: PowerId::Vulnerable,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            };
            store::set_powers_for(
                &mut state,
                7,
                if at_use { vec![vulnerable()] } else { vec![] },
            );
            let card = CombatCard::for_scenario(CardId::Dropkick, 1);
            let pending = resolve_card_play(card.id, &state, &card, Some(7));
            let Action::DropkickDamageAndEffect {
                target,
                damage_info,
            } = pending[0].action.clone()
            else {
                panic!("Dropkick must defer its condition")
            };
            store::set_powers_for(
                &mut state,
                7,
                if at_resolution {
                    vec![vulnerable()]
                } else {
                    vec![]
                },
            );
            crate::engine::action_handlers::damage::handle_dropkick(
                target,
                damage_info.clone(),
                &mut state,
            );
            assert_eq!(state.pop_next_action(), Some(Action::Damage(damage_info)));
            if at_resolution {
                assert_eq!(
                    state.pop_next_action(),
                    Some(Action::GainEnergy { amount: 1 })
                );
                assert_eq!(state.pop_next_action(), Some(Action::DrawCards(1)));
            }
            assert_eq!(state.pop_next_action(), None);
        }
    }
}
