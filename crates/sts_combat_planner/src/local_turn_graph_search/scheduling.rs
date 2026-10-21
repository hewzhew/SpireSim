use super::*;
use std::collections::BTreeSet;

pub(super) fn update_max_guide(
    current: &mut GuideRankMap,
    lane: CombatGuideLaneId,
    candidate: &CombatStateGuideRank,
) -> bool {
    current.update_max(lane, candidate)
}

pub(super) fn guide_choice_order(
    left_rank: &CombatStateGuideRank,
    left_anchor: f64,
    left_visits: usize,
    left_successor: usize,
    right_rank: &CombatStateGuideRank,
    right_anchor: f64,
    right_visits: usize,
    right_successor: usize,
) -> std::cmp::Ordering {
    // The policy-only anchor already owns completeness and fair service. An
    // auxiliary guide must remain exploitative; charging it service debt at
    // every tree level makes a good multi-turn corridor lose a fresh fraction
    // of its budget at every parent.
    right_rank
        .cmp(left_rank)
        .then_with(|| left_anchor.total_cmp(&right_anchor))
        .then_with(|| left_visits.cmp(&right_visits))
        .then_with(|| left_successor.cmp(&right_successor))
}

pub(super) fn local_path_base(atomic_depth: usize, negative_log_policy: f64) -> f64 {
    negative_log_policy + (atomic_depth.max(1) as f64).ln()
}

pub(super) fn guide_rank(
    node: &GraphNode,
    lane: CombatGuideLaneId,
) -> Option<&CombatStateGuideRank> {
    node.guides
        .iter()
        .find(|guide| guide.lane == lane)
        .map(|guide| &guide.rank)
}

pub(super) fn backed_guide_rank<'a>(
    edge: &'a GraphEdge,
    successor: &'a GraphNode,
    lane: CombatGuideLaneId,
) -> Option<&'a CombatStateGuideRank> {
    edge.backed_guides
        .get(&lane)
        .or_else(|| guide_rank(successor, lane))
}

pub(super) fn guide_rank_map(guides: &[CombatStateGuide]) -> GuideRankMap {
    GuideRankMap::from_guides(guides)
}

pub(super) fn generation_service_views_from_lanes(
    lanes: impl IntoIterator<Item = CombatGuideLaneId>,
) -> Vec<LocalServiceView> {
    let lanes = lanes.into_iter().collect::<BTreeSet<_>>();
    std::iter::once(LocalServiceView::Anchor)
        .chain(lanes.into_iter().map(LocalServiceView::Guide))
        .collect()
}

pub(super) fn replay_witness(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    negative_log_policy: f64,
    discovery_source: OracleCombatWitnessDiscoverySource,
    stepper: &dyn CombatStepper,
) -> Result<OracleCombatWitness, OracleCombatWitnessReplayError> {
    let mut position = root.clone();
    let mut engine_steps = 0usize;
    for (action_index, action) in actions.iter().enumerate() {
        if stepper
            .choice_for_legal_input(&position, &action.input)
            .is_none()
        {
            return Err(OracleCombatWitnessReplayError::IllegalInput { action_index });
        }
        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: action.engine_steps.max(1),
                deadline: None,
            },
        );
        engine_steps = engine_steps.saturating_add(result.engine_steps);
        if result.truncated || result.timed_out {
            return Err(OracleCombatWitnessReplayError::TransitionStepLimit { action_index });
        }
        if exact_hash(&result.position) != action.expected_successor_hash.as_str() {
            return Err(OracleCombatWitnessReplayError::SuccessorMismatch { action_index });
        }
        position = result.position;
    }
    if !stepper.is_exact_victory(&position) {
        return Err(OracleCombatWitnessReplayError::FinalStateIsNotWin);
    }
    Ok(OracleCombatWitness {
        actions: actions.to_vec(),
        final_position: position,
        negative_log_policy,
        replay_engine_steps: engine_steps,
        discovery_source,
    })
}

