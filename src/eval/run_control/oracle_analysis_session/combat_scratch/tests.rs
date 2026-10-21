use super::*;

use crate::content::relics::{RelicId, RelicState};
use crate::eval::run_control::{
    seed_oracle_run_explorer_from_session_v1, OracleCombatWitnessOptionsV1,
    OracleRunCombatWitnessBudgetsV1, RunControlConfig, RunControlSession, RunProgressJournalV1,
};
use crate::runtime::combat::CombatCard;
use crate::sim::combat::CombatTerminal;
use crate::state::core::{
    ActiveCombat, ClientInput, CombatContext, EngineState, GridSelectReason, HandSelectReason,
    PendingChoice, PileType, RoomCombatContext,
};
use crate::state::map::node::RoomType;

fn scratch_analysis_at_engine(
    combat: crate::runtime::combat::CombatState,
    engine_state: EngineState,
) -> OracleAnalysisSessionV1 {
    let mut run = RunControlSession::new(RunControlConfig::default());
    run.run_state.act_num = 2;
    run.run_state.floor_num = 17;
    run.run_state.gold = 233;
    run.run_state.relics = vec![RelicState::new(RelicId::PenNib)];
    run.engine_state = engine_state.clone();
    run.active_combat = Some(ActiveCombat::new(
        engine_state,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    let combat_budgets =
        OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default());
    let explorer = seed_oracle_run_explorer_from_session_v1(
        run,
        RunProgressJournalV1::default(),
        &combat_budgets,
        None,
    )
    .expect("seed scratch analysis");
    OracleAnalysisSessionV1::from_explorer(explorer, Some(0), combat_budgets, None, None)
        .expect("scratch analysis")
}

fn one_strike_scratch_analysis() -> OracleAnalysisSessionV1 {
    scratch_analysis_at_engine(one_strike_combat(), EngineState::CombatPlayerTurn)
}

fn one_strike_combat() -> crate::runtime::combat::CombatState {
    let mut combat = crate::test_support::blank_test_combat();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::PenNib));
    let mut monster = crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
    let plan = crate::content::monsters::roll_monster_turn_plan(
        &mut combat.rng.ai_rng,
        &monster,
        combat.meta.ascension_level,
        99,
        std::slice::from_ref(&monster),
        &[],
    );
    monster.set_planned_move_id(plan.move_id);
    monster.set_planned_steps(plan.steps);
    monster.set_planned_visible_spec(plan.visible_spec);
    monster.current_hp = 6;
    monster.max_hp = 6;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        1,
    )];
    combat
}

fn strike_baseline_line_lab(duplicate_strike: bool) -> (OracleAnalysisSessionV1, u32) {
    let mut combat = one_strike_combat();
    if duplicate_strike {
        combat.zones.hand.push(CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            2,
        ));
    }
    let mut analysis = scratch_analysis_at_engine(combat, EngineState::CombatPlayerTurn);
    let (context, root) = {
        let branch = analysis.require_branch(0).expect("combat branch");
        (
            OracleAnalysisCombatScratchContextV1 {
                act: branch.session.run_state.act_num,
                floor: branch.session.run_state.floor_num,
                gold: branch.session.run_state.gold,
            },
            branch
                .session
                .current_active_combat_position()
                .expect("exact combat root"),
        )
    };
    let target = root.combat.entities.monsters[0].id;
    let baseline_turn = root.combat.turn.turn_count;
    analysis.combat_scratch = Some(
        OracleAnalysisCombatLineLabV1::start_with_baseline(
            0,
            context,
            root,
            OracleAnalysisCombatLineLabBaselineSourceV1::ResidentIncumbent,
            &[ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            }],
            250,
        )
        .expect("import verified baseline"),
    );
    (analysis, baseline_turn)
}

