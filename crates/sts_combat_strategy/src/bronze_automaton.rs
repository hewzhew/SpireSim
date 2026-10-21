use super::*;
use sts_core::content::monsters::city::bronze_automaton::bronze_automaton_cadence_v1;

/// Projects Bronze Automaton's Artifact, Stasis, and forced-attack cadence
/// into one typed phase plan.
///
/// Weak and enemy-Strength-down resources have unusually high value against
/// Hyper Beam, but Artifact consumes those debuffs before they take effect.
/// This plan therefore exposes exact progress toward opening the boss without
/// prescribing a particular strip card, potion, or action sequence. Cards
/// held by Stasis are represented as recoverable rather than currently
/// accessible, and the deterministic Hyper Beam cadence is projected by the
/// encounter mechanics owner.
pub fn bronze_automaton_combat_plan_v1(
    position: &CombatPosition,
) -> Option<CombatPlanProjectionV1> {
    if combat_terminal(&position.engine, &position.combat) != CombatTerminal::Unresolved {
        return None;
    }
    let combat = &position.combat;
    let automaton = bronze_automaton(combat)?;
    if !automaton.is_alive_for_action() {
        return None;
    }

    let mut resources = combat_plan_resources_v1(combat);
    resources.remaining_artifact_sensitive_mitigation = count_live_cards(combat, |card| {
        card_supplies_artifact_sensitive_mitigation(card)
    });
    resources.recoverable_withheld_block_sources = withheld_block_sources(combat);
    let mut envelope = combat_plan_state_envelope_v1(combat);
    let cadence = bronze_automaton_cadence_v1(combat, automaton)?;
    envelope.forced_attack_turns_remaining = cadence.player_turns_until_beam;
    envelope.forced_attack_recovery_window = cadence.recovery_window;
    let withheld_target = withheld_block_target(combat);
    let priority_target = withheld_target.unwrap_or(automaton);
    envelope.priority_target_hp_with_block = Some(
        priority_target
            .current_hp
            .max(0)
            .saturating_add(priority_target.block.max(0)),
    );
    envelope.priority_target_artifact =
        Some(combat.get_power(automaton.id, PowerId::Artifact).max(0));

    let artifact = envelope.priority_target_artifact.unwrap_or_default();
    let stage = if cadence.recovery_window {
        CombatPlanStageV1::ExploitForcedAttackRecoveryWindow
    } else if cadence.player_turns_until_beam == Some(0)
        && resources.recoverable_withheld_block_sources > 0
    {
        CombatPlanStageV1::RecoverWithheldSurvivalResource
    } else if cadence.player_turns_until_beam == Some(0) {
        CombatPlanStageV1::SurviveForcedAttackWindow
    } else if resources.recoverable_withheld_block_sources > 0
        && cadence.player_turns_until_beam.is_some()
    {
        CombatPlanStageV1::PrepareForcedAttackWindow
    } else if artifact > 0 && resources.remaining_artifact_sensitive_mitigation > 0 {
        CombatPlanStageV1::ExposeAttackMitigationTarget
    } else if cadence.player_turns_until_beam.is_some() {
        CombatPlanStageV1::PrepareForcedAttackWindow
    } else {
        CombatPlanStageV1::ConvertToLethal
    };
    let (next_milestone, primary) = match stage {
        CombatPlanStageV1::ExposeAttackMitigationTarget => (
            CombatPlanMilestoneV1::AttackMitigationTargetExposed,
            CombatPlanObligationV1::ExposeAttackMitigationTarget {
                protected_attackers: 1,
            },
        ),
        CombatPlanStageV1::RecoverWithheldSurvivalResource => (
            CombatPlanMilestoneV1::WithheldSurvivalResourceRecovered,
            CombatPlanObligationV1::RecoverWithheldSurvivalResource {
                remaining_sources: resources.recoverable_withheld_block_sources,
            },
        ),
        CombatPlanStageV1::PrepareForcedAttackWindow => (
            CombatPlanMilestoneV1::ForcedAttackWindowReached,
            CombatPlanObligationV1::PrepareForcedAttackSurvival {
                player_turns_remaining: cadence.player_turns_until_beam.unwrap_or_default(),
            },
        ),
        CombatPlanStageV1::SurviveForcedAttackWindow => (
            CombatPlanMilestoneV1::ForcedAttackWindowSurvived,
            CombatPlanObligationV1::SurviveForcedAttackWindow,
        ),
        CombatPlanStageV1::ExploitForcedAttackRecoveryWindow => (
            CombatPlanMilestoneV1::ForcedAttackRecoveryWindowClosed,
            CombatPlanObligationV1::ExploitForcedAttackRecoveryWindow,
        ),
        CombatPlanStageV1::ConvertToLethal => (
            CombatPlanMilestoneV1::EncounterDefeated,
            CombatPlanObligationV1::ConvertPreparedEngineToLethal,
        ),
        _ => unreachable!("Bronze Automaton plan uses only its encounter-owned stages"),
    };

    Some(CombatPlanProjectionV1 {
        schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
        plan: CombatPlanIdV1::BronzeAutomatonControl,
        stage,
        next_milestone,
        primary,
        supporting: Vec::new(),
        resources,
        envelope,
    })
}

