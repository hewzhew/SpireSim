use super::*;
use crate::CompleteTurnOptionBoundary;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use sts_core::sim::combat::EngineCombatStepper;

struct CountingGenerationGuides {
    calls: Arc<AtomicUsize>,
}

impl super::super::policy::CombatActionPolicy for CountingGenerationGuides {
    fn weights(
        &self,
        _position: &CombatPosition,
        choices: &[super::super::policy::CombatPolicyChoice<'_>],
    ) -> Vec<f64> {
        vec![1.0; choices.len()]
    }

    fn turn_generation_guides(&self, _position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.calls.fetch_add(1, AtomicOrdering::Relaxed);
        vec![CombatStateGuide::new(CombatGuideLaneId::new(98), vec![0])]
    }
}

fn test_root() -> CombatDecisionRoot {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.entities.monsters = vec![sts_core::test_support::test_monster(
        sts_core::content::monsters::EnemyId::JawWorm,
    )];
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .expect("test combat is a player-turn root")
}

fn guided_entry(
    guide: i32,
    cumulative_negative_log_policy: f64,
    atomic_depth: usize,
    sequence_id: u64,
) -> GuidedGeneratorQueueEntry {
    GuidedGeneratorQueueEntry {
        guide_lane: CombatGuideLaneId::new(0),
        work_id: sequence_id as usize,
        sequence_id,
        guide_rank: CombatStateGuideRank::new(vec![guide]),
        ready_transition: false,
        anchor_priority: GeneratorWorkPriority::for_path(
            atomic_depth,
            cumulative_negative_log_policy,
        ),
    }
}

#[test]
fn singleton_guide_frontier_preserves_heap_order_after_promotion() {
    let mut entries = super::guide_frontier::GuidedGeneratorEntries::new();
    entries.push(guided_entry(1, 1.0, 1, 1));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries.capacity(), 1);
    assert_eq!(entries.peek().map(|entry| entry.sequence_id), Some(1));

    entries.push(guided_entry(3, 3.0, 1, 3));
    entries.push(guided_entry(2, 2.0, 1, 2));
    assert_eq!(
        std::iter::from_fn(|| entries.pop())
            .map(|entry| entry.sequence_id)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
}

#[test]
fn guide_frontier_rebuild_demotes_one_survivor_to_inline_storage() {
    let mut entries = super::guide_frontier::GuidedGeneratorEntries::new();
    entries.push(guided_entry(1, 1.0, 1, 1));
    entries.push(guided_entry(3, 3.0, 1, 3));
    entries.push(guided_entry(2, 2.0, 1, 2));

    entries.retain(|entry| entry.sequence_id == 2);
    entries.shrink_to_fit();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries.capacity(), 1);
    assert_eq!(entries.pop().map(|entry| entry.sequence_id), Some(2));
    assert_eq!(entries.len(), 0);
    assert_eq!(entries.capacity(), 0);
}

#[test]
fn generator_work_slot_keeps_large_expand_payload_indirect() {
    const MAX_WORK_SLOT_BYTES: usize = 160;

    assert!(
        std::mem::size_of::<GeneratorWork>() <= MAX_WORK_SLOT_BYTES,
        "GeneratorWork grew to {} bytes; keep the full combat position behind the shared Expand Arc",
        std::mem::size_of::<GeneratorWork>()
    );
}

#[test]
fn guided_prefix_priority_uses_exact_state_before_anchor_policy() {
    let improved_after_setup = guided_entry(10, 8.0, 3, 0);
    let locally_greedy = guided_entry(9, 0.01, 1, 1);

    assert!(improved_after_setup > locally_greedy);
}

#[test]
fn equal_guide_rank_finishes_prepared_edges_before_expanding_siblings() {
    let mut prepared = guided_entry(4, 8.0, 3, 0);
    prepared.ready_transition = true;
    let cheaper_expansion = guided_entry(4, 0.01, 2, 1);
    let better_state = guided_entry(5, 20.0, 4, 2);

    assert!(prepared > cheaper_expansion);
    assert!(
        better_state > prepared,
        "readiness must not override the guide's state ordering"
    );
}

