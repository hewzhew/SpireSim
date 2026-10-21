//! Reviewed potion requests shared by execution and acquisition readings.
//! Potency is supplied by the caller; legality and on-use relic hooks remain
//! with the potion owner. This is not a projection of a hypothetical combat.

use super::PotionId;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

pub fn explanation(potion: PotionId) -> Option<&'static str> {
    sts_presentation::Locale::En.entry("game", &format!("potionRule.{}", potion.stable_id()))
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewedPotionUseV1 {
    HealMaxHpPercent { percent: i32 },
    ApplyEnemyWeak { amount: i32 },
    GainThorns { amount: i32 },
    ChooseHandExhaust,
}

pub fn reviewed_potion_use_v1(potion: PotionId, potency: i32) -> Option<ReviewedPotionUseV1> {
    use ReviewedPotionUseV1::*;
    match potion {
        PotionId::BloodPotion => Some(HealMaxHpPercent { percent: potency }),
        PotionId::WeakenPotion => Some(ApplyEnemyWeak { amount: potency }),
        PotionId::LiquidBronze => Some(GainThorns { amount: potency }),
        PotionId::Elixir => Some(ChooseHandExhaust),
        _ => None,
    }
}

/// Java BloodPotion captures a float percentage of max HP when used. It does
/// not guarantee one HP; healing prevention, modifiers and the HP cap come later.
pub fn blood_potion_heal_request_v1(max_hp: i32, percent: i32) -> i32 {
    (max_hp as f32 * (percent as f32 / 100.0)) as i32
}

impl ReviewedPotionUseV1 {
    pub fn combat_actions(self, state: &CombatState, target: usize) -> SmallVec<[ActionInfo; 4]> {
        let action = match self {
            Self::HealMaxHpPercent { percent } => Action::Heal {
                target: 0,
                amount: blood_potion_heal_request_v1(state.entities.player.max_hp, percent),
            },
            Self::ApplyEnemyWeak { amount } => Action::ApplyPower {
                source: 0,
                target,
                power_id: PowerId::Weak,
                amount,
            },
            Self::GainThorns { amount } => Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Thorns,
                amount,
            },
            Self::ChooseHandExhaust => Action::SuspendForHandSelect {
                min: 0,
                max: 99,
                can_cancel: true,
                filter: crate::state::HandSelectFilter::Any,
                reason: crate::state::HandSelectReason::PotionExhaust {
                    has_pending_use_effect: super::potion_effects::elixir_has_pending_use_effect(
                        state,
                    ),
                },
            },
        };
        smallvec![ActionInfo {
            action,
            insertion_mode: AddTo::Bottom
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_potion_has_a_shared_name_and_rule_in_both_locales() {
        for id in PotionId::STABLE_IDS {
            let potion = PotionId::from_stable_id(id).unwrap();
            assert_eq!(
                explanation(potion),
                sts_presentation::Locale::En.entry("game", &format!("potionRule.{id}"))
            );
            for locale in [sts_presentation::Locale::En, sts_presentation::Locale::ZhCn] {
                assert!(
                    locale.entry("game", &format!("potion.{id}")).is_some(),
                    "{locale}:{id}"
                );
                assert!(!locale
                    .entry("game", &format!("potionRule.{id}"))
                    .unwrap()
                    .trim()
                    .is_empty());
            }
        }
    }

    #[test]
    fn reviewed_requests_keep_percentage_units_targets_and_optional_exhaust() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.max_hp = 73;
        for (potency, expected) in [(20, 14), (40, 29)] {
            let actions = super::super::potion_effects::get_potion_actions(
                &state,
                PotionId::BloodPotion,
                None,
                potency,
            );
            assert_eq!(
                actions[0].action,
                Action::Heal {
                    target: 0,
                    amount: expected
                }
            );
        }
        assert_eq!(blood_potion_heal_request_v1(1, 20), 0);
        assert_eq!(blood_potion_heal_request_v1(75, 20), 15);
        for (id, target, power_id) in [
            (PotionId::WeakenPotion, 17, PowerId::Weak),
            (PotionId::LiquidBronze, 0, PowerId::Thorns),
        ] {
            let actions = super::super::potion_effects::get_potion_actions(&state, id, Some(17), 6);
            assert_eq!(
                actions[0].action,
                Action::ApplyPower {
                    source: 0,
                    target,
                    power_id,
                    amount: 6
                }
            );
        }
        for potency in [0, 2] {
            let actions = super::super::potion_effects::get_potion_actions(
                &state,
                PotionId::Elixir,
                None,
                potency,
            );
            assert!(matches!(
                actions[0].action,
                Action::SuspendForHandSelect {
                    min: 0,
                    max: 99,
                    can_cancel: true,
                    reason: crate::state::HandSelectReason::PotionExhaust { .. },
                    ..
                }
            ));
        }
        assert!(reviewed_potion_use_v1(PotionId::FairyPotion, 30).is_none());
        assert!(reviewed_potion_use_v1(PotionId::RegenPotion, 5).is_none());
    }

    #[test]
    fn potion_owner_consumes_elixir_before_choice_and_keeps_on_use_heal() {
        use crate::content::{
            cards::CardId,
            relics::{RelicId, RelicState},
        };
        use crate::runtime::combat::{CombatCard, CombatPhase};
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = 50;
        state.entities.player.max_hp = 75;
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::ToyOrnithopter));
        state.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        )];
        state.turn.current_phase = CombatPhase::PlayerTurn;
        state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        state.entities.potions = vec![Some(super::super::Potion::new(PotionId::Elixir, 1))];
        crate::engine::action_handlers::cards::handle_use_potion(0, None, &mut state);
        assert!(state.entities.potions[0].is_none());
        assert!(matches!(
            state.pop_next_action(),
            Some(Action::SuspendForHandSelect {
                min: 0,
                can_cancel: true,
                reason: crate::state::HandSelectReason::PotionExhaust {
                    has_pending_use_effect: true
                },
                ..
            })
        ));
        assert_eq!(
            state.pop_next_action(),
            Some(Action::Heal {
                target: 0,
                amount: 5
            })
        );
    }
}