#[test]
fn combat_scratch_projects_strength_adjusted_monster_damage_from_execution_semantics() {
    let mut combat = crate::test_support::blank_test_combat();
    let cultist =
        crate::test_support::planned_monster(crate::content::monsters::EnemyId::Cultist, 1);
    let monster_id = cultist.id;
    combat.entities.monsters = vec![cultist];
    crate::content::powers::store::set_powers_for(
        &mut combat,
        monster_id,
        vec![crate::runtime::combat::Power {
            power_type: crate::content::powers::PowerId::Strength,
            instance_id: None,
            amount: 6,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let mut analysis = scratch_analysis_at_engine(combat, EngineState::CombatPlayerTurn);

    let root = analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start strength-adjusted intent scratch");
    let monster = &root.position.monsters[0];
    let preview = monster
        .move_preview
        .as_ref()
        .expect("living attacker has an execution-semantic preview");
    assert_eq!(preview.damage_per_hit, Some(12));
    assert_eq!(preview.hits, 1);
    assert_eq!(preview.total_damage, Some(12));
    let Some(crate::runtime::monster_move::MonsterMoveSpec::Attack(attack)) =
        monster.intent.as_ref()
    else {
        panic!("Cultist attack retains its typed base intent");
    };
    assert_eq!(attack.base_damage, 6);
}

#[test]
fn combat_scratch_preserves_sentry_bolts_original_debuff_intent() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut sentry = crate::test_support::test_monster(crate::content::monsters::EnemyId::Sentry);
    let plan = crate::content::monsters::roll_monster_turn_plan(
        &mut combat.rng.ai_rng,
        &sentry,
        combat.meta.ascension_level,
        99,
        std::slice::from_ref(&sentry),
        &[],
    );
    sentry.set_planned_move_id(plan.move_id);
    sentry.set_planned_steps(plan.steps);
    sentry.set_planned_visible_spec(plan.visible_spec);
    combat.entities.monsters = vec![sentry];
    let mut analysis = scratch_analysis_at_engine(combat, EngineState::CombatPlayerTurn);

    let root = analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start sentry scratch");
    let intent = root.position.monsters[0]
        .intent
        .as_ref()
        .expect("Sentry Bolt has a typed scratch intent");
    let crate::runtime::monster_move::MonsterMoveSpec::AddCard(add_card) = intent else {
        panic!("Sentry Bolt should project AddCard, got {intent:?}");
    };
    assert_eq!(add_card.card_id, crate::content::cards::CardId::Dazed);
    assert_eq!(add_card.amount, 2);
    assert_eq!(
        add_card.destination,
        crate::runtime::monster_move::CardDestination::Discard
    );
    assert_eq!(
        add_card.visible_strength,
        crate::runtime::monster_move::EffectStrength::Normal
    );
}

#[test]
fn combat_scratch_short_selector_forks_from_a_retained_node_and_observes_decision_facts() {
    let mut analysis = one_strike_scratch_analysis();
    let root = analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start combat scratch");
    assert_eq!(root.context.act, 2);
    assert_eq!(root.context.floor, 17);
    assert_eq!(root.context.gold, 233);
    assert_eq!(root.position.player.relics[0].id, RelicId::PenNib);
    assert_eq!(root.position.hand[0].effective_cost, 1);
    assert!(!root.position.monsters[0].planned_steps.is_empty());
    let root_decision = analysis
        .combat_scratch_decision_view(0, 16)
        .expect("observe local scratch capabilities");
    assert_eq!(root_decision.hand[0].hand_index, 0);
    assert!(!root_decision.hand[0].playable_without_target);
    assert_eq!(root_decision.hand[0].playable_target_indices, vec![0]);
    assert_eq!(root_decision.monsters[0].monster_index, 0);

    let end_turn = OracleAnalysisCombatScratchActionSelectorV1::EndTurn { scratch_node_id: 0 };
    let strike = OracleAnalysisCombatScratchActionSelectorV1::HandCard {
        scratch_node_id: 0,
        hand_index: 0,
        target_index: Some(0),
    };
    analysis
        .play_combat_scratch_selector(end_turn, 0, 16)
        .expect("play short end-turn selector");
    let victory = analysis
        .play_combat_scratch_selector(strike, 0, 16)
        .expect("fork from root with cached short selector");
    assert_eq!(victory.position.terminal, CombatTerminal::Win);
    assert_eq!(victory.parent_scratch_node_id, Some(0));
    assert_eq!(victory.scratch_node_count, 3);

    let before_invalid = serde_json::to_value(analysis.checkpoint().expect("before bad identity"))
        .expect("serialize before bad identity");
    let error = analysis
        .play_combat_scratch_selector(
            OracleAnalysisCombatScratchActionSelectorV1::HandCard {
                scratch_node_id: 0,
                hand_index: usize::MAX,
                target_index: None,
            },
            0,
            16,
        )
        .expect_err("unknown node-local hand index must fail");
    assert!(error.contains("local card index"), "{error}");
    assert_eq!(
        serde_json::to_value(analysis.checkpoint().expect("after bad local index"))
            .expect("serialize after bad local index"),
        before_invalid,
        "a rejected local selector must not move the cursor or mutate scratch"
    );
    let error = analysis
        .play_combat_scratch_selector(
            OracleAnalysisCombatScratchActionSelectorV1::Card {
                scratch_node_id: 0,
                card_lineage: crate::test_support::scenario_lineage(999_999),
                target: Some(1),
            },
            0,
            16,
        )
        .expect_err("unknown card identity must fail");
    assert!(error.contains("card lineage"), "{error}");
    assert_eq!(
        serde_json::to_value(analysis.checkpoint().expect("after bad identity"))
            .expect("serialize after bad identity"),
        before_invalid,
        "a rejected identity selector must not move the cursor or mutate scratch"
    );

    let decision = analysis
        .combat_scratch_decision_view(0, 16)
        .expect("compact decision view");
    let encoded = serde_json::to_value(decision).expect("serialize decision view");
    assert!(encoded.get("position").is_none());
    assert!(encoded.get("draw_pile_top_first").is_some());
    assert!(!encoded.to_string().contains("\"uuid\""));
    assert!(!encoded.to_string().contains("\"entity_id\""));
    assert!(encoded["atomic_actions"]
        .as_array()
        .expect("decision actions")
        .iter()
        .all(|action| action.get("action_ref").is_none() && action.get("action_key").is_none()));
}

#[test]
fn combat_scratch_decision_delta_reconstructs_exact_observation_and_rejects_stale_base() {
    let mut analysis = one_strike_scratch_analysis();
    analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start combat scratch");
    let base = analysis
        .combat_scratch_decision_view(0, 16)
        .expect("base decision observation");
    let delta = analysis
        .play_combat_scratch_selector_delta(
            OracleAnalysisCombatScratchActionSelectorV1::HandCard {
                scratch_node_id: 0,
                hand_index: 0,
                target_index: Some(0),
            },
            0,
            16,
        )
        .expect("play local card with delta response");
    let result = analysis
        .combat_scratch_decision_view(0, 16)
        .expect("result decision observation");

    assert_eq!(
        delta.apply_to(&base).expect("apply exact decision delta"),
        result
    );
    assert!(delta.apply_to(&result).is_err());
    assert_eq!(delta.base_scratch_node_id, 0);
    assert_eq!(delta.cursor_scratch_node_id, 1);
    let encoded_delta = serde_json::to_vec(&delta).expect("encode delta");
    assert!(
        encoded_delta.len()
            < serde_json::to_vec(&result)
                .expect("encode full result")
                .len()
    );
    let encoded_delta = String::from_utf8(encoded_delta).expect("delta is UTF-8 JSON");
    assert!(!encoded_delta.contains("uuid"));
    assert!(!encoded_delta.contains("entity_id"));

    let mut turn_analysis = one_strike_scratch_analysis();
    turn_analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start turn-transition scratch");
    let turn_base = turn_analysis
        .combat_scratch_decision_view(0, 16)
        .expect("turn-transition base");
    let turn_delta = turn_analysis
        .play_combat_scratch_selector_delta(
            OracleAnalysisCombatScratchActionSelectorV1::EndTurn { scratch_node_id: 0 },
            0,
            16,
        )
        .expect("end turn with delta response");
    let turn_result = turn_analysis
        .combat_scratch_decision_view(0, 16)
        .expect("turn-transition result");
    assert_eq!(
        turn_delta
            .apply_to(&turn_base)
            .expect("apply turn-transition delta"),
        turn_result
    );

    let mut branch_analysis = one_strike_scratch_analysis();
    branch_analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start branch-delta scratch");
    branch_analysis
        .play_combat_scratch_selector(
            OracleAnalysisCombatScratchActionSelectorV1::EndTurn { scratch_node_id: 0 },
            0,
            16,
        )
        .expect("move cursor away from branch source");
    let branch_base = branch_analysis
        .combat_scratch_decision_view_at(0, 0, 16)
        .expect("observe retained branch source");
    let branch_delta = branch_analysis
        .play_combat_scratch_selector_delta(
            OracleAnalysisCombatScratchActionSelectorV1::HandCard {
                scratch_node_id: 0,
                hand_index: 0,
                target_index: Some(0),
            },
            0,
            16,
        )
        .expect("fork with source-bound delta");
    let branch_result = branch_analysis
        .combat_scratch_decision_view(0, 16)
        .expect("observe branched result");
    assert_eq!(branch_delta.base_scratch_node_id, 0);
    assert_eq!(
        branch_delta
            .apply_to(&branch_base)
            .expect("apply retained-source delta"),
        branch_result
    );
}

fn scratch_action_ref(
    view: &OracleAnalysisCombatScratchViewV1,
    predicate: impl Fn(&ClientInput) -> bool,
) -> String {
    view.legal_actions
        .atomic_actions
        .iter()
        .find(|action| predicate(&action.input))
        .expect("matching scratch action")
        .action_ref
        .clone()
}

#[test]
fn combat_scratch_branches_with_deltas_without_mutating_the_run_tree() {
    let mut analysis = one_strike_scratch_analysis();
    let run_tree_before = serde_json::to_value(analysis.tree()).expect("run tree before scratch");
    let root = analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start combat scratch");
    let end_turn = scratch_action_ref(&root, |input| matches!(input, ClientInput::EndTurn));
    let after_end_turn = analysis
        .play_combat_scratch_action(&end_turn, 0, 16)
        .expect("play scratch end turn");
    assert_eq!(after_end_turn.scratch_node_count, 2);

    analysis
        .back_combat_scratch(0, 16)
        .expect("back to scratch root");
    let root = analysis
        .combat_scratch_view(0, 16)
        .expect("view scratch root");
    let strike = scratch_action_ref(&root, |input| matches!(input, ClientInput::PlayCard { .. }));
    let victory = analysis
        .play_combat_scratch_action(&strike, 0, 16)
        .expect("play scratch strike");
    assert_eq!(victory.position.terminal, CombatTerminal::Win);
    assert_eq!(victory.scratch_node_count, 3);

    let checkpoint = analysis.checkpoint().expect("scratch checkpoint");
    let scratch = checkpoint.combat_scratch.expect("persisted scratch");
    assert_eq!(scratch.nodes.len(), 3);
    assert!(scratch.nodes.iter().all(|node| {
        (node.scratch_node_id == 0 && node.input.is_none())
            || (node.parent_scratch_node_id.is_some() && node.input.is_some())
    }));
    let serialized = serde_json::to_value(&scratch).expect("serialize scratch checkpoint");
    assert!(serialized["nodes"]
        .as_array()
        .expect("scratch nodes")
        .iter()
        .all(|node| node.get("position").is_none()));
    assert_eq!(
        serde_json::to_value(analysis.tree()).expect("run tree after scratch"),
        run_tree_before,
        "scratch branching must not materialize run variations"
    );
}

#[test]
fn combat_scratch_navigation_receipts_select_cached_immutable_frames() {
    let mut analysis = one_strike_scratch_analysis();
    analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start navigation scratch");
    let root = analysis
        .combat_scratch_decision_view(0, 16)
        .expect("cache root decision frame");
    let descendant = analysis
        .play_combat_scratch_selector(
            OracleAnalysisCombatScratchActionSelectorV1::EndTurn { scratch_node_id: 0 },
            0,
            16,
        )
        .map(OracleAnalysisCombatScratchDecisionViewV1::from)
        .expect("create retained descendant");

    let back = analysis
        .back_combat_scratch_receipt()
        .expect("navigate back with receipt");
    assert_eq!(back.kind, ORACLE_ANALYSIS_COMBAT_SCRATCH_NAVIGATION_KIND);
    assert_eq!(
        back.source_scratch_node_id,
        descendant.cursor_scratch_node_id
    );
    assert_eq!(back.cursor_scratch_node_id, root.cursor_scratch_node_id);
    assert_eq!(back.parent_scratch_node_id, root.parent_scratch_node_id);
    assert_eq!(back.scratch_node_count, 2);
    assert_eq!(
        back.apply_to_cached(&descendant, &root)
            .expect("select cached root frame"),
        analysis
            .combat_scratch_decision_view(0, 16)
            .expect("observe selected root")
    );
    assert!(back.apply_to_cached(&root, &root).is_err());
    assert!(back.apply_to_cached(&descendant, &descendant).is_err());

    let focus = analysis
        .focus_combat_scratch_node_receipt(descendant.cursor_scratch_node_id)
        .expect("focus descendant with receipt");
    assert_eq!(focus.source_scratch_node_id, root.cursor_scratch_node_id);
    assert_eq!(
        focus.cursor_scratch_node_id,
        descendant.cursor_scratch_node_id
    );
    assert_eq!(
        focus.parent_scratch_node_id,
        Some(root.cursor_scratch_node_id)
    );
    assert_eq!(
        focus
            .apply_to_cached(&root, &descendant)
            .expect("select cached descendant frame"),
        analysis
            .combat_scratch_decision_view(0, 16)
            .expect("observe selected descendant")
    );
}

#[test]
fn combat_scratch_restore_replays_exact_hashes_and_rejects_stale_refs_atomically() {
    let mut analysis = one_strike_scratch_analysis();
    let budgets = analysis.combat_budgets.clone();
    let root = analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start combat scratch");
    let stale_strike =
        scratch_action_ref(&root, |input| matches!(input, ClientInput::PlayCard { .. }));
    let end_turn = scratch_action_ref(&root, |input| matches!(input, ClientInput::EndTurn));
    let descendant = analysis
        .play_combat_scratch_action(&end_turn, 0, 16)
        .expect("play scratch end turn");
    let expected_hash = descendant.position.fingerprint.exact_state_hash.clone();
    let before_failure = serde_json::to_value(analysis.checkpoint().expect("before stale ref"))
        .expect("serialize before stale ref");
    let error = analysis
        .play_combat_scratch_action(&stale_strike, 0, 16)
        .expect_err("root action ref must be stale at descendant");
    assert!(
        error.contains("stale"),
        "unexpected stale-ref error: {error}"
    );
    assert_eq!(
        serde_json::to_value(analysis.checkpoint().expect("after stale ref"))
            .expect("serialize after stale ref"),
        before_failure,
        "a stale action ref must not mutate scratch or run state"
    );

    let checkpoint = analysis
        .checkpoint()
        .expect("checkpoint scratch descendant");
    let restored = OracleAnalysisSessionV1::restore(checkpoint, budgets, None, None)
        .expect("restore scratch descendant");
    assert_eq!(
        restored
            .combat_scratch_view(0, 16)
            .expect("restored scratch view")
            .position
            .fingerprint
            .exact_state_hash,
        expected_hash
    );
}

#[test]
fn combat_scratch_commit_requires_victory_and_then_materializes_one_atomic_witness() {
    let mut analysis = one_strike_scratch_analysis();
    analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start combat scratch");
    let before_rejection =
        serde_json::to_value(analysis.checkpoint().expect("before nonterminal commit"))
            .expect("serialize before nonterminal commit");
    let error = analysis
        .commit_combat_scratch()
        .expect_err("nonterminal scratch must not commit");
    assert!(error.contains("not a terminal victory"), "{error}");
    assert_eq!(
        serde_json::to_value(analysis.checkpoint().expect("after nonterminal commit"))
            .expect("serialize after nonterminal commit"),
        before_rejection
    );

    let root = analysis
        .combat_scratch_view(0, 16)
        .expect("view scratch root");
    let strike = scratch_action_ref(&root, |input| matches!(input, ClientInput::PlayCard { .. }));
    analysis
        .play_combat_scratch_action(&strike, 0, 16)
        .expect("play winning strike");
    let child = analysis
        .commit_combat_scratch()
        .expect("commit terminal scratch witness");

    assert_eq!(analysis.cursor_node_id(), child);
    assert_eq!(
        analysis.view_cursor().expect("committed child").boundary,
        OracleRunBoundaryV1::Reward
    );
    assert!(analysis
        .checkpoint()
        .expect("post-commit checkpoint")
        .combat_scratch
        .is_none());
    assert_eq!(analysis.tree().nodes.len(), 2);
}

#[test]
fn combat_scratch_bounded_search_appends_a_verified_suffix_without_touching_run_history() {
    let mut analysis = one_strike_scratch_analysis();
    analysis
        .start_combat_scratch(None, 250, 0, 16)
        .expect("start combat scratch");
    let run_tree_before = serde_json::to_value(analysis.tree()).expect("run tree before search");
    let (report, view) = analysis
        .search_combat_scratch(
            OracleAnalysisCombatScratchSearchRequestV1 {
                max_quanta: 4,
                quantum_generation_work: 64,
                quantum_ms: 100,
                wall_ms: 500,
            },
            0,
            16,
        )
        .expect("bounded scratch descendant search");

    assert_eq!(
        report.exit,
        OracleAnalysisCombatScratchSearchExitV1::WitnessAdded
    );
    assert!(report.appended_action_count > 0);
    assert_eq!(report.additional_potions_allowed, 0);
    assert_eq!(view.position.terminal, CombatTerminal::Win);
    assert_eq!(
        serde_json::to_value(analysis.tree()).expect("run tree after search"),
        run_tree_before,
        "search may append scratch deltas but cannot commit run history"
    );
}

#[test]
fn combat_scratch_pages_structured_selection_inputs_without_eager_storage() {
    let mut combat = one_strike_combat();
    combat.zones.draw_pile = vec![
        CombatCard::for_scenario(crate::content::cards::CardId::Strike, 11),
        CombatCard::for_scenario(crate::content::cards::CardId::Defend, 22),
        CombatCard::for_scenario(crate::content::cards::CardId::Bash, 33),
    ]
    .into();
    let engine = EngineState::PendingChoice(PendingChoice::ScrySelect {
        cards: vec![
            crate::content::cards::CardId::Strike,
            crate::content::cards::CardId::Defend,
            crate::content::cards::CardId::Bash,
        ],
        card_lineages: crate::test_support::scenario_lineages([11, 22, 33]),
    });
    let mut analysis = scratch_analysis_at_engine(combat, engine);
    let first_page = analysis
        .start_combat_scratch(None, 250, 0, 2)
        .expect("start structured combat scratch");
    let family = &first_page.legal_actions.selection_families[0];
    assert_eq!(family.total_input_count, 16);
    assert_eq!(family.actions.len(), 2);
    assert_eq!(family.next_page_offset, Some(2));
    assert!(analysis
        .checkpoint()
        .expect("structured scratch checkpoint")
        .combat_scratch
        .expect("structured scratch")
        .nodes
        .iter()
        .all(|node| node.input.is_none()));
    let decision = analysis
        .combat_scratch_decision_view(0, 2)
        .expect("project structured local selections");
    let decision_family = &decision.selection_families[0];
    assert_eq!(
        decision_family
            .domain
            .iter()
            .map(|candidate| candidate.domain_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(decision_family
        .actions
        .iter()
        .flat_map(|action| action.selected_domain_indices.iter())
        .all(|domain_index| *domain_index < 3));
    let encoded = serde_json::to_string(&decision).expect("encode structured local selections");
    assert!(!encoded.contains("uuid"), "{encoded}");

    let second_page = analysis
        .combat_scratch_view(2, 2)
        .expect("second structured page");
    assert_eq!(
        second_page.legal_actions.selection_families[0].page_offset,
        2
    );
    assert_ne!(
        first_page.legal_actions.selection_families[0].actions[0].action_ref,
        second_page.legal_actions.selection_families[0].actions[0].action_ref
    );
}

#[test]
fn combat_line_lab_submits_scry_by_observation_local_domain_indices() {
    let mut combat = one_strike_combat();
    combat.zones.draw_pile = vec![
        CombatCard::for_scenario(crate::content::cards::CardId::Strike, 11),
        CombatCard::for_scenario(crate::content::cards::CardId::Defend, 22),
        CombatCard::for_scenario(crate::content::cards::CardId::Bash, 33),
    ]
    .into();
    let engine = EngineState::PendingChoice(PendingChoice::ScrySelect {
        cards: vec![
            crate::content::cards::CardId::Strike,
            crate::content::cards::CardId::Defend,
            crate::content::cards::CardId::Bash,
        ],
        card_lineages: crate::test_support::scenario_lineages([11, 22, 33]),
    });
    let mut analysis = scratch_analysis_at_engine(combat, engine);
    analysis
        .start_combat_scratch(None, 250, 0, 2)
        .expect("start structured line lab");

    let delta = analysis
        .select_combat_line_lab(vec![2, 0], 0, 2)
        .expect("submit local scry indices");

    let scratch = analysis.combat_scratch.as_ref().expect("active line lab");
    let cursor = scratch
        .nodes
        .get(&scratch.cursor_scratch_node_id)
        .expect("selection child");
    assert_eq!(
        cursor.input.as_ref(),
        Some(&ClientInput::SubmitScryDiscard(vec![2, 0]))
    );
    let encoded = serde_json::to_string(&delta).expect("encode semantic selection delta");
    assert!(!encoded.contains("scratch_node"), "{encoded}");
    assert!(!encoded.contains("uuid"), "{encoded}");
}

#[test]
fn combat_line_lab_rejects_invalid_local_selection_without_mutation() {
    let mut combat = one_strike_combat();
    combat.zones.hand.push(CombatCard::for_scenario(
        crate::content::cards::CardId::Defend,
        22,
    ));
    let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([1, 22, 99]),
        min_cards: 1,
        max_cards: 2,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let mut analysis = scratch_analysis_at_engine(combat, engine);
    analysis
        .start_combat_scratch(None, 250, 0, 2)
        .expect("start structured line lab");
    let root_cursor = analysis
        .combat_scratch
        .as_ref()
        .expect("active line lab")
        .cursor_scratch_node_id;

    for (indices, expected_error) in [
        (vec![], "requires 1..=2"),
        (vec![0, 0], "repeated local domain index"),
        (vec![3], "no local domain index 3"),
        (vec![2], "local domain index 2 is not eligible"),
        (vec![0, 1, 2], "requires 1..=2"),
    ] {
        let error = analysis
            .select_combat_line_lab(indices, 0, 2)
            .expect_err("invalid local selection must fail");
        assert!(error.contains(expected_error), "{error}");
        assert_eq!(
            analysis
                .combat_scratch
                .as_ref()
                .expect("line lab remains active")
                .cursor_scratch_node_id,
            root_cursor,
            "a rejected local selection cannot move the line"
        );
    }
}

#[test]
fn observation_local_selection_compiles_hand_and_grid_card_lineages_in_order() {
    let mut hand_combat = one_strike_combat();
    hand_combat.zones.hand.push(CombatCard::for_scenario(
        crate::content::cards::CardId::Defend,
        22,
    ));
    let hand_analysis = scratch_analysis_at_engine(
        hand_combat,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: crate::test_support::scenario_lineages([1, 22]),
            min_cards: 1,
            max_cards: 2,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        }),
    );
    let hand_position = hand_analysis
        .require_branch(0)
        .expect("hand branch")
        .session
        .current_active_combat_position()
        .expect("hand position");
    assert_eq!(
        resolve_observation_local_selection(&hand_position, &[1, 0]).expect("compile hand indices"),
        ClientInput::SubmitSelection(crate::state::selection::SelectionResolution::card_lineages(
            crate::state::selection::SelectionScope::Hand,
            [
                crate::test_support::scenario_lineage(22),
                crate::test_support::scenario_lineage(1),
            ],
        ))
    );

    let mut grid_combat = one_strike_combat();
    grid_combat.zones.discard_pile = vec![
        CombatCard::for_scenario(crate::content::cards::CardId::Strike, 31),
        CombatCard::for_scenario(crate::content::cards::CardId::Defend, 32),
    ]
    .into();
    let grid_analysis = scratch_analysis_at_engine(
        grid_combat,
        EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile: PileType::Discard,
            candidate_lineages: crate::test_support::scenario_lineages([31, 32]),
            min_cards: 1,
            max_cards: 2,
            can_cancel: false,
            reason: GridSelectReason::DiscardToHand,
        }),
    );
    let grid_position = grid_analysis
        .require_branch(0)
        .expect("grid branch")
        .session
        .current_active_combat_position()
        .expect("grid position");
    assert_eq!(
        resolve_observation_local_selection(&grid_position, &[1, 0]).expect("compile grid indices"),
        ClientInput::SubmitSelection(crate::state::selection::SelectionResolution::card_lineages(
            crate::state::selection::SelectionScope::Grid,
            [
                crate::test_support::scenario_lineage(32),
                crate::test_support::scenario_lineage(31),
            ],
        ))
    );
}

