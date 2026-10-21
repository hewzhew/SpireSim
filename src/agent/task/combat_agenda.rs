//! Public, state-derived combat planning opportunities.
//!
//! An agenda names observable conditions worth preserving across exact
//! successor generation.  It does not select a card, inspect a successor, or
//! encode an encounter/card-name script.

use serde::{Deserialize, Serialize};

use crate::agent::information::state::PublicCombatStateV1;
use crate::content::cards::mechanics::{card_mechanic_profile, CardMechanicRole};
use crate::content::cards::{get_card_definition, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::content::relics::RelicId;
use crate::runtime::combat::Intent;

pub const PUBLIC_COMBAT_AGENDA_SCHEMA_NAME: &str = "PublicCombatAgenda";
pub const PUBLIC_COMBAT_AGENDA_SCHEMA_VERSION: u32 = 2;
pub const PUBLIC_COMBAT_PLAN_SCHEMA_NAME: &str = "PublicCombatPlan";
pub const PUBLIC_COMBAT_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatPlanStageV1 {
    EstablishEngine,
    ExploitPreparationWindow,
    StabilizeThreats,
    ConvertToLethal,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatPlanStateV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub stage: PublicCombatPlanStageV1,
    pub requires_exhaust_payoff: bool,
    pub exhaust_payoff_established: bool,
    pub requires_persistent_attack_reduction: bool,
    pub persistent_attack_reduction_deployed: bool,
    pub multiple_threats_neutralized: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatAgendaV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub entries: Vec<PublicCombatAgendaEntryV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatAgendaPriorityV1 {
    Commitment,
    ThreatResponse,
    Development,
    ResourcePreference,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatAgendaEntryV1 {
    pub priority: PublicCombatAgendaPriorityV1,
    pub option: PublicCombatAgendaOptionV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "option", rename_all = "snake_case")]
pub enum PublicCombatAgendaOptionV1 {
    EstablishPersistentMechanic {
        role: CardMechanicRole,
    },
    ReduceLivingEnemyCount {
        current_living_enemies: usize,
    },
    ApplySharedEnemyControl {
        roles: Vec<CardMechanicRole>,
        minimum_living_targets: usize,
    },
    ConvertCardThroughEstablishedPayoff {
        payoff_role: CardMechanicRole,
        conversion_role: CardMechanicRole,
    },
    PreserveCardPlayThreshold {
        maximum_cards_played_this_turn: u8,
    },
    PreserveNoAttackTurn,
    ExerciseCardAccess {
        role: CardMechanicRole,
    },
    ApplyPersistentEnemyAttackReduction {
        role: CardMechanicRole,
    },
    ExploitVisiblePreparationWindow {
        deployment_role: CardMechanicRole,
        minimum_sleeping_targets: usize,
    },
    PreservePlayerLife {
        current_player_hp: i32,
    },
    AdvanceLethalConversion {
        current_total_enemy_hp: i32,
    },
}

pub fn public_combat_agenda_v1(observation: &PublicCombatStateV1) -> PublicCombatAgendaV1 {
    let plan = public_combat_plan_v1(observation);
    public_combat_agenda_for_plan_v1(observation, &plan)
}

pub fn public_combat_plan_v1(observation: &PublicCombatStateV1) -> PublicCombatPlanStateV1 {
    let permanent_exhaust_payoff = observation.cards.master_deck.cards.iter().any(|card| {
        card_mechanic_profile(card.card_id, card.upgrades)
            .roles
            .contains(CardMechanicRole::ExhaustPayoff)
    });
    let accessible_temporary_exhaust_payoff = accessible_combat_cards(observation).any(|card| {
        card_mechanic_profile(card.card_id, card.upgrades)
            .roles
            .contains(CardMechanicRole::ExhaustPayoff)
    });
    let has_exhaust_conversion_or_fuel = observation
        .cards
        .master_deck
        .cards
        .iter()
        .chain(accessible_combat_cards(observation))
        .any(|card| {
            let profile = card_mechanic_profile(card.card_id, card.upgrades);
            profile.roles.contains(CardMechanicRole::ExhaustOtherCards)
                || profile.exhausts_when_played
        });
    let requires_exhaust_payoff = permanent_exhaust_payoff
        || (accessible_temporary_exhaust_payoff && has_exhaust_conversion_or_fuel);
    let requires_persistent_attack_reduction =
        observation.cards.master_deck.cards.iter().any(|card| {
            card_mechanic_profile(card.card_id, card.upgrades)
                .roles
                .contains(CardMechanicRole::PersistentEnemyAttackReduction)
        });
    let exhaust_payoff_established = exhaust_payoff_active(observation);
    let persistent_attack_reduction_deployed = enemy_attack_reduction_active(observation);
    let visible_preparation_window =
        visible_preparation_window_available(observation, persistent_attack_reduction_deployed);
    let multiple_threats_neutralized = living_enemy_count(observation) <= 1;
    let stage = plan_stage(
        requires_exhaust_payoff,
        exhaust_payoff_established,
        visible_preparation_window,
        multiple_threats_neutralized,
    );
    PublicCombatPlanStateV1 {
        schema_name: PUBLIC_COMBAT_PLAN_SCHEMA_NAME.to_string(),
        schema_version: PUBLIC_COMBAT_PLAN_SCHEMA_VERSION,
        stage,
        requires_exhaust_payoff,
        exhaust_payoff_established,
        requires_persistent_attack_reduction,
        persistent_attack_reduction_deployed,
        multiple_threats_neutralized,
    }
}

fn accessible_combat_cards(
    observation: &PublicCombatStateV1,
) -> impl Iterator<Item = &crate::agent::information::state::CombatLearningCardV1> {
    observation
        .cards
        .hand
        .cards
        .iter()
        .chain(observation.cards.draw.cards.iter())
        .chain(observation.cards.discard.cards.iter())
}

pub fn advance_public_combat_plan_v1(
    prior: &PublicCombatPlanStateV1,
    observation: &PublicCombatStateV1,
) -> PublicCombatPlanStateV1 {
    let exhaust_payoff_established =
        prior.exhaust_payoff_established || exhaust_payoff_active(observation);
    let persistent_attack_reduction_deployed =
        prior.persistent_attack_reduction_deployed || enemy_attack_reduction_active(observation);
    let visible_preparation_window =
        visible_preparation_window_available(observation, persistent_attack_reduction_deployed);
    let multiple_threats_neutralized = living_enemy_count(observation) <= 1;
    let stage = plan_stage(
        prior.requires_exhaust_payoff,
        exhaust_payoff_established,
        visible_preparation_window,
        multiple_threats_neutralized,
    );
    PublicCombatPlanStateV1 {
        schema_name: PUBLIC_COMBAT_PLAN_SCHEMA_NAME.to_string(),
        schema_version: PUBLIC_COMBAT_PLAN_SCHEMA_VERSION,
        stage,
        requires_exhaust_payoff: prior.requires_exhaust_payoff,
        exhaust_payoff_established,
        requires_persistent_attack_reduction: prior.requires_persistent_attack_reduction,
        persistent_attack_reduction_deployed,
        multiple_threats_neutralized,
    }
}

pub fn public_combat_agenda_for_plan_v1(
    observation: &PublicCombatStateV1,
    plan: &PublicCombatPlanStateV1,
) -> PublicCombatAgendaV1 {
    let living_enemies = observation
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .count();
    let hand_profiles = observation
        .cards
        .hand
        .cards
        .iter()
        .map(|card| {
            (
                card,
                get_card_definition(card.card_id),
                card_mechanic_profile(card.card_id, card.upgrades),
            )
        })
        .collect::<Vec<_>>();
    let has_pocketwatch = observation
        .player
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Pocketwatch && !relic.used_up);
    let has_art_of_war = observation
        .player
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::ArtOfWar && !relic.used_up);
    let establish_exhaust_payoff = hand_profiles.iter().any(|(_, definition, profile)| {
        definition.card_type == CardType::Power
            && profile.roles.contains(CardMechanicRole::ExhaustPayoff)
    });
    let establish_supported_status_draw_payoff = !status_draw_payoff_active(observation)
        && status_draw_payoff_has_accessible_support(observation)
        && hand_profiles.iter().any(|(_, definition, profile)| {
            definition.card_type == CardType::Power
                && profile.roles.contains(CardMechanicRole::StatusDrawPayoff)
        });
    let visible_sleeping_targets = visible_sleeping_enemy_count(observation);

    let mut options = Vec::new();
    if plan.stage == PublicCombatPlanStageV1::EstablishEngine && establish_exhaust_payoff {
        options.push(entry(
            PublicCombatAgendaPriorityV1::Commitment,
            PublicCombatAgendaOptionV1::EstablishPersistentMechanic {
                role: CardMechanicRole::ExhaustPayoff,
            },
        ));
        // Establishment is a commitment boundary.  Other tactical aspirations
        // are reconsidered from the observed next turn rather than appended as
        // an arbitrary same-turn script.
        if has_pocketwatch {
            options.push(entry(
                PublicCombatAgendaPriorityV1::ResourcePreference,
                PublicCombatAgendaOptionV1::PreserveCardPlayThreshold {
                    maximum_cards_played_this_turn: 3,
                },
            ));
        }
        if has_art_of_war {
            options.push(entry(
                PublicCombatAgendaPriorityV1::ResourcePreference,
                PublicCombatAgendaOptionV1::PreserveNoAttackTurn,
            ));
        }
        return agenda(options);
    }

    if plan.stage == PublicCombatPlanStageV1::ExploitPreparationWindow {
        options.push(entry(
            PublicCombatAgendaPriorityV1::ThreatResponse,
            PublicCombatAgendaOptionV1::ExploitVisiblePreparationWindow {
                deployment_role: CardMechanicRole::PersistentEnemyAttackReduction,
                minimum_sleeping_targets: visible_sleeping_targets,
            },
        ));
        return agenda(options);
    }

    options.push(entry(
        PublicCombatAgendaPriorityV1::ThreatResponse,
        PublicCombatAgendaOptionV1::PreservePlayerLife {
            current_player_hp: observation.player.hp,
        },
    ));

    if plan.stage == PublicCombatPlanStageV1::StabilizeThreats && living_enemies > 1 {
        options.push(entry(
            PublicCombatAgendaPriorityV1::ThreatResponse,
            PublicCombatAgendaOptionV1::ReduceLivingEnemyCount {
                current_living_enemies: living_enemies,
            },
        ));
    }

    let mut shared_control_roles = [
        CardMechanicRole::ApplyWeak,
        CardMechanicRole::ApplyVulnerable,
    ]
    .into_iter()
    .filter(|role| {
        hand_profiles.iter().any(|(_, definition, profile)| {
            definition.target == CardTarget::AllEnemy && profile.roles.contains(*role)
        })
    })
    .collect::<Vec<_>>();
    shared_control_roles.sort_by_key(|role| *role as u8);
    if plan.stage == PublicCombatPlanStageV1::StabilizeThreats
        && living_enemies > 1
        && !shared_control_roles.is_empty()
    {
        options.push(entry(
            PublicCombatAgendaPriorityV1::ThreatResponse,
            PublicCombatAgendaOptionV1::ApplySharedEnemyControl {
                roles: shared_control_roles,
                minimum_living_targets: living_enemies,
            },
        ));
    }

    if establish_supported_status_draw_payoff {
        options.push(entry(
            PublicCombatAgendaPriorityV1::Development,
            PublicCombatAgendaOptionV1::EstablishPersistentMechanic {
                role: CardMechanicRole::StatusDrawPayoff,
            },
        ));
    }

    let exhaust_payoff_is_active = exhaust_payoff_active(observation);
    let hand_can_convert = hand_profiles.iter().any(|(_, _, profile)| {
        profile.roles.contains(CardMechanicRole::ExhaustOtherCards)
            && profile.roles.contains(CardMechanicRole::SelectFromHand)
    });
    if exhaust_payoff_is_active && hand_can_convert {
        options.push(entry(
            PublicCombatAgendaPriorityV1::Development,
            PublicCombatAgendaOptionV1::ConvertCardThroughEstablishedPayoff {
                payoff_role: CardMechanicRole::ExhaustPayoff,
                conversion_role: CardMechanicRole::ExhaustOtherCards,
            },
        ));
    }
    if has_pocketwatch {
        options.push(entry(
            PublicCombatAgendaPriorityV1::ResourcePreference,
            PublicCombatAgendaOptionV1::PreserveCardPlayThreshold {
                maximum_cards_played_this_turn: 3,
            },
        ));
    }
    if has_art_of_war {
        options.push(entry(
            PublicCombatAgendaPriorityV1::ResourcePreference,
            PublicCombatAgendaOptionV1::PreserveNoAttackTurn,
        ));
    }
    let hand_can_draw = hand_profiles
        .iter()
        .any(|(_, _, profile)| profile.roles.contains(CardMechanicRole::DrawCards));
    if (!observation.cards.draw.cards.is_empty() || !observation.cards.discard.cards.is_empty())
        && hand_can_draw
    {
        options.push(entry(
            PublicCombatAgendaPriorityV1::Development,
            PublicCombatAgendaOptionV1::ExerciseCardAccess {
                role: CardMechanicRole::DrawCards,
            },
        ));
    }
    let persistent_reduction_is_accessible = observation
        .cards
        .hand
        .cards
        .iter()
        .chain(observation.cards.draw.cards.iter())
        .chain(
            hand_can_draw
                .then_some(observation.cards.discard.cards.iter())
                .into_iter()
                .flatten(),
        )
        .any(|card| {
            card_mechanic_profile(card.card_id, card.upgrades)
                .roles
                .contains(CardMechanicRole::PersistentEnemyAttackReduction)
        });
    if plan.stage != PublicCombatPlanStageV1::EstablishEngine
        && !plan.persistent_attack_reduction_deployed
        && persistent_reduction_is_accessible
        && living_enemies > 0
    {
        options.push(entry(
            PublicCombatAgendaPriorityV1::ThreatResponse,
            PublicCombatAgendaOptionV1::ApplyPersistentEnemyAttackReduction {
                role: CardMechanicRole::PersistentEnemyAttackReduction,
            },
        ));
    }
    if plan.stage == PublicCombatPlanStageV1::ConvertToLethal {
        options.push(entry(
            PublicCombatAgendaPriorityV1::ThreatResponse,
            PublicCombatAgendaOptionV1::AdvanceLethalConversion {
                current_total_enemy_hp: total_enemy_hp(observation),
            },
        ));
    }
    agenda(options)
}

