use super::*;
use crate::draft_adoption::{prepare_manual_draft_adoption_v1, prepare_verified_suffix};
use std::{cell::Cell, rc::Rc};
use sts_oracle_run_control::{
    content::events::neow::NeowEligibilityV1,
    eval::run_control::{RunControlConfig, RunControlSession, RunControlSessionCheckpointV1},
};
use sts_parity_lockstep::{
    DecisionSurfaceV1, DomainEvidenceV1, LockstepModeV1, LockstepRunnerV1, ParityDomainV1,
    ProducerIdentityV1,
};
use sts_parity_lockstep_java::{
    JavaLockstepReceiptV1, JavaRuntimeObservationV1, RecordedJavaActionPhaseV1,
    RecordedJavaEvidenceStoreV1,
};

struct Fixture {
    repository: PathBuf,
    session: ManualSessionV1,
    root: RustLockstepAdapterV1,
    draft: ManualCombatDraftV1,
}

impl Fixture {
    fn new() -> Self {
        Self::with_seed(2_026_090_301)
    }

    fn with_seed(seed: u64) -> Self {
        Self::with_route(seed, None)
    }

    fn with_route(seed: u64, first_node: Option<&str>) -> Self {
        let repository =
            std::env::temp_dir().join(format!("private-draft-test-{}", uuid::Uuid::new_v4()));
        let session = ManualSessionV1::prepare(&repository).unwrap();
        let config = RunControlConfig {
            seed,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&config).unwrap();
        let mut root = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap,
            "fixture",
            "fixture",
        )
        .unwrap();
        // Fixed natural first combat; the greedy line is only a small test fixture.
        for surface in [
            DecisionSurfaceV1::Neow,
            DecisionSurfaceV1::Neow,
            DecisionSurfaceV1::Neow,
            DecisionSurfaceV1::Map,
        ] {
            let action = actions(&mut root).into_iter().filter(|action| match action {
                SemanticActionV1::Select { surface: DecisionSurfaceV1::Map, choice } => first_node.is_none_or(|node| choice.id == node),
                _ => true,
            }).find(|a| matches!(a,
                SemanticActionV1::Proceed { surface: s } | SemanticActionV1::Select { surface: s, .. } if *s == surface)).unwrap();
            step(&mut root, &action);
        }
        root.validate_combat_draft_root_v1().unwrap();
        let mut end = root.clone();
        let mut selected = Vec::new();
        for _ in 0..100 {
            if end.session().active_combat.is_none() {
                break;
            }
            let action = actions(&mut end)
                .into_iter()
                .find(|a| matches!(a, SemanticActionV1::PlayCard { .. }))
                .unwrap_or(SemanticActionV1::EndTurn);
            step(&mut end, &action);
            selected.push(action);
        }
        let terminal = root.verify_semantic_combat_victory_v1(&selected).unwrap();
        assert!(selected.len() > 2);
        let draft = ManualCombatDraftV1 {
            schema: "sts-manual-combat-draft-v1".into(),
            session_id: session.session_id().into(),
            rust_version: "fixture".into(),
            source: Some(ManualDraftSourceV1 {
                session_id: session.session_id().into(),
                committed_actions: 0,
            }),
            root: root.observe().unwrap(),
            actions: selected,
            terminal,
        };
        Self {
            repository,
            session,
            root,
            draft,
        }
    }
    fn workspace(&self) -> ManualDraftWorkspaceV1 {
        ManualDraftWorkspaceV1::create(
            &self.session,
            self.draft.source.clone().unwrap(),
            self.root.clone(),
            worker(),
        )
        .unwrap()
    }
    fn reopen(&self, directory: PathBuf, handle: String) -> ManualDraftWorkspaceV1 {
        ManualDraftWorkspaceV1::open(directory, handle, self.root.clone(), worker()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.repository.is_absolute() && self.repository.starts_with(std::env::temp_dir()));
        assert!(self
            .repository
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("private-draft-test-"));
        fs::remove_dir_all(&self.repository).unwrap();
    }
}

fn worker() -> DraftWorker {
    DraftWorker {
        package_version: "fixture".into(),
        source_revision: "constructed-test".into(),
        executable_sha256: "constructed-test".into(),
    }
}

fn turn_trial_request(work: usize) -> crate::TurnResearchRequestV1 {
    crate::TurnResearchRequestV1 {
        work,
        engine_steps: 65_536,
        potion_slots: vec![],
        offset: 0,
        limit: 8,
        outcome: None,
        minimum_hp: None,
        minimum_block: None,
        enemy_hp_at_most: None,
        player_powers: vec![],
    }
}

#[test]
fn turn_continuation_keeps_source_diagnostics_out_of_kernel_equality() {
    use sts_oracle_run_control::state::selection::{
        EngineDiagnostic, EngineDiagnosticClass, EngineDiagnosticSeverity,
    };
    let fixture = Fixture::new();
    let mut run = fixture.root.session().clone();
    let note = EngineDiagnostic {
        severity: EngineDiagnosticSeverity::Info,
        class: EngineDiagnosticClass::Normalization,
        message: "normalized GamblingChip count to the available hand".into(),
    };
    run.active_combat
        .as_mut()
        .unwrap()
        .combat_state
        .emit_diagnostic(note.clone());
    let root = RustLockstepAdapterV1::new(
        run,
        sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        })
        .unwrap(),
        "constructed",
        "diagnostic-continuation",
    )
    .unwrap();
    let mut draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        fixture.draft.source.clone().unwrap(),
        root,
        worker(),
    )
    .unwrap();
    let original = draft.snapshot().unwrap();
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let parent = page
        .options
        .iter()
        .find(|option| option.boundary == "next_turn")
        .unwrap();
    let next = draft
        .research_turns_after(&turn_trial_request(256), Some(&parent.reference))
        .unwrap();
    assert!(next.total > 0);
    let remainder = draft
        .research_turns_from(
            &turn_trial_request(256),
            Some(&parent.reference),
            crate::TurnResearchStartV1::BeforeEndTurn,
        )
        .unwrap();
    assert!(remainder.total > 0);
    assert_eq!(draft.snapshot().unwrap(), original);
    assert!(draft.selected_actions().is_empty());
    assert_eq!(
        draft
            .states
            .last()
            .unwrap()
            .adapter
            .session()
            .active_combat
            .as_ref()
            .unwrap()
            .combat_state
            .runtime
            .engine_diagnostics,
        vec![note],
        "normalizing a private comparison must not erase the source's diagnostic output",
    );
}

#[test]
fn turn_research_keeps_enemy_filters_bound_across_collector_summons() {
    use sts_oracle_run_control::{
        content::monsters::factory::EncounterId, sim::combat_start::build_natural_combat_start,
        state::map::node::RoomType,
    };
    let fixture = Fixture::new();
    let mut run = fixture.root.session().clone();
    let (engine, combat) = build_natural_combat_start(
        &mut run.run_state,
        EncounterId::Collector,
        RoomType::MonsterRoomBoss,
    )
    .unwrap();
    let active = run.active_combat.as_mut().unwrap();
    active.engine_state = engine;
    active.combat_state = combat;
    active.combat_state.turn.energy = 0;
    let collector_id = active.combat_state.entities.monsters[0].id;
    let collector_hp = active.combat_state.entities.monsters[0].current_hp;
    let root = RustLockstepAdapterV1::new(
        run,
        sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        })
        .unwrap(),
        "constructed",
        "collector-filter-reindex",
    )
    .unwrap();
    let mut draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        fixture.draft.source.clone().unwrap(),
        root,
        worker(),
    )
    .unwrap();
    let original = draft.snapshot().unwrap();
    let mut request = turn_trial_request(256);
    request.enemy_hp_at_most = Some(crate::TurnEnemyHpLimitV1 { slot: 0, hp: 45 });
    let filtered = draft.research_turns(&request).unwrap();
    assert_eq!(filtered.coverage, "complete");
    assert_eq!(filtered.total, 1);
    assert_eq!(
        filtered.matching, 0,
        "a new Torch Head must not satisfy the Collector HP filter"
    );
    request.work = 0;
    request.enemy_hp_at_most.as_mut().unwrap().hp = collector_hp;
    let all = draft.research_turns(&request).unwrap();
    assert_eq!(all.matching, 1);
    assert_eq!(all.generation_work, filtered.generation_work);
    let parent = &all.options[0];
    let detail = draft.inspect_turn(&parent.reference).unwrap();
    assert_eq!(detail.successor.monsters.len(), 3);
    assert_eq!(detail.successor.monsters[2].hp, collector_hp);
    assert!(detail.successor.monsters[0].hp <= 45);
    // A continuation has its own explicit root: there M0 names a Torch Head
    // and M2 names the original Collector. Both filters keep that identity.
    let mut next = turn_trial_request(256);
    next.enemy_hp_at_most = Some(crate::TurnEnemyHpLimitV1 { slot: 0, hp: 45 });
    let torches = draft
        .research_turns_after(&next, Some(&parent.reference))
        .unwrap();
    assert!(torches.matching > 0);
    next.work = 0;
    next.enemy_hp_at_most.as_mut().unwrap().slot = 2;
    let collector = draft
        .research_turns_after(&next, Some(&parent.reference))
        .unwrap();
    assert_eq!(collector.matching, 0);
    assert_eq!(collector.generation_work, torches.generation_work);
    assert_eq!(draft.snapshot().unwrap(), original);
    draft.choose_turn(&parent.reference).unwrap();
    let moved = draft
        .states
        .last()
        .unwrap()
        .adapter
        .session()
        .active_combat
        .as_ref()
        .unwrap()
        .combat_state
        .entities
        .monsters
        .iter()
        .find(|m| m.id == collector_id)
        .unwrap();
    assert_eq!(moved.slot, 2);
    assert_eq!(moved.current_hp, collector_hp);
    assert!(draft.inspect_turn(&parent.reference).is_err());
}

#[test]
fn turn_research_filters_block_after_settlement_and_retention() {
    use sts_oracle_run_control::{
        content::{
            potions::{Potion, PotionId},
            powers::PowerId,
        },
        runtime::combat::{Power, PowerPayload},
    };
    let fixture = Fixture::new();
    for retain in [false, true] {
        let mut run = fixture.root.session().clone();
        let active = run.active_combat.as_mut().unwrap();
        active.combat_state.turn.energy = 0;
        active.combat_state.entities.player.block = 40;
        active.combat_state.entities.potions =
            vec![Some(Potion::new(PotionId::BlockPotion, 91_004))];
        if retain {
            active.combat_state.entities.power_db.insert(
                active.combat_state.entities.player.id,
                vec![Power {
                    power_type: PowerId::Barricade,
                    amount: -1,
                    instance_id: None,
                    extra_data: 0,
                    payload: PowerPayload::None,
                    just_applied: false,
                }],
            );
        }
        let root = RustLockstepAdapterV1::new(
            run,
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
                seed: 2_026_090_301,
                ascension_level: 20,
                final_act: true,
                neow_eligibility: NeowEligibilityV1::Mini,
                ..Default::default()
            })
            .unwrap(),
            "constructed",
            "block-filter",
        )
        .unwrap();
        let mut draft = ManualDraftWorkspaceV1::create(
            &fixture.session,
            fixture.draft.source.clone().unwrap(),
            root,
            worker(),
        )
        .unwrap();
        let original = draft.snapshot().unwrap();
        let mut request = turn_trial_request(0);
        request.potion_slots = vec![0];
        request.minimum_block = Some(-1);
        assert!(draft.research_turns(&request).is_err());
        assert!(draft.research.is_empty());
        request.minimum_block = None;
        let empty = draft.research_turns(&request).unwrap();
        assert!(empty.block_range.is_none());
        request.work = 256;
        request.limit = 0;
        let all = draft.research_turns(&request).unwrap();
        assert_eq!(all.coverage, "complete");
        let range = all.block_range.as_ref().unwrap();
        if retain {
            assert!(range.maximum > range.minimum);
            assert!(
                range.minimum < 40,
                "enemy attacks have already consumed block"
            );
        } else {
            assert_eq!((range.minimum, range.maximum), (0, 0));
        }
        request.work = 0;
        request.limit = 1;
        request.minimum_block = Some(range.maximum.max(1));
        request.outcome = Some(crate::TurnOutcomeV1::NextTurn);
        let filtered = draft.research_turns(&request).unwrap();
        assert_eq!(filtered.generation_work, all.generation_work);
        assert_eq!(filtered.engine_steps, all.engine_steps);
        if retain {
            assert!(filtered.matching > 0 && filtered.matching < all.total);
            let filtered_range = filtered.block_range.as_ref().unwrap();
            assert_eq!(
                (filtered_range.minimum, filtered_range.maximum),
                (range.maximum, range.maximum)
            );
            let candidate = &filtered.options[0];
            let detail = draft.inspect_turn(&candidate.reference).unwrap();
            assert_eq!(detail.successor.player.block, range.maximum);
            let mut next_request = turn_trial_request(256);
            next_request.minimum_block = Some(1);
            let next = draft
                .research_turns_after(&next_request, Some(&candidate.reference))
                .unwrap();
            for option in &next.options {
                assert!(
                    draft
                        .inspect_turn(&option.reference)
                        .unwrap()
                        .successor
                        .player
                        .block
                        >= 1
                );
            }
            assert_eq!(draft.snapshot().unwrap(), original);
            draft.choose_turn(&candidate.reference).unwrap();
            assert_eq!(
                draft.decision_state().unwrap().combat.unwrap().player.block,
                range.maximum
            );
        } else {
            assert_eq!(filtered.matching, 0);
            assert!(filtered.hp_range.is_none());
            assert!(filtered.block_range.is_none());
            assert_eq!(draft.snapshot().unwrap(), original);
            assert!(draft.selected_actions().is_empty());
        }
    }
}

#[test]
fn turn_research_filters_actual_outcomes_without_researching_or_inferring_victory_from_hp() {
    use crate::TurnOutcomeV1 as Outcome;
    use sts_oracle_run_control::content::potions::{Potion, PotionId};
    let fixture = Fixture::new();
    let mut run = fixture.root.session().clone();
    run.run_state.current_hp = 1;
    run.run_state.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 91_003))];
    let active = run.active_combat.as_mut().unwrap();
    active.combat_state.entities.player.current_hp = 1;
    active.combat_state.entities.potions = run.run_state.potions.clone();
    for monster in &mut active.combat_state.entities.monsters {
        monster.current_hp = 1;
    }
    let root = RustLockstepAdapterV1::new(
        run,
        sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        })
        .unwrap(),
        "constructed",
        "outcome-filter",
    )
    .unwrap();
    let mut draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        fixture.draft.source.clone().unwrap(),
        root,
        worker(),
    )
    .unwrap();
    let original = draft.snapshot().unwrap();
    let mut request = turn_trial_request(0);
    request.potion_slots = vec![0];
    request.outcome = Some(Outcome::Victory);
    let empty = draft.research_turns(&request).unwrap();
    assert_eq!(empty.coverage, "open");
    assert!(empty.outcomes.is_empty());
    assert_eq!(empty.matching, 0);
    request.work = 8192;
    request.limit = 0;
    request.outcome = None;
    let all = draft.research_turns(&request).unwrap();
    assert_eq!(all.coverage, "complete");
    assert_eq!(all.outcomes.values().sum::<usize>(), all.total);
    for outcome in [
        Outcome::NextTurn,
        Outcome::Victory,
        Outcome::Defeat,
        Outcome::Escape,
    ] {
        assert!(all.outcomes[&outcome] > 0);
        request.work = 0;
        request.limit = 8;
        request.outcome = Some(outcome);
        let filtered = draft.research_turns(&request).unwrap();
        assert_eq!(filtered.outcome, Some(outcome));
        assert_eq!(filtered.matching, all.outcomes[&outcome]);
        assert_eq!(filtered.outcomes, all.outcomes);
        assert_eq!(filtered.generation_work, all.generation_work);
        assert_eq!(filtered.engine_steps, all.engine_steps);
        let hp = filtered.hp_range.as_ref().unwrap();
        assert!(hp.minimum >= all.hp_range.as_ref().unwrap().minimum);
        assert!(hp.maximum <= all.hp_range.as_ref().unwrap().maximum);
        if outcome == Outcome::NextTurn {
            assert_eq!((hp.minimum, hp.maximum), (1, 1));
            assert!(hp.maximum < all.hp_range.as_ref().unwrap().maximum);
        }
        // Changing the page does not restrict ranges to the visible candidates.
        request.offset = filtered.matching;
        let beyond = draft.research_turns(&request).unwrap();
        assert!(beyond.options.is_empty());
        assert_eq!(beyond.hp_range, filtered.hp_range);
        assert_eq!(beyond.block_range, filtered.block_range);
        request.offset = 0;
        let expected = serde_json::to_value(outcome).unwrap();
        for option in &filtered.options {
            assert_eq!(option.boundary, expected.as_str().unwrap());
        }
        if outcome == Outcome::Escape {
            let option = &filtered.options[0];
            let detail = draft.inspect_turn(&option.reference).unwrap();
            assert!(detail.successor.player.hp > 0);
            assert!(detail.successor.monsters.iter().any(|m| m.hp > 0));
            draft.choose_turn(&option.reference).unwrap();
            assert!(draft.states[0]
                .adapter
                .verify_semantic_combat_victory_v1(draft.selected_actions())
                .is_err());
            draft.states[0]
                .adapter
                .verify_semantic_combat_completion_v1(draft.selected_actions())
                .unwrap();
            let line = draft.selected_actions().to_vec();
            assert!(draft.states[0]
                .adapter
                .verify_semantic_combat_completion_v1(&line[..line.len() - 1])
                .is_err());
            let mut beyond = line.clone();
            beyond.push(SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::CombatReward,
            });
            assert!(draft.states[0]
                .adapter
                .verify_semantic_combat_completion_v1(&beyond)
                .is_err());
            let complete = ManualCombatDraftV1 {
                schema: "sts-manual-combat-draft-v1".into(),
                session_id: fixture.session.session_id().into(),
                rust_version: "fixture".into(),
                source: None,
                root: draft.states[0].snapshot.clone(),
                actions: line,
                terminal: draft.snapshot().unwrap(),
            };
            complete.validate_for_v1(&draft.states[0].adapter).unwrap();
            while !draft.selected_actions().is_empty() {
                draft.undo().unwrap();
            }
        }
    }
    request.outcome = Some(Outcome::Victory);
    request.minimum_hp = Some(i32::MAX);
    let none = draft.research_turns(&request).unwrap();
    assert_eq!(none.matching, 0);
    assert!(none.hp_range.is_none());
    assert!(none.block_range.is_none());
    assert_eq!(none.outcomes, all.outcomes);
    assert_eq!(none.generation_work, all.generation_work);
    assert_eq!(draft.snapshot().unwrap(), original);
    assert!(draft.selected_actions().is_empty());
    assert!(draft.certify(&fixture.session).is_err());
    let mut invalid = serde_json::to_value(&request).unwrap();
    invalid["outcome"] = serde_json::json!("win");
    assert!(serde_json::from_value::<crate::TurnResearchRequestV1>(invalid).is_err());
}