pub fn bronze_automaton_plan_transition_v1(
    before: &CombatPosition,
    after: &CombatPosition,
) -> Option<CombatPlanTransitionV1> {
    let before_plan = bronze_automaton_combat_plan_v1(before)?;
    if combat_terminal(&after.engine, &after.combat) == CombatTerminal::Win {
        return Some(CombatPlanTransitionV1 {
            schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
            plan: before_plan.plan,
            before_stage: before_plan.stage,
            after_stage: None,
            completed_milestones: vec![CombatPlanMilestoneV1::EncounterDefeated],
            events: Vec::new(),
            resources_before: before_plan.resources,
            resources_after: None,
            envelope_before: before_plan.envelope,
            envelope_after: None,
        });
    }

    let after_plan = bronze_automaton_combat_plan_v1(after);
    let mut completed_milestones = Vec::new();
    if before_plan
        .envelope
        .priority_target_artifact
        .is_some_and(|artifact| artifact > 0)
        && after_plan.as_ref().is_some_and(|plan| {
            plan.envelope
                .priority_target_artifact
                .is_some_and(|artifact| artifact <= 0)
        })
    {
        completed_milestones.push(CombatPlanMilestoneV1::AttackMitigationTargetExposed);
    }
    if before_plan.resources.recoverable_withheld_block_sources > 0
        && after_plan.as_ref().is_some_and(|plan| {
            plan.resources.recoverable_withheld_block_sources == 0
                && plan.resources.remaining_accessible_block_sources
                    > before_plan.resources.remaining_accessible_block_sources
        })
    {
        completed_milestones.push(CombatPlanMilestoneV1::WithheldSurvivalResourceRecovered);
    }
    if before_plan.envelope.forced_attack_turns_remaining == Some(0)
        && after_plan
            .as_ref()
            .is_some_and(|plan| plan.envelope.forced_attack_recovery_window)
    {
        completed_milestones.push(CombatPlanMilestoneV1::ForcedAttackWindowSurvived);
    }
    if before_plan.envelope.forced_attack_recovery_window
        && after_plan
            .as_ref()
            .is_some_and(|plan| !plan.envelope.forced_attack_recovery_window)
    {
        completed_milestones.push(CombatPlanMilestoneV1::ForcedAttackRecoveryWindowClosed);
    }
    let events = after_plan
        .as_ref()
        .map(|after_plan| combat_plan_transition_events_v1(&before_plan, after_plan))
        .unwrap_or_default();
    Some(CombatPlanTransitionV1 {
        schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
        plan: before_plan.plan,
        before_stage: before_plan.stage,
        after_stage: after_plan.as_ref().map(|plan| plan.stage),
        completed_milestones,
        events,
        resources_before: before_plan.resources,
        resources_after: after_plan.as_ref().map(|plan| plan.resources),
        envelope_before: before_plan.envelope,
        envelope_after: after_plan.as_ref().map(|plan| plan.envelope),
    })
}

