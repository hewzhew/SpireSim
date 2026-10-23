use std::cmp::Reverse;
use std::fs;
use sts_presentation::Locale;

use sts_combat_planner::{
    CombatPlanningCounters, CombatPlanningQuantum, CompleteTurnOptionBoundary,
    TurnOptionGenerationStatus, TurnOptionGeneratorConfig,
};
use sts_oracle_learning::agent::task::{TaskBudgetV1, TaskDecisionSiteV1};
use sts_oracle_learning::content::{
    cards::CardId, events::neow::NeowEligibilityV1, monsters::EnemyId,
};
use sts_oracle_learning::eval::run_control::CombatResearchPlanOptionsV1;
use sts_oracle_learning::runtime::combat::CombatCard;
use sts_oracle_learning::sim::combat::CombatPosition;
use sts_oracle_learning::state::core::EngineState;
use sts_oracle_learning::test_support::{blank_test_combat, planned_monster};

use crate::agent_combat_draft::{prepare_commit_attempt, AgentCombatDraftV1};
use crate::driver::{
    combat_research_candidate_id_v1, create_natural_session_v1, RunPlayControllerV1,
};
use crate::selector::{parse_action_selector_v1, semantic_action_for_choice_v1};
use crate::storage::RunPlayPathsV1;

fn test_position(card: CardId, enemy_hp: i32) -> CombatPosition {
    let mut combat = blank_test_combat();
    let mut monster = planned_monster(EnemyId::JawWorm, 1);
    monster.current_hp = enemy_hp;
    monster.max_hp = enemy_hp;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(card, 11)];
    combat.turn.energy = 1;
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

fn draft(position: CombatPosition) -> AgentCombatDraftV1 {
    AgentCombatDraftV1::new(
        7,
        position,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            allow_potion_expenditure: false,
            allow_potion_discard: false,
            ..TurnOptionGeneratorConfig::default()
        },
    )
    .expect("draft")
}

#[test]
fn one_advance_consumes_at_most_one_supplied_quantum_and_reads_consume_none() {
    let position = test_position(CardId::Defend, 40);
    let mut draft = draft(position.clone());

    assert_eq!(draft.consumed(), CombatPlanningCounters::default());
    draft.render_status().expect("status");
    draft.verify_root(7, position).expect("same root");
    assert_eq!(draft.consumed(), CombatPlanningCounters::default());

    let report = draft
        .advance(CombatPlanningQuantum::deterministic(3, 1_000))
        .expect("one quantum");
    assert!(report.after.generation_work <= 3);
    assert!(report.after.engine_steps <= 1_000);
    assert_eq!(draft.consumed(), report.after);
    let before_read = draft.consumed();
    draft.render_status().expect("status after advance");
    assert_eq!(draft.consumed(), before_read);
}

#[test]
fn durable_head_and_exact_root_drift_are_rejected_without_exposing_identity() {
    let position = test_position(CardId::Defend, 40);
    let draft = draft(position.clone());

    let head_error = draft.verify_root(8, position.clone()).unwrap_err();
    assert!(head_error.contains("belongs to head 7"));

    let mut changed = position;
    changed.combat.entities.player.current_hp -= 1;
    let exact_error = draft.verify_root(7, changed).unwrap_err();
    assert!(exact_error.contains("exact root changed"));
    assert!(!exact_error.contains("sha"));
}