#[test]
fn prepared_guide_cursor_and_anchor_expansion_keep_independent_round_heads() {
    let policy = Arc::new(CountingGenerationGuides {
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let mut session = TurnOptionGeneratorSession::with_policy(
        test_root(),
        TurnOptionGeneratorConfig::default(),
        policy,
    );
    let GeneratorWork::Expand(parent) = session.pop_scheduled_work().unwrap() else {
        panic!("root expansion");
    };
    let sibling = session.push_work(
        GeneratorWork::Expand(parent.clone()),
        GeneratorWorkPriority::for_path(1, 0.0),
    );
    let cursor = AtomicActionCursorWork::new(
        parent,
        vec![ClientInput::EndTurn],
        vec![0.01],
        vec![CombatStateGuide::new(CombatGuideLaneId::new(98), vec![0])],
    )
    .unwrap();
    let priority = cursor.priority().unwrap();
    let prepared = session.push_work(GeneratorWork::AtomicActions(cursor), priority);
    session.next_scheduler_lane = 0;

    assert_eq!(
        session.snapshot_scheduling_round(),
        VecDeque::from([(0, sibling), (1, prepared)])
    );
}

#[test]
fn depth_guidance_advances_action_cursors_without_losing_complete_outcomes() {
    use sts_core::{content::cards::CardId, runtime::combat::CombatCard};
    struct DepthPolicy;
    impl super::super::policy::CombatActionPolicy for DepthPolicy {
        fn weights(
            &self,
            _: &CombatPosition,
            choices: &[super::super::policy::CombatPolicyChoice<'_>],
        ) -> Vec<f64> {
            vec![1.0; choices.len()]
        }
        fn turn_generation_guides(&self, p: &CombatPosition) -> Vec<CombatStateGuide> {
            vec![CombatStateGuide::new(
                CombatGuideLaneId::new(98),
                vec![i32::from(p.combat.turn.counters.cards_played_this_turn)],
            )]
        }
    }
    let mut position = test_root().position().clone();
    position.combat.entities.monsters[0].set_planned_move_id(2);
    position.combat.entities.monsters[0].current_hp = 24;
    position.combat.entities.monsters[0].max_hp = 24;
    position.combat.zones.hand = (1..=4)
        .map(|id| CombatCard::for_scenario(CardId::Anger, id))
        .collect();
    let root = CombatDecisionRoot::new(position).unwrap();
    let collect = |policy| {
        let mut generator = TurnOptionGeneratorSession::with_policy(
            root.clone(),
            TurnOptionGeneratorConfig::default(),
            policy,
        );
        let mut first_win = None;
        for _ in 0..2048 {
            let report = generator.advance(
                &EngineCombatStepper,
                CombatPlanningQuantum::deterministic(1, 512),
            );
            if first_win.is_none()
                && generator
                    .completed_options()
                    .iter()
                    .any(|o| o.boundary() == CompleteTurnOptionBoundary::TerminalWin)
            {
                first_win = Some(report.after.generation_work);
            }
            if generator.is_finished() {
                break;
            }
        }
        assert!(generator.is_finished());
        assert!(generator.gaps().is_empty());
        let outcomes = generator
            .completed_options()
            .iter()
            .map(|option| {
                crate::replay_turn_option(
                    &root,
                    option,
                    &EngineCombatStepper,
                    crate::ReplayLimits::deterministic(4096),
                )
                .unwrap();
                option.exact_successor_hash().to_owned()
            })
            .collect::<std::collections::BTreeSet<_>>();
        (first_win.unwrap(), outcomes)
    };
    let uniform = collect(crate::policy::uniform_policy());
    let guided = collect(Arc::new(DepthPolicy));
    assert!(guided.0 < uniform.0, "an explicit depth guide must advance its cursors instead of only expanding anchor-fed siblings");
    assert_eq!(guided.1, uniform.1);
}

#[test]
fn one_partial_state_computes_base_generation_guides_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let policy = Arc::new(CountingGenerationGuides {
        calls: calls.clone(),
    });
    let mut session = TurnOptionGeneratorSession::with_policy(
        test_root(),
        TurnOptionGeneratorConfig::default(),
        policy,
    );

    assert_eq!(calls.load(AtomicOrdering::Relaxed), 1);
    let report = session.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(1, 250_000),
    );

    assert_eq!(report.after.generation_work, 1);
    assert_eq!(
        calls.load(AtomicOrdering::Relaxed),
        1,
        "expanding a queued partial must reuse the guide bundle computed at publication"
    );
}

#[test]
fn atomic_cursor_conserves_residual_probability_mass() {
    let mut session =
        TurnOptionGeneratorSession::new(test_root(), TurnOptionGeneratorConfig::default());
    let GeneratorWork::Expand(parent) = session.pop_scheduled_work().expect("root expansion work")
    else {
        panic!("root work must be an expansion");
    };
    let mut cursor = AtomicActionCursorWork::new(
        parent,
        vec![
            ClientInput::EndTurn,
            ClientInput::Cancel,
            ClientInput::Proceed,
        ],
        vec![0.2, 0.5, 0.3],
        Vec::new(),
    )
    .expect("non-empty action surface");

    let initial = cursor.priority().unwrap();
    assert!(initial.negative_log_policy.abs() < 1.0e-12);
    assert_eq!(
        cursor.current_transition().unwrap().input,
        ClientInput::Cancel,
        "the most probable concrete edge is emitted first"
    );

    cursor.consume_current();
    let residual = cursor.priority().unwrap();
    assert!((residual.negative_log_policy - (-0.5_f64.ln())).abs() < 1.0e-12);
    let next_concrete = cursor.current_transition().unwrap();
    assert!(residual.negative_log_policy <= next_concrete.negative_log_policy);

    cursor.consume_current();
    let final_residual = cursor.priority().unwrap();
    let final_concrete = cursor.current_transition().unwrap();
    assert_eq!(
        final_residual.negative_log_policy.to_bits(),
        final_concrete.negative_log_policy.to_bits(),
        "one remaining edge has exactly the cursor's residual bound"
    );
    cursor.consume_current();
    assert!(cursor.priority().is_none());
}

#[test]
fn action_transition_does_not_bypass_explicit_anchor_priority() {
    let mut session =
        TurnOptionGeneratorSession::new(test_root(), TurnOptionGeneratorConfig::default());
    let GeneratorWork::Expand(parent) = session.pop_scheduled_work().expect("root expansion work")
    else {
        panic!("root work must be an expansion");
    };

    for _ in 0..32 {
        session.push_work(
            GeneratorWork::Expand(parent.clone()),
            GeneratorWorkPriority::for_path(1, 0.0),
        );
    }
    session.push_work(
        GeneratorWork::AtomicActions(
            AtomicActionCursorWork::new(
                parent,
                vec![ClientInput::EndTurn],
                vec![(-100.0_f64).exp()],
                Vec::new(),
            )
            .unwrap(),
        ),
        GeneratorWorkPriority::for_path(1, 100.0),
    );
    session.prefer_lane(TurnOptionGeneratorPreferredLane::Anchor);

    assert!(matches!(
        session.pop_scheduled_work(),
        Some(GeneratorWork::Expand(_))
    ));
}

#[test]
fn scheduling_round_heads_cannot_be_overtaken_by_new_arrivals() {
    let mut session =
        TurnOptionGeneratorSession::new(test_root(), TurnOptionGeneratorConfig::default());
    let GeneratorWork::Expand(parent) = session.pop_scheduled_work().expect("root expansion work")
    else {
        panic!("root work must be an expansion");
    };
    let anchor_head = session.push_work(
        GeneratorWork::Expand(parent.clone()),
        GeneratorWorkPriority::for_path(1, 0.0),
    );
    let guide_head = session.push_work(
        GeneratorWork::Expand(parent.clone()),
        GeneratorWorkPriority::for_path(1, 10.0),
    );
    let lane = CombatGuideLaneId::new(99);
    let guide_index = session.ensure_guide_frontier(lane);
    session.guided_frontiers[guide_index]
        .entries
        .push(GuidedGeneratorQueueEntry {
            guide_lane: lane,
            work_id: anchor_head.work_id,
            sequence_id: anchor_head.sequence_id,
            guide_rank: CombatStateGuideRank::new(vec![0]),
            ready_transition: false,
            anchor_priority: GeneratorWorkPriority::for_path(1, 0.0),
        });
    session.guided_frontiers[guide_index]
        .entries
        .push(GuidedGeneratorQueueEntry {
            guide_lane: lane,
            work_id: guide_head.work_id,
            sequence_id: guide_head.sequence_id,
            guide_rank: CombatStateGuideRank::new(vec![10]),
            ready_transition: false,
            anchor_priority: GeneratorWorkPriority::for_path(1, 10.0),
        });

    session.next_scheduler_lane = 0;
    let round = session.snapshot_scheduling_round();
    assert_eq!(
        round.iter().copied().collect::<Vec<_>>(),
        vec![(0, anchor_head), (1, guide_head)]
    );

    let newcomer = session.push_work(
        GeneratorWork::Expand(parent),
        GeneratorWorkPriority::for_path(1, 0.0),
    );
    session.guided_frontiers[guide_index]
        .entries
        .push(GuidedGeneratorQueueEntry {
            guide_lane: lane,
            work_id: newcomer.work_id,
            sequence_id: newcomer.sequence_id,
            guide_rank: CombatStateGuideRank::new(vec![20]),
            ready_transition: false,
            anchor_priority: GeneratorWorkPriority::for_path(1, 0.0),
        });

    assert_eq!(
        round.iter().copied().collect::<Vec<_>>(),
        vec![(0, anchor_head), (1, guide_head)],
        "a later arrival belongs to the next round even when it becomes the new guide head"
    );
}

#[test]
fn withdrawing_intent_cannot_remove_a_shared_frozen_anchor_head() {
    let mut session =
        TurnOptionGeneratorSession::new(test_root(), TurnOptionGeneratorConfig::default());
    let handle = session.peek_anchor_work_handle().unwrap();
    // Model a root expansion shared by both scheduling views. Withdrawing
    // must retain its ordinary service identity even though intent moved it
    // to the front of the frozen round.
    session.selection_intent_members = vec![ClientInput::EndTurn];
    session.refresh_selection_intent();
    session.scheduled_round = session.snapshot_scheduling_round();
    assert_eq!(session.scheduled_round.front(), Some(&(0, handle)));
    let source = session.root_selection_intent().source;
    session
        .replace_root_selection_intent(&source, &[], &EngineCombatStepper)
        .unwrap();
    assert_eq!(session.scheduled_round.front(), Some(&(0, handle)));
}

#[test]
fn late_empty_preference_schedules_the_existing_cancel_continuation() {
    use sts_core::sim::combat_action_equivalence::equivalent_combat_selection_input_v1;
    use sts_core::state::core::{HandSelectReason, PendingChoice};
    let mut position = test_root().position().clone();
    position.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: vec![],
        min_cards: 0,
        max_cards: 0,
        can_cancel: true,
        reason: HandSelectReason::GamblingChip,
    });
    let preferred = equivalent_combat_selection_input_v1(&position.engine, &ClientInput::Cancel)
        .unwrap()
        .input;
    let mut session = TurnOptionGeneratorSession::new(
        CombatDecisionRoot::new(position).unwrap(),
        TurnOptionGeneratorConfig::default(),
    );
    let source = session.root_selection_intent().source;
    let follows_cancel = |work: &GeneratorWork| {
        let partial = match work {
            GeneratorWork::Expand(partial) => partial,
            GeneratorWork::AtomicActions(cursor) => &cursor.parent,
            GeneratorWork::StructuredSelection(cursor) => &cursor.parent,
        };
        partial
            .trace
            .as_ref()
            .is_some_and(|trace| trace.input == ClientInput::Cancel)
    };
    let handle = (0..16)
        .find_map(|_| {
            session.advance(
                &EngineCombatStepper,
                CombatPlanningQuantum::deterministic(1, 500),
            );
            session.work.iter().enumerate().find_map(|(id, work)| {
                work.as_ref()
                    .filter(|w| follows_cancel(w))
                    .and_then(|_| session.live_work_handle(id))
            })
        })
        .expect("retained cancel continuation");
    let counters = session.counters();
    session
        .replace_selection_intent(&source, &[preferred], &EngineCombatStepper)
        .unwrap();
    assert_eq!(session.peek_selection_intent_work_handle(), Some(handle));
    assert_eq!(session.counters(), counters);
    session
        .replace_selection_intent(&source, &[], &EngineCombatStepper)
        .unwrap();
    assert!(session.peek_selection_intent_work_handle().is_none());
    assert!(session.is_live_work_handle(handle));
}