pub(super) fn bronze_automaton_state_guide_rank_v1(
    _position: &CombatPosition,
    plan: &CombatPlanProjectionV1,
) -> Option<CombatPlanStateGuideRankV1> {
    if plan.plan != CombatPlanIdV1::BronzeAutomatonControl {
        return None;
    }
    let artifact = plan.envelope.priority_target_artifact.unwrap_or_default();
    let mitigation_supply = plan.resources.remaining_artifact_sensitive_mitigation as i32;
    let accessible_block = plan.resources.remaining_accessible_block_sources as i32;
    let hand_block = plan.resources.remaining_hand_block_sources as i32;
    let withheld_block = plan.resources.recoverable_withheld_block_sources as i32;
    let hp_with_block = plan
        .envelope
        .player_hp
        .saturating_add(plan.envelope.player_block);
    let boss_hp_with_block = plan
        .envelope
        .priority_target_hp_with_block
        .unwrap_or_default();

    // This lane is deliberately categorical. In particular, a block source
    // still held by Stasis is preparation before Hyper Beam, not a resource
    // already available in draw/discard. Once Beam is visible, a recoverable
    // withheld source outranks a source stranded outside the current hand;
    // exact turn generation must still prove the holder can be killed and the
    // returned card played. Ordinary survival/progress lanes stay independent.
    let forced_window_readiness = if plan.envelope.forced_attack_recovery_window {
        5
    } else {
        match plan.envelope.forced_attack_turns_remaining {
            Some(0) if withheld_block > 0 => 4,
            Some(0) if hand_block > 0 => 3,
            Some(0) => 0,
            Some(_) if withheld_block > 0 => 2,
            Some(_) => 1,
            None => 0,
        }
    };
    let milestone = match plan.stage {
        CombatPlanStageV1::ExposeAttackMitigationTarget => 0,
        CombatPlanStageV1::RecoverWithheldSurvivalResource => 1,
        CombatPlanStageV1::PrepareForcedAttackWindow => 2,
        CombatPlanStageV1::SurviveForcedAttackWindow => 3,
        CombatPlanStageV1::ExploitForcedAttackRecoveryWindow => 4,
        CombatPlanStageV1::ConvertToLethal => 5,
        _ => 0,
    };
    let components = vec![
        forced_window_readiness,
        i32::from(withheld_block > 0),
        i32::from(artifact <= 0 || mitigation_supply > 0),
        milestone,
        i32::from(plan.envelope.visible_damage_margin >= 0),
        hand_block,
        accessible_block,
        mitigation_supply,
        -artifact,
        plan.envelope.visible_damage_margin,
        hp_with_block,
        -boss_hp_with_block,
    ];
    Some(CombatPlanStateGuideRankV1 { components })
}

pub(super) fn bronze_automaton_action_timing_v1(
    before_position: &CombatPosition,
    after_position: &CombatPosition,
) -> CombatPlanActionTimingV1 {
    let Some(before) = bronze_automaton_combat_plan_v1(before_position) else {
        return CombatPlanActionTimingV1::Neutral;
    };
    if before.stage != CombatPlanStageV1::ExposeAttackMitigationTarget {
        return CombatPlanActionTimingV1::Neutral;
    }
    let Some(after) = bronze_automaton_combat_plan_v1(after_position) else {
        return CombatPlanActionTimingV1::Neutral;
    };
    let before_artifact = before.envelope.priority_target_artifact.unwrap_or_default();
    let after_artifact = after.envelope.priority_target_artifact.unwrap_or_default();
    if after_artifact >= before_artifact {
        return CombatPlanActionTimingV1::Neutral;
    }
    if after.resources.remaining_artifact_sensitive_mitigation
        < before.resources.remaining_artifact_sensitive_mitigation
    {
        CombatPlanActionTimingV1::Defer(
            CombatPlanActionDeferralV1::PreserveArtifactSensitiveMitigationUntilTargetExposed,
        )
    } else {
        CombatPlanActionTimingV1::PreferNow
    }
}

