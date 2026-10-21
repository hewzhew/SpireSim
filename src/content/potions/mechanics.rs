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
    // Exhaustive over PotionId: a newly introduced potion must supply its rule.
    Some(match potion {
        PotionId::ExplosivePotion => "Deal damage equal to potency to all enemies (base 10).",
        PotionId::SwiftPotion => "Draw cards equal to potency (base 3).",
        PotionId::Elixir => "Choose any number of cards in your hand to Exhaust.",
        PotionId::AttackPotion => "Choose one of three generated Attack cards. Add potency copies of the chosen card to your hand (base 1); they cost 0 this turn. You may skip the choice.",
        PotionId::SkillPotion => "Choose one of three generated Skill cards. Add potency copies of the chosen card to your hand (base 1); they cost 0 this turn. You may skip the choice.",
        PotionId::PowerPotion => "Choose one of three generated Power cards. Add potency copies of the chosen card to your hand (base 1); they cost 0 this turn. You may skip the choice.",
        PotionId::ColorlessPotion => "Choose one of three generated Colorless cards. Add potency copies of the chosen card to your hand (base 1); the chosen card costs 0 this turn. This choice cannot be skipped.",
        PotionId::StrengthPotion => "Gain Strength equal to potency for this combat (base 2).",
        PotionId::BloodPotion => "Heal for potency percent of max HP (base 20%), rounded down when used; actual healing cannot exceed missing HP.",
        PotionId::HeartOfIron => "Gain Metallicize equal to potency for this combat (base 6).",
        PotionId::DuplicationPotion => "This turn, each of the next potency cards you play is played twice (base 1). Applies to any card type, not only Attacks; unused charges expire at turn end.",
        PotionId::BlessingOfTheForge => "Upgrade all cards currently in your hand for this combat.",
        PotionId::FirePotion => "Deal damage equal to potency to one enemy (base 20).",
        PotionId::PoisonPotion => "Apply Poison equal to potency to one enemy (base 6). Artifact can block it.",
        PotionId::WeakenPotion => "Apply Weak equal to potency to one enemy (base 3). Artifact can block it.",
        PotionId::FearPotion => "Apply Vulnerable equal to potency to one enemy (base 3). Artifact can block it.",
        PotionId::BlockPotion => "Gain Block equal to potency (base 12).",
        PotionId::EnergyPotion => "Gain Energy equal to potency this turn (base 2).",
        PotionId::DexterityPotion => "Gain Dexterity equal to potency for this combat (base 2).",
        PotionId::SpeedPotion => "Gain Dexterity equal to potency (base 5), then apply an equal Dexterity Down debuff that removes it at the end of this turn. Artifact can block the removal debuff.",
        PotionId::SteroidPotion => "Gain Strength equal to potency (base 5), then apply an equal Lose Strength debuff that removes it at the end of this turn. Artifact can block the removal debuff.",
        PotionId::FocusPotion => "Gain Focus equal to potency for this combat (base 2).",
        PotionId::BottledMiracle => "Add Miracles equal to potency to your hand (base 2). A Miracle costs 0, gives 1 Energy and Exhausts; hand capacity applies.",
        PotionId::AncientPotion => "Gain Artifact equal to potency (base 1). Each charge blocks one incoming debuff, including a temporary buff's removal debuff if it is applied afterward.",
        PotionId::RegenPotion => "Gain Regeneration equal to potency (base 5). At each player turn end, heal for the current stacks, then lose 1 stack; healing restrictions and the HP cap apply.",
        PotionId::EssenceOfSteel => "Gain Plated Armor equal to potency (base 4). It grants that much Block at player turn end and loses 1 stack when an enemy Attack causes unblocked HP loss.",
        PotionId::LiquidBronze => "Gain Thorns equal to potency for this combat (base 3). It damages an attacker for that amount whenever you are attacked, even if the hit is blocked.",
        PotionId::DistilledChaosPotion => "Play the next potency cards from the top of your draw pile without paying their energy costs (base 3). Enemy targets are random; normal card effects and play restrictions still apply.",
        PotionId::CunningPotion => "Add upgraded Shivs equal to potency to your hand (base 3). Each Shiv+ costs 0, deals 6 base damage and Exhausts; hand capacity applies.",
        PotionId::PotionOfCapacity => "Gain Orb slots equal to potency (base 2).",
        PotionId::LiquidMemories => "Choose potency cards from your discard pile to return to your hand (base 1); they cost 0 this turn. If fewer cards are available, return the available cards; hand capacity applies.",
        PotionId::GamblersBrew => "Choose any number of cards from your hand to discard, then draw that many cards. You may choose none; draw prevention and hand capacity still apply.",
        PotionId::StancePotion => "Choose to enter Wrath or Calm. Wrath doubles Attack damage dealt and received; leaving Calm grants Energy under the current stance rules.",
        PotionId::FairyPotion => "Passive: when lethal HP loss occurs, consume this potion and heal for 30% of Max HP (60% with Sacred Bark), rounded down with a minimum request of 1 HP. Cannot be used manually. Mark of the Bloom prevents this revival.",
        PotionId::SmokeBomb => "Escape the current non-boss combat instead of winning it; no combat rewards are earned. Cannot be used against bosses or while the Spire Shield and Spire Spear surround you.",
        PotionId::FruitJuice => "Gain Max HP equal to potency and request the same amount of healing (base 5). May be used outside combat; healing restrictions still apply.",
        PotionId::EntropicBrew => "Consume this potion, then generate random potions to fill empty slots, including its freed slot. Potions already held remain. Sozu prevents obtaining the generated potions.",
        PotionId::SneckoOil => "Draw cards equal to potency (base 5), then randomize eligible costs of all cards in your hand to 0-3. X-cost and negative unplayable costs are excluded.",
        PotionId::GhostInAJar => "Gain Intangible equal to potency (base 1), reducing incoming damage and HP loss to 1 per instance while active.",
        PotionId::CultistPotion => "Gain Ritual equal to potency (base 1). Gain that much Strength at the end of each player turn, including the turn it is used; it does not grant Strength immediately on use.",
        PotionId::Ambrosia => "Enter Divinity this turn: gain 3 Energy and triple Attack damage. Return to Neutral at the start of the next turn.",
        PotionId::EssenceOfDarkness => "Channel potency Dark orbs for each current Orb slot (base 1 per slot). Normal channel and evoke rules apply.",
    })
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