#[test]
fn turn_remainder_preserves_prefix_and_keeps_end_turn_and_next_turn_separate() {
    use crate::TurnResearchStartV1::BeforeEndTurn;
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let original = draft.snapshot().unwrap();
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let parent = page
        .options
        .iter()
        .find(|c| c.action_count == 2 && c.boundary == "next_turn")
        .unwrap();
    let parent_detail = draft.inspect_turn(&parent.reference).unwrap();
    assert!(draft
        .research_turns_from(&turn_trial_request(0), None, BeforeEndTurn)
        .is_err());
    let empty = draft
        .research_turns_from(
            &turn_trial_request(0),
            Some(&parent.reference),
            BeforeEndTurn,
        )
        .unwrap();
    assert_eq!(empty.total, 0);
    assert_eq!(empty.coverage, "open");
    let remainder = draft
        .research_turns_from(
            &turn_trial_request(512),
            Some(&parent.reference),
            BeforeEndTurn,
        )
        .unwrap();
    // Ending immediately is still legal. Reopening the prefix does not assume
    // that another card is beneficial or force spending the remaining energy.
    let stop = remainder
        .options
        .iter()
        .find(|c| c.action_count == parent.action_count)
        .unwrap();
    let stop_detail = draft.inspect_turn(&stop.reference).unwrap();
    assert_eq!(
        serde_json::to_value(stop_detail.successor).unwrap(),
        serde_json::to_value(&parent_detail.successor).unwrap()
    );
    let child = remainder
        .options
        .iter()
        .find(|c| c.action_count > parent.action_count && c.boundary == "next_turn")
        .unwrap();
    assert_eq!(child.prefix_action_count, parent.action_count - 1);
    let detail = draft.inspect_turn(&child.reference).unwrap();
    assert_eq!(
        detail.successor.turn.turn_count,
        parent_detail.successor.turn.turn_count
    );
    assert_eq!(
        serde_json::to_value(&detail.actions[..child.prefix_action_count]).unwrap(),
        serde_json::to_value(&parent_detail.actions[..child.prefix_action_count]).unwrap()
    );
    let future = draft
        .research_turns_after(&turn_trial_request(256), Some(&parent.reference))
        .unwrap();
    assert_ne!(future.options[0].reference, remainder.options[0].reference);
    let cached = draft
        .research_turns_from(
            &turn_trial_request(0),
            Some(&parent.reference),
            BeforeEndTurn,
        )
        .unwrap();
    assert_eq!(cached.generation_work, remainder.generation_work);
    assert_eq!(cached.options[0].reference, remainder.options[0].reference);
    assert!(draft
        .research_turns_from(
            &turn_trial_request(0),
            Some(&child.reference),
            BeforeEndTurn
        )
        .is_err());
    assert_eq!(draft.snapshot().unwrap(), original);
    assert!(draft.selected_actions().is_empty());
    assert!(draft.certify(&fixture.session).is_err());
    draft.choose_turn(&child.reference).unwrap();
    assert_eq!(draft.selected_actions().len(), child.action_count);
    assert_eq!(
        draft
            .selected_actions()
            .iter()
            .filter(|a| **a == SemanticActionV1::EndTurn)
            .count(),
        1
    );
    assert!(draft.inspect_turn(&parent.reference).is_err());
    let chosen = draft.snapshot().unwrap();
    let selected = draft.selected_actions().to_vec();
    let directory = draft.directory.clone();
    let handle = draft.handle.clone();
    drop(draft);
    let mut reopened = fixture.reopen(directory, handle);
    assert_eq!(reopened.snapshot().unwrap(), chosen);
    assert_eq!(reopened.selected_actions(), selected);
}

#[test]
fn turn_remainder_allows_an_empty_prefix_without_granting_nested_research() {
    use crate::TurnResearchStartV1::BeforeEndTurn;
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let original = draft.snapshot().unwrap();
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let parent = page
        .options
        .iter()
        .find(|c| c.action_count == 1 && c.boundary == "next_turn")
        .unwrap();
    let repeated_root = draft
        .research_turns_from(
            &turn_trial_request(256),
            Some(&parent.reference),
            BeforeEndTurn,
        )
        .unwrap();
    assert!(repeated_root.after.is_some());
    assert_eq!(repeated_root.total, page.total);
    assert_eq!(repeated_root.generation_work, page.generation_work);
    for (original_option, continued) in page.options.iter().zip(&repeated_root.options) {
        assert_eq!(continued.prefix_action_count, 0);
        assert_eq!(continued.action_count, original_option.action_count);
        assert_ne!(continued.reference, original_option.reference);
    }
    assert!(draft
        .research_turns_after(
            &turn_trial_request(0),
            Some(&repeated_root.options[0].reference)
        )
        .is_err());
    assert_eq!(draft.snapshot().unwrap(), original);
    assert!(draft.selected_actions().is_empty());
}

#[test]
fn turn_remainder_replays_spent_potions_but_does_not_apply_the_removed_enemy_turn() {
    use crate::TurnResearchStartV1::BeforeEndTurn;
    use sts_complete_turn_view::CompleteTurnSelectedActionV1;
    use sts_oracle_run_control::content::potions::{Potion, PotionId};
    let mut fixture = Fixture::new();
    let mut run = fixture.root.session().clone();
    let potions = vec![
        Some(Potion::new(PotionId::FairyPotion, 92_001)),
        Some(Potion::new(PotionId::StrengthPotion, 92_002)),
    ];
    run.run_state.current_hp = 1;
    run.run_state.potions = potions.clone();
    let active = run.active_combat.as_mut().unwrap();
    active.combat_state.entities.player.current_hp = 1;
    active.combat_state.entities.potions = potions;
    active.combat_state.turn.energy = 1;
    for monster in &mut active.combat_state.entities.monsters {
        monster.current_hp = 1;
    }
    fixture.root = RustLockstepAdapterV1::new(
        run,
        sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        })
        .unwrap(),
        "constructed",
        "turn-remainder-potions",
    )
    .unwrap();
    let mut draft = fixture.workspace();
    let original = draft.snapshot().unwrap();
    let mut request = turn_trial_request(256);
    request.potion_slots = vec![1];
    let parents = draft.research_turns(&request).unwrap();
    let parent = parents
        .options
        .iter()
        .filter(|c| c.action_count == 2 && c.boundary == "next_turn")
        .find(|c| {
            let detail = draft.inspect_turn(&c.reference).unwrap();
            matches!(
                &detail.actions[0],
                CompleteTurnSelectedActionV1::Atomic {
                    potion: Some(_),
                    ..
                }
            )
        })
        .unwrap();
    let parent_detail = draft.inspect_turn(&parent.reference).unwrap();
    assert!(
        parent_detail.successor.potions[0].is_none(),
        "the original EndTurn used Fairy"
    );
    let children = draft
        .research_turns_from(
            &turn_trial_request(256),
            Some(&parent.reference),
            BeforeEndTurn,
        )
        .unwrap();
    let winner = children
        .options
        .iter()
        .find(|c| c.boundary == "victory")
        .unwrap();
    let winning_detail = draft.inspect_turn(&winner.reference).unwrap();
    assert!(
        winning_detail.successor.potions[0].is_some(),
        "the removed EndTurn must not consume Fairy"
    );
    assert!(
        winning_detail.successor.potions[1].is_none(),
        "the prefix still consumed Strength Potion"
    );
    assert_eq!(winner.prefix_action_count, 1);
    let empty = draft
        .research_turns_from(
            &turn_trial_request(0),
            Some(&parent.reference),
            BeforeEndTurn,
        )
        .unwrap();
    assert_eq!(empty.generation_work, children.generation_work);
    request.work = 0;
    let other_scope = draft
        .research_turns_from(&request, Some(&parent.reference), BeforeEndTurn)
        .unwrap();
    assert_eq!(
        other_scope.total, 0,
        "suffix potion permission has its own cache"
    );
    assert_eq!(draft.snapshot().unwrap(), original);
    assert!(draft.selected_actions().is_empty());
    draft.choose_turn(&winner.reference).unwrap();
    assert!(draft
        .selected_actions()
        .iter()
        .all(|a| *a != SemanticActionV1::EndTurn));
    assert!(draft.certify(&fixture.session).is_ok());
}

#[test]
fn turn_lookahead_keeps_candidates_private_and_selects_the_complete_two_turn_line() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let original = draft.snapshot().unwrap();
    let node = draft.current_node().unwrap();
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let parent = page
        .options
        .iter()
        .find(|c| c.action_count == 1 && c.boundary == "next_turn")
        .unwrap();
    let other = page
        .options
        .iter()
        .find(|c| c.action_count > 1 && c.boundary == "next_turn")
        .unwrap();
    let empty = draft
        .research_turns_after(&turn_trial_request(0), Some(&parent.reference))
        .unwrap();
    assert_eq!(empty.total, 0);
    assert!(empty.hp_range.is_none());
    assert_eq!(empty.coverage, "open");
    assert!(empty.after.is_some());
    let children = draft
        .research_turns_after(&turn_trial_request(256), Some(&parent.reference))
        .unwrap();
    let child = children
        .options
        .iter()
        .find(|c| c.boundary == "next_turn")
        .unwrap();
    assert_eq!(child.prefix_action_count, parent.action_count);
    assert!(child.action_count > child.prefix_action_count);
    let detail = draft.inspect_turn(&child.reference).unwrap();
    assert_eq!(detail.actions.len(), child.action_count);
    assert_eq!(detail.successor.turn.turn_count, 2);
    let range = children.hp_range.as_ref().unwrap();
    assert!((range.minimum..=range.maximum).contains(&detail.successor.player.hp));
    assert!(draft
        .research_turns_after(&turn_trial_request(0), Some(&child.reference))
        .is_err());
    draft
        .research_turns_after(&turn_trial_request(256), Some(&other.reference))
        .unwrap();
    let resumed = draft
        .research_turns_after(&turn_trial_request(0), Some(&parent.reference))
        .unwrap();
    assert_eq!(resumed.generation_work, children.generation_work);
    assert_eq!(resumed.engine_steps, children.engine_steps);
    assert_eq!(resumed.options[0].reference, children.options[0].reference);
    assert_eq!(draft.snapshot().unwrap(), original);
    assert_eq!(draft.current_node().unwrap().reference, node.reference);
    assert!(draft.selected_actions().is_empty());
    assert!(draft.certify(&fixture.session).is_err());
    draft.choose_turn(&child.reference).unwrap();
    assert_eq!(draft.selected_actions().len(), child.action_count);
    assert_eq!(
        draft
            .decision_state()
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .player
            .hp,
        detail.successor.player.hp
    );
    assert!(draft.inspect_turn(&parent.reference).is_err());
    assert!(draft.inspect_turn(&child.reference).is_err());
    let chosen = draft.snapshot().unwrap();
    let selected = draft.selected_actions().to_vec();
    let directory = draft.directory.clone();
    let handle = draft.handle.clone();
    // Warm return keeps generation work, but all old references remain expired.
    draft.switch_to(&node.reference).unwrap();
    let warm = draft.research_turns(&turn_trial_request(0)).unwrap();
    let parent_again = warm
        .options
        .iter()
        .find(|c| c.action_count == 1 && c.boundary == "next_turn")
        .unwrap();
    let warm_children = draft
        .research_turns_after(&turn_trial_request(0), Some(&parent_again.reference))
        .unwrap();
    assert_eq!(warm_children.generation_work, children.generation_work);
    assert_ne!(warm_children.options[0].reference, child.reference);
    draft
        .choose_turn(
            &warm_children
                .options
                .iter()
                .find(|c| c.action_count == child.action_count && c.boundary == "next_turn")
                .unwrap()
                .reference,
        )
        .unwrap();
    assert_eq!(draft.snapshot().unwrap(), chosen);
    drop(draft);
    let mut reopened = fixture.reopen(directory, handle);
    assert_eq!(reopened.snapshot().unwrap(), chosen);
    assert_eq!(reopened.selected_actions(), selected);
    assert!(reopened.inspect_turn(&child.reference).is_err());
    assert!(reopened.research.is_empty());
}

#[test]
fn turn_lookahead_keeps_potion_scopes_separate_and_certifies_only_a_complete_victory() {
    use sts_oracle_run_control::content::potions::{Potion, PotionId};
    let mut fixture = Fixture::new();
    let mut run = fixture.root.session().clone();
    let slots = vec![
        Some(Potion::new(PotionId::FairyPotion, 91_001)),
        Some(Potion::new(PotionId::StrengthPotion, 91_002)),
    ];
    run.run_state.current_hp = 1;
    run.run_state.potions = slots.clone();
    let active = run.active_combat.as_mut().unwrap();
    active.combat_state.entities.player.current_hp = 1;
    active.combat_state.entities.potions = slots;
    active.combat_state.turn.energy = 0;
    for monster in &mut active.combat_state.entities.monsters {
        monster.current_hp = 1;
    }
    fixture.root = RustLockstepAdapterV1::new(
        run,
        sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        })
        .unwrap(),
        "constructed",
        "lookahead-potion-victory",
    )
    .unwrap();
    let mut draft = fixture.workspace();
    let root = draft.snapshot().unwrap();
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let parent = page
        .options
        .iter()
        .find(|c| c.action_count == 1 && c.boundary == "next_turn")
        .unwrap();
    let children = draft
        .research_turns_after(&turn_trial_request(256), Some(&parent.reference))
        .unwrap();
    let winner = children
        .options
        .iter()
        .find(|c| c.boundary == "victory")
        .unwrap();
    let detail = draft.inspect_turn(&winner.reference).unwrap();
    let mut winning_request = turn_trial_request(0);
    winning_request.outcome = Some(crate::TurnOutcomeV1::Victory);
    let winners = draft
        .research_turns_after(&winning_request, Some(&parent.reference))
        .unwrap();
    assert_eq!(winners.outcomes, children.outcomes);
    assert_eq!(
        winners.matching,
        children.outcomes[&crate::TurnOutcomeV1::Victory]
    );
    assert!(winners
        .options
        .iter()
        .all(|option| option.boundary == "victory"));
    assert_eq!(winners.generation_work, children.generation_work);
    assert_eq!(winners.options[0].reference, winner.reference);
    assert!(
        detail.successor.potions[0].is_none(),
        "passive Fairy use in the prefix is retained"
    );
    assert!(detail.successor.potions[1].is_some());
    let mut request = turn_trial_request(0);
    request.potion_slots = vec![1];
    let alternate = draft
        .research_turns_after(&request, Some(&parent.reference))
        .unwrap();
    assert_eq!(alternate.total, 0);
    assert_eq!(alternate.generation_work, 0);
    request.work = 256;
    let allowed = draft
        .research_turns_after(&request, Some(&parent.reference))
        .unwrap();
    assert!(allowed.total > 0);
    assert_ne!(allowed.options[0].reference, children.options[0].reference);
    let preserved = draft
        .research_turns_after(&turn_trial_request(0), Some(&parent.reference))
        .unwrap();
    assert_eq!(preserved.generation_work, children.generation_work);
    assert!(draft
        .research_turns_after(&request, Some(&winner.reference))
        .is_err());
    assert_eq!(draft.snapshot().unwrap(), root);
    assert!(draft.selected_actions().is_empty());
    assert!(draft.certify(&fixture.session).is_err());
    draft.choose_turn(&winner.reference).unwrap();
    assert_eq!(draft.selected_actions().len(), winner.action_count);
    assert!(draft.certify(&fixture.session).is_ok());
    // A terminal root candidate also cannot acquire a continuation generator.
    for _ in 0..(winner.action_count - parent.action_count) {
        draft.undo().unwrap();
    }
    let terminal_page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let terminal = terminal_page
        .options
        .iter()
        .find(|c| c.boundary == "victory")
        .unwrap();
    let retained = draft.research.len();
    assert!(draft
        .research_turns_after(&request, Some(&terminal.reference))
        .is_err());
    assert_eq!(draft.research.len(), retained);
}

