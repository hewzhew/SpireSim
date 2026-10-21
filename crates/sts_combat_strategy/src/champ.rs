use super::*;

const LIMIT_BREAK_MOVE_ID: u8 = 7;
const EXECUTE_MOVE_ID: u8 = 3;

/// Projects the Champ encounter into its exact threshold-control protocol.
///
/// The Java encounter removes every debuff, removes Shackled, and gains
/// Strength during Limit Break after falling strictly below half HP. The next
/// move is Execute. This read-only plan records the remaining mitigation and
/// phase-survival resources around that boundary. It does not score, defer, or
/// prune any combat action.
pub fn champ_combat_plan_v1(position: &CombatPosition) -> Option<CombatPlanProjectionV1> {
    if combat_terminal(&position.engine, &position.combat) != CombatTerminal::Unresolved {
        return None;
    }
    let combat = &position.combat;
    let champ = combat
        .entities
        .monsters
        .iter()
        .find(|monster| enemy_id(monster) == EnemyId::Champ && !monster.is_escaped)?;
    if !champ.is_alive_for_action() {
        return None;
    }

    let mut resources = combat_plan_resources_v1(combat);
    resources.remaining_strength_reduction = count_live_cards(combat, |card| {
        matches!(
            card.id,
            CardId::Disarm | CardId::DarkShackles | CardId::Intimidate
        )
    });
    let mut envelope = combat_plan_state_envelope_v1(combat);
    envelope.priority_target_hp_with_block =
        Some(champ.current_hp.max(0).saturating_add(champ.block.max(0)));
    let last_hp_before_threshold = champ.max_hp.saturating_div(2).saturating_sub(1);
    envelope.phase_transition_damage_remaining = Some(
        champ
            .current_hp
            .max(0)
            .saturating_add(champ.block.max(0))
            .saturating_sub(last_hp_before_threshold)
            .max(0),
    );
    let stage = champ_stage(champ);
    let (next_milestone, primary) = match stage {
        CombatPlanStageV1::PrepareThresholdCommit => (
            CombatPlanMilestoneV1::ThresholdCommitted,
            CombatPlanObligationV1::ProvePhaseTransitionSurvival,
        ),
        CombatPlanStageV1::AwaitDebuffCleanse => (
            CombatPlanMilestoneV1::DebuffCleanseCompleted,
            CombatPlanObligationV1::PreservePostCleanseStrengthReduction {
                remaining_sources: resources.remaining_strength_reduction,
            },
        ),
        CombatPlanStageV1::SurviveExecuteWindow => (
            CombatPlanMilestoneV1::ExecuteWindowSurvived,
            CombatPlanObligationV1::SurviveExecuteWindow,
        ),
        CombatPlanStageV1::ConvertToLethal => (
            CombatPlanMilestoneV1::EncounterDefeated,
            CombatPlanObligationV1::ConvertPreparedEngineToLethal,
        ),
        _ => unreachable!("Champ plan uses only threshold-control stages"),
    };

    let mut supporting = Vec::new();
    if matches!(
        stage,
        CombatPlanStageV1::PrepareThresholdCommit | CombatPlanStageV1::AwaitDebuffCleanse
    ) && resources.remaining_strength_reduction > 0
    {
        supporting.push(
            CombatPlanObligationV1::PreservePostCleanseStrengthReduction {
                remaining_sources: resources.remaining_strength_reduction,
            },
        );
    }
    if resources.remaining_intangible_sources > 0 && stage != CombatPlanStageV1::ConvertToLethal {
        supporting.push(CombatPlanObligationV1::PreserveExecuteSurvivalResources {
            remaining_sources: resources.remaining_intangible_sources,
        });
    }

    Some(CombatPlanProjectionV1 {
        schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
        plan: CombatPlanIdV1::ChampPhaseControl,
        stage,
        next_milestone,
        primary,
        supporting,
        resources,
        envelope,
    })
}