fn plan_stage(
    requires_exhaust_payoff: bool,
    exhaust_payoff_established: bool,
    visible_preparation_window: bool,
    multiple_threats_neutralized: bool,
) -> PublicCombatPlanStageV1 {
    if requires_exhaust_payoff && !exhaust_payoff_established {
        PublicCombatPlanStageV1::EstablishEngine
    } else if visible_preparation_window {
        PublicCombatPlanStageV1::ExploitPreparationWindow
    } else if !multiple_threats_neutralized {
        PublicCombatPlanStageV1::StabilizeThreats
    } else {
        PublicCombatPlanStageV1::ConvertToLethal
    }
}

fn visible_preparation_window_available(
    observation: &PublicCombatStateV1,
    persistent_attack_reduction_deployed: bool,
) -> bool {
    let living_enemies = living_enemy_count(observation);
    !persistent_attack_reduction_deployed
        && living_enemies > 0
        && visible_sleeping_enemy_count(observation) == living_enemies
        && observation.cards.hand.cards.iter().any(|card| {
            card_mechanic_profile(card.card_id, card.upgrades)
                .roles
                .contains(CardMechanicRole::PersistentEnemyAttackReduction)
        })
}

fn visible_sleeping_enemy_count(observation: &PublicCombatStateV1) -> usize {
    observation
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .filter(|monster| monster.intent.intent == Some(Intent::Sleep))
        .count()
}