#[test]
fn turn_lookahead_requests_reuse_receipts_and_preserve_only_acknowledged_edits() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let parent = page
        .options
        .iter()
        .find(|c| c.action_count == 1 && c.boundary == "next_turn")
        .unwrap();
    let scope = draft.action_view().unwrap().reference_prefix().to_owned();
    let directory = draft.directory.clone();
    let handle = draft.handle.clone();
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let request = Request {
        id: 1,
        view: Some(scope.clone()),
        operation: Op::ResearchTurns {
            start: Default::default(),
            request: turn_trial_request(256),
            after: Some(parent.reference.clone()),
        },
    };
    let result = service.handle(request.clone()).unwrap();
    assert_eq!(result.status, "ok");
    assert_eq!(result.selected_actions, 0);
    let research = result.research.as_ref().unwrap();
    let retry = service.handle(request).unwrap();
    assert_eq!(
        retry.research.unwrap().generation_work,
        research.generation_work
    );
    let child = research
        .options
        .iter()
        .find(|c| c.boundary == "next_turn")
        .unwrap();
    let conflict = directory.join("00000001.json");
    fs::write(&conflict, b"test-injected publication conflict").unwrap();
    let choose = Request {
        id: 2,
        view: Some(scope.clone()),
        operation: Op::ChooseTurn {
            reference: child.reference.clone(),
        },
    };
    let partial = service.handle(choose.clone()).unwrap();
    assert_eq!(partial.status, "partial");
    assert_eq!(partial.selected_actions, parent.action_count);
    assert_eq!(
        service.handle(choose).unwrap().selected_actions,
        parent.action_count
    );
    let stale = service
        .handle(Request {
            id: 3,
            view: Some(scope),
            operation: Op::ResearchTurns {
                start: Default::default(),
                request: turn_trial_request(256),
                after: Some(parent.reference.clone()),
            },
        })
        .unwrap();
    assert_eq!(stale.status, "rejected");
    assert!(stale.research.is_none());
    drop(service);
    assert!(conflict.is_absolute() && conflict.starts_with(&fixture.repository));
    fs::remove_file(conflict).unwrap();
    let reopened = fixture.reopen(directory, handle);
    assert_eq!(reopened.selected_actions(), &[SemanticActionV1::EndTurn]);
    assert!(reopened.replay_stop().is_none());
}

#[test]
fn turn_research_retains_work_pages_and_manual_freedom_without_editing_the_root() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let root = draft.snapshot().unwrap();
    let mut page = draft.research_turns(&turn_trial_request(256)).unwrap();
    for _ in 0..8 {
        if page.total > 8 {
            break;
        }
        page = draft.research_turns(&turn_trial_request(1024)).unwrap();
    }
    assert!(page.total > 8, "fixture must exercise pagination");
    assert_eq!(draft.snapshot().unwrap(), root);
    assert!(draft.selected_actions().is_empty());
    let read = draft.research_turns(&turn_trial_request(0)).unwrap();
    assert_eq!(read.generation_work, page.generation_work);
    assert_eq!(read.engine_steps, page.engine_steps);
    assert_eq!(read.options[0].reference, page.options[0].reference);
    let mut request = turn_trial_request(0);
    request.offset = page.next_offset.unwrap();
    let next = draft.research_turns(&request).unwrap();
    assert_ne!(next.options[0].reference, page.options[0].reference);
    request.minimum_hp = Some(i32::MAX);
    let filtered = draft.research_turns(&request).unwrap();
    assert_eq!(filtered.total, page.total);
    assert_eq!(filtered.matching, 0);
    assert_eq!(filtered.generation_work, page.generation_work);

    let candidate = page
        .options
        .iter()
        .find(|c| c.boundary == "next_turn")
        .unwrap();
    let detail = draft.inspect_turn(&candidate.reference).unwrap();
    assert_eq!(detail.actions.len(), candidate.action_count);
    assert_eq!(draft.snapshot().unwrap(), root);
    draft.choose_turn(&candidate.reference).unwrap();
    assert_eq!(draft.selected_actions().len(), candidate.action_count);
    assert!(draft.choose_turn(&candidate.reference).is_err());
    assert!(draft.inspect_turn(&candidate.reference).is_err());
    for _ in 0..candidate.action_count {
        draft.undo().unwrap();
    }
    assert_eq!(draft.snapshot().unwrap(), root);
    let retained = draft.research_turns(&turn_trial_request(0)).unwrap();
    assert_eq!(retained.generation_work, page.generation_work);
    assert_ne!(retained.options[0].reference, page.options[0].reference);
    assert!(draft.inspect_turn(&candidate.reference).is_err());
    // A generated menu never restricts the ordinary legal action surface.
    let action = fixture.draft.actions[0].clone();
    draft.choose(action.clone()).unwrap();
    let prefix = draft.snapshot().unwrap();
    let directory = draft.directory.clone();
    let handle = draft.handle.clone();
    drop(draft);
    let mut reopened = fixture.reopen(directory, handle);
    assert_eq!(reopened.snapshot().unwrap(), prefix);
    assert_eq!(reopened.selected_actions(), &[action]);
    assert!(reopened
        .inspect_turn(&retained.options[0].reference)
        .is_err());
    let cold = reopened.research_turns(&turn_trial_request(0)).unwrap();
    assert_eq!(cold.generation_work, 0);
    assert_eq!(cold.coverage, "open");
}

#[test]
fn turn_research_requests_are_source_bound_and_retries_do_not_spend_more_work() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::new();
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), fixture.workspace());
    let first = service
        .handle(Request {
            id: 1,
            view: None,
            operation: Op::Observe,
        })
        .unwrap();
    let request = Request {
        id: 2,
        view: Some(first.view.clone()),
        operation: Op::ResearchTurns {
            start: Default::default(),
            request: turn_trial_request(256),
            after: None,
        },
    };
    let result = service.handle(request.clone()).unwrap();
    assert_eq!(result.status, "ok");
    assert_eq!(result.selected_actions, 0);
    let repeated = service.handle(request).unwrap();
    assert_eq!(
        result.research.as_ref().unwrap().generation_work,
        repeated.research.as_ref().unwrap().generation_work
    );
    let candidate = result
        .research
        .unwrap()
        .options
        .into_iter()
        .find(|c| c.boundary == "next_turn")
        .unwrap();
    let chosen_request = Request {
        id: 3,
        view: Some(first.view.clone()),
        operation: Op::ChooseTurn {
            reference: candidate.reference.clone(),
        },
    };
    let chosen = service.handle(chosen_request.clone()).unwrap();
    assert_eq!(chosen.status, "ok");
    assert_eq!(
        service.handle(chosen_request).unwrap().selected_actions,
        chosen.selected_actions
    );
    let stale = service
        .handle(Request {
            id: 4,
            view: Some(first.view),
            operation: Op::ResearchTurns {
                start: Default::default(),
                request: turn_trial_request(256),
                after: None,
            },
        })
        .unwrap();
    assert_eq!(stale.status, "rejected");
    assert!(stale.research.is_none());
    assert_eq!(stale.selected_actions, chosen.selected_actions);
}

#[test]
fn turn_research_rejects_invalid_scope_and_budget_and_cold_replays_selected_turns() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let mut request = turn_trial_request(0);
    request.work = 65_537;
    assert!(draft.research_turns(&request).is_err());
    request = turn_trial_request(0);
    request.potion_slots = vec![0, 0];
    assert!(draft.research_turns(&request).is_err());
    request.potion_slots = vec![63];
    assert!(draft.research_turns(&request).is_err());
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let candidate = page
        .options
        .iter()
        .find(|c| c.boundary == "next_turn")
        .unwrap();
    draft.choose_turn(&candidate.reference).unwrap();
    let after = draft.snapshot().unwrap();
    let actions = draft.selected_actions().to_vec();
    let directory = draft.directory.clone();
    let handle = draft.handle.clone();
    drop(draft);
    let mut reopened = fixture.reopen(directory, handle);
    assert_eq!(reopened.snapshot().unwrap(), after);
    assert_eq!(reopened.selected_actions(), actions);
    assert!(reopened.inspect_turn(&candidate.reference).is_err());
}

#[test]
fn turn_research_distinguishes_empty_results_budget_and_explicit_filters() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let initial = draft.research_turns(&turn_trial_request(0)).unwrap();
    assert_eq!(initial.total, 0);
    assert_eq!(initial.coverage, "open");
    assert_eq!(initial.generation_work, 0);
    let mut request = turn_trial_request(256);
    request.limit = 0;
    let overview = draft.research_turns(&request).unwrap();
    assert!(overview.total > 0);
    assert!(overview.options.is_empty());
    let mut request = turn_trial_request(0);
    request.enemy_hp_at_most = Some(crate::TurnEnemyHpLimitV1 { slot: 0, hp: -1 });
    let none = draft.research_turns(&request).unwrap();
    assert_eq!(none.matching, 0);
    assert_eq!(none.total, overview.total);
    assert_eq!(none.generation_work, overview.generation_work);
    request.enemy_hp_at_most = Some(crate::TurnEnemyHpLimitV1 { slot: 255, hp: 0 });
    assert!(draft.research_turns(&request).is_err());
    assert!(draft.selected_actions().is_empty());
}

#[test]
fn turn_research_potion_contract_keeps_passive_effects_and_separate_search_progress() {
    use sts_oracle_run_control::{
        agent::information::action::PublicCombatAtomicActionV1,
        content::potions::{Potion, PotionId},
    };
    let fixture = Fixture::new();
    let mut run = fixture.root.session().clone();
    let slots = vec![
        Some(Potion::new(PotionId::FairyPotion, 91_001)),
        Some(Potion::new(PotionId::BlockPotion, 91_002)),
    ];
    run.run_state.current_hp = 1;
    run.run_state.potions = slots.clone();
    let active = run.active_combat.as_mut().unwrap();
    active.combat_state.entities.player.current_hp = 1;
    active.combat_state.entities.potions = slots;
    let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    })
    .unwrap();
    let root =
        RustLockstepAdapterV1::new(run, bootstrap, "constructed", "passive-potion-trials").unwrap();
    let mut draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        fixture.draft.source.clone().unwrap(),
        root,
        worker(),
    )
    .unwrap();
    let original = draft.snapshot().unwrap();
    let page = draft.research_turns(&turn_trial_request(512)).unwrap();
    let end_turn = page.options.iter().find(|c| c.action_count == 1).unwrap();
    let detail = draft.inspect_turn(&end_turn.reference).unwrap();
    assert!(
        detail.successor.potions[0].is_none(),
        "passive Fairy can trigger even with no potion plays"
    );
    assert!(detail.successor.potions[1].is_some());
    let mut request = turn_trial_request(1024);
    request.potion_slots = vec![1];
    let mut allowed = draft.research_turns(&request).unwrap();
    let mut found_use = false;
    loop {
        for candidate in &allowed.options {
            let detail = draft.inspect_turn(&candidate.reference).unwrap();
            found_use |= detail.actions.iter().any(|a| {
                matches!(
                    a,
                    sts_complete_turn_view::CompleteTurnSelectedActionV1::Atomic {
                        action: PublicCombatAtomicActionV1::UsePotion {
                            potion_index: 1,
                            ..
                        },
                        ..
                    }
                )
            });
        }
        if found_use || allowed.next_offset.is_none() {
            break;
        }
        request.work = 0;
        request.offset = allowed.next_offset.unwrap();
        allowed = draft.research_turns(&request).unwrap();
    }
    assert!(found_use);
    let retained = draft.research_turns(&turn_trial_request(0)).unwrap();
    assert_eq!(retained.generation_work, page.generation_work);
    assert_eq!(draft.snapshot().unwrap(), original);
}

#[test]
fn turn_research_filters_settled_powers_without_equating_presence_with_positive_amount() {
    use sts_oracle_run_control::{
        content::{
            potions::{Potion, PotionId},
            powers::PowerId,
        },
        runtime::combat::{Power, PowerPayload},
    };
    let fixture = Fixture::new();
    let mut run = fixture.root.session().clone();
    let active = run.active_combat.as_mut().unwrap();
    active.combat_state.turn.energy = 0;
    active.combat_state.entities.potions = vec![
        Some(Potion::new(PotionId::StrengthPotion, 91_001)),
        Some(Potion::new(PotionId::SteroidPotion, 91_002)),
    ];
    active.combat_state.entities.power_db.insert(
        active.combat_state.entities.player.id,
        vec![
            (PowerId::Barricade, -1),
            (PowerId::Strength, -1),
            (PowerId::Dexterity, 0),
            (PowerId::Weak, 1),
        ]
        .into_iter()
        .map(|(power_type, amount)| Power {
            power_type,
            amount,
            instance_id: None,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        })
        .collect(),
    );
    let root = RustLockstepAdapterV1::new(
        run,
        sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        })
        .unwrap(),
        "constructed",
        "settled-power-filters",
    )
    .unwrap();
    let mut draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        fixture.draft.source.clone().unwrap(),
        root,
        worker(),
    )
    .unwrap();
    let original = draft.snapshot().unwrap();
    let condition = |power: &str, present, minimum_amount| crate::TurnPlayerPowerFilterV1 {
        power: power.into(),
        present,
        minimum_amount,
    };
    let mut request = turn_trial_request(256);
    request.potion_slots = vec![0, 1];
    // Invalid conditions do not create a generator or spend its allowance.
    for filters in [
        vec![condition("not a power", true, None)],
        vec![
            condition("Strength", true, None),
            condition("strength", false, None),
        ],
        vec![condition("Barricade", true, Some(1))],
        vec![condition("Weak", false, Some(0))],
    ] {
        request.player_powers = filters;
        assert!(draft.research_turns(&request).is_err());
        assert!(draft.research.is_empty());
    }
    request.player_powers.clear();
    let all = draft.research_turns(&request).unwrap();
    assert_eq!(all.coverage, "complete");
    assert!(all.total > 1);
    request.work = 0;
    request.player_powers = vec![
        condition("Barricade", true, None),
        condition("Dexterity", true, Some(0)),
        condition("Weak", false, None),
    ];
    let present = draft.research_turns(&request).unwrap();
    assert_eq!(present.matching, all.total);
    request
        .player_powers
        .push(condition("Strength", true, Some(1)));
    let permanent = draft.research_turns(&request).unwrap();
    assert!(
        permanent.matching > 0 && permanent.matching < all.total,
        "{}\n{}",
        all.summary,
        permanent.summary
    );
    assert_eq!(permanent.generation_work, all.generation_work);
    for option in &permanent.options {
        let detail = draft.inspect_turn(&option.reference).unwrap();
        assert_eq!(
            detail
                .successor
                .player
                .powers
                .iter()
                .find(|p| p.power == PowerId::Strength)
                .unwrap()
                .amount,
            1
        );
        assert!(detail.successor.potions[0].is_none());
        assert!(option
            .player_power_changes
            .iter()
            .any(|p| p.power == PowerId::Strength && p.before == Some(-1) && p.after == Some(1)));
        assert!(option
            .player_power_changes
            .iter()
            .any(|p| p.power == PowerId::Weak && p.before == Some(1) && p.after.is_none()));
        assert!(!option
            .player_power_changes
            .iter()
            .any(|p| p.power == PowerId::Barricade));
    }
    // Flex's temporary strength has expired at this boundary, even if played.
    request.player_powers = vec![condition("Strength", true, Some(2))];
    assert_eq!(draft.research_turns(&request).unwrap().matching, 0);
    request.player_powers = vec![condition("Strength", false, None)];
    assert_eq!(draft.research_turns(&request).unwrap().matching, 0);
    assert_eq!(draft.snapshot().unwrap(), original);
    assert!(draft.selected_actions().is_empty());
    // Filtering neither expires an existing candidate nor loses other choices.
    let selected = &permanent.options[0];
    draft.choose_turn(&selected.reference).unwrap();
    assert_eq!(draft.selected_actions().len(), selected.action_count);
}