#[test]
fn frame_ref_rejects_sibling_labels_and_resume_keeps_the_search_budget() {
    let mut draft = draft(test_position(CardId::Defend, 40));
    let report = draft
        .advance(CombatPlanningQuantum::deterministic(1_000, 65_536))
        .expect("complete turn surface");
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    let frame_ref =
        |draft: &AgentCombatDraftV1| draft.test_env().current_frame_handle().unwrap().to_string();
    let parent = frame_ref(&draft);
    draft.verify_frame_ref(&parent).expect("current frame");

    draft.choose("T0").expect("choose first turn");

    let child = frame_ref(&draft);
    assert!(draft.verify_frame_ref(&parent).is_err());
    draft.verify_frame_ref(&child).expect("current child");
    let child_generation = draft
        .advance(CombatPlanningQuantum::deterministic(1, 65_536))
        .unwrap();
    let child_view = draft.render_status().unwrap();
    let consumed = draft.consumed();
    draft.undo().expect("undo");
    assert_eq!(draft.consumed(), consumed, "undo performs no generation");
    draft
        .verify_frame_ref(&parent)
        .expect("parent frame restored");
    assert!(draft
        .render_status()
        .unwrap()
        .contains(&format!("T0 -> {child}")));
    draft.choose("T1").expect("sibling at same depth");
    assert_ne!(frame_ref(&draft), child);
    assert!(draft.verify_frame_ref(&child).is_err());
    draft.undo().unwrap();
    draft.choose("T0").unwrap();
    draft
        .verify_frame_ref(&child)
        .expect("original child restored");
    assert_eq!(draft.render_status().unwrap(), child_view);
    assert_eq!(
        draft.consumed(),
        consumed,
        "switching adds no generation work"
    );
    let continued = draft
        .advance(CombatPlanningQuantum::deterministic(1, 65_536))
        .unwrap();
    assert_eq!(continued.before, child_generation.after);
    assert_eq!(
        draft.consumed().generation_work,
        consumed.generation_work + continued.after.generation_work
            - continued.before.generation_work
    );
    assert_eq!(
        draft.consumed().engine_steps,
        consumed.engine_steps + continued.after.engine_steps - continued.before.engine_steps
    );
}

#[test]
fn only_an_exact_terminal_win_exports_a_witness() {
    let mut draft = draft(test_position(CardId::Strike, 1));
    let report = draft
        .advance(CombatPlanningQuantum::deterministic(1_000, 65_536))
        .expect("complete turn surface");
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    assert!(draft.victory_witness().expect("nonterminal").is_none());
    let winning = draft
        .test_env()
        .observe()
        .expect("turn view")
        .options
        .into_iter()
        .find(|option| option.boundary == CompleteTurnOptionBoundary::TerminalWin)
        .expect("lethal strike option")
        .handle
        .to_string();

    assert_eq!(
        draft.choose(&winning).expect("choose lethal"),
        CompleteTurnOptionBoundary::TerminalWin
    );
    assert!(draft.victory_witness().expect("winning replay").is_some());
}

struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn analyst_draft_victory_uses_the_existing_combat_plan_transaction() {
    let sandbox = Sandbox(std::env::temp_dir().join(format!(
        "sts_run_play_agent_draft_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    )));
    let paths = RunPlayPathsV1::new(&sandbox.0);
    create_natural_session_v1(
        &paths,
        sts_oracle_learning::eval::run_control::RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            neow_eligibility: NeowEligibilityV1::Mini,
            final_act: true,
            ..Default::default()
        },
        TaskBudgetV1 {
            max_world_transitions: 4_096,
            max_reloads: 0,
        },
        CombatResearchPlanOptionsV1::default(),
    )
    .expect("natural session");
    let mut controller = RunPlayControllerV1::open(paths).expect("controller");

    for _ in 0..8 {
        if controller
            .head()
            .current_decision
            .as_ref()
            .is_some_and(|decision| decision.site == TaskDecisionSiteV1::Combat)
        {
            break;
        }
        let semantic = {
            let inventory = controller
                .head()
                .current_decision
                .as_ref()
                .expect("public decision before combat");
            let decision = inventory.public_decision_v1().expect("public decision");
            inventory
                .candidates
                .iter()
                .filter_map(|candidate| candidate.direct_choice.as_ref())
                .find_map(|choice| semantic_action_for_choice_v1(&decision, choice).ok())
                .expect("one direct action before combat")
        };
        let selector =
            parse_action_selector_v1(&semantic, "draft integration", sts_presentation::Locale::En)
                .expect("semantic selector");
        controller
            .choose_action_v1(&selector, true)
            .expect("advance toward first combat");
    }
    assert_eq!(
        controller
            .head()
            .current_decision
            .as_ref()
            .expect("combat decision")
            .site,
        TaskDecisionSiteV1::Combat
    );

    let root_head_revision = controller.head().head_revision;
    let root_position = controller.current_combat_position_for_lab_v1().unwrap();
    // Exercise the real JSONL dispatcher at this unchanged natural combat
    // root. Inspection and a rejected edit must grant neither search work nor
    // durable commit authority.
    let requests = [
        serde_json::json!({"schema":"sts-run-play-agent-request-v1","request_id":"start","command":"combat_draft_advance","generation_work":0,"engine_steps":0}),
        serde_json::json!({"schema":"sts-run-play-agent-request-v1","request_id":"selection","command":"combat_draft_selection"}),
        serde_json::json!({"schema":"sts-run-play-agent-request-v1","request_id":"expired","command":"combat_draft_selection","edit":{"source_ref":"S0","preferences":[]}}),
        serde_json::json!({"schema":"sts-run-play-agent-request-v1","request_id":"piles","command":"sl_piles"}),
    ];
    let input = requests
        .iter()
        .map(|request| format!("{request}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    crate::agent_protocol::run_agent_jsonl_v1(
        &mut controller,
        crate::agent_protocol::AgentCapabilitiesV1::new(
            crate::agent_protocol::AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None),
        ),
        std::io::Cursor::new(input),
        &mut output,
        Locale::En,
    )
    .unwrap();
    let responses = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 4);
    assert_eq!(responses[1]["result"]["sources"], serde_json::json!([]));
    assert_eq!(responses[1]["result"]["total"], 0);
    assert!(responses[2]
        .to_string()
        .contains("selection source expired"));
    assert_eq!(controller.head().head_revision, root_head_revision);
    assert!(responses[3]["result"]["view"]
        .as_str()
        .unwrap()
        .contains("SL internal combat piles"));
    assert_eq!(
        controller.current_combat_position_for_lab_v1().unwrap(),
        root_position
    );
    let public_permission =
        crate::agent_protocol::AgentPermissionEnvelopeV1::public_manual_v1(None);
    let mut denied = Vec::new();
    crate::agent_protocol::run_agent_jsonl_v1(
        &mut controller,
        crate::agent_protocol::AgentCapabilitiesV1::new(public_permission),
        std::io::Cursor::new(format!("{}\n", requests[3])),
        &mut denied,
        Locale::En,
    )
    .unwrap();
    assert!(String::from_utf8(denied)
        .unwrap()
        .contains("Public manual play disables"));
    assert_eq!(controller.head().head_revision, root_head_revision);
    assert_eq!(
        controller.current_combat_position_for_lab_v1().unwrap(),
        root_position
    );
    let position = controller
        .current_combat_position_for_lab_v1()
        .expect("combat root");
    let mut draft = AgentCombatDraftV1::new(
        root_head_revision,
        position,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            allow_potion_expenditure: false,
            allow_potion_discard: false,
            ..TurnOptionGeneratorConfig::default()
        },
    )
    .expect("analyst draft");
    for _ in 0..8 {
        let generation = draft
            .advance(CombatPlanningQuantum::deterministic(1_024, 65_536))
            .expect("one complete-turn generation quantum");
        assert_eq!(generation.status, TurnOptionGenerationStatus::Complete);
        let selected = draft
            .test_env()
            .observe()
            .expect("draft turn")
            .options
            .into_iter()
            .filter_map(|option| {
                let detail = draft
                    .test_env()
                    .inspect_handle(option.handle)
                    .expect("draft option detail");
                (!matches!(
                    detail.boundary,
                    CompleteTurnOptionBoundary::TerminalLoss | CompleteTurnOptionBoundary::Escape
                ))
                .then_some((
                    detail.boundary != CompleteTurnOptionBoundary::TerminalWin,
                    detail
                        .successor
                        .monsters
                        .iter()
                        .filter(|monster| monster.alive)
                        .map(|monster| monster.hp.max(0))
                        .sum::<i32>(),
                    Reverse(detail.successor.player.hp),
                    option.handle.ordinal(),
                    option.handle.to_string(),
                ))
            })
            .min()
            .expect("non-loss analyst option");
        if draft.choose(&selected.4).expect("analyst selection")
            == CompleteTurnOptionBoundary::TerminalWin
        {
            break;
        }
    }
    let witness = draft
        .victory_witness()
        .expect("draft replay")
        .expect("winning draft");
    let attempt = prepare_commit_attempt(&controller, draft.consumed(), &witness)
        .expect("ordinary commit attempt");
    assert_eq!(attempt.attempt.candidates.len(), 1);
    assert_eq!(attempt.witness_lines.len(), 1);
    let candidate_id =
        combat_research_candidate_id_v1(&attempt.attempt.candidates[0]).expect("candidate id");

    controller
        .anchor_cached_combat_research_candidate_id_v1(
            root_head_revision,
            &attempt.attempt,
            &candidate_id,
        )
        .expect("existing combat-plan transaction");

    assert_eq!(controller.head().head_revision, root_head_revision + 1);
    assert_ne!(
        controller
            .head()
            .current_decision
            .as_ref()
            .expect("combat reward decision")
            .site,
        TaskDecisionSiteV1::Combat
    );
}