#[test]
fn combat_line_lab_imports_a_complete_baseline_and_branches_by_turn_without_node_ids() {
    let (mut analysis, baseline_turn) = strike_baseline_line_lab(false);
    let scratch = analysis.combat_scratch.as_ref().expect("active line lab");
    assert_eq!(
        scratch
            .line_summary(&scratch.baseline_scratch_node_ids)
            .expect("baseline summary")
            .terminal,
        CombatTerminal::Win
    );

    let open_frame = analysis
        .combat_line_lab_frame(0, 16)
        .expect("terminal baseline frame");
    assert_eq!(open_frame.baseline_action_count, 1);
    assert_eq!(open_frame.terminal, CombatTerminal::Win);
    let encoded = serde_json::to_string(&open_frame).expect("encode line lab frame");
    assert!(!encoded.contains("scratch_node"), "{encoded}");

    analysis
        .goto_combat_line_lab(
            OracleAnalysisCombatLineLabLineV1::Baseline,
            baseline_turn,
            0,
            0,
            16,
        )
        .expect("goto root action");
    let played = analysis
        .play_combat_line_lab_card(crate::content::cards::CardId::Strike, None, None, 0, 16)
        .expect("play unique typed card");
    let encoded = serde_json::to_string(&played).expect("encode line lab delta");
    assert!(!encoded.contains("scratch_node"), "{encoded}");
    let comparison = analysis
        .compare_combat_line_lab()
        .expect("compare exact matching line");
    assert_eq!(comparison.common_prefix_actions, 1);
    assert!(comparison.first_divergence.is_none());
    assert_eq!(comparison.current.terminal, CombatTerminal::Win);
    let encoded = serde_json::to_string(&comparison).expect("encode semantic comparison");
    assert!(!encoded.contains("uuid"), "{encoded}");
    assert!(!encoded.contains("action_key"), "{encoded}");

    analysis
        .goto_combat_line_lab(
            OracleAnalysisCombatLineLabLineV1::Baseline,
            baseline_turn,
            0,
            0,
            16,
        )
        .expect("rewind to baseline root");
    analysis
        .end_combat_line_lab_turn(0, 16)
        .expect("branch with end turn");
    let back = analysis
        .back_combat_line_lab(0, 16)
        .expect("back one current-line action");
    assert_eq!(back.location.action_index, 0);
    analysis
        .end_combat_line_lab_turn(0, 16)
        .expect("restore divergent current line");
    let current_root = analysis
        .goto_combat_line_lab(
            OracleAnalysisCombatLineLabLineV1::Current,
            baseline_turn,
            0,
            0,
            16,
        )
        .expect("goto current-line root action");
    assert_eq!(current_root.location.action_index, 0);
    analysis
        .end_combat_line_lab_turn(0, 16)
        .expect("restore divergent line after current goto");
    let comparison = analysis
        .compare_combat_line_lab()
        .expect("compare divergent line");
    let divergence = comparison
        .first_divergence
        .expect("end turn diverges from strike");
    assert_eq!(divergence.action_index, 0);
    assert!(matches!(
        divergence.baseline_action,
        Some(OracleAnalysisCombatLineLabActionV1::PlayCard { .. })
    ));
    assert_eq!(
        divergence.current_action,
        Some(OracleAnalysisCombatLineLabActionV1::EndTurn)
    );
    assert!(!comparison.current.suffix_known);
}