#[test]
fn turn_research_storage_failure_preserves_only_acknowledged_actions() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let page = draft.research_turns(&turn_trial_request(256)).unwrap();
    let candidate = page
        .options
        .iter()
        .find(|c| c.action_count > 1 && c.boundary == "next_turn")
        .unwrap();
    let directory = draft.directory.clone();
    let handle = draft.handle.clone();
    let conflict = directory.join("00000001.json");
    fs::write(&conflict, b"test-injected publication conflict").unwrap();
    let scope = draft.action_view().unwrap().reference_prefix().to_owned();
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let request = Request {
        id: 1,
        view: Some(scope),
        operation: Op::ChooseTurn {
            reference: candidate.reference.clone(),
        },
    };
    let result = service.handle(request.clone()).unwrap();
    assert_eq!(result.status, "partial");
    assert_eq!(result.selected_actions, 1);
    assert_eq!(service.handle(request).unwrap().selected_actions, 1);
    drop(service);
    assert!(conflict.is_absolute() && conflict.starts_with(&fixture.repository));
    fs::remove_file(conflict).unwrap();
    let reopened = fixture.reopen(directory, handle);
    assert_eq!(reopened.selected_actions().len(), 1);
    assert!(reopened.replay_stop().is_none());
}

#[test]
fn match_workspace_preserves_layout_branches_and_rejects_an_invalid_batch_before_editing() {
    use sts_oracle_run_control::{
        content::events::match_and_keep::init_match_game_board,
        state::{
            core::EngineState,
            events::{EventId, EventState},
            map::RoomType,
        },
    };
    let mut fixture = Fixture::new();
    let config = RunControlConfig {
        seed: 137,
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&config).unwrap();
    let mut run = RunControlSession::new(config);
    run.run_state.map.current_x = 0;
    run.run_state.map.current_y = 0;
    run.run_state.map.graph[0][0].class = Some(RoomType::EventRoom);
    run.run_state.floor_num = 1;
    let mut event = EventState::new(EventId::MatchAndKeep);
    init_match_game_board(&mut run.run_state, &mut event);
    event.current_screen = 2;
    run.run_state.event_state = Some(event);
    run.engine_state = EngineState::EventRoom;
    fixture.root = RustLockstepAdapterV1::new(run, bootstrap, "fixture", "match").unwrap();
    let mut draft = fixture.workspace();
    let room: serde_json::Value =
        crate::manual::decode_domain(&draft.snapshot().unwrap(), ParityDomainV1::Room).unwrap();
    let board: sts_parity_lockstep::NaturalMatchGameV1 =
        serde_json::from_value(room["match_game"].clone()).unwrap();
    assert!(draft.decision_state().unwrap().room["match_game"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .all(|cell| cell["card_id"].is_null()));
    let first = &board.cards[0];
    let matching = board
        .cards
        .iter()
        .find(|c| c.position != first.position && c.card_id == first.card_id)
        .unwrap()
        .position;
    let other = board
        .cards
        .iter()
        .find(|c| c.card_id != first.card_id)
        .unwrap()
        .position;
    draft.mark("root").unwrap();
    let start = draft.current_node().unwrap();
    let before = draft.snapshot().unwrap();
    assert!(draft
        .flip_match_cells(&[first.position, first.position])
        .is_err());
    assert_eq!(draft.snapshot().unwrap(), before);
    draft.flip_match_cells(&[first.position, matching]).unwrap();
    draft.mark("take pair").unwrap();
    let taken = draft.current_node().unwrap();
    let taken_state = draft.snapshot().unwrap();
    assert_eq!(
        draft.decision_state().unwrap().room["match_game"]["attempts_remaining"],
        4
    );
    draft.switch_to(&start.reference).unwrap();
    draft.flip_match_cells(&[first.position, other]).unwrap();
    assert_eq!(
        draft.decision_state().unwrap().room["match_game"]["cards"][first.position as usize]
            ["matched"],
        false
    );
    assert!(draft.compare_with(&taken.reference).is_ok());
    let directory = draft.directory.clone();
    let handle = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(directory, handle);
    draft.switch_to(&taken.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), taken_state);
    assert!(draft.flip_match_cells(&[first.position]).is_err());
    assert!(draft.certify(&fixture.session).is_err());

    // A late award is the fifth attempt's product, not the first obtained card.
    draft.switch_to(&start.reference).unwrap();
    let mut late_pair = [first.position, other].repeat(4);
    late_pair.extend([first.position, matching]);
    draft.flip_match_cells(&late_pair).unwrap();
    let final_state = draft.decision_state().unwrap();
    assert_eq!(final_state.room["screen"], 3);
    assert_eq!(final_state.room["match_game"]["attempts_remaining"], 0);
    let binding = sts_parity_lockstep::natural_run_card_binding_v1(
        &sts_parity_lockstep::NaturalRunCardSourceV1::EventGrant {
            room_id: sts_parity_lockstep::natural_map_node_id_v1(0, 0).unwrap(),
            event_id: "matchandkeep".into(),
            product_slot: 4,
        },
    )
    .unwrap();
    assert!(final_state
        .deck
        .as_array()
        .unwrap()
        .iter()
        .any(|card| { card["binding"] == binding && card["card_id"] == first.card_id }));
    assert!(draft.flip_match_cells(&[other]).is_err());
}

#[test]
fn astrolabe_workspace_keeps_ordered_branches_and_stops_at_the_next_act() {
    use sts_oracle_run_control::{
        content::relics::{astrolabe, RelicId, RelicState},
        state::{core::EngineState, map::RoomType},
    };
    let mut fixture = Fixture::new();
    let config = RunControlConfig {
        seed: 137,
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&config).unwrap();
    let mut run = RunControlSession::new(config);
    run.run_state.event_state = None;
    run.run_state.act_num = 2;
    run.run_state.floor_num = 34;
    run.run_state.map.current_x = 0;
    run.run_state.map.current_y = 0;
    run.run_state.map.graph[0][0].class = Some(RoomType::TreasureRoom);
    run.run_state
        .relics
        .push(RelicState::new(RelicId::Astrolabe));
    run.run_state.pending_boss_act_transition = true;
    run.engine_state = astrolabe::on_equip(&mut run.run_state, EngineState::MapNavigation).unwrap();
    fixture.root = RustLockstepAdapterV1::new(run, bootstrap, "fixture", "astrolabe").unwrap();
    let mut draft = fixture.workspace();
    draft.mark("before transform").unwrap();
    let start = draft.current_node().unwrap();
    let choices = actions(&mut fixture.root)
        .into_iter()
        .filter(|a| {
            matches!(a,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::GridSelect, choice }
        if choice.kind == "deck_card")
        })
        .take(3)
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 3);
    let old_view = draft.action_view().unwrap();
    let old_handle = old_view.actions().actions[0].handle.clone();
    draft.choose(choices[0].clone()).unwrap();
    draft.mark("first choice").unwrap();
    let partial = draft.current_node().unwrap();
    let before_invalid = draft.snapshot().unwrap();
    assert!(draft.choose(choices[0].clone()).is_err());
    assert!(draft.choose_handle(&old_handle).is_err());
    assert!(draft.choose(SemanticActionV1::EndTurn).is_err());
    assert_eq!(draft.snapshot().unwrap(), before_invalid);
    for action in &choices[1..] {
        draft.choose(action.clone()).unwrap();
    }
    draft.mark("first result").unwrap();
    let first = draft.current_node().unwrap();
    let first_snapshot = draft.snapshot().unwrap();
    assert_eq!(first_snapshot.boundary_kind, "natural_map");
    assert_eq!(
        draft
            .states
            .last()
            .unwrap()
            .adapter
            .session()
            .run_state
            .act_num,
        3
    );
    let map_action = actions(&mut draft.states.last().unwrap().adapter.clone())
        .into_iter()
        .find(|a| {
            matches!(
                a,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
            )
        })
        .unwrap();
    assert!(draft.choose(map_action).is_err());
    assert!(draft.certify(&fixture.session).is_err());
    draft.switch_to(&start.reference).unwrap();
    for action in choices.iter().rev() {
        draft.choose(action.clone()).unwrap();
    }
    draft.mark("reversed selection").unwrap();
    let second = draft.current_node().unwrap();
    assert_eq!(
        draft.selected_actions(),
        choices.iter().rev().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        draft.compare_with(&first.reference).unwrap().common_actions,
        0
    );
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path, name);
    assert_eq!(draft.current_node().unwrap().reference, second.reference);
    draft.switch_to(&first.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), first_snapshot);
    draft.switch_to(&partial.reference).unwrap();
    assert_eq!(draft.selected_actions(), &choices[..1]);
    draft.undo().unwrap();
    assert_eq!(draft.current_node().unwrap().reference, start.reference);
}

#[test]
fn decision_inspection_keeps_public_map_without_parity_schedules() {
    let mut fixture = Fixture::new();
    let before = fixture.root.observe().unwrap();
    let state = crate::DecisionStateV1::capture(fixture.root.session(), &before).unwrap();
    let inspected = serde_json::to_value(&state).unwrap();
    assert_eq!(inspected["run"]["gold"], state.run.gold);
    for hidden in [
        "encounter_schedule",
        "event_schedule",
        "relic_pools",
        "drop_state",
        "seed",
    ] {
        assert!(inspected["run"].get(hidden).is_none(), "{hidden}");
    }
    assert_eq!(
        state.map,
        sts_oracle_run_control::eval::run_control::planner_public_map_v1(fixture.root.session())
    );
    assert!(!state.map.nodes.is_empty());
    assert!(state.render_map().is_ascii());
    assert_eq!(fixture.root.observe().unwrap(), before);
}

#[test]
fn strategic_card_branches_reopen_and_stop_before_combat_without_exporting_plans() {
    let mut fixture = Fixture::new();
    let mut root = reward_fixture_root(&fixture);
    let open_cards = actions(&mut root)
        .into_iter()
        .find(|a| {
            matches!(a,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::CombatReward, choice }
        if choice.id == "card")
        })
        .unwrap();
    step(&mut root, &open_cards);
    fixture.root = root;
    let mut draft = fixture.workspace();
    let start = draft.current_node().unwrap();
    let before = draft.snapshot().unwrap();
    let pick = actions(&mut fixture.root)
        .into_iter()
        .find(|a| {
            matches!(
                a,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CardReward,
                    ..
                }
            )
        })
        .unwrap();
    let stale = draft.action_view().unwrap().actions().actions[0]
        .handle
        .clone();
    draft.choose(pick.clone()).unwrap();
    let picked = draft.snapshot().unwrap();
    draft.mark("take card").unwrap();
    let taken = draft.current_node().unwrap();
    assert!(draft.choose_handle(&stale).is_err());
    draft.switch_to(&start.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), before);
    draft
        .choose(SemanticActionV1::Cancel {
            surface: DecisionSurfaceV1::CardReward,
        })
        .unwrap();
    draft.mark("skip card").unwrap();
    assert_ne!(
        draft.snapshot().unwrap().domains[&ParityDomainV1::MasterDeck],
        picked.domains[&ParityDomainV1::MasterDeck]
    );
    assert_eq!(
        draft.compare_with(&taken.reference).unwrap().common_actions,
        0
    );
    let next = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::CombatReward,
    };
    draft.choose(next).unwrap();
    let map = actions(&mut draft.states.last().unwrap().adapter.clone()).into_iter().find(|a| matches!(a,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Map, choice }
        if choice.attributes.get("room") == Some(&sts_parity_lockstep::SemanticScalarV1::Text("monster".into())))).unwrap();
    draft.choose(map).unwrap();
    assert!(draft
        .states
        .last()
        .unwrap()
        .adapter
        .session()
        .active_combat
        .is_some());
    assert!(draft.strategic_preview_stop().is_some());
    let terminal = draft.snapshot().unwrap();
    let combat_node = draft.current_node().unwrap();
    let count = draft.selected_actions().len();
    assert!(draft.choose(SemanticActionV1::EndTurn).is_err());
    assert!(draft.certify(&fixture.session).is_err());
    assert_eq!(draft.snapshot().unwrap(), terminal);
    assert_eq!(draft.selected_actions().len(), count);
    assert!(!fixture.session.directory().join("drafts").exists());
    assert!(!fixture.session.directory().join("reward-plans").exists());
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path, name);
    assert_eq!(draft.snapshot().unwrap(), terminal);
    assert!(draft.strategic_preview_stop().is_some());
    draft.switch_to(&taken.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), picked);
    assert!(draft.strategic_preview_stop().is_none());
    draft.undo().unwrap();
    assert_eq!(draft.snapshot().unwrap(), before);
    draft.switch_to(&combat_node.reference).unwrap();
    let mut service = crate::DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let reply = service
        .handle(crate::DecisionRequestV1 {
            id: 1,
            view: None,
            operation: crate::DecisionOperationV1::Inspect {
                section: crate::DecisionSectionV1::Position,
            },
        })
        .unwrap();
    assert!(reply.choices.unwrap().actions.is_empty());
    assert!(reply.state.unwrap().combat.is_some());
}

#[test]
fn strategic_shop_and_smith_use_normal_resources_and_keep_alternatives() {
    use sts_oracle_run_control::{
        content::cards::CardId,
        state::{
            core::EngineState,
            shop::{ShopCard, ShopState},
        },
    };
    for smith in [false, true] {
        let mut fixture = Fixture::new();
        let config = RunControlConfig {
            seed: 137,
            ascension_level: 20,
            final_act: true,
            ..Default::default()
        };
        let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&config).unwrap();
        let mut run = RunControlSession::new(config);
        run.run_state.event_state = None;
        run.run_state.gold = 100;
        run.run_state.map.current_x = 1;
        run.run_state.map.current_y = 4;
        run.engine_state = if smith {
            EngineState::Campfire
        } else {
            EngineState::Shop(ShopState {
                cards: vec![
                    ShopCard {
                        card_id: CardId::Exhume,
                        upgrades: 0,
                        price: 40,
                        can_buy: true,
                        blocked_reason: None,
                    },
                    ShopCard {
                        card_id: CardId::Offering,
                        upgrades: 0,
                        price: 80,
                        can_buy: true,
                        blocked_reason: None,
                    },
                ],
                ..Default::default()
            })
        };
        fixture.root = RustLockstepAdapterV1::new(run, bootstrap, "fixture", "strategic").unwrap();
        let mut draft = fixture.workspace();
        let start = draft.current_node().unwrap();
        let original = draft.snapshot().unwrap();
        if smith {
            let action = actions(&mut fixture.root).into_iter().find(|a| matches!(a,
                SemanticActionV1::Select { surface: DecisionSurfaceV1::Campfire, choice } if choice.id == "smith")).unwrap();
            draft.choose(action).unwrap();
            let options = actions(&mut draft.states.last().unwrap().adapter.clone()).into_iter().filter(|a| matches!(a,
                SemanticActionV1::Select { surface: DecisionSurfaceV1::Campfire, choice } if choice.kind == "deck_card")).collect::<Vec<_>>();
            let selection = draft.current_node().unwrap();
            draft.choose(options[0].clone()).unwrap();
            let first = draft.snapshot().unwrap();
            draft.switch_to(&selection.reference).unwrap();
            draft.choose(options[1].clone()).unwrap();
            assert_ne!(
                draft.snapshot().unwrap().domains[&ParityDomainV1::MasterDeck],
                first.domains[&ParityDomainV1::MasterDeck]
            );
        } else {
            let options = actions(&mut fixture.root).into_iter().filter(|a| matches!(a,
                SemanticActionV1::Select { surface: DecisionSurfaceV1::Shop, choice } if choice.kind == "shop_card")).collect::<Vec<_>>();
            draft.choose(options[0].clone()).unwrap();
            let after_buy = draft.snapshot().unwrap();
            assert_eq!(
                draft
                    .states
                    .last()
                    .unwrap()
                    .adapter
                    .session()
                    .run_state
                    .gold,
                60
            );
            assert!(draft.choose(options[1].clone()).is_err());
            assert_eq!(draft.snapshot().unwrap(), after_buy);
            draft.switch_to(&start.reference).unwrap();
            draft.choose(options[1].clone()).unwrap();
            assert_eq!(
                draft
                    .states
                    .last()
                    .unwrap()
                    .adapter
                    .session()
                    .run_state
                    .gold,
                20
            );
        }
        assert!(draft.certify(&fixture.session).is_err());
        let second = draft.current_node().unwrap();
        let result = draft.snapshot().unwrap();
        let path = draft.directory.clone();
        let name = draft.handle.clone();
        drop(draft);
        let mut draft = fixture.reopen(path, name);
        assert_eq!(draft.snapshot().unwrap(), result);
        draft.switch_to(&start.reference).unwrap();
        assert_eq!(draft.snapshot().unwrap(), original);
        draft.switch_to(&second.reference).unwrap();
        assert_eq!(draft.snapshot().unwrap(), result);
    }
}