#[test]
fn ordinary_search_does_not_materialize_an_analyst_root_snapshot() {
    let mut position = test_root().position().clone();
    position.combat.entities.monsters = vec![sts_core::test_support::planned_monster(
        sts_core::content::monsters::EnemyId::JawWorm,
        1,
    )];
    let mut session = TurnOptionGeneratorSession::new(
        CombatDecisionRoot::new(position).unwrap(),
        TurnOptionGeneratorConfig::default(),
    );
    assert!(session.root_selection_source.get().is_none());
    session.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(8, 65536),
    );
    assert!(session.root_selection_source.get().is_none());
    let source = session.root_selection_intent().source;
    assert_eq!(source.position(), session.root().position());
    assert_eq!(session.root_selection_intent().source, source);
}

#[test]
fn interrupted_scheduling_round_resumes_before_new_arrivals() {
    let mut session =
        TurnOptionGeneratorSession::new(test_root(), TurnOptionGeneratorConfig::default());
    let GeneratorWork::Expand(parent) = session.pop_scheduled_work().expect("root expansion work")
    else {
        panic!("root work must be an expansion");
    };
    let anchor_head = session.push_work(
        GeneratorWork::Expand(parent.clone()),
        GeneratorWorkPriority::for_path(1, 0.0),
    );
    let guide_head = session.push_work(
        GeneratorWork::Expand(parent.clone()),
        GeneratorWorkPriority::for_path(1, 10.0),
    );
    let lane = CombatGuideLaneId::new(99);
    let guide_index = session.ensure_guide_frontier(lane);
    session.guided_frontiers[guide_index]
        .entries
        .push(GuidedGeneratorQueueEntry {
            guide_lane: lane,
            work_id: anchor_head.work_id,
            sequence_id: anchor_head.sequence_id,
            guide_rank: CombatStateGuideRank::new(vec![0]),
            ready_transition: false,
            anchor_priority: GeneratorWorkPriority::for_path(1, 0.0),
        });
    session.guided_frontiers[guide_index]
        .entries
        .push(GuidedGeneratorQueueEntry {
            guide_lane: lane,
            work_id: guide_head.work_id,
            sequence_id: guide_head.sequence_id,
            guide_rank: CombatStateGuideRank::new(vec![10]),
            ready_transition: false,
            anchor_priority: GeneratorWorkPriority::for_path(1, 10.0),
        });

    session.next_scheduler_lane = 0;
    let first = session.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(1, 250),
    );
    assert_eq!(
        first.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::GenerationWorkBudget)
    );
    assert_eq!(
        session.scheduled_round.front().copied(),
        Some((1, guide_head)),
        "the unserved guide head remains frozen across the quantum boundary"
    );

    let newcomer = session.push_work(
        GeneratorWork::Expand(parent),
        GeneratorWorkPriority::for_path(1, 0.0),
    );
    session.guided_frontiers[guide_index]
        .entries
        .push(GuidedGeneratorQueueEntry {
            guide_lane: lane,
            work_id: newcomer.work_id,
            sequence_id: newcomer.sequence_id,
            guide_rank: CombatStateGuideRank::new(vec![20]),
            ready_transition: false,
            anchor_priority: GeneratorWorkPriority::for_path(1, 0.0),
        });

    session.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(1, 250),
    );
    assert!(!session.is_live_work_handle(guide_head));
    assert!(session.is_live_work_handle(newcomer));
}