#[test]
fn combat_line_lab_keeps_a_divergent_win_before_rewind_and_restores_it_without_node_ids() {
    let (mut analysis, baseline_turn) = strike_baseline_line_lab(true);
    let baseline_root = analysis
        .goto_combat_line_lab(
            OracleAnalysisCombatLineLabLineV1::Baseline,
            baseline_turn,
            0,
            0,
            16,
        )
        .expect("leave baseline terminal at root");
    assert!(baseline_root.kept_line.is_none());
    let played = analysis
        .play_combat_line_lab_card(crate::content::cards::CardId::Strike, Some(1), None, 0, 16)
        .expect("play divergent duplicate strike");
    assert!(matches!(
        played,
        OracleAnalysisCombatLineLabPlayCardResultV1::Played { .. }
    ));

    let rewound = analysis
        .goto_combat_line_lab(
            OracleAnalysisCombatLineLabLineV1::Baseline,
            baseline_turn,
            0,
            0,
            16,
        )
        .expect("rewind while protecting divergent win");
    let kept = rewound.kept_line.expect("divergent win was kept");
    assert_eq!(kept.terminal, CombatTerminal::Win);
    assert_eq!(kept.action_count, 1);
    let restored = analysis
        .restore_kept_combat_line_lab(0, 16)
        .expect("restore kept win");
    assert_eq!(restored.terminal, CombatTerminal::Win);
    assert_eq!(restored.location.action_index, 1);
    let encoded = serde_json::to_string(&restored).expect("encode restored frame");
    assert!(!encoded.contains("scratch_node"), "{encoded}");
    assert!(!encoded.contains("uuid"), "{encoded}");

    let scratch = analysis.combat_scratch.as_ref().expect("active line lab");
    let checkpoint = scratch.checkpoint();
    assert!(checkpoint.kept_scratch_node_id.is_some());
    let mut hydrated = OracleAnalysisCombatLineLabV1::restore(
        checkpoint,
        scratch.context.clone(),
        scratch.positions.get(&0).expect("root position").clone(),
    )
    .expect("restore kept line checkpoint");
    assert_eq!(
        hydrated
            .restore_kept_line(0, 16)
            .expect("restore kept win after hydration")
            .terminal,
        CombatTerminal::Win
    );
}