fn bronze_automaton(combat: &CombatState) -> Option<&MonsterEntity> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| enemy_id(monster) == EnemyId::BronzeAutomaton && !monster.is_escaped)
}

fn card_supplies_artifact_sensitive_mitigation(card: &CombatCard) -> bool {
    card_definition_with_upgrades(card.id, card.upgrades)
        .play_effects
        .iter()
        .any(|effect| {
            matches!(
                effect,
                PlayEffect::Provide(
                    Mechanic::Weak
                        | Mechanic::EnemyStrengthDown
                        | Mechanic::TemporaryEnemyStrengthDown
                )
            )
        })
}

fn withheld_block_target(combat: &CombatState) -> Option<&MonsterEntity> {
    combat.entities.monsters.iter().find(|monster| {
        monster.is_alive_for_action()
            && sts_core::content::powers::store::powers_for(combat, monster.id)
                .unwrap_or_default()
                .iter()
                .any(|power| {
                    power.power_type == PowerId::Stasis
                        && sts_core::content::powers::core::stasis::held_card_lineage(
                            power.extra_data,
                        )
                        .is_some_and(|lineage| {
                            combat
                                .zones
                                .limbo
                                .iter()
                                .any(|card| card.lineage == lineage && card_supplies_block(card))
                        })
                })
    })
}