#[test]
fn scheduling_rebuild_rejects_stale_generations_after_slot_reuse() {
    let calls = Arc::new(AtomicUsize::new(0));
    let policy = Arc::new(CountingGenerationGuides { calls });
    let mut session = TurnOptionGeneratorSession::with_policy(
        test_root(),
        TurnOptionGeneratorConfig::default(),
        policy,
    );
    let GeneratorWork::Expand(parent) = session.pop_scheduled_work().expect("root expansion work")
    else {
        panic!("root work must be an expansion");
    };

    let mut stale_handles = Vec::new();
    for negative_log_policy in [100.0, 110.0, 120.0] {
        let stale = session.push_work(
            GeneratorWork::Expand(parent.clone()),
            GeneratorWorkPriority::for_path(1, negative_log_policy),
        );
        session.take_live_work(stale);
        stale_handles.push(stale);
    }
    let first_live = session.push_work(
        GeneratorWork::Expand(parent.clone()),
        GeneratorWorkPriority::for_path(1, 0.0),
    );
    let second_live = session.push_work(
        GeneratorWork::Expand(parent),
        GeneratorWorkPriority::for_path(1, 1.0),
    );

    assert_eq!(session.live_work_items, 2);
    assert_eq!(session.live_guide_entries, 2);
    assert_eq!(session.guide_entries_per_work.len(), session.work.len());
    let round_before = session.snapshot_scheduling_round();
    let scheduled_sequences = |round: &VecDeque<(usize, GeneratorWorkHandle)>| {
        round
            .iter()
            .map(|(lane, handle)| (*lane, handle.sequence_id))
            .collect::<Vec<_>>()
    };
    let scheduled_sequences_before = scheduled_sequences(&round_before);
    let mut live_sequences_before = [first_live, second_live]
        .into_iter()
        .map(|handle| handle.sequence_id)
        .collect::<Vec<_>>();
    live_sequences_before.sort_unstable();
    let work_slots_before = session.work.len();
    let anchor_entries_before = session.anchor_frontier.len();
    let guide_entries_before = session.guided_frontiers[0].entries.len();
    let storage_before = session.storage_snapshot();
    assert_eq!(storage_before.live_anchor_entries, 2);
    assert_eq!(storage_before.live_guide_entries, 2);
    assert_eq!(storage_before.free_work_slots, 0);
    assert_eq!(storage_before.reused_work_slots, 4);

    session.reclaim_stale_scheduling_entries();

    let round_after = session.snapshot_scheduling_round();
    assert_eq!(
        scheduled_sequences(&round_after),
        scheduled_sequences_before
    );
    let mut live_sequences_after = session
        .anchor_frontier
        .iter()
        .map(|entry| entry.sequence_id)
        .collect::<Vec<_>>();
    live_sequences_after.sort_unstable();
    assert_eq!(live_sequences_after, live_sequences_before);
    assert_eq!(
        scheduled_sequences_before
            .first()
            .map(|(_, sequence)| *sequence),
        live_sequences_before.first().copied()
    );
    assert!(session.work.iter().all(Option::is_some));
    assert!(stale_handles
        .iter()
        .all(|handle| !session.is_live_work_handle(*handle)));
    assert!(stale_handles
        .iter()
        .all(|handle| handle.work_id == first_live.work_id));
    assert_ne!(
        stale_handles.last().map(|handle| handle.sequence_id),
        Some(first_live.sequence_id)
    );
    assert_eq!(session.work.len(), 2);
    assert_eq!(work_slots_before, 2);
    assert_eq!(session.free_work_ids.len(), 0);
    assert_eq!(session.guide_entries_per_work.len(), 2);
    assert_eq!(session.anchor_frontier.len(), 2);
    assert_eq!(session.guided_frontiers[0].entries.len(), 2);
    assert_eq!(session.scheduling_rebuilds, 1);
    assert_eq!(session.reused_work_slots, 4);
    assert_eq!(
        session.reclaimed_anchor_entries,
        anchor_entries_before.saturating_sub(2)
    );
    assert_eq!(
        session.reclaimed_guide_entries,
        guide_entries_before.saturating_sub(2)
    );
}