#[test]
fn combat_line_lab_explicit_keep_can_restore_an_unresolved_exact_line() {
    let (mut analysis, baseline_turn) = strike_baseline_line_lab(false);
    analysis
        .goto_combat_line_lab(
            OracleAnalysisCombatLineLabLineV1::Baseline,
            baseline_turn,
            0,
            0,
            16,
        )
        .expect("goto unresolved root");
    let kept = analysis.keep_combat_line_lab().expect("keep exact root");
    assert_eq!(kept.terminal, CombatTerminal::Unresolved);
    analysis
        .play_combat_line_lab_card(crate::content::cards::CardId::Strike, None, None, 0, 16)
        .expect("leave kept root");
    let restored = analysis
        .restore_kept_combat_line_lab(0, 16)
        .expect("restore exact root");
    assert_eq!(restored.terminal, CombatTerminal::Unresolved);
    assert_eq!(restored.location.action_index, 0);
}

#[test]
fn combat_line_lab_returns_typed_duplicate_potion_ambiguity_without_mutation() {
    let mut combat = one_strike_combat();
    combat.entities.potions = vec![
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FearPotion,
            41,
        )),
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FearPotion,
            42,
        )),
    ];
    let mut analysis = scratch_analysis_at_engine(combat, EngineState::CombatPlayerTurn);
    analysis
        .open_combat_line_lab(
            None,
            OracleAnalysisCombatLineLabBaselineSourceV1::Root,
            250,
            0,
            16,
        )
        .expect("open root line lab");
    let before = serde_json::to_value(analysis.checkpoint().expect("before ambiguity"))
        .expect("encode checkpoint");
    let result = analysis
        .use_combat_line_lab_potion(
            crate::content::potions::PotionId::FearPotion,
            None,
            None,
            0,
            16,
        )
        .expect("typed duplicate potion ambiguity");
    let OracleAnalysisCombatLineLabUsePotionResultV1::AmbiguousPotion { candidates, .. } = result
    else {
        panic!("duplicate typed potion id must not be guessed");
    };
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].occurrence, 0);
    assert_eq!(candidates[1].occurrence, 1);
    assert_eq!(
        serde_json::to_value(analysis.checkpoint().expect("after ambiguity"))
            .expect("encode checkpoint"),
        before
    );
}