fn exhaust_payoff_active(observation: &PublicCombatStateV1) -> bool {
    observation.player.powers.iter().any(|power| {
        matches!(power.power, PowerId::FeelNoPain | PowerId::DarkEmbrace) && power.amount > 0
    })
}

fn status_draw_payoff_active(observation: &PublicCombatStateV1) -> bool {
    observation.player.powers.iter().any(|power| {
        matches!(power.power, PowerId::Evolve | PowerId::FireBreathing) && power.amount > 0
    })
}

fn status_draw_payoff_has_accessible_support(observation: &PublicCombatStateV1) -> bool {
    accessible_combat_cards(observation).any(|card| {
        card_mechanic_profile(card.card_id, card.upgrades)
            .roles
            .contains(CardMechanicRole::GenerateStatusCard)
    }) || observation
        .cards
        .draw
        .cards
        .iter()
        .chain(observation.cards.discard.cards.iter())
        .any(|card| get_card_definition(card.card_id).card_type == CardType::Status)
}

fn enemy_attack_reduction_active(observation: &PublicCombatStateV1) -> bool {
    observation
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .flat_map(|monster| monster.powers.iter())
        .any(|power| power.power == PowerId::Strength && power.amount < 0)
}

fn living_enemy_count(observation: &PublicCombatStateV1) -> usize {
    observation
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .count()
}