fn withheld_block_sources(combat: &CombatState) -> u16 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .flat_map(|monster| {
            sts_core::content::powers::store::powers_for(combat, monster.id)
                .unwrap_or_default()
                .iter()
        })
        .filter(|power| power.power_type == PowerId::Stasis)
        .filter(|power| {
            sts_core::content::powers::core::stasis::held_card_lineage(power.extra_data)
                .is_some_and(|lineage| {
                    combat
                        .zones
                        .limbo
                        .iter()
                        .any(|card| card.lineage == lineage && card_supplies_block(card))
                })
        })
        .count()
        .min(u16::MAX as usize) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::powers::store;
    use sts_core::runtime::combat::{Power, PowerPayload};
    use sts_core::state::core::EngineState;
    use sts_core::test_support::{blank_test_combat, test_monster};

    fn bronze_position(artifact: i32) -> CombatPosition {
        let mut combat = blank_test_combat();
        let mut automaton = test_monster(EnemyId::BronzeAutomaton);
        automaton.id = 10;
        automaton.current_hp = 300;
        automaton.max_hp = 300;
        combat.entities.monsters.push(automaton);
        store::set_powers_for(
            &mut combat,
            10,
            vec![Power {
                power_type: PowerId::Artifact,
                instance_id: None,
                amount: artifact,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Disarm, 1),
            CombatCard::for_scenario(CardId::Shockwave, 2),
            CombatCard::for_scenario(CardId::ThunderClap, 3),
        ];
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    fn stasis_block_position(turns_until_beam: u8) -> CombatPosition {
        let mut combat = blank_test_combat();
        let mut automaton = test_monster(EnemyId::BronzeAutomaton);
        automaton.id = 10;
        automaton.current_hp = 300;
        automaton.max_hp = 300;
        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 5_u8.saturating_sub(turns_until_beam);
        automaton.set_planned_move_id(if turns_until_beam == 0 { 2 } else { 1 });
        let mut orb = test_monster(EnemyId::BronzeOrb);
        orb.id = 11;
        orb.current_hp = 40;
        orb.max_hp = 57;
        let second_wind = CombatCard::for_scenario(CardId::SecondWind, 41);
        let second_wind_lineage = second_wind.lineage;
        combat.zones.limbo.push(second_wind);
        combat.entities.monsters = vec![automaton, orb];
        store::set_powers_for(
            &mut combat,
            11,
            vec![sts_core::content::powers::core::stasis::power_holding_card(
                second_wind_lineage,
            )],
        );
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    #[test]
    fn projection_exposes_artifact_barrier_only_with_mitigation_payoff() {
        let position = bronze_position(3);
        let plan = bronze_automaton_combat_plan_v1(&position).expect("Bronze plan");

        assert_eq!(plan.plan, CombatPlanIdV1::BronzeAutomatonControl);
        assert_eq!(plan.stage, CombatPlanStageV1::ExposeAttackMitigationTarget);
        assert_eq!(
            plan.next_milestone,
            CombatPlanMilestoneV1::AttackMitigationTargetExposed
        );
        assert_eq!(plan.resources.remaining_artifact_sensitive_mitigation, 2);
        assert_eq!(plan.envelope.priority_target_artifact, Some(3));
    }

    #[test]
    fn stasis_block_is_preserved_as_preparation_until_beam_is_visible() {
        let position = stasis_block_position(2);

        let plan = bronze_automaton_combat_plan_v1(&position).expect("Bronze plan");

        assert_eq!(plan.stage, CombatPlanStageV1::PrepareForcedAttackWindow);
        assert_eq!(plan.resources.remaining_accessible_block_sources, 0);
        assert_eq!(plan.resources.remaining_hand_block_sources, 0);
        assert_eq!(plan.resources.recoverable_withheld_block_sources, 1);
        assert_eq!(plan.envelope.forced_attack_turns_remaining, Some(2));
        assert_eq!(plan.envelope.priority_target_hp_with_block, Some(40));
    }

    #[test]
    fn accessible_block_is_not_reported_as_withheld() {
        let mut position = stasis_block_position(2);
        let second_wind = position.combat.zones.limbo.pop().expect("withheld block");
        position.combat.zones.draw_pile.push_top(second_wind);
        store::set_powers_for(&mut position.combat, 11, Vec::new());

        let plan = bronze_automaton_combat_plan_v1(&position).expect("Bronze plan");

        assert_ne!(
            plan.stage,
            CombatPlanStageV1::RecoverWithheldSurvivalResource
        );
        assert_eq!(plan.resources.remaining_accessible_block_sources, 1);
        assert_eq!(plan.resources.remaining_hand_block_sources, 0);
        assert_eq!(plan.resources.recoverable_withheld_block_sources, 0);
    }

    #[test]
    fn visible_hyper_beam_recovers_a_withheld_survival_resource_first() {
        let position = stasis_block_position(0);

        let plan = bronze_automaton_combat_plan_v1(&position).expect("Bronze plan");

        assert_eq!(
            plan.stage,
            CombatPlanStageV1::RecoverWithheldSurvivalResource
        );
        assert_eq!(plan.envelope.forced_attack_turns_remaining, Some(0));
    }

    #[test]
    fn only_the_live_forced_attack_turn_is_eligible_for_turn_closure() {
        let preparing = stasis_block_position(1);
        assert_eq!(crate::combat_plan_turn_closure_target_v1(&preparing), None);

        let withheld = stasis_block_position(0);
        assert_eq!(
            crate::combat_plan_turn_closure_target_v1(&withheld),
            None,
            "recovering a Stasis card is a different plan obligation"
        );

        let mut live_window = withheld;
        live_window.combat.zones.limbo.clear();
        store::set_powers_for(&mut live_window.combat, 11, Vec::new());
        assert_eq!(
            crate::combat_plan_turn_closure_target_v1(&live_window),
            Some(CombatPlanMilestoneV1::ForcedAttackWindowSurvived)
        );
    }

    #[test]
    fn guide_prefers_non_payoff_artifact_progress_without_losing_supply() {
        let protected = bronze_position(3);
        let stripped_once = bronze_position(2);
        let mut spent_payoff = bronze_position(2);
        spent_payoff
            .combat
            .zones
            .hand
            .retain(|card| card.id == CardId::ThunderClap);

        let protected_rank = combat_plan_state_guide_rank_v1(&protected).expect("protected rank");
        let stripped_rank = combat_plan_state_guide_rank_v1(&stripped_once).expect("stripped rank");
        let spent_rank = combat_plan_state_guide_rank_v1(&spent_payoff).expect("spent-payoff rank");

        assert!(stripped_rank.components() > protected_rank.components());
        assert!(protected_rank.components() > spent_rank.components());
    }

    #[test]
    fn removing_final_artifact_completes_exposure_milestone() {
        let before = bronze_position(1);
        let after = bronze_position(0);

        let transition =
            bronze_automaton_plan_transition_v1(&before, &after).expect("Bronze transition");

        assert_eq!(
            transition.completed_milestones,
            vec![CombatPlanMilestoneV1::AttackMitigationTargetExposed]
        );
        assert!(transition.events.contains(
            &CombatPlanTransitionEventV1::PriorityTargetArtifactChanged {
                before: Some(1),
                after: Some(0),
            }
        ));
    }

    #[test]
    fn surviving_a20_beam_into_boost_completes_forced_window_milestone() {
        let before = stasis_block_position(0);
        let mut after = before.clone();
        let automaton = after
            .combat
            .entities
            .monsters
            .iter_mut()
            .find(|monster| monster.id == 10)
            .expect("Bronze Automaton");
        automaton.bronze_automaton.num_turns = 0;
        automaton.set_planned_move_id(5); // A19+ post-Beam Boost.
        automaton.move_history_mut().push_back(5);
        after.combat.record_monster_protocol_executed_move(10, 2); // Hyper Beam.

        let transition =
            bronze_automaton_plan_transition_v1(&before, &after).expect("Bronze transition");

        assert_eq!(
            transition.after_stage,
            Some(CombatPlanStageV1::ExploitForcedAttackRecoveryWindow)
        );
        assert!(transition
            .completed_milestones
            .contains(&CombatPlanMilestoneV1::ForcedAttackWindowSurvived));
    }

    #[test]
    fn ending_post_beam_boost_closes_the_recovery_window_milestone() {
        let mut before = stasis_block_position(0);
        let automaton = before
            .combat
            .entities
            .monsters
            .iter_mut()
            .find(|monster| monster.id == 10)
            .expect("Bronze Automaton");
        automaton.bronze_automaton.num_turns = 0;
        automaton.set_planned_move_id(5); // A19+ post-Beam Boost.
        automaton.move_history_mut().push_back(5);
        before.combat.record_monster_protocol_executed_move(10, 2); // Hyper Beam.

        let mut after = before.clone();
        let automaton = after
            .combat
            .entities
            .monsters
            .iter_mut()
            .find(|monster| monster.id == 10)
            .expect("Bronze Automaton");
        automaton.bronze_automaton.num_turns = 1;
        automaton.set_planned_move_id(1); // Flail.
        automaton.move_history_mut().push_back(1);
        after.combat.record_monster_protocol_executed_move(10, 5); // Boost.

        let transition =
            bronze_automaton_plan_transition_v1(&before, &after).expect("Bronze transition");

        assert_eq!(
            transition.before_stage,
            CombatPlanStageV1::ExploitForcedAttackRecoveryWindow
        );
        assert!(transition
            .completed_milestones
            .contains(&CombatPlanMilestoneV1::ForcedAttackRecoveryWindowClosed));
    }

    #[test]
    fn action_timing_prefers_cheap_strips_and_defers_spending_the_payoff() {
        let before = bronze_position(3);
        let cheap_strip = bronze_position(2);
        assert_eq!(
            bronze_automaton_action_timing_v1(&before, &cheap_strip),
            CombatPlanActionTimingV1::PreferNow
        );

        let mut spent_payoff = bronze_position(2);
        spent_payoff
            .combat
            .zones
            .hand
            .retain(|card| card.id != CardId::Disarm);
        assert_eq!(
            bronze_automaton_action_timing_v1(&before, &spent_payoff),
            CombatPlanActionTimingV1::Defer(
                CombatPlanActionDeferralV1::PreserveArtifactSensitiveMitigationUntilTargetExposed
            )
        );
        assert!(combat_plan_has_timed_action_preference_v1(&before));
        assert!(combat_plan_supports_initial_policy_prefix_v1(&before));
    }
}