#[test]
fn combat_line_lab_returns_typed_potion_target_ambiguity_without_mutation() {
    let mut combat = one_strike_combat();
    let mut second_monster = combat.entities.monsters[0].clone();
    second_monster.id = 2;
    second_monster.slot = 1;
    second_monster.logical_position = 1;
    combat.entities.monsters.push(second_monster);
    combat.entities.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FearPotion,
        41,
    ))];
    let mut analysis = scratch_analysis_at_engine(combat, EngineState::CombatPlayerTurn);
    analysis
        .open_combat_line_lab(
            None,
            OracleAnalysisCombatLineLabBaselineSourceV1::Root,
            250,
            0,
            16,
        )
        .expect("open root line lab");
    let before = serde_json::to_value(analysis.checkpoint().expect("before ambiguity"))
        .expect("encode checkpoint");
    let result = analysis
        .use_combat_line_lab_potion(
            crate::content::potions::PotionId::FearPotion,
            None,
            None,
            0,
            16,
        )
        .expect("typed potion target ambiguity");
    let OracleAnalysisCombatLineLabUsePotionResultV1::AmbiguousTarget {
        usable_target_indices,
        ..
    } = result
    else {
        panic!("multiple legal potion targets must not be guessed");
    };
    assert_eq!(usable_target_indices, vec![0, 1]);
    assert_eq!(
        serde_json::to_value(analysis.checkpoint().expect("after ambiguity"))
            .expect("encode checkpoint"),
        before
    );
}