fn total_enemy_hp(observation: &PublicCombatStateV1) -> i32 {
    observation
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .map(|monster| monster.hp.max(0))
        .sum()
}

fn agenda(entries: Vec<PublicCombatAgendaEntryV1>) -> PublicCombatAgendaV1 {
    PublicCombatAgendaV1 {
        schema_name: PUBLIC_COMBAT_AGENDA_SCHEMA_NAME.to_string(),
        schema_version: PUBLIC_COMBAT_AGENDA_SCHEMA_VERSION,
        entries,
    }
}

fn entry(
    priority: PublicCombatAgendaPriorityV1,
    option: PublicCombatAgendaOptionV1,
) -> PublicCombatAgendaEntryV1 {
    PublicCombatAgendaEntryV1 { priority, option }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::information::state::public_combat_state_v1;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::CombatCard;
    use crate::runtime::monster_move::MonsterMoveSpec;
    use crate::test_support::{blank_test_combat, test_monster};

    #[test]
    fn engine_access_creates_a_commitment_without_enemy_or_card_identity_script() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::FeelNoPain, 1),
            CombatCard::for_scenario(CardId::BurningPact, 2),
        ];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        let agenda = public_combat_agenda_v1(&public_combat_state_v1(&combat));

        assert_eq!(
            agenda.entries.first(),
            Some(&PublicCombatAgendaEntryV1 {
                priority: PublicCombatAgendaPriorityV1::Commitment,
                option: PublicCombatAgendaOptionV1::EstablishPersistentMechanic {
                    role: CardMechanicRole::ExhaustPayoff,
                },
            })
        );
        assert!(!agenda.entries.iter().any(|entry| matches!(
            entry.option,
            PublicCombatAgendaOptionV1::ExerciseCardAccess { .. }
        )));
    }

    #[test]
    fn accessible_temporary_payoff_with_exhaust_conversion_creates_a_commitment() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::DarkEmbrace, 1)];
        combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::SecondWind, 2)].into();
        combat.meta.master_deck_snapshot =
            vec![CombatCard::for_scenario(CardId::SecondWind, 2)].into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

        let plan = public_combat_plan_v1(&public_combat_state_v1(&combat));

        assert!(plan.requires_exhaust_payoff);
        assert_eq!(plan.stage, PublicCombatPlanStageV1::EstablishEngine);
    }

    #[test]
    fn accessible_temporary_payoff_with_self_exhausting_fuel_creates_a_commitment() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::DarkEmbrace, 1)];
        combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Offering, 2)].into();
        combat.meta.master_deck_snapshot =
            vec![CombatCard::for_scenario(CardId::Offering, 2)].into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

        let plan = public_combat_plan_v1(&public_combat_state_v1(&combat));

        assert!(plan.requires_exhaust_payoff);
        assert_eq!(plan.stage, PublicCombatPlanStageV1::EstablishEngine);
    }

    #[test]
    fn isolated_temporary_payoff_does_not_force_an_engine_commitment() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::DarkEmbrace, 1)];
        combat.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Strike, 2)].into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

        let plan = public_combat_plan_v1(&public_combat_state_v1(&combat));

        assert!(!plan.requires_exhaust_payoff);
        assert_eq!(plan.stage, PublicCombatPlanStageV1::ConvertToLethal);
    }

    #[test]
    fn supported_status_draw_payoff_is_development_not_a_forced_commitment() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::FireBreathing, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::RecklessCharge, 3)].into();
        combat.meta.master_deck_snapshot = combat
            .zones
            .hand
            .iter()
            .chain(combat.zones.draw_pile.iter())
            .cloned()
            .collect::<Vec<_>>()
            .into();
        combat.entities.monsters = vec![
            test_monster(EnemyId::JawWorm),
            test_monster(EnemyId::Cultist),
        ];

        let agenda = public_combat_agenda_v1(&public_combat_state_v1(&combat));

        assert!(agenda.entries.iter().any(|entry| matches!(
            entry,
            PublicCombatAgendaEntryV1 {
                priority: PublicCombatAgendaPriorityV1::Development,
                option: PublicCombatAgendaOptionV1::EstablishPersistentMechanic {
                    role: CardMechanicRole::StatusDrawPayoff,
                },
            }
        )));
        assert!(!agenda.entries.iter().any(|entry| matches!(
            entry,
            PublicCombatAgendaEntryV1 {
                priority: PublicCombatAgendaPriorityV1::Commitment,
                option: PublicCombatAgendaOptionV1::EstablishPersistentMechanic {
                    role: CardMechanicRole::StatusDrawPayoff,
                },
            }
        )));
    }

    #[test]
    fn unsupported_status_draw_payoff_is_not_called_an_engine() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::FireBreathing, 1),
            CombatCard::for_scenario(CardId::Dazed, 2),
        ];
        combat.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Strike, 2)].into();
        combat.entities.monsters = vec![
            test_monster(EnemyId::JawWorm),
            test_monster(EnemyId::Cultist),
        ];

        let agenda = public_combat_agenda_v1(&public_combat_state_v1(&combat));

        assert!(!agenda.entries.iter().any(|entry| matches!(
            entry.option,
            PublicCombatAgendaOptionV1::EstablishPersistentMechanic {
                role: CardMechanicRole::StatusDrawPayoff,
            }
        )));
    }

    #[test]
    fn visible_sleep_and_held_reduction_open_a_generic_preparation_window() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Disarm, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
        ];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.set_planned_visible_spec(Some(MonsterMoveSpec::Sleep));
        combat.entities.monsters = vec![monster];

        let public = public_combat_state_v1(&combat);
        let plan = public_combat_plan_v1(&public);
        let agenda = public_combat_agenda_for_plan_v1(&public, &plan);

        assert_eq!(
            plan.stage,
            PublicCombatPlanStageV1::ExploitPreparationWindow
        );
        assert_eq!(
            agenda.entries,
            vec![PublicCombatAgendaEntryV1 {
                priority: PublicCombatAgendaPriorityV1::ThreatResponse,
                option: PublicCombatAgendaOptionV1::ExploitVisiblePreparationWindow {
                    deployment_role: CardMechanicRole::PersistentEnemyAttackReduction,
                    minimum_sleeping_targets: 1,
                },
            }]
        );
    }

    #[test]
    fn sleeping_enemy_beside_another_living_threat_is_not_a_preparation_window() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Disarm, 1)];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        let mut sleeping = test_monster(EnemyId::JawWorm);
        sleeping.set_planned_visible_spec(Some(MonsterMoveSpec::Sleep));
        let mut other_threat = test_monster(EnemyId::Cultist);
        other_threat.id = 2;
        other_threat.slot = 1;
        other_threat.set_planned_visible_spec(Some(MonsterMoveSpec::Unknown));
        combat.entities.monsters = vec![sleeping, other_threat];

        let plan = public_combat_plan_v1(&public_combat_state_v1(&combat));

        assert_ne!(
            plan.stage,
            PublicCombatPlanStageV1::ExploitPreparationWindow
        );
    }

    #[test]
    fn multiple_enemies_expose_shared_control_without_naming_the_encounter() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Shockwave, 1)];
        combat.entities.monsters = vec![
            test_monster(EnemyId::JawWorm),
            test_monster(EnemyId::Cultist),
            test_monster(EnemyId::LouseNormal),
        ];
        let agenda = public_combat_agenda_v1(&public_combat_state_v1(&combat));

        assert!(agenda.entries.iter().any(|entry| matches!(
            &entry.option,
            PublicCombatAgendaOptionV1::ApplySharedEnemyControl {
                roles,
                minimum_living_targets: 3,
            } if roles == &[CardMechanicRole::ApplyWeak, CardMechanicRole::ApplyVulnerable]
        )));
    }

    #[test]
    fn discard_mechanic_is_publicly_accessible_when_hand_can_trigger_a_shuffle_draw() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::PommelStrike, 1)];
        combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Disarm, 2)].into();
        combat.meta.master_deck_snapshot = vec![
            CombatCard::for_scenario(CardId::PommelStrike, 1),
            CombatCard::for_scenario(CardId::Disarm, 2),
        ]
        .into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        let agenda = public_combat_agenda_v1(&public_combat_state_v1(&combat));

        assert!(agenda.entries.iter().any(|entry| matches!(
            entry.option,
            PublicCombatAgendaOptionV1::ApplyPersistentEnemyAttackReduction {
                role: CardMechanicRole::PersistentEnemyAttackReduction,
            }
        )));
        assert!(agenda.entries.iter().any(|entry| matches!(
            entry.option,
            PublicCombatAgendaOptionV1::ExerciseCardAccess {
                role: CardMechanicRole::DrawCards,
            }
        )));
    }

    #[test]
    fn completed_commitments_do_not_disappear_when_their_cards_leave_public_piles() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        let prior = PublicCombatPlanStateV1 {
            schema_name: PUBLIC_COMBAT_PLAN_SCHEMA_NAME.to_string(),
            schema_version: PUBLIC_COMBAT_PLAN_SCHEMA_VERSION,
            stage: PublicCombatPlanStageV1::ConvertToLethal,
            requires_exhaust_payoff: true,
            exhaust_payoff_established: true,
            requires_persistent_attack_reduction: true,
            persistent_attack_reduction_deployed: true,
            multiple_threats_neutralized: true,
        };

        let advanced = advance_public_combat_plan_v1(&prior, &public_combat_state_v1(&combat));

        assert_eq!(advanced.stage, PublicCombatPlanStageV1::ConvertToLethal);
        assert!(advanced.exhaust_payoff_established);
        assert!(advanced.persistent_attack_reduction_deployed);
    }

    #[test]
    fn pending_defensive_upgrade_does_not_block_lethal_conversion_forever() {
        let mut combat = blank_test_combat();
        combat.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Disarm, 1)].into();
        combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Disarm, 1)].into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

        let public = public_combat_state_v1(&combat);
        let plan = public_combat_plan_v1(&public);
        let agenda = public_combat_agenda_for_plan_v1(&public, &plan);

        assert_eq!(plan.stage, PublicCombatPlanStageV1::ConvertToLethal);
        assert!(!plan.persistent_attack_reduction_deployed);
        assert!(agenda.entries.iter().any(|entry| matches!(
            entry.option,
            PublicCombatAgendaOptionV1::AdvanceLethalConversion { .. }
        )));
    }

    #[test]
    fn newly_visible_additional_threats_reopen_stabilization() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![
            test_monster(EnemyId::JawWorm),
            test_monster(EnemyId::Cultist),
        ];
        let prior = PublicCombatPlanStateV1 {
            schema_name: PUBLIC_COMBAT_PLAN_SCHEMA_NAME.to_string(),
            schema_version: PUBLIC_COMBAT_PLAN_SCHEMA_VERSION,
            stage: PublicCombatPlanStageV1::ConvertToLethal,
            requires_exhaust_payoff: true,
            exhaust_payoff_established: true,
            requires_persistent_attack_reduction: false,
            persistent_attack_reduction_deployed: false,
            multiple_threats_neutralized: true,
        };

        let advanced = advance_public_combat_plan_v1(&prior, &public_combat_state_v1(&combat));

        assert_eq!(advanced.stage, PublicCombatPlanStageV1::StabilizeThreats);
        assert!(!advanced.multiple_threats_neutralized);
    }
}