pub fn champ_plan_transition_v1(
    before: &CombatPosition,
    after: &CombatPosition,
) -> Option<CombatPlanTransitionV1> {
    let before_plan = champ_combat_plan_v1(before)?;
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

    let after_plan = champ_combat_plan_v1(after);
    let mut completed_milestones = Vec::new();
    if let Some(after_plan) = &after_plan {
        match (before_plan.stage, after_plan.stage) {
            (CombatPlanStageV1::PrepareThresholdCommit, CombatPlanStageV1::AwaitDebuffCleanse) => {
                completed_milestones.push(CombatPlanMilestoneV1::ThresholdCommitted)
            }
            (CombatPlanStageV1::AwaitDebuffCleanse, CombatPlanStageV1::SurviveExecuteWindow) => {
                completed_milestones.push(CombatPlanMilestoneV1::DebuffCleanseCompleted)
            }
            (CombatPlanStageV1::SurviveExecuteWindow, CombatPlanStageV1::ConvertToLethal) => {
                completed_milestones.push(CombatPlanMilestoneV1::ExecuteWindowSurvived)
            }
            _ => {}
        }
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

fn champ_stage(champ: &MonsterEntity) -> CombatPlanStageV1 {
    if !champ.champ.threshold_reached {
        CombatPlanStageV1::PrepareThresholdCommit
    } else if champ.planned_move_id() == LIMIT_BREAK_MOVE_ID {
        CombatPlanStageV1::AwaitDebuffCleanse
    } else if champ.planned_move_id() == EXECUTE_MOVE_ID {
        CombatPlanStageV1::SurviveExecuteWindow
    } else {
        CombatPlanStageV1::ConvertToLethal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::powers::store;
    use sts_core::runtime::combat::{Power, PowerPayload};
    use sts_core::state::core::EngineState;
    use sts_core::test_support::{blank_test_combat, test_monster};

    fn champ_position(threshold_reached: bool, move_id: u8) -> CombatPosition {
        let mut combat = blank_test_combat();
        let mut champ = test_monster(EnemyId::Champ);
        champ.id = 10;
        champ.champ.protocol_seeded = true;
        champ.champ.threshold_reached = threshold_reached;
        champ.set_planned_move_id(move_id);
        combat.entities.monsters.push(champ);
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    #[test]
    fn exact_move_protocol_projects_all_champ_stages() {
        let prepare = champ_combat_plan_v1(&champ_position(false, 1)).expect("prepare");
        let cleanse = champ_combat_plan_v1(&champ_position(true, 7)).expect("cleanse");
        let execute = champ_combat_plan_v1(&champ_position(true, 3)).expect("execute");
        let convert = champ_combat_plan_v1(&champ_position(true, 1)).expect("convert");

        assert_eq!(prepare.stage, CombatPlanStageV1::PrepareThresholdCommit);
        assert_eq!(cleanse.stage, CombatPlanStageV1::AwaitDebuffCleanse);
        assert_eq!(execute.stage, CombatPlanStageV1::SurviveExecuteWindow);
        assert_eq!(convert.stage, CombatPlanStageV1::ConvertToLethal);
        assert!(combat_plan_state_guide_rank_v1(&champ_position(false, 1)).is_none());
        assert!(combat_plan_state_guide_rank_v1(&champ_position(true, 1)).is_none());
    }

    #[test]
    fn projection_observes_apparition_becoming_active_intangible() {
        let mut held = champ_position(false, 1);
        held.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Apparition, 1)];
        let mut active = held.clone();
        active.combat.zones.hand.clear();
        store::set_powers_for(
            &mut active.combat,
            0,
            vec![Power {
                power_type: PowerId::IntangiblePlayer,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        let held_plan = champ_combat_plan_v1(&held).expect("held plan");
        let active_plan = champ_combat_plan_v1(&active).expect("active plan");
        assert_eq!(held_plan.resources.remaining_intangible_sources, 1);
        assert_eq!(held_plan.envelope.player_intangible_turns, 0);
        assert_eq!(active_plan.resources.remaining_intangible_sources, 0);
        assert_eq!(active_plan.envelope.player_intangible_turns, 1);
    }
}