#[test]
fn decision_rewards_keep_full_slot_potions_visible_without_granting_pickup() {
    use sts_oracle_run_control::{
        content::potions::{Potion, PotionId},
        state::{
            core::EngineState,
            rewards::{RewardItem, RewardState},
        },
    };
    let config = RunControlConfig::default();
    let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&config).unwrap();
    let mut run = RunControlSession::new(config);
    run.run_state.event_state = None;
    run.run_state.map.current_x = 0;
    run.run_state.map.current_y = 0;
    for (index, slot) in run.run_state.potions.iter_mut().enumerate() {
        *slot = Some(Potion::new(PotionId::BlockPotion, index as u32));
    }
    run.engine_state = EngineState::RewardScreen(RewardState {
        items: vec![
            RewardItem::Gold { amount: 25 },
            RewardItem::Potion {
                potion_id: PotionId::StrengthPotion,
            },
        ],
        ..RewardState::new()
    });
    let mut rust = RustLockstepAdapterV1::new(run, bootstrap, "fixture", "full-rewards").unwrap();
    let before = rust.observe().unwrap();
    let full = crate::DecisionStateV1::capture(rust.session(), &before).unwrap();
    let text = full.render(None, true).unwrap();
    assert!(text.contains("Strength Potion"));
    assert!(text.contains("slots full"));
    assert!(full
        .render_with_action_menu(None, false)
        .unwrap()
        .contains("Strength Potion"));
    assert!(!full
        .offers
        .iter()
        .any(|a| matches!(a, SemanticActionV1::Select { choice, .. } if choice.id == "potion")));
    assert_eq!(rust.observe().unwrap(), before);
    let discard = actions(&mut rust)
        .into_iter()
        .find(|a| matches!(a, SemanticActionV1::DiscardPotion { .. }))
        .unwrap();
    step(&mut rust, &discard);
    let open =
        crate::DecisionStateV1::capture(rust.session(), &rust.clone().observe().unwrap()).unwrap();
    assert_eq!(open.rewards, full.rewards);
    assert!(open
        .offers
        .iter()
        .any(|a| matches!(a, SemanticActionV1::Select { choice, .. } if choice.id == "potion")));
    let delta = open.render(Some(&full), false).unwrap();
    assert!(delta.contains("Strength Potion"));
    assert!(!delta.contains("slots full"));
    let pickup = actions(&mut rust)
        .into_iter()
        .find(|a| matches!(a, SemanticActionV1::Select { choice, .. } if choice.id == "potion"))
        .unwrap();
    step(&mut rust, &pickup);
    let taken =
        crate::DecisionStateV1::capture(rust.session(), &rust.clone().observe().unwrap()).unwrap();
    assert_ne!(taken.rewards, open.rewards);
    assert!(!taken
        .render(Some(&open), false)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("Rewards:"))
        .any(|line| line.contains("Strength Potion")));
    step(
        &mut rust,
        &SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward,
        },
    );
    assert!(
        crate::DecisionStateV1::capture(rust.session(), &rust.clone().observe().unwrap())
            .unwrap()
            .rewards
            .is_none()
    );
}

#[test]
fn decision_monster_census_keeps_revival_and_new_summons_visible() {
    use sts_oracle_run_control::{
        agent::information::state::CombatLearningEnemyIdentityV1, content::monsters::EnemyId,
    };
    let mut fixture = Fixture::new();
    let snapshot = fixture.root.observe().unwrap();
    let mut state = crate::DecisionStateV1::capture(fixture.root.session(), &snapshot).unwrap();
    // Presentation-only observations, not constructed parity evidence.
    let combat = state.combat.as_mut().unwrap();
    let template = combat.monsters[0].clone();
    combat.monsters = (0..40)
        .map(|slot| {
            let mut m = template.clone();
            m.slot = slot;
            m.enemy = CombatLearningEnemyIdentityV1::Known {
                enemy_id: EnemyId::GremlinWarrior,
            };
            m.hp = 0;
            m.block = 0;
            m.alive = false;
            m.half_dead = false;
            m.escaped = false;
            m
        })
        .collect();
    let mut reviving = template.clone();
    reviving.slot = 40;
    reviving.enemy = CombatLearningEnemyIdentityV1::Known {
        enemy_id: EnemyId::AwakenedOne,
    };
    reviving.hp = 0;
    reviving.block = 0;
    reviving.alive = false;
    reviving.half_dead = true;
    combat.monsters.push(reviving);
    let full = state.render(None, false).unwrap();
    assert!(full.contains("Defeated: 40x Mad Gremlin"));
    let revived_row = full
        .lines()
        .find(|line| line.starts_with("Awakened One:"))
        .unwrap();
    assert!(revived_row.contains("Reviving"));
    assert!(!revived_row.contains("Block 0"));
    assert_eq!(
        full.lines()
            .filter(|line| line.contains("Mad Gremlin"))
            .count(),
        1
    );
    assert!(state.render(Some(&state), false).unwrap().is_empty());

    let before = state.clone();
    let combat = state.combat.as_mut().unwrap();
    combat.monsters[40].alive = true;
    combat.monsters[40].half_dead = false;
    combat.monsters[40].hp = 300;
    // A replaced slot must show a new live minion even after many dead ones.
    combat.monsters[0].alive = true;
    combat.monsters[0].hp = 20;
    let delta = state.render(Some(&before), false).unwrap();
    assert!(delta.contains("Mad Gremlin #1: HP 20"));
    assert!(delta.contains("Awakened One: HP 300"));
    assert!(!delta.contains("Reviving"));
    assert!(!delta.contains("Defeated:"));
}

#[test]
fn decision_reward_labels_keep_exact_actions_and_map_return_destination() {
    let fixture = Fixture::new();
    let mut root = reward_fixture_root(&fixture);
    let snapshot = root.observe().unwrap();
    let actions = sts_parity_lockstep::SemanticActionSpaceV1::from_snapshot(&snapshot).unwrap();
    let unchanged = actions.clone();
    let labels = crate::describe_manual_actions_v1(&snapshot, &actions, &[]).unwrap();
    let offer = actions.actions.iter().find(|entry| matches!(&entry.action,
        SemanticActionV1::Select { choice, .. } if choice.kind == "run_reward" && choice.id == "card"
    )).unwrap();
    let label = &labels[&offer.handle];
    assert!(label.starts_with("Take Card reward:"));
    assert!(!label.contains("card_0="));
    assert_eq!(actions, unchanged);
    step(
        &mut root,
        &SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward,
        },
    );
    let snapshot = root.observe().unwrap();
    let state = crate::DecisionStateV1::capture(root.session(), &snapshot).unwrap();
    assert_eq!(state.back.as_deref(), Some("Back to rewards"));
    step(
        &mut root,
        &SemanticActionV1::Cancel {
            surface: DecisionSurfaceV1::Map,
        },
    );
    assert_eq!(
        root.observe().unwrap().boundary_kind,
        "natural_combat_reward"
    );
}

fn burning_pact_fixture() -> (Fixture, RustLockstepAdapterV1) {
    let fixture = Fixture::with_route(2_026_090_709, Some("F01X05"));
    let mut root = reward_fixture_root(&fixture);
    for wanted in ["card", "burningpact"] {
        let action = actions(&mut root)
            .into_iter()
            .find(|action| {
                matches!(action,
            SemanticActionV1::Select { choice, .. } if choice.id == wanted)
            })
            .unwrap();
        step(&mut root, &action);
    }
    step(
        &mut root,
        &SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward,
        },
    );
    let next = actions(&mut root).into_iter().find(|action| matches!(action,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Map, choice } if choice.id == "F02X04")).unwrap();
    step(&mut root, &next);
    for _ in 0..3 {
        if let Some(pact) = actions(&mut root).into_iter().find(|a| {
            matches!(a,
            SemanticActionV1::PlayCard {card_id, ..} if card_id == "burningpact")
        }) {
            step(&mut root, &pact);
            assert_eq!(root.observe().unwrap().boundary_kind, "natural_hand_select");
            return (fixture, root);
        }
        step(&mut root, &SemanticActionV1::EndTurn);
    }
    panic!("fixed natural fixture must draw Burning Pact within its first deck");
}

#[test]
fn burning_pact_branches_preserve_selection_queue_cards_rng_and_parent() {
    let (fixture, root) = burning_pact_fixture();
    let mut draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        fixture.draft.source.clone().unwrap(),
        root.clone(),
        worker(),
    )
    .unwrap();
    draft.mark("choose exhaust").unwrap();
    let pending = draft.current_node().unwrap();
    let pending_state = draft.snapshot().unwrap();
    let sts_parity_lockstep::NaturalRunCombatV4::PlayerDecision {
        state: pending_combat,
    } = crate::manual::decode_domain(&pending_state, ParityDomainV1::Combat).unwrap()
    else {
        panic!("Burning Pact selection must retain its active combat projection");
    };
    let actual = &root.session().active_combat.as_ref().unwrap().combat_state;
    assert_eq!(pending_combat.player.energy, actual.turn.energy);
    assert_eq!(
        pending_combat.player.current_hp,
        actual.entities.player.current_hp
    );
    assert_eq!(pending_combat.zones.hand.len(), actual.zones.hand.len());
    assert_eq!(
        pending_combat.zones.draw_next_first.len(),
        actual.zones.draw_pile.len()
    );
    assert_eq!(pending_combat.zones.limbo.len(), 1);
    assert_eq!(pending_combat.zones.limbo[0].card_id, "burningpact");
    assert_eq!(
        pending_combat.monsters.len(),
        actual.entities.monsters.len()
    );
    let pending_internal = serde_json::to_value(RunControlSessionCheckpointV1::from_session(
        draft.states.last().unwrap().adapter.session(),
    ))
    .unwrap();
    let selections = draft
        .action_view()
        .unwrap()
        .actions()
        .actions
        .iter()
        .filter(|entry| matches!(entry.action, SemanticActionV1::SubmitSelection { .. }))
        .map(|entry| entry.action.clone())
        .collect::<Vec<_>>();
    assert!(selections.len() >= 2);
    draft.choose(selections[0].clone()).unwrap();
    draft.mark("exhaust first").unwrap();
    let first = draft.current_node().unwrap();
    let first_state = draft.snapshot().unwrap();
    assert_eq!(first_state.boundary_kind, "natural_combat");
    draft.switch_to(&pending.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), pending_state);
    assert_eq!(
        serde_json::to_value(RunControlSessionCheckpointV1::from_session(
            draft.states.last().unwrap().adapter.session()
        ))
        .unwrap(),
        pending_internal
    );
    draft.choose(selections[1].clone()).unwrap();
    draft.mark("exhaust second").unwrap();
    let second = draft.current_node().unwrap();
    let second_state = draft.snapshot().unwrap();
    let comparison = draft.compare_with(&first.reference).unwrap();
    assert_eq!(comparison.common_actions, 0);
    assert_ne!(
        comparison.current_snapshot.domains[&ParityDomainV1::Combat],
        comparison.other_snapshot.domains[&ParityDomainV1::Combat]
    );
    let cache_size = draft.cache.len();
    draft.switch_to(&first.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), first_state);
    draft.switch_to(&second.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), second_state);
    assert_eq!(draft.cache.len(), cache_size);
    assert!(
        draft.certify(&fixture.session).is_err(),
        "a local exhaust experiment is not a complete victory"
    );
    let path = draft.directory.clone();
    let handle = draft.handle.clone();
    drop(draft);
    let mut draft = ManualDraftWorkspaceV1::open(path, handle, root, worker()).unwrap();
    assert_eq!(draft.snapshot().unwrap(), second_state);
    draft.switch_to(&pending.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), pending_state);
    assert_eq!(
        serde_json::to_value(RunControlSessionCheckpointV1::from_session(
            draft.states.last().unwrap().adapter.session()
        ))
        .unwrap(),
        pending_internal
    );
    draft.switch_to(&first.reference).unwrap();
    assert_eq!(draft.snapshot().unwrap(), first_state);
}

#[test]
fn decision_branch_operations_keep_source_binding_and_comparison_read_only() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionSelectorV1 as Selector,
        DecisionWorkspaceServiceV1,
    };
    let (fixture, root) = burning_pact_fixture();
    let draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        fixture.draft.source.clone().unwrap(),
        root.clone(),
        worker(),
    )
    .unwrap();
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let mut id = 0;
    let mut call = |service: &mut DecisionWorkspaceServiceV1, view: Option<String>, operation| {
        id += 1;
        service
            .handle(Request {
                id,
                view,
                operation,
            })
            .unwrap()
    };
    let initial = call(&mut service, None, Op::Observe);
    assert!(initial
        .choices
        .as_ref()
        .is_some_and(|choices| !choices.actions.is_empty()));
    assert!(initial.unprojected_domains.is_empty());
    assert!(initial.summary.is_ascii());
    let marked = call(
        &mut service,
        Some(initial.view),
        Op::Mark {
            name: "pending exhaust".into(),
        },
    );
    let pending = marked.node.clone();
    let inspected = call(
        &mut service,
        Some(marked.view),
        Op::Inspect {
            section: crate::DecisionSectionV1::Map,
        },
    );
    assert!(inspected.summary.contains("map: Boss"));
    let pending_facts = inspected.state.as_ref().unwrap();
    assert_eq!(
        pending_facts.combat.as_ref().unwrap(),
        &sts_oracle_run_control::agent::information::state::public_combat_state_v1(
            &root.session().active_combat.as_ref().unwrap().combat_state
        )
    );
    assert!(matches!(
        pending_facts.selection,
        Some(sts_oracle_run_control::state::core::PendingChoice::HandSelect { .. })
    ));
    assert_eq!(
        inspected.bookmarks.unwrap()[0].name.as_deref(),
        Some("pending exhaust")
    );
    let action = inspected
        .choices
        .unwrap()
        .actions
        .into_iter()
        .find(|entry| matches!(entry.action, SemanticActionV1::SubmitSelection { .. }))
        .unwrap();
    let SemanticActionV1::SubmitSelection { choices, .. } = &action.action else {
        unreachable!()
    };
    let Some(sts_parity_lockstep::SemanticScalarV1::Text(card_id)) =
        choices[0].attributes.get("card_id")
    else {
        panic!("selection must describe its card")
    };
    let played = call(
        &mut service,
        Some(inspected.view.clone()),
        Op::Choose {
            selector: Selector::Card {
                card_id: card_id.clone(),
                binding: Some(choices[0].id.clone()),
            },
        },
    );
    assert_eq!(played.status, "ok");
    assert_eq!(played.selected_actions, 1);
    let compared = call(
        &mut service,
        Some(played.view.clone()),
        Op::Compare {
            reference: pending.reference.clone(),
        },
    );
    assert_eq!(compared.view, played.view);
    assert_eq!(compared.node.reference, played.node.reference);
    let comparison = compared.comparison.unwrap();
    assert!(comparison
        .different_domains
        .contains(&ParityDomainV1::Combat));
    assert!(comparison.unprojected_domains.is_empty());
    let switched = call(
        &mut service,
        Some(compared.view),
        Op::Switch {
            reference: pending.reference,
        },
    );
    assert_eq!(switched.selected_actions, 0);
    assert_ne!(switched.view, inspected.view);
    let stale = call(
        &mut service,
        Some(inspected.view),
        Op::Choose {
            selector: Selector::Reference {
                reference: action.handle,
            },
        },
    );
    assert_eq!(stale.status, "rejected");
    assert_eq!(stale.selected_actions, 0);
    let resumed = call(
        &mut service,
        Some(stale.view),
        Op::Switch {
            reference: played.node.reference,
        },
    );
    assert_eq!(resumed.selected_actions, 1);
    assert_eq!(resumed.stop, "natural_combat");
}

#[test]
fn public_pile_sources_select_a_specific_duplicate_without_exposing_draw() {
    use crate::{DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionSelectorV1};

    let fixture = Fixture::new();
    let mut service =
        crate::DecisionWorkspaceServiceV1::new(fixture.session.clone(), fixture.workspace());
    let first = service
        .handle(Request {
            id: 1,
            view: None,
            operation: Op::Observe,
        })
        .unwrap();
    let inspected = service
        .handle(Request {
            id: 2,
            view: Some(first.view),
            operation: Op::Inspect {
                section: crate::DecisionSectionV1::Position,
            },
        })
        .unwrap();
    let sources = &inspected.state.as_ref().unwrap().card_sources;
    assert_eq!(
        sources.keys().map(String::as_str).collect::<Vec<_>>(),
        ["discard", "exhaust", "hand"]
    );
    let hand = &sources["hand"];
    let menu = inspected.choices.as_ref().unwrap();
    let (kept, played) = hand.iter().enumerate().find_map(|(i, first)| {
        hand.iter().skip(i + 1).find(|second| {
            first.attributes == second.attributes && menu.actions.iter().any(|entry| {
                matches!(&entry.action, SemanticActionV1::PlayCard { card_binding, .. } if card_binding == &second.id)
            })
        }).map(|second| (first.id.clone(), second.id.clone()))
    }).expect("natural opening has two playable copies of a starter card");
    let reference = menu.actions.iter().find(|entry| {
        matches!(&entry.action, SemanticActionV1::PlayCard { card_binding, .. } if card_binding == &played)
    }).unwrap().handle.clone();
    let chosen = service
        .handle(Request {
            id: 3,
            view: Some(inspected.view),
            operation: Op::Choose {
                selector: DecisionSelectorV1::Reference { reference },
            },
        })
        .unwrap();
    assert_eq!(chosen.status, "ok");
    let after = service
        .handle(Request {
            id: 4,
            view: Some(chosen.view),
            operation: Op::Inspect {
                section: crate::DecisionSectionV1::Position,
            },
        })
        .unwrap();
    let sources = after.state.unwrap().card_sources;
    assert!(sources["hand"].iter().any(|card| card.id == kept));
    assert!(!sources["hand"].iter().any(|card| card.id == played));
    assert!(sources["discard"].iter().any(|card| card.id == played));
}