pub(super) fn deadline_reached(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

pub(super) fn elapsed_nanos_u64(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

pub(super) fn witness_potion_expenditures(
    root: &CombatPosition,
    witness: &OracleCombatWitness,
) -> u32 {
    crate::witness::trajectory_potion_contract_usage(
        root,
        &witness.actions,
        &witness.final_position,
    )
    .expenditures
}

pub(super) fn witness_within_potion_contract(
    root: &CombatPosition,
    witness: &OracleCombatWitness,
    max_potions_used: Option<u32>,
    allowed_potion_slots: Option<u64>,
) -> bool {
    crate::witness::trajectory_within_potion_contract(
        root,
        &witness.actions,
        &witness.final_position,
        max_potions_used,
        allowed_potion_slots,
    )
}

pub(super) fn witness_better_with_potion_budget(
    root: &CombatPosition,
    left: &OracleCombatWitness,
    right: &OracleCombatWitness,
    max_potions_used: Option<u32>,
) -> bool {
    let left_potions = witness_potion_expenditures(root, left);
    let right_potions = witness_potion_expenditures(root, right);
    observable_witness_quality_order(
        ObservableWitnessQuality {
            within_budget: max_potions_used.is_none_or(|limit| left_potions <= limit),
            final_hp: left.final_position.combat.entities.player.current_hp,
            action_count: left.actions.len(),
            negative_log_policy: left.negative_log_policy,
            potion_expenditures: left_potions,
        },
        ObservableWitnessQuality {
            within_budget: max_potions_used.is_none_or(|limit| right_potions <= limit),
            final_hp: right.final_position.combat.entities.player.current_hp,
            action_count: right.actions.len(),
            negative_log_policy: right.negative_log_policy,
            potion_expenditures: right_potions,
        },
    ) == std::cmp::Ordering::Greater
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct WitnessAdmission {
    pub(super) frontier_changed: bool,
    pub(super) selected_changed: bool,
}

pub(super) fn remember_nondominated_witness(
    root: &CombatPosition,
    frontier: &mut Vec<OracleCombatWitness>,
    candidate: &OracleCombatWitness,
) -> bool {
    let candidate_resources = terminal_outcome_snapshot(root, candidate, false);
    if frontier.iter().any(|existing| {
        terminal_outcome_snapshot(root, existing, false)
            .strictly_dominates_resources(&candidate_resources)
    }) {
        return false;
    }

    if let Some(equal_index) = frontier.iter().position(|existing| {
        terminal_outcome_snapshot(root, existing, false).has_same_resources_as(&candidate_resources)
    }) {
        if witness_better_with_potion_budget(root, candidate, &frontier[equal_index], None) {
            frontier[equal_index] = candidate.clone();
            return true;
        }
        return false;
    }

    frontier.retain(|existing| {
        !candidate_resources
            .strictly_dominates_resources(&terminal_outcome_snapshot(root, existing, false))
    });
    frontier.push(candidate.clone());
    true
}

pub(super) fn terminal_outcome_snapshot(
    root: &CombatPosition,
    witness: &OracleCombatWitness,
    selected_by_local_hp_view: bool,
) -> LocalTurnGraphTerminalOutcomeSnapshotV1 {
    terminal_outcome_snapshot_from_parts(
        root,
        &witness.final_position,
        &witness.actions,
        witness.negative_log_policy,
        selected_by_local_hp_view,
    )
}

fn terminal_outcome_snapshot_from_parts(
    root: &CombatPosition,
    final_position: &CombatPosition,
    actions: &[TurnOptionAction],
    negative_log_policy: f64,
    selected_by_local_hp_view: bool,
) -> LocalTurnGraphTerminalOutcomeSnapshotV1 {
    let inputs = actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    LocalTurnGraphTerminalOutcomeSnapshotV1::from_replay_checked_position_v1(
        root,
        &inputs,
        final_position,
        actions.len(),
        negative_log_policy,
        selected_by_local_hp_view,
    )
}

pub(super) fn terminal_candidate_could_improve_witness_frontier(
    root: &CombatPosition,
    current_frontier: &[OracleCombatWitness],
    candidate_final_position: &CombatPosition,
    candidate_actions: &[TurnOptionAction],
    candidate_negative_log_policy: f64,
    max_potions_used: Option<u32>,
) -> bool {
    let candidate_potions = crate::witness::trajectory_potion_contract_usage(
        root,
        candidate_actions,
        candidate_final_position,
    )
    .expenditures;
    if max_potions_used.is_some_and(|limit| candidate_potions > limit) {
        return false;
    }
    let candidate_resources = terminal_outcome_snapshot_from_parts(
        root,
        candidate_final_position,
        candidate_actions,
        candidate_negative_log_policy,
        false,
    );
    if current_frontier.iter().any(|current| {
        terminal_outcome_snapshot(root, current, false)
            .strictly_dominates_resources(&candidate_resources)
    }) {
        return false;
    }
    let Some(equal) = current_frontier.iter().find(|current| {
        terminal_outcome_snapshot(root, current, false).has_same_resources_as(&candidate_resources)
    }) else {
        return true;
    };
    observable_witness_quality_order(
        ObservableWitnessQuality {
            within_budget: true,
            final_hp: candidate_final_position.combat.entities.player.current_hp,
            action_count: candidate_actions.len(),
            negative_log_policy: candidate_negative_log_policy,
            potion_expenditures: candidate_potions,
        },
        ObservableWitnessQuality {
            within_budget: true,
            final_hp: equal.final_position.combat.entities.player.current_hp,
            action_count: equal.actions.len(),
            negative_log_policy: equal.negative_log_policy,
            potion_expenditures: witness_potion_expenditures(root, equal),
        },
    ) == std::cmp::Ordering::Greater
}

#[derive(Clone, Copy)]
struct ObservableWitnessQuality {
    within_budget: bool,
    final_hp: i32,
    action_count: usize,
    negative_log_policy: f64,
    potion_expenditures: u32,
}

fn observable_witness_quality_order(
    left: ObservableWitnessQuality,
    right: ObservableWitnessQuality,
) -> std::cmp::Ordering {
    left.within_budget
        .cmp(&right.within_budget)
        .then_with(|| left.final_hp.cmp(&right.final_hp))
        .then_with(|| right.action_count.cmp(&left.action_count))
        .then_with(|| {
            right
                .negative_log_policy
                .total_cmp(&left.negative_log_policy)
        })
        .then_with(|| right.potion_expenditures.cmp(&left.potion_expenditures))
}

pub(super) fn local_deep_state_snapshot(
    node: &GraphNode,
    path_atomic_depth: usize,
) -> OracleCombatDeepStateSnapshot {
    let combat = &node.generator.root().position().combat;
    let alive_monsters = combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .collect::<Vec<_>>();
    OracleCombatDeepStateSnapshot {
        player_turn: combat.turn.turn_count,
        player_hp: combat.entities.player.current_hp,
        player_block: combat.entities.player.block,
        alive_enemy_count: alive_monsters.len(),
        enemy_total_hp: alive_monsters
            .into_iter()
            .map(|monster| monster.current_hp.max(0))
            .sum(),
        hand_size: combat.zones.hand.len(),
        draw_pile_size: combat.zones.draw_pile.len(),
        discard_pile_size: combat.zones.discard_pile.len(),
        exhaust_pile_size: combat.zones.exhaust_pile.len(),
        path_atomic_depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::potions::{Potion, PotionId};
    use sts_core::state::core::{EngineState, RunResult};

    struct SmokeEscapeStepper;

    impl CombatStepper for SmokeEscapeStepper {
        fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
            vec![ClientInput::EndTurn]
        }

        fn apply_to_stable(
            &self,
            position: &CombatPosition,
            _input: ClientInput,
            _limits: CombatStepLimits,
        ) -> sts_core::sim::combat::CombatStepResult {
            let mut escaped = position.clone();
            escaped.engine = EngineState::GameOver(RunResult::Victory);
            escaped.combat.runtime.combat_smoked = true;
            sts_core::sim::combat::CombatStepResult {
                position: escaped,
                terminal: CombatTerminal::Win,
                alive: true,
                truncated: false,
                timed_out: false,
                engine_steps: 1,
            }
        }

        fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
            sts_core::sim::combat::combat_terminal(&position.engine, &position.combat)
        }
    }

    fn witness(final_hp: i32, uses_potion: bool) -> OracleCombatWitness {
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.entities.player.current_hp = final_hp;
        OracleCombatWitness {
            actions: uses_potion
                .then(|| TurnOptionAction {
                    input: ClientInput::UsePotion {
                        potion_index: 0,
                        target: None,
                    },
                    expected_successor_hash: "test".into(),
                    engine_steps: 1,
                })
                .into_iter()
                .collect(),
            final_position: CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            negative_log_policy: 1.0,
            replay_engine_steps: 1,
            discovery_source: OracleCombatWitnessDiscoverySource::PlannerSearch,
        }
    }

    #[test]
    fn replay_witness_rejects_smoke_bomb_escape_as_victory() {
        let root = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        let escaped = SmokeEscapeStepper.apply_to_stable(
            &root,
            ClientInput::EndTurn,
            CombatStepLimits {
                max_engine_steps: 1,
                deadline: None,
            },
        );
        let actions = vec![TurnOptionAction {
            input: ClientInput::EndTurn,
            expected_successor_hash: exact_hash(&escaped.position).into(),
            engine_steps: 1,
        }];

        assert!(matches!(
            replay_witness(
                &root,
                &actions,
                0.0,
                OracleCombatWitnessDiscoverySource::PlannerSearch,
                &SmokeEscapeStepper,
            ),
            Err(OracleCombatWitnessReplayError::FinalStateIsNotWin)
        ));
    }

    #[test]
    fn potion_budget_is_a_constraint_without_globally_overriding_hp_quality() {
        let root = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        let high_hp_potion = witness(50, true);
        let low_hp_clean = witness(20, false);

        assert!(witness_better_with_potion_budget(
            &root,
            &high_hp_potion,
            &low_hp_clean,
            None
        ));
        assert!(witness_better_with_potion_budget(
            &root,
            &low_hp_clean,
            &high_hp_potion,
            Some(0)
        ));
    }

    #[test]
    fn implicit_starting_potion_consumption_obeys_identity_contract() {
        let mut start_combat = sts_core::test_support::blank_test_combat();
        start_combat.entities.potions = vec![
            Some(Potion::new(PotionId::ColorlessPotion, 7)),
            Some(Potion::new(PotionId::FairyPotion, 8)),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, start_combat);
        let mut implicit_fairy = witness(10, false);
        implicit_fairy.final_position.combat.entities.potions =
            vec![Some(Potion::new(PotionId::ColorlessPotion, 7)), None];

        assert!(!witness_within_potion_contract(
            &root,
            &implicit_fairy,
            Some(1),
            Some(1 << 0),
        ));
        assert!(witness_within_potion_contract(
            &root,
            &implicit_fairy,
            Some(1),
            Some(1 << 1),
        ));
        assert!(!witness_within_potion_contract(
            &root,
            &implicit_fairy,
            Some(0),
            Some(1 << 1),
        ));

        let mut explicit_colorless_and_implicit_fairy = witness(10, true);
        explicit_colorless_and_implicit_fairy
            .final_position
            .combat
            .entities
            .potions = vec![None, None];
        assert!(!witness_within_potion_contract(
            &root,
            &explicit_colorless_and_implicit_fairy,
            Some(1),
            Some((1 << 0) | (1 << 1)),
        ));
        assert!(witness_within_potion_contract(
            &root,
            &explicit_colorless_and_implicit_fairy,
            Some(2),
            Some((1 << 0) | (1 << 1)),
        ));
    }

    #[test]
    fn terminal_candidate_filter_uses_the_same_complete_witness_order() {
        let root = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        let current = witness(50, false);

        let candidate = witness(49, false);
        assert!(!terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&current),
            &candidate.final_position,
            &candidate.actions,
            0.5,
            None
        ));
        let candidate = witness(51, true);
        assert!(terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&current),
            &candidate.final_position,
            &candidate.actions,
            20.0,
            None
        ));
        let candidate = witness(50, false);
        assert!(terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&current),
            &candidate.final_position,
            &candidate.actions,
            0.5,
            None
        ));
        assert!(!terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&current),
            &candidate.final_position,
            &candidate.actions,
            1.0,
            None
        ));
        let candidate = witness(51, true);
        assert!(!terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&current),
            &candidate.final_position,
            &candidate.actions,
            0.5,
            Some(0),
        ));
    }

    #[test]
    fn lower_hp_recoverable_gold_outcome_survives_terminal_frontier_filter() {
        let root = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        let mut escaped = witness(82, false);
        escaped
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        let mut killed = witness(71, false);
        killed
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        killed
            .final_position
            .combat
            .runtime
            .pending_rewards
            .push(sts_core::state::rewards::RewardItem::StolenGold { amount: 75 });

        assert!(terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&escaped),
            &killed.final_position,
            &killed.actions,
            killed.negative_log_policy,
            Some(0),
        ));

        let mut frontier = vec![escaped];
        assert!(remember_nondominated_witness(&root, &mut frontier, &killed));
        assert_eq!(frontier.len(), 2);
    }

    #[test]
    fn potion_for_recoverable_gold_remains_a_run_control_tradeoff() {
        let mut start_combat = sts_core::test_support::blank_test_combat();
        start_combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 9))];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, start_combat);
        let mut escaped = witness(82, false);
        escaped.final_position.combat.entities.potions =
            vec![Some(Potion::new(PotionId::FirePotion, 9))];
        escaped
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        let mut killed_with_potion = witness(82, true);
        killed_with_potion
            .final_position
            .combat
            .entities
            .player
            .gold_delta_this_combat = -75;
        killed_with_potion
            .final_position
            .combat
            .runtime
            .pending_rewards
            .push(sts_core::state::rewards::RewardItem::StolenGold { amount: 75 });

        assert!(terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&escaped),
            &killed_with_potion.final_position,
            &killed_with_potion.actions,
            killed_with_potion.negative_log_policy,
            Some(1),
        ));

        let mut frontier = vec![escaped];
        assert!(remember_nondominated_witness(
            &root,
            &mut frontier,
            &killed_with_potion,
        ));
        assert_eq!(frontier.len(), 2);
    }

    #[test]
    fn implicit_potion_consumption_participates_in_witness_ordering() {
        let mut start_combat = sts_core::test_support::blank_test_combat();
        start_combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 8))];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, start_combat);
        let mut clean = witness(50, false);
        clean.final_position.combat.entities.potions =
            vec![Some(Potion::new(PotionId::FairyPotion, 8))];
        let mut implicit_fairy = witness(50, false);
        implicit_fairy.final_position.combat.entities.potions = vec![None];

        assert!(witness_better_with_potion_budget(
            &root,
            &clean,
            &implicit_fairy,
            Some(1),
        ));
        assert!(!terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&clean),
            &implicit_fairy.final_position,
            &implicit_fairy.actions,
            implicit_fairy.negative_log_policy,
            Some(1),
        ));
        assert!(terminal_candidate_could_improve_witness_frontier(
            &root,
            std::slice::from_ref(&implicit_fairy),
            &clean.final_position,
            &clean.actions,
            clean.negative_log_policy,
            Some(1),
        ));
    }
}