#[test]
fn finite_potion_allowance_is_part_of_generator_transposition_identity() {
    let root = test_root();
    let exact = combat_exact_state_key(&root.position().engine, &root.position().combat);
    let without_spend = IndexedExactStateKey::new(exact.clone(), Some(0), None);
    let after_one_spend = IndexedExactStateKey::new(exact, Some(1), None);

    assert_ne!(without_spend, after_one_spend);
    assert_eq!(
        HashSet::from([without_spend, after_one_spend]).len(),
        2,
        "equal simulator states with different remaining finite resources cannot transpose"
    );
}

#[test]
fn trajectory_progress_is_part_of_generator_transposition_identity() {
    let root = test_root();
    let exact = combat_exact_state_key(&root.position().engine, &root.position().combat);
    let query = crate::CombatTrajectoryQueryV1::eventually(
        crate::CombatTrajectoryAtomV1::GeneratedCardSelected {
            card_id: sts_core::content::cards::CardId::MasterOfStrategy,
        },
    );
    let compiled = CompiledCombatTrajectoryQueryV1::compile(&query).expect("supported query");
    let pending = CombatPosition::new(
        EngineState::PendingChoice(sts_core::state::core::PendingChoice::DiscoverySelect(
            sts_core::state::core::DiscoveryChoiceState {
                cards: vec![sts_core::content::cards::CardId::MasterOfStrategy],
                colorless: true,
                card_type: None,
                amount: 1,
                can_skip: false,
            },
        )),
        root.position().combat.clone(),
    );
    let initial = compiled.initial_progress();
    let satisfied = compiled.advance(
        &initial,
        &pending,
        &ClientInput::SubmitDiscoverChoice(0),
        root.position(),
    );
    let without_event = IndexedExactStateKey::new(exact.clone(), None, Some(initial));
    let after_event = IndexedExactStateKey::new(exact, None, Some(satisfied));

    assert_ne!(without_event, after_event);
    assert_eq!(
        HashSet::from([without_event, after_event]).len(),
        2,
        "equal simulator states with different completed path obligations cannot transpose"
    );
}

#[test]
fn structural_hash_collision_still_compares_the_complete_typed_state() {
    let root = test_root();
    let position = root.position();
    let player_turn = combat_exact_state_key(&position.engine, &position.combat);
    let processing = combat_exact_state_key(&EngineState::CombatProcessing, &position.combat);
    let first = IndexedExactStateKey::new(player_turn, None, None);
    let mut collided = IndexedExactStateKey::new(processing, None, None);

    collided.structural_hash = first.structural_hash;

    assert_ne!(first, collided);
    assert_eq!(
        HashSet::from([first, collided]).len(),
        2,
        "a private structural-hash collision must not merge exact simulator states"
    );
}

#[test]
fn detail_timing_sampler_is_sparse_without_periodic_action_order_aliasing() {
    let sampled = (1..=16_384)
        .filter(|ordinal| detail_timing_scale(*ordinal).is_some())
        .collect::<Vec<_>>();

    assert!((900..=1_150).contains(&sampled.len()));
    assert!(
        sampled.windows(2).any(|pair| pair[1] - pair[0] != 16),
        "samples must not always select the same member of 16-wide action families"
    );
}