#[test]
fn workspace_combat_completion_keeps_prefix_and_cursor_unchanged() {
    let fixture = Fixture::new();
    let mut workspace = fixture.workspace();
    workspace.choose(fixture.draft.actions[0].clone()).unwrap();
    let before = workspace.snapshot().unwrap();
    let node = workspace.current_node().unwrap();
    let mut wrong_source = fixture.draft.source.clone().unwrap();
    wrong_source.committed_actions += 1;
    assert!(workspace
        .plan_combat_completion(&wrong_source, 1, 1, None)
        .unwrap_err()
        .contains("source prefix"));
    let draft = workspace
        .plan_combat_completion(fixture.draft.source.as_ref().unwrap(), 10_000, 10_000, None)
        .unwrap();
    assert_eq!(draft.source, fixture.draft.source);
    assert_eq!(draft.root, fixture.draft.root);
    assert_eq!(draft.actions.first(), fixture.draft.actions.first());
    let terminal = fixture
        .root
        .verify_semantic_combat_victory_v1(&draft.actions)
        .unwrap();
    assert_eq!(draft.terminal, terminal);
    assert_eq!(workspace.snapshot().unwrap(), before);
    assert_eq!(workspace.current_node().unwrap().reference, node.reference);
    assert_eq!(workspace.selected_actions().len(), 1);
}

#[test]
fn workspace_branches_keep_shared_nodes_and_fixed_bookmarks_across_reopen() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    draft.mark("start").unwrap();
    let start = draft.current_node().unwrap();
    draft.choose(fixture.draft.actions[0].clone()).unwrap();
    draft.mark("first choice").unwrap();
    let first = draft.current_node().unwrap();
    let first_state = draft.snapshot().unwrap();
    let first_cache = draft.states.last().unwrap().clone();
    let old_view = draft.action_view().unwrap();
    draft.switch_to(&start.reference).unwrap();
    assert!(draft
        .choose_handle(&old_view.actions().actions[0].handle)
        .is_err());
    draft.choose(SemanticActionV1::EndTurn).unwrap();
    draft.mark("pass turn").unwrap();
    let alternative = draft.current_node().unwrap();
    let cursor = draft.action_view().unwrap().reference_prefix().to_owned();
    let comparison = draft.compare_with(&first.reference).unwrap();
    assert_eq!(comparison.common_actions, 0);
    assert_eq!(comparison.other_snapshot, first_state);
    assert_eq!(draft.action_view().unwrap().reference_prefix(), cursor);
    assert!(draft.mark("first choice").is_err());
    draft.switch_to(&first.reference).unwrap();
    assert!(Rc::ptr_eq(&first_cache, draft.states.last().unwrap()));
    assert_eq!(draft.snapshot().unwrap(), first_state);
    let other_workspace = fixture.workspace();
    assert_ne!(
        other_workspace.current_node().unwrap().reference,
        start.reference
    );
    assert!(draft
        .switch_to(&other_workspace.current_node().unwrap().reference)
        .is_err());
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path, name);
    assert_eq!(draft.current_node().unwrap().reference, first.reference);
    assert_eq!(draft.bookmarks().unwrap().len(), 3);
    assert_eq!(draft.snapshot().unwrap(), first_state);
    draft.switch_to(&alternative.reference).unwrap();
    assert_eq!(draft.selected_actions(), &[SemanticActionV1::EndTurn]);
    draft.switch_to(&first.reference).unwrap();
    draft.choose(fixture.draft.actions[1].clone()).unwrap();
    assert_eq!(
        draft.compare_with(&first.reference).unwrap().common_actions,
        1
    );
}

#[test]
fn workspace_branch_publication_failure_leaves_cursor_and_views_intact() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let root = draft.current_node().unwrap();
    draft.choose(fixture.draft.actions[0].clone()).unwrap();
    let current = draft.snapshot().unwrap();
    let view = draft.action_view().unwrap().reference_prefix().to_owned();
    let conflict = draft
        .directory
        .join(format!("{:08}.json", draft.next_revision));
    fs::write(&conflict, "test publication conflict").unwrap();
    assert!(draft.switch_to(&root.reference).is_err());
    assert!(draft.mark("unsaved").is_err());
    assert_eq!(draft.snapshot().unwrap(), current);
    assert_eq!(draft.action_view().unwrap().reference_prefix(), view);
    assert!(draft.bookmarks().unwrap().is_empty());
    assert!(conflict.is_absolute() && conflict.starts_with(&fixture.repository));
    fs::remove_file(conflict).unwrap();
    draft.switch_to(&root.reference).unwrap();
    assert!(draft.selected_actions().is_empty());
}

#[test]
fn workspace_branches_replay_off_path_lazily_and_keep_an_invalid_target_explicit() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    draft.choose(fixture.draft.actions[0].clone()).unwrap();
    let valid = draft.current_node().unwrap();
    let valid_node = draft.graph.cursor;
    draft
        .append(Edit::Choose {
            action: SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::Neow,
            },
        })
        .unwrap();
    let invalid = draft.current_node().unwrap();
    draft.append(Edit::Jump { node: valid_node }).unwrap();
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path, name);
    let before = draft.snapshot().unwrap();
    assert!(draft.replay_stop().is_none());
    assert!(draft.compare_with(&invalid.reference).is_err());
    assert_eq!(draft.snapshot().unwrap(), before);
    assert_eq!(draft.current_node().unwrap().reference, valid.reference);
    draft.switch_to(&invalid.reference).unwrap();
    assert_eq!(draft.selected_actions().len(), 2);
    assert_eq!(draft.verified_actions(), 1);
    assert!(draft.replay_stop().is_some());
    assert!(draft.certify(&fixture.session).is_err());
    draft.undo().unwrap();
    assert!(draft.replay_stop().is_none());
    assert_eq!(draft.current_node().unwrap().reference, valid.reference);
}

#[test]
fn decision_service_keeps_views_and_deduplicates_mutations_across_calls() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionSelectorV1 as Selector,
        DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::with_seed(2_026_090_709);
    let root = reward_fixture_root(&fixture);
    let draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        ManualDraftSourceV1 {
            session_id: fixture.session.session_id().into(),
            committed_actions: 0,
        },
        root.clone(),
        worker(),
    )
    .unwrap();
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let first = service
        .handle(Request {
            id: 1,
            view: None,
            operation: Op::Observe,
        })
        .unwrap();
    assert!(first.choices.is_none());
    assert!(first.state.is_none());
    let choose = Request {
        id: 2,
        view: Some(first.view.clone()),
        operation: Op::Choose {
            selector: Selector::Reward {
                reward: crate::RewardPlanStepV1::Potion,
            },
        },
    };
    let second = service.handle(choose.clone()).unwrap();
    assert_eq!(second.status, "ok");
    assert_eq!(second.selected_actions, 1);
    assert!(second.summary.contains("Fire Potion"));
    let duplicate = service.handle(choose).unwrap();
    assert_eq!(
        serde_json::to_value(&duplicate).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
    let stale = service
        .handle(Request {
            id: 3,
            view: Some(first.view),
            operation: Op::Choose {
                selector: Selector::Reward {
                    reward: crate::RewardPlanStepV1::Gold,
                },
            },
        })
        .unwrap();
    assert_eq!(stale.status, "rejected");
    assert_eq!(stale.selected_actions, 1);
    assert!(stale.summary.contains("Fire Potion"));
    let undo = service
        .handle(Request {
            id: 4,
            view: Some(second.view),
            operation: Op::Undo,
        })
        .unwrap();
    assert_eq!(undo.selected_actions, 0);
    let preview = service
        .handle(Request {
            id: 5,
            view: Some(undo.view),
            operation: Op::PreviewRewards {
                steps: crate::parse_reward_plan_steps_v1(&["potion", "gold", "cards"]).unwrap(),
            },
        })
        .unwrap();
    assert_eq!(preview.status, "ok");
    assert_eq!(preview.selected_actions, 3);
    assert_eq!(preview.stop, "natural_card_reward");
    let mismatch = service
        .handle(Request {
            id: 5,
            view: Some(preview.view.clone()),
            operation: Op::Undo,
        })
        .unwrap();
    assert_eq!(mismatch.status, "rejected");
    assert_eq!(mismatch.selected_actions, 3);
    let inspect = service
        .handle(Request {
            id: 6,
            view: Some(preview.view),
            operation: Op::Inspect {
                section: crate::DecisionSectionV1::Position,
            },
        })
        .unwrap();
    assert!(inspect.state.as_ref().unwrap().deck.is_array());
    assert!(!inspect.choices.unwrap().actions.is_empty());
    let certify = service
        .handle(Request {
            id: 7,
            view: Some(inspect.view),
            operation: Op::Certify,
        })
        .unwrap();
    assert_eq!(certify.status, "ok");
    assert!(certify.proposal.as_ref().unwrap().starts_with('P'));
    service
        .handle(Request {
            id: 8,
            view: None,
            operation: Op::Close,
        })
        .unwrap();
    drop(service);
    assert_eq!(read_workspace(&path).unwrap().2.unwrap().len(), 33);
    let draft = ManualDraftWorkspaceV1::open(path, name, root, worker()).unwrap();
    let mut resumed = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let reopened = resumed
        .handle(Request {
            id: 1,
            view: None,
            operation: Op::Observe,
        })
        .unwrap();
    assert_eq!(reopened.selected_actions, 3);
    assert_ne!(reopened.view, certify.view);
    let stale = resumed
        .handle(Request {
            id: 2,
            view: Some(certify.view),
            operation: Op::Undo,
        })
        .unwrap();
    assert_eq!(stale.status, "rejected");
    assert_eq!(stale.selected_actions, 3);
}

#[test]
fn decision_service_reports_durable_prefix_when_batch_save_fails() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::with_seed(2_026_090_709);
    let root = reward_fixture_root(&fixture);
    let mut draft = ManualDraftWorkspaceV1::create(
        &fixture.session,
        ManualDraftSourceV1 {
            session_id: fixture.session.session_id().into(),
            committed_actions: 0,
        },
        root.clone(),
        worker(),
    )
    .unwrap();
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    let scope = draft.action_view().unwrap().reference_prefix().to_owned();
    let conflict = path.join("00000001.json");
    fs::write(&conflict, b"test-injected publication conflict").unwrap();
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let request = Request {
        id: 1,
        view: Some(scope),
        operation: Op::PreviewRewards {
            steps: crate::parse_reward_plan_steps_v1(&["potion", "gold", "cards"]).unwrap(),
        },
    };
    let reply = service.handle(request.clone()).unwrap();
    assert_eq!(reply.status, "partial");
    assert_eq!(reply.selected_actions, 1);
    assert!(reply.summary.contains("Fire Potion"));
    assert_eq!(service.handle(request).unwrap().selected_actions, 1);
    drop(service);
    assert!(conflict.is_absolute() && conflict.starts_with(&fixture.repository));
    fs::remove_file(conflict).unwrap();
    let mut restored = ManualDraftWorkspaceV1::open(path, name, root, worker()).unwrap();
    assert_eq!(restored.selected_actions().len(), 1);
    restored
        .preview_rewards(crate::parse_reward_plan_steps_v1(&["gold", "cards"]).unwrap())
        .unwrap();
    assert_eq!(restored.selected_actions().len(), 3);
    assert_eq!(
        restored.snapshot().unwrap().boundary_kind,
        "natural_card_reward"
    );
}

#[test]
fn decision_service_rejects_ambiguous_cards_and_does_not_replay_on_observe() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionSelectorV1 as Selector,
        DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    // Reach a hand with multiple equally named playable cards/targets.
    let ambiguous = loop {
        let menu = draft.action_view().unwrap();
        let ids = menu
            .actions()
            .actions
            .iter()
            .filter_map(|a| match &a.action {
                SemanticActionV1::PlayCard { card_id, .. } => Some(card_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if let Some(id) = ids
            .iter()
            .find(|id| ids.iter().filter(|other| *other == *id).count() > 1)
        {
            break id.clone();
        }
        let index = draft.selected_actions().len();
        draft.choose(fixture.draft.actions[index].clone()).unwrap();
    };
    let selected = draft.selected_actions().len();
    let available = draft.action_view().unwrap().actions().actions.len();
    assert!(available <= 12);
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let first = service
        .handle(Request {
            id: 1,
            view: None,
            operation: Op::Observe,
        })
        .unwrap();
    let repeated = service
        .handle(Request {
            id: 2,
            view: None,
            operation: Op::Observe,
        })
        .unwrap();
    assert_eq!(first.view, repeated.view);
    let choices = first.choices.as_ref().unwrap();
    assert_eq!(choices.actions.len(), available);
    let exact = choices
        .actions
        .iter()
        .find(|entry| {
            matches!(&entry.action,
        SemanticActionV1::PlayCard { card_id, .. } if *card_id == ambiguous)
        })
        .unwrap();
    let exact_reference = exact.handle.clone();
    let rejected = service
        .handle(Request {
            id: 3,
            view: Some(first.view),
            operation: Op::Choose {
                selector: Selector::Card {
                    card_id: ambiguous,
                    binding: None,
                },
            },
        })
        .unwrap();
    assert!(rejected.error.unwrap().contains("ambiguous"));
    assert_eq!(rejected.selected_actions, selected);
    let played = service
        .handle(Request {
            id: 4,
            view: Some(rejected.view),
            operation: Op::Choose {
                selector: Selector::Reference {
                    reference: exact_reference.clone(),
                },
            },
        })
        .unwrap();
    assert_eq!(played.status, "ok");
    assert_eq!(played.selected_actions, selected + 1);
    let stale = service
        .handle(Request {
            id: 5,
            view: Some(repeated.view),
            operation: Op::Choose {
                selector: Selector::Reference {
                    reference: exact_reference,
                },
            },
        })
        .unwrap();
    assert_eq!(stale.status, "rejected");
    assert_eq!(stale.selected_actions, selected + 1);
}

#[test]
fn decision_service_observation_exposes_unreplayable_edits_and_undo_clears_the_stop() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    draft.choose(fixture.draft.actions[0].clone()).unwrap();
    draft
        .append(Edit::Choose {
            action: SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::Neow,
            },
        })
        .unwrap();
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    drop(draft);
    let draft = fixture.reopen(path, name);
    let mut service = DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft);
    let observed = service
        .handle(Request {
            id: 1,
            view: None,
            operation: Op::Observe,
        })
        .unwrap();
    assert_eq!(observed.selected_actions, 2);
    assert_eq!(observed.verified_actions, 1);
    assert!(observed.replay_stop.is_some());
    assert!(observed
        .summary
        .contains(observed.replay_stop.as_ref().unwrap()));
    let recovered = service
        .handle(Request {
            id: 2,
            view: Some(observed.view),
            operation: Op::Undo,
        })
        .unwrap();
    assert_eq!(recovered.status, "ok");
    assert_eq!(recovered.selected_actions, 1);
    assert_eq!(recovered.verified_actions, 1);
    assert!(recovered.replay_stop.is_none());
}

#[test]
fn decision_stdio_is_structured_and_eof_preserves_saved_edits() {
    use crate::{
        DecisionOperationV1 as Op, DecisionRequestV1 as Request, DecisionSelectorV1 as Selector,
        DecisionWorkspaceServiceV1,
    };
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    let view = draft.action_view().unwrap();
    let request = Request {
        id: 1,
        view: Some(view.reference_prefix().into()),
        operation: Op::Choose {
            selector: Selector::Reference {
                reference: view.actions().actions[0].handle.clone(),
            },
        },
    };
    let line = serde_json::to_string(&request).unwrap() + "\n";
    let mut output = Vec::new();
    crate::serve_decision_workspace_v1(
        DecisionWorkspaceServiceV1::new(fixture.session.clone(), draft),
        &mut std::io::Cursor::new(line),
        &mut output,
    )
    .unwrap();
    let result: crate::DecisionReplyV1 = serde_json::from_slice(&output).unwrap();
    assert_eq!(result.status, "ok");
    assert!(
        !output.contains(&27),
        "no ANSI control characters in the transport"
    );
    assert_eq!(result.selected_actions, 1);
    assert_eq!(fixture.reopen(path, name).selected_actions().len(), 1);
}

#[test]
fn draft_view_rejects_old_references_after_undo_and_reopen() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let view = draft.action_view().unwrap();
    let handle = view
        .actions()
        .actions
        .iter()
        .find(|a| a.action == fixture.draft.actions[0])
        .unwrap()
        .handle
        .clone();
    draft.choose_handle(&handle).unwrap();
    draft.undo().unwrap();
    assert!(draft.choose_handle(&handle).is_err());
    assert!(draft.selected_actions().is_empty());
    let current_handle = draft.action_view().unwrap().actions().actions[0]
        .handle
        .clone();
    let path = draft.directory.clone();
    let name = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path, name);
    assert!(draft.choose_handle(&current_handle).is_err());
    assert!(draft.selected_actions().is_empty());
}