#[test]
fn combat_line_lab_resolves_one_potion_copy_and_one_target_without_internal_ids() {
    let mut combat = one_strike_combat();
    combat.entities.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FearPotion,
        41,
    ))];
    let mut analysis = scratch_analysis_at_engine(combat, EngineState::CombatPlayerTurn);
    analysis
        .open_combat_line_lab(
            None,
            OracleAnalysisCombatLineLabBaselineSourceV1::Root,
            250,
            0,
            16,
        )
        .expect("open root line lab");
    let result = analysis
        .use_combat_line_lab_potion(
            crate::content::potions::PotionId::FearPotion,
            None,
            None,
            0,
            16,
        )
        .expect("resolve unique potion and target");
    assert!(matches!(
        result,
        OracleAnalysisCombatLineLabUsePotionResultV1::Used { .. }
    ));
    let encoded = serde_json::to_string(&result).expect("encode potion delta");
    assert!(!encoded.contains("uuid"), "{encoded}");
    assert!(!encoded.contains("scratch_node"), "{encoded}");
    assert!(!encoded.contains("action_key"), "{encoded}");
}

#[test]
fn combat_line_lab_returns_typed_duplicate_card_ambiguity_without_mutation() {
    let mut combat = one_strike_combat();
    combat.zones.hand.push(CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        2,
    ));
    let mut analysis = scratch_analysis_at_engine(combat, EngineState::CombatPlayerTurn);
    analysis
        .open_combat_line_lab(
            None,
            OracleAnalysisCombatLineLabBaselineSourceV1::Root,
            250,
            0,
            16,
        )
        .expect("open root line lab");
    let before = serde_json::to_value(analysis.checkpoint().expect("before ambiguity"))
        .expect("encode checkpoint");
    let result = analysis
        .play_combat_line_lab_card(crate::content::cards::CardId::Strike, None, None, 0, 16)
        .expect("typed duplicate ambiguity");
    let OracleAnalysisCombatLineLabPlayCardResultV1::AmbiguousCard { candidates, .. } = result
    else {
        panic!("duplicate typed card id must not be guessed");
    };
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].occurrence, 0);
    assert_eq!(candidates[1].occurrence, 1);
    assert_eq!(
        serde_json::to_value(analysis.checkpoint().expect("after ambiguity"))
            .expect("encode checkpoint"),
        before
    );
}