#[test]
fn reward_plan_demo_and_interrupted_suffix() {
    let fixture = Fixture::with_seed(2_026_090_709);
    let mut root = fixture.root.clone();
    for action in &fixture.draft.actions {
        step(&mut root, action);
    }
    let before = root.observe().unwrap();
    let menu = sts_parity_lockstep::SemanticActionSpaceV1::from_snapshot(&before).unwrap();
    println!(
        "Rust-only natural reward fixture:\n{}",
        crate::render_manual_observation_v1(&before, &menu, &[])
            .unwrap()
            .replacen("Exact |", "Rust only |", 1)
    );
    let command = crate::parse_manual_command_v1("plan rewards potion gold cards").unwrap();
    let crate::ManualCommandV1::PlanRewards(steps) = command else {
        panic!("plan command");
    };
    let plan = crate::ManualRewardPlanV1::compile(
        ManualDraftSourceV1 {
            session_id: fixture.session.session_id().into(),
            committed_actions: 0,
        },
        &root,
        steps,
    )
    .unwrap();
    println!(
        "manual> plan rewards potion gold cards\nRust preview | {}\nJava unchanged",
        plan.summary()
    );
    assert_eq!(root.observe().unwrap(), before);
    let calls = Rc::new(Cell::new(0));
    let mut runner = LockstepRunnerV1::start_new(
        CountedOracle {
            rust: root.clone(),
            calls: calls.clone(),
        },
        root.clone(),
        LockstepModeV1::Audit,
        &fixture.repository.join("rewards"),
        "fixture",
    )
    .unwrap();
    let menu = runner.current_action_space().unwrap();
    let old_gold = menu
        .actions
        .iter()
        .find(
            |a| matches!(&a.action, SemanticActionV1::Select { choice, .. } if choice.id == "gold"),
        )
        .unwrap()
        .handle
        .clone();
    let prepared = crate::reward_plan::prepare_reward_suffix(&plan, &root, &[], &runner).unwrap();
    let mut other_branch = LockstepRunnerV1::start_new(
        CountedOracle {
            rust: root.clone(),
            calls: calls.clone(),
        },
        root.clone(),
        LockstepModeV1::Audit,
        &fixture.repository.join("other-rewards"),
        "fixture",
    )
    .unwrap();
    assert!(prepared.commit(&mut other_branch, |_, _| Ok(())).is_err());
    assert_eq!(
        calls.get(),
        0,
        "same-looking branches cannot share a prepared adoption"
    );
    assert!(prepared
        .commit(&mut runner, |index, _| if index == 1 {
            Err("interrupted after confirmed potion pickup".into())
        } else {
            Ok(())
        })
        .is_err());
    assert_eq!(calls.get(), 1);
    assert!(prepared.commit(&mut runner, |_, _| Ok(())).is_err());
    assert_eq!(
        calls.get(),
        1,
        "an interrupted plan must be re-prepared from the confirmed prefix"
    );
    assert!(runner.step_handle(&old_gold).is_err());
    assert_eq!(calls.get(), 1, "stale input must not reach the oracle");
    let (oracle, resumed_rust) = runner.into_adapters();
    let mut runner = LockstepRunnerV1::start_new(
        oracle,
        resumed_rust,
        LockstepModeV1::Audit,
        &fixture.repository.join("rewards-resumed"),
        "resumed-fixture",
    )
    .unwrap();
    assert!(runner.step_handle(&old_gold).is_err());
    assert_eq!(
        calls.get(),
        1,
        "reconnecting does not revive old view references"
    );
    let potion_action = actions(&mut root)
        .into_iter()
        .find(|a| matches!(a, SemanticActionV1::Select { choice, .. } if choice.id == "potion"))
        .unwrap();
    let suffix = crate::reward_plan::prepare_reward_suffix(
        &plan,
        &root,
        std::slice::from_ref(&potion_action),
        &runner,
    )
    .unwrap();
    assert_eq!(suffix.completed_actions(), 1);
    assert_eq!(suffix.remaining_actions(), 2);
    suffix.commit(&mut runner, |_, _| Ok(())).unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(
        runner.current_snapshots().1.boundary_kind,
        "natural_card_reward"
    );
    let all = [potion_action, menu.actions.iter().find(|a| a.handle == old_gold).unwrap().action.clone(),
        menu.actions.iter().find(|a| matches!(&a.action, SemanticActionV1::Select { choice, .. } if choice.id == "card")).unwrap().action.clone()];
    let done = crate::reward_plan::prepare_reward_suffix(&plan, &root, &all, &runner).unwrap();
    assert_eq!(done.commit(&mut runner, |_, _| Ok(())).unwrap(), 0);
    assert_eq!(calls.get(), 3);
    let view = runner.current_action_view().unwrap();
    let text =
        crate::render_manual_action_view_v1(runner.current_snapshots().0, &view, &[]).unwrap();
    assert_eq!(text.matches(view.reference_prefix()).count(), 1);
    let reference = view.actions().actions[0].handle.clone();
    assert_eq!(
        crate::parse_manual_command_v1(&reference),
        Ok(crate::ManualCommandV1::Step(reference))
    );
    println!(
        "Rust preview result:\n{}",
        crate::render_manual_observation_v1(
            plan.terminal().unwrap(),
            &sts_parity_lockstep::SemanticActionSpaceV1::from_snapshot(plan.terminal().unwrap())
                .unwrap(),
            &[],
        )
        .unwrap()
        .replacen("Exact |", "Rust only |", 1)
    );
}

fn reward_fixture_root(fixture: &Fixture) -> RustLockstepAdapterV1 {
    let mut root = fixture.root.clone();
    for action in &fixture.draft.actions {
        step(&mut root, action);
    }
    root
}

#[test]
fn reward_plan_persistence_keeps_identity_and_requires_real_source_evidence() {
    let fixture = Fixture::with_seed(2_026_090_709);
    let root = reward_fixture_root(&fixture);
    let plan = crate::ManualRewardPlanV1::compile(
        ManualDraftSourceV1 {
            session_id: fixture.session.session_id().into(),
            committed_actions: 0,
        },
        &root,
        crate::parse_reward_plan_steps_v1(&["potion", "gold", "cards"]).unwrap(),
    )
    .unwrap();
    // Constructed envelopes exercise storage and rejection, not Java parity.
    record_actions(&fixture.session, &[]);
    let (handle, summary) =
        crate::reward_plan::save_reward_plan(&fixture.session, &plan, worker()).unwrap();
    let reopened =
        ManualSessionV1::load_existing(&fixture.repository, fixture.session.session_id()).unwrap();
    assert_eq!(
        crate::list_manual_reward_plans_v1(&reopened).unwrap(),
        vec![(handle.clone(), summary.clone())]
    );
    let path = fs::read_dir(fixture.session.directory().join("reward-plans"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["worker"]["source_revision"], "constructed-test");
    assert_eq!(
        saved["plan"]["source"]["session_id"],
        fixture.session.session_id()
    );
    assert!(saved["plan"]["successors"].as_array().unwrap().len() == 3);
    let calls = Rc::new(Cell::new(0));
    let runner = LockstepRunnerV1::start_new(
        CountedOracle {
            rust: root.clone(),
            calls: calls.clone(),
        },
        root,
        LockstepModeV1::Audit,
        &fixture.repository.join("saved-plan"),
        "fixture",
    )
    .unwrap();
    assert!(
        crate::prepare_manual_reward_adoption_v1(&reopened, &handle, &runner, &mut |_| {}).is_err()
    );
    assert_eq!(
        calls.get(),
        0,
        "saved projections cannot substitute for original evidence"
    );
    let id = path.file_stem().unwrap().to_str().unwrap();
    let mut other = id.to_owned();
    other.replace_range(31..32, if other.ends_with('0') { "1" } else { "0" });
    fs::copy(&path, path.with_file_name(format!("{other}.json"))).unwrap();
    let listed = crate::list_manual_reward_plans_v1(&reopened).unwrap();
    assert!(listed.iter().all(|(longer, _)| longer.len() > handle.len()));
    let error = crate::prepare_manual_reward_adoption_v1(&reopened, &handle, &runner, &mut |_| {})
        .err()
        .unwrap();
    assert!(error.contains("ambiguous"));
    assert_eq!(calls.get(), 0);
}

fn compile_rewards(root: &RustLockstepAdapterV1) -> Result<crate::ManualRewardPlanV1, String> {
    crate::ManualRewardPlanV1::compile(
        ManualDraftSourceV1 {
            session_id: "test-only-source".into(),
            committed_actions: 0,
        },
        root,
        crate::parse_reward_plan_steps_v1(&["potion", "gold", "cards"]).unwrap(),
    )
}

#[test]
fn reward_plan_rejects_full_slots_blocked_obtain_ambiguous_and_missing_rewards() {
    use sts_oracle_run_control::{
        content::{
            potions::{Potion, PotionId},
            relics::{RelicId, RelicState},
        },
        state::{core::EngineState, rewards::RewardItem},
    };
    let fixture = Fixture::with_seed(2_026_090_709);
    let root = reward_fixture_root(&fixture);
    let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&RunControlConfig {
        seed: 2_026_090_709,
        ascension_level: 20,
        final_act: true,
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    })
    .unwrap();
    // Deliberately constructed Rust test inputs; never saved as natural evidence.
    for case in ["full", "sozu", "ambiguous", "missing", "missing_later"] {
        let mut session = root.session().clone();
        match case {
            "full" => {
                for (index, slot) in session.run_state.potions.iter_mut().enumerate() {
                    *slot = Some(Potion::new(PotionId::BlockPotion, 90000 + index as u32));
                }
            }
            "sozu" => session
                .run_state
                .relics
                .push(RelicState::new(RelicId::Sozu)),
            _ => {
                let EngineState::RewardScreen(rewards) = &mut session.engine_state else {
                    panic!("rewards");
                };
                match case {
                    "ambiguous" => rewards.items.push(RewardItem::Potion {
                        potion_id: PotionId::BlockPotion,
                    }),
                    "missing_later" => rewards
                        .items
                        .retain(|r| !matches!(r, RewardItem::Gold { .. })),
                    _ => rewards
                        .items
                        .retain(|r| !matches!(r, RewardItem::Potion { .. })),
                }
            }
        }
        let mut constructed =
            RustLockstepAdapterV1::new(session, bootstrap.clone(), "constructed", case).unwrap();
        let before = constructed.observe().unwrap();
        let error = compile_rewards(&constructed).unwrap_err();
        assert!(
            error.contains("potion") || error.contains("gold"),
            "{case}: {error}"
        );
        assert_eq!(
            constructed.observe().unwrap(),
            before,
            "failed preview mutates no source: {case}"
        );
    }
    assert!(compile_rewards(&fixture.root)
        .unwrap_err()
        .contains("post-combat"));
}

struct FaultingRewardOracle {
    inner: CountedOracle,
    transport_failure: bool,
}

impl LockstepAdapterV1 for FaultingRewardOracle {
    type PreparedAction = <CountedOracle as LockstepAdapterV1>::PreparedAction;
    fn observe(&mut self) -> Result<ParitySnapshotV1, sts_parity_lockstep::AdapterFailureV1> {
        self.inner.observe()
    }
    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        before: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, sts_parity_lockstep::AdapterFailureV1> {
        self.inner.prepare(action, before)
    }
    fn execute(
        &mut self,
        action: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, sts_parity_lockstep::AdapterFailureV1> {
        let mut after = self.inner.execute(action)?;
        if self.inner.calls.get() == 2 {
            if self.transport_failure {
                return Err(sts_parity_lockstep::AdapterFailureV1::new(
                    sts_parity_lockstep::AdapterFailureKindV1::Transport,
                    "executed but response lost",
                ));
            }
            let DomainEvidenceV1::Observed { value, .. } = &mut after
                .domains
                .get_mut(&ParityDomainV1::Run)
                .unwrap()
                .evidence
            else {
                panic!("observed run");
            };
            value["gold"] = serde_json::json!(999);
        }
        Ok(after)
    }
    fn last_rng_call_ledger_v1(&self) -> Option<&sts_parity_lockstep::NaturalRunRngCallLedgerV1> {
        self.inner.last_rng_call_ledger_v1()
    }
}

#[test]
fn reward_plan_stops_on_difference_or_uncertain_execution_without_retry() {
    let fixture = Fixture::with_seed(2_026_090_709);
    let root = reward_fixture_root(&fixture);
    let plan = compile_rewards(&root).unwrap();
    for transport_failure in [false, true] {
        let calls = Rc::new(Cell::new(0));
        let mut runner = LockstepRunnerV1::start_new(
            FaultingRewardOracle {
                inner: CountedOracle {
                    rust: root.clone(),
                    calls: calls.clone(),
                },
                transport_failure,
            },
            root.clone(),
            LockstepModeV1::Audit,
            &fixture
                .repository
                .join(format!("fault-{transport_failure}")),
            "fixture",
        )
        .unwrap();
        let prepared =
            crate::reward_plan::prepare_reward_suffix(&plan, &root, &[], &runner).unwrap();
        let mut confirmed = 0;
        assert!(prepared
            .commit(&mut runner, |index, _| {
                confirmed = index;
                Ok(())
            })
            .is_err());
        assert_eq!(confirmed, 1);
        assert_eq!(calls.get(), 2);
        assert!(runner.is_stopped());
        assert!(prepared.commit(&mut runner, |_, _| Ok(())).is_err());
        assert_eq!(calls.get(), 2);
        let steps = fixture
            .repository
            .join(format!("fault-{transport_failure}/steps"));
        assert_eq!(
            fs::read_dir(steps).unwrap().count(),
            2,
            "retain the Exact prefix and failed transition"
        );
    }
}
fn actions(root: &mut RustLockstepAdapterV1) -> Vec<SemanticActionV1> {
    let snapshot = root.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::ActionSpace].evidence
    else {
        panic!("unobserved action space")
    };
    serde_json::from_value(value.clone()).unwrap()
}
fn step(root: &mut RustLockstepAdapterV1, action: &SemanticActionV1) {
    let before = root.observe().unwrap();
    let prepared = root.prepare(action, &before).unwrap();
    root.execute(prepared).unwrap();
}

#[test]
fn private_draft_choices_survive_reopen_undo_and_branching() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let path = draft.directory.clone();
    let handle = draft.handle.clone();
    for action in &fixture.draft.actions[..2] {
        draft.choose(action.clone()).unwrap();
    }
    let snapshot = draft.snapshot().unwrap();
    let old_branch = fs::read(path.join("00000001.json")).unwrap();
    drop(draft);
    let mut draft = fixture.reopen(path.clone(), handle.clone());
    assert_eq!(draft.snapshot().unwrap(), snapshot);
    assert_eq!(draft.selected_actions(), &fixture.draft.actions[..2]);
    draft.undo().unwrap();
    draft.choose(SemanticActionV1::EndTurn).unwrap();
    let changed = draft.snapshot().unwrap();
    drop(draft);
    let mut draft = fixture.reopen(path.clone(), handle);
    assert_eq!(draft.snapshot().unwrap(), changed);
    assert_eq!(draft.selected_actions().len(), 2);
    assert_eq!(draft.selected_actions()[1], SemanticActionV1::EndTurn);
    assert_eq!(fs::read(path.join("00000001.json")).unwrap(), old_branch);
    assert_eq!(draft.next_revision, 4);
    // Neither partial work nor its recovery creates a Java journal or a witness.
    assert!(!fixture.session.evidence_directory().exists());
    assert!(!fixture.session.directory().join("drafts").exists());
    assert!(draft.certify(&fixture.session).is_err());
}

#[test]
fn private_draft_atomic_save_and_writer_lease_do_not_advance_on_failure() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    let path = draft.directory.clone();
    let handle = draft.handle.clone();
    assert!(ManualDraftWorkspaceV1::open(
        path.clone(),
        handle.clone(),
        fixture.root.clone(),
        worker()
    )
    .is_err());
    let before = draft.snapshot().unwrap();
    // Refuse an existing destination; an unacknowledged choice cannot move memory.
    let collision = path.join("00000000.json");
    fs::write(&collision, b"reserved").unwrap();
    assert!(draft.choose(fixture.draft.actions[0].clone()).is_err());
    assert_eq!(draft.selected_actions().len(), 0);
    assert_eq!(draft.snapshot().unwrap(), before);
    assert_eq!(fs::read(&collision).unwrap(), b"reserved");
    fs::remove_file(collision).unwrap();
    draft.choose(fixture.draft.actions[0].clone()).unwrap();
    fs::write(path.join("interrupted.part"), b"incomplete").unwrap();
    drop(draft);
    let draft = fixture.reopen(path, handle);
    assert_eq!(draft.selected_actions(), &fixture.draft.actions[..1]);
}

#[test]
fn private_draft_broken_or_gapped_revision_chain_is_rejected() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    for action in &fixture.draft.actions[..2] {
        draft.choose(action.clone()).unwrap();
    }
    let path = draft.directory.clone();
    let handle = draft.handle.clone();
    drop(draft);
    let second = path.join("00000001.json");
    let bytes = fs::read(&second).unwrap();
    let mut record: Revision = serde_json::from_slice(&bytes).unwrap();
    record.previous_digest = "wrong-parent".into();
    fs::write(&second, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(ManualDraftWorkspaceV1::open(
        path.clone(),
        handle.clone(),
        fixture.root.clone(),
        worker()
    )
    .is_err());
    fs::write(&second, bytes).unwrap();
    fs::remove_file(path.join("00000000.json")).unwrap();
    assert!(ManualDraftWorkspaceV1::open(path, handle, fixture.root.clone(), worker()).is_err());
}

#[test]
fn private_draft_unreplayable_tail_is_retained_until_explicit_undo() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    draft.choose(fixture.draft.actions[0].clone()).unwrap();
    let valid = draft.snapshot().unwrap();
    // Construct a previously chosen action that current semantics cannot resolve.
    let invalid = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::Neow,
    };
    draft
        .append(Edit::Choose {
            action: invalid.clone(),
        })
        .unwrap();
    draft
        .append(Edit::Choose {
            action: SemanticActionV1::EndTurn,
        })
        .unwrap();
    let path = draft.directory.clone();
    let handle = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path.clone(), handle.clone());
    assert_eq!(draft.selected_actions().len(), 3);
    assert_eq!(draft.selected_actions()[1], invalid);
    assert_eq!(draft.verified_actions(), 1);
    assert!(draft.replay_stop().is_some());
    assert_eq!(draft.snapshot().unwrap(), valid);
    assert!(draft.choose(SemanticActionV1::EndTurn).is_err());
    assert!(draft.certify(&fixture.session).is_err());
    draft.undo().unwrap();
    assert!(draft.replay_stop().is_some());
    draft.undo().unwrap();
    assert!(draft.replay_stop().is_none());
    draft.choose(fixture.draft.actions[1].clone()).unwrap();
    drop(draft);
    assert!(fixture.reopen(path, handle).replay_stop().is_none());
}

#[test]
fn private_draft_certification_replays_and_preserves_the_source() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    for action in &fixture.draft.actions {
        draft.choose(action.clone()).unwrap();
    }
    let handle = draft.certify(&fixture.session).unwrap();
    assert_eq!(draft.certify(&fixture.session).unwrap(), handle);
    let saved = crate::load_manual_combat_draft_v1(&fixture.session, Path::new(&handle)).unwrap();
    assert_eq!(saved.source, fixture.draft.source);
    saved.validate_for_v1(&fixture.root).unwrap();
    let path = draft.directory.clone();
    let workspace_handle = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path, workspace_handle);
    assert_eq!(draft.certify(&fixture.session).unwrap(), handle);
    draft.undo().unwrap();
    assert!(draft.certify(&fixture.session).is_err());
    draft
        .choose(fixture.draft.actions.last().unwrap().clone())
        .unwrap();
    assert_ne!(draft.certify(&fixture.session).unwrap(), handle);
}

struct CountedOracle {
    rust: RustLockstepAdapterV1,
    calls: Rc<Cell<usize>>,
}
impl LockstepAdapterV1 for CountedOracle {
    type PreparedAction = <RustLockstepAdapterV1 as LockstepAdapterV1>::PreparedAction;
    fn observe(&mut self) -> Result<ParitySnapshotV1, sts_parity_lockstep::AdapterFailureV1> {
        self.rust.observe()
    }
    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        before: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, sts_parity_lockstep::AdapterFailureV1> {
        self.rust.prepare(action, before)
    }
    fn execute(
        &mut self,
        action: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, sts_parity_lockstep::AdapterFailureV1> {
        self.calls.set(self.calls.get() + 1);
        self.rust.execute(action)
    }
    fn last_rng_call_ledger_v1(&self) -> Option<&sts_parity_lockstep::NaturalRunRngCallLedgerV1> {
        self.rust.last_rng_call_ledger_v1()
    }
}

#[test]
fn private_draft_interrupted_adoption_commits_only_verified_remainder() {
    let fixture = Fixture::new();
    let calls = Rc::new(Cell::new(0));
    let mut runner = LockstepRunnerV1::start_new(
        CountedOracle {
            rust: fixture.root.clone(),
            calls: calls.clone(),
        },
        fixture.root.clone(),
        LockstepModeV1::Audit,
        &fixture.repository.join("adoption"),
        "fixture",
    )
    .unwrap();
    let prepared =
        prepare_verified_suffix(&fixture.draft, &fixture.root, &[], runner.rust_adapter()).unwrap();
    assert!(prepared
        .commit(&mut runner, |index, _| if index == 2 {
            Err("capture interrupted".into())
        } else {
            Ok(())
        })
        .is_err());
    assert_eq!(calls.get(), 2);
    let suffix = prepare_verified_suffix(
        &fixture.draft,
        &fixture.root,
        &fixture.draft.actions[..2],
        runner.rust_adapter(),
    )
    .unwrap();
    assert_eq!(suffix.completed_actions(), 2);
    assert_eq!(
        suffix.commit(&mut runner, |_, _| Ok(())).unwrap(),
        fixture.draft.actions.len() - 2
    );
    assert_eq!(calls.get(), fixture.draft.actions.len());
    let done = prepare_verified_suffix(
        &fixture.draft,
        &fixture.root,
        &fixture.draft.actions,
        runner.rust_adapter(),
    )
    .unwrap();
    assert_eq!(
        done.commit(&mut runner, |_, _| panic!("already complete"))
            .unwrap(),
        0
    );
    assert_eq!(calls.get(), fixture.draft.actions.len());
    // A prepared plan cannot be reused after its original boundary moved.
    assert!(prepared.commit(&mut runner, |_, _| Ok(())).is_err());
    assert_eq!(calls.get(), fixture.draft.actions.len());
}

#[test]
fn private_draft_continuation_rejects_wrong_prefix_state_and_obsolete_victory() {
    let fixture = Fixture::new();
    let mut current = fixture.root.clone();
    step(&mut current, &fixture.draft.actions[0]);
    assert!(prepare_verified_suffix(
        &fixture.draft,
        &fixture.root,
        &[SemanticActionV1::EndTurn],
        &current
    )
    .is_err());
    assert!(prepare_verified_suffix(&fixture.draft, &fixture.root, &[], &current).is_err());
    let mut incomplete = fixture.draft.clone();
    incomplete.actions.truncate(1);
    assert!(prepare_verified_suffix(&incomplete, &fixture.root, &[], &fixture.root).is_err());
    let mut stale = fixture.draft.clone();
    stale.terminal = stale.root.clone();
    assert!(prepare_verified_suffix(&stale, &fixture.root, &[], &fixture.root).is_err());
    stale = fixture.draft.clone();
    stale.root = current.observe().unwrap();
    assert!(prepare_verified_suffix(&stale, &fixture.root, &[], &fixture.root).is_err());
}

// Constructed envelopes test ancestry/prefix extraction, never game parity.
// Empty raw frames MUST fail the real source-replay gate.
fn record_actions(session: &ManualSessionV1, selected: &[SemanticActionV1]) {
    let bootstrap =
        sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
    let mut store =
        RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap).unwrap();
    let mut raw = JavaRuntimeObservationV1 {
        producer: ProducerIdentityV1 {
            implementation: "java".into(),
            version: "constructed".into(),
            session_epoch: "constructed".into(),
        },
        frame_id: 1,
        frame: serde_json::json!({}),
        actions: vec![],
    };
    store.append_boundary(&raw).unwrap();
    for (index, action) in selected.iter().enumerate() {
        store
            .append_action(
                RecordedJavaActionPhaseV1::Committed,
                action,
                &JavaLockstepReceiptV1 {
                    status: "completed".into(),
                    request_id: format!("test-{index}"),
                    based_on_frame_id: raw.frame_id,
                    action_id: "constructed".into(),
                    successor_frame_id: Some(raw.frame_id + 1),
                },
            )
            .unwrap();
        raw.frame_id += 1;
        store.append_boundary(&raw).unwrap();
    }
}

#[test]
fn private_draft_source_follows_only_adopted_prefix_and_raw_replay_is_required() {
    let fixture = Fixture::new();
    let source = fixture.draft.source.clone().unwrap();
    record_actions(&fixture.session, &fixture.draft.actions);
    let child = ManualSessionV1::prepare(&fixture.repository).unwrap();
    child
        .publish_rust_root(
            fixture.session.session_id(),
            1,
            RunControlSessionCheckpointV1::from_session(fixture.root.session()),
        )
        .unwrap();
    record_actions(&child, &fixture.draft.actions[1..2]);
    assert_eq!(
        source.committed_suffix(&child).unwrap(),
        fixture.draft.actions[..2]
    );
    let unrelated = ManualSessionV1::prepare(&fixture.repository).unwrap();
    assert!(source.committed_suffix(&unrelated).is_err());
    let later = ManualDraftSourceV1 {
        committed_actions: 2,
        ..source.clone()
    };
    assert!(later.committed_suffix(&child).is_err());
    let future = ManualDraftSourceV1 {
        committed_actions: fixture.draft.actions.len() + 1,
        ..source.clone()
    };
    assert!(future.committed_suffix(&fixture.session).is_err());
    assert!(prepare_manual_draft_adoption_v1(
        &unrelated,
        &fixture.draft,
        &fixture.root,
        &mut |_| {}
    )
    .is_err());
    let mut candidate = fixture.draft.clone();
    candidate.source = Some(later);
    assert!(
        prepare_manual_draft_adoption_v1(&child, &candidate, &fixture.root, &mut |_| {}).is_err()
    );
    candidate = fixture.draft.clone();
    candidate.actions[0] = SemanticActionV1::EndTurn;
    assert!(
        prepare_manual_draft_adoption_v1(&child, &candidate, &fixture.root, &mut |_| {})
            .err()
            .unwrap()
            .contains("diverges")
    );
    let error =
        prepare_manual_draft_adoption_v1(&child, &fixture.draft, &fixture.root, &mut |_| {})
            .err()
            .unwrap();
    assert!(
        !error.contains("diverges"),
        "matching prefix must still reach and fail raw replay: {error}"
    );
    // A persisted Rust checkpoint and matching actions cannot replace Java evidence.
    assert!(!child.directory().join("drafts").exists());
}

#[test]
fn private_draft_at_another_branch_root_still_requires_raw_provenance() {
    let fixture = Fixture::new();
    record_actions(&fixture.session, &[]);
    let sibling = ManualSessionV1::prepare(&fixture.repository).unwrap();
    record_actions(&sibling, &[]);
    let mut phases = Vec::new();
    let error =
        prepare_manual_draft_adoption_v1(&sibling, &fixture.draft, &fixture.root, &mut |event| {
            phases.push(event)
        })
        .err()
        .expect("matching saved snapshots cannot stand in for raw replay");
    assert!(!error.contains("outside the selected trajectory"));
    assert!(
        !phases.is_empty(),
        "the new branch must reach raw verification"
    );
    assert!(!sibling.directory().join("drafts").exists());

    let mut moved = fixture.root.clone();
    step(&mut moved, &fixture.draft.actions[0]);
    phases.clear();
    let error = prepare_manual_draft_adoption_v1(&sibling, &fixture.draft, &moved, &mut |event| {
        phases.push(event)
    })
    .err()
    .unwrap();
    assert!(error.contains("draft root differs"));
    assert!(
        phases.is_empty(),
        "cross-branch suffix reuse must stop before replay"
    );
}

#[test]
fn private_draft_selection_never_guesses_a_branch_or_ignores_a_changed_source() {
    let mut fixture = Fixture::new();
    fixture.draft.source.as_mut().unwrap().committed_actions = 1;
    record_actions(&fixture.session, &[SemanticActionV1::EndTurn]);
    let first = fixture.workspace();
    let mut files = workspace_files(&fixture.session).unwrap();
    assert_eq!(
        unfinished_workspace_at(&fixture.session, &files, &fixture.draft.root).unwrap(),
        Some(&files[0])
    );
    assert!(unfinished_workspace_at(&fixture.session, &files, &fixture.draft.terminal).is_err());
    let second = fixture.workspace();
    files = workspace_files(&fixture.session).unwrap();
    assert!(unfinished_workspace_at(&fixture.session, &files, &fixture.draft.root).is_err());
    assert_eq!(
        resolve_workspace_handle(first.handle(), &files).unwrap().0,
        first.directory
    );
    assert_eq!(
        resolve_workspace_handle(second.handle(), &files).unwrap().0,
        second.directory
    );
    let unrelated = ManualSessionV1::prepare(&fixture.repository).unwrap();
    assert!(list_manual_draft_workspaces_v1(&unrelated)
        .unwrap()
        .is_empty());
    let child = ManualSessionV1::prepare(&fixture.repository).unwrap();
    child
        .publish_rust_root(
            fixture.session.session_id(),
            1,
            RunControlSessionCheckpointV1::from_session(fixture.root.session()),
        )
        .unwrap();
    record_actions(&child, &[]);
    assert_eq!(list_manual_draft_workspaces_v1(&child).unwrap().len(), 2);
    // Same first eight characters need an explicit longer handle, never latest.
    let mut collisions = files[..1].to_vec();
    let mut id = collisions[0].1.clone();
    id.replace_range(31..32, if id.ends_with('0') { "1" } else { "0" });
    collisions.push((fixture.repository.join("different"), id));
    assert!(resolve_workspace_handle(&format!("W{}", &collisions[0].1[..8]), &collisions).is_err());
    assert_eq!(
        resolve_workspace_handle(
            &workspace_handle(&collisions[0].1, &collisions),
            &collisions
        )
        .unwrap(),
        &collisions[0]
    );
}

#[test]
fn private_draft_after_an_earlier_native_fork_requires_same_root_and_raw_provenance() {
    let fixture = Fixture::new();
    record_actions(&fixture.session, &fixture.draft.actions[..2]);
    let child = ManualSessionV1::prepare(&fixture.repository).unwrap();
    child
        .publish_rust_root(
            fixture.session.session_id(),
            1,
            RunControlSessionCheckpointV1::from_session(fixture.root.session()),
        )
        .unwrap();
    record_actions(&child, &[]);
    let mut candidate = fixture.draft.clone();
    candidate.source.as_mut().unwrap().committed_actions = 2;
    assert!(
        candidate
            .source
            .as_ref()
            .unwrap()
            .committed_suffix(&child)
            .is_err(),
        "the abandoned source suffix never becomes a committed prefix"
    );
    let mut phases = Vec::new();
    let error = prepare_manual_draft_adoption_v1(&child, &candidate, &fixture.root, &mut |event| {
        phases.push(event)
    })
    .err()
    .unwrap();
    assert!(
        !phases.is_empty(),
        "the identical root must reach raw verification: {error}"
    );
    assert!(!error.contains("beyond the selected source prefix"));
    assert!(!child.directory().join("drafts").exists());
    let mut moved = fixture.root.clone();
    step(&mut moved, &fixture.draft.actions[0]);
    phases.clear();
    let error = prepare_manual_draft_adoption_v1(&child, &candidate, &moved, &mut |event| {
        phases.push(event)
    })
    .err()
    .unwrap();
    assert!(error.contains("draft root differs"));
    assert!(
        phases.is_empty(),
        "a matching suffix cannot replace identical-root verification"
    );
}

#[test]
fn private_draft_saved_victory_uses_durable_identity_and_rechecks_its_terminal() {
    let fixture = Fixture::new();
    let mut draft = fixture.workspace();
    for action in &fixture.draft.actions {
        draft.choose(action.clone()).unwrap();
    }
    let short = draft.certify(&fixture.session).unwrap();
    let full = draft.certified.clone().unwrap();
    assert_eq!(full.len(), 33);
    let original = fixture
        .session
        .directory()
        .join("drafts")
        .join(format!("{}.json", &full[1..]));
    let mut other = full[1..].to_owned();
    other.replace_range(31..32, if other.ends_with('0') { "1" } else { "0" });
    fs::copy(&original, original.with_file_name(format!("{other}.json"))).unwrap();
    let longer = draft.certify(&fixture.session).unwrap();
    assert!(longer.len() > short.len());
    assert!(full.starts_with(&longer));
    // Simulate an obsolete saved terminal assertion; verification must publish
    // a fresh proposal instead of returning a stale Certified marker.
    let mut obsolete =
        crate::load_manual_combat_draft_v1(&fixture.session, Path::new(&full)).unwrap();
    obsolete.terminal = obsolete.root.clone();
    fs::write(&original, serde_json::to_vec(&obsolete).unwrap()).unwrap();
    let path = draft.directory.clone();
    let workspace = draft.handle.clone();
    drop(draft);
    let mut draft = fixture.reopen(path, workspace);
    let updated = draft.certify(&fixture.session).unwrap();
    assert_ne!(draft.certified.as_ref().unwrap(), &full);
    crate::load_manual_combat_draft_v1(&fixture.session, Path::new(&updated))
        .unwrap()
        .validate_for_v1(&fixture.root)
        .unwrap();
}
