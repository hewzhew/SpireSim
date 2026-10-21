use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::runtime::combat::CombatState;
use crate::sim::combat::{
    apply_combat_input_to_stable_observed_v1, CombatPosition, CombatStepLimits, CombatStepper,
    CombatTerminal, EngineCombatStepper,
};
use crate::state::core::{ClientInput, EngineState};
use crate::state::DomainCardSnapshot;

use super::{
    combat_exact_state_hash_v2, living_enemy_count, CombatSearchV2ActionPreview,
    CombatSearchV2RootActionPrior, SearchTerminalLabel,
};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2WitnessLine {
    pub source: &'static str,
    pub terminal: SearchTerminalLabel,
    pub final_hp: i32,
    pub total_enemy_hp: i32,
    pub action_count: Option<usize>,
    pub actions: Vec<CombatSearchV2ActionPreview>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2WitnessReplay {
    pub action_count: Option<usize>,
    pub replayed_actions: usize,
    pub truncated_by_preview_limit: bool,
    pub terminal: CombatTerminal,
    pub final_hp: i32,
    pub total_enemy_hp: i32,
    pub living_enemy_count: usize,
    pub truncated: bool,
    pub timed_out: bool,
    pub matched_witness_terminal: bool,
    pub matched_witness_final_hp: bool,
    pub matched_witness_enemy_hp: bool,
    pub steps: Vec<CombatSearchV2WitnessReplayStep>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2WitnessReplayStep {
    pub step_index: usize,
    pub action_key: String,
    pub input: ClientInput,
    pub terminal: CombatTerminal,
    pub truncated: bool,
    pub timed_out: bool,
    pub engine_steps: usize,
    pub engine_state: String,
    pub final_hp: i32,
    pub total_enemy_hp: i32,
    pub living_enemy_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatSearchV2WitnessReplayV1 {
    pub terminal: CombatTerminal,
    pub replayed_actions: usize,
    pub steps: Vec<CombatSearchV2WitnessReplayStepV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatSearchV2WitnessReplayStepV1 {
    pub action_index: usize,
    pub action: ClientInput,
    pub drawn_cards: Vec<DomainCardSnapshot>,
    pub terminal: CombatTerminal,
    pub player_hp: i32,
}

#[derive(Clone, Debug)]
pub struct CombatSearchV2WitnessPrior {
    pub prior: CombatSearchV2RootActionPrior,
    pub prior_states: usize,
    pub duplicate_prior_hints: usize,
}

pub fn replay_combat_search_witness_line_v0(
    root: &CombatPosition,
    witness: &CombatSearchV2WitnessLine,
) -> CombatSearchV2WitnessReplay {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut steps = Vec::new();
    let mut truncated = false;
    let mut timed_out = false;
    for (index, action) in witness.actions.iter().cloned().enumerate() {
        if stepper.terminal(&position) != CombatTerminal::Unresolved {
            break;
        }
        let step = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: 250,
                deadline: None,
            },
        );
        truncated |= step.truncated;
        timed_out |= step.timed_out;
        steps.push(witness_replay_step(index + 1, action, &step));
        position = step.position;
        if truncated || timed_out || step.terminal != CombatTerminal::Unresolved {
            break;
        }
    }

    let terminal = stepper.terminal(&position);
    let final_hp = position.combat.entities.player.current_hp;
    let final_enemy_hp = total_enemy_hp(&position.combat);
    CombatSearchV2WitnessReplay {
        action_count: witness.action_count,
        replayed_actions: steps.len(),
        truncated_by_preview_limit: witness
            .action_count
            .is_some_and(|count| count > witness.actions.len()),
        terminal,
        final_hp,
        total_enemy_hp: final_enemy_hp,
        living_enemy_count: living_enemy_count(&position.combat),
        truncated,
        timed_out,
        matched_witness_terminal: terminal_matches(terminal, witness.terminal),
        matched_witness_final_hp: final_hp == witness.final_hp,
        matched_witness_enemy_hp: final_enemy_hp == witness.total_enemy_hp,
        steps,
    }
}

pub fn replay_combat_search_witness_line_v1(
    start: &CombatPosition,
    line: &CombatSearchV2WitnessLine,
    max_engine_steps_per_action: usize,
) -> Result<CombatSearchV2WitnessReplayV1, String> {
    if let Some(expected) = line.action_count {
        if expected != line.actions.len() {
            return Err(format!(
                "witness action preview incomplete: expected {expected}, found {}",
                line.actions.len()
            ));
        }
    }

    let mut position = start.clone();
    let mut steps = Vec::with_capacity(line.actions.len());

    for (action_index, action) in line.actions.iter().enumerate() {
        if !EngineCombatStepper.is_legal_action(&position, &action.input) {
            return Err(format!(
                "illegal witness action at index {action_index}: {:?}",
                action.input
            ));
        }
        let observed = apply_combat_input_to_stable_observed_v1(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_action,
                deadline: None,
            },
        );
        if observed.step.timed_out {
            return Err(format!("timed-out witness step at index {action_index}"));
        }
        if observed.step.truncated {
            return Err(format!("truncated witness step at index {action_index}"));
        }
        let player_hp = observed.step.position.combat.entities.player.current_hp;
        steps.push(CombatSearchV2WitnessReplayStepV1 {
            action_index,
            action: action.input.clone(),
            drawn_cards: observed.drawn_cards,
            terminal: observed.step.terminal,
            player_hp,
        });
        position = observed.step.position;
    }

    let terminal = EngineCombatStepper.terminal(&position);
    if !terminal_matches(terminal, line.terminal) {
        return Err(format!(
            "witness terminal mismatch: expected {:?}, replayed {terminal:?}",
            line.terminal
        ));
    }

    Ok(CombatSearchV2WitnessReplayV1 {
        terminal,
        replayed_actions: steps.len(),
        steps,
    })
}

pub fn compile_combat_search_witness_prior_v0(
    root: &CombatPosition,
    witness: &CombatSearchV2WitnessLine,
) -> CombatSearchV2WitnessPrior {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut scores_by_state: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut duplicate_prior_hints = 0usize;
    for action in witness.actions.iter().cloned() {
        if stepper.terminal(&position) != CombatTerminal::Unresolved {
            break;
        }
        let state_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
        let state_scores = scores_by_state.entry(state_hash).or_default();
        if state_scores.insert(action.action_key, 1.0).is_some() {
            duplicate_prior_hints = duplicate_prior_hints.saturating_add(1);
        }
        let step = stepper.apply_to_stable(
            &position,
            action.input,
            CombatStepLimits {
                max_engine_steps: 250,
                deadline: None,
            },
        );
        position = step.position;
        if step.truncated || step.timed_out || step.terminal != CombatTerminal::Unresolved {
            break;
        }
    }
    let prior_states = scores_by_state.len();
    CombatSearchV2WitnessPrior {
        prior: CombatSearchV2RootActionPrior::from_scores_with_duplicate_count(
            scores_by_state,
            duplicate_prior_hints,
        ),
        prior_states,
        duplicate_prior_hints,
    }
}

fn witness_replay_step(
    step_index: usize,
    action: CombatSearchV2ActionPreview,
    step: &crate::sim::combat::CombatStepResult,
) -> CombatSearchV2WitnessReplayStep {
    let combat = &step.position.combat;
    CombatSearchV2WitnessReplayStep {
        step_index,
        action_key: action.action_key,
        input: action.input,
        terminal: step.terminal,
        truncated: step.truncated,
        timed_out: step.timed_out,
        engine_steps: step.engine_steps,
        engine_state: engine_state_label(&step.position.engine),
        final_hp: combat.entities.player.current_hp,
        total_enemy_hp: total_enemy_hp(combat),
        living_enemy_count: living_enemy_count(combat),
    }
}

fn terminal_matches(actual: CombatTerminal, expected: SearchTerminalLabel) -> bool {
    matches!(
        (actual, expected),
        (CombatTerminal::Win, SearchTerminalLabel::Win)
            | (CombatTerminal::Loss, SearchTerminalLabel::Loss)
            | (CombatTerminal::Unresolved, SearchTerminalLabel::Unresolved)
    )
}

fn engine_state_label(engine: &EngineState) -> String {
    match engine {
        EngineState::PendingChoice(choice) => {
            format!("{}:{}", engine.kind_v1(), choice.kind_v1())
        }
        _ => engine.kind_v1().to_string(),
    }
}

fn total_enemy_hp(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0) + monster.block.max(0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::CombatCard;
    use crate::state::DomainCardSnapshot;
    use crate::test_support::{blank_test_combat, test_monster};

    fn draw_witness_fixture() -> (CombatPosition, CombatSearchV2WitnessLine) {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::BattleTrance, 1)];
        combat.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Defend, 20),
            CombatCard::for_scenario(CardId::Strike, 21),
            CombatCard::for_scenario(CardId::Bash, 22),
        ])
        .into();
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let line = CombatSearchV2WitnessLine {
            source: "test",
            terminal: SearchTerminalLabel::Unresolved,
            final_hp: 80,
            total_enemy_hp: 20,
            action_count: Some(1),
            actions: vec![CombatSearchV2ActionPreview {
                action_key: "battle_trance".to_string(),
                input: ClientInput::PlayCard {
                    card_index: 0,
                    target: None,
                },
            }],
        };
        (position, line)
    }

    #[test]
    fn witness_replay_v1_records_draw_history() {
        let (position, line) = draw_witness_fixture();

        let replay = super::replay_combat_search_witness_line_v1(&position, &line, 20)
            .expect("legal witness should replay exactly");

        assert_eq!(replay.replayed_actions, 1);
        assert_eq!(replay.terminal, CombatTerminal::Unresolved);
        assert_eq!(replay.steps[0].action_index, 0);
        assert_eq!(
            replay.steps[0].drawn_cards,
            vec![
                DomainCardSnapshot {
                    id: CardId::Defend,
                    upgrades: 0,
                    lineage: crate::test_support::scenario_lineage(20),
                },
                DomainCardSnapshot {
                    id: CardId::Strike,
                    upgrades: 0,
                    lineage: crate::test_support::scenario_lineage(21),
                },
                DomainCardSnapshot {
                    id: CardId::Bash,
                    upgrades: 0,
                    lineage: crate::test_support::scenario_lineage(22),
                },
            ]
        );
    }

    #[test]
    fn witness_replay_v1_rejects_illegal_divergence() {
        let (position, mut line) = draw_witness_fixture();
        line.actions[0].input = ClientInput::Proceed;

        let error = super::replay_combat_search_witness_line_v1(&position, &line, 20)
            .expect_err("illegal witness action must be rejected");

        assert!(
            error.contains("illegal witness action at index 0"),
            "{error}"
        );
    }

    #[test]
    fn witness_replay_accepts_legal_selection_outside_legacy_candidate_cap() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat.zones.hand = (0..10)
            .map(|index| CombatCard::for_scenario(CardId::Strike, 1_000 + index))
            .collect();
        let position = CombatPosition::new(
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
                candidate_lineages: crate::test_support::scenario_lineages(1_000..1_010),
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: crate::state::core::HandSelectReason::Discard,
            }),
            combat,
        );
        let line = CombatSearchV2WitnessLine {
            source: "test",
            terminal: SearchTerminalLabel::Unresolved,
            final_hp: 80,
            total_enemy_hp: 20,
            action_count: Some(1),
            actions: vec![CombatSearchV2ActionPreview {
                action_key: "discard_cap_external_card".to_string(),
                input: ClientInput::SubmitSelection(
                    crate::state::selection::SelectionResolution::card_lineages(
                        crate::state::selection::SelectionScope::Hand,
                        [crate::test_support::scenario_lineage(1_009)],
                    ),
                ),
            }],
        };

        let replay = super::replay_combat_search_witness_line_v1(&position, &line, 20)
            .expect("structured legality must not depend on the legacy candidate prefix");

        assert_eq!(replay.replayed_actions, 1);
    }

    #[test]
    fn witness_replay_v1_rejects_truncated_step() {
        let (position, line) = draw_witness_fixture();

        let error = super::replay_combat_search_witness_line_v1(&position, &line, 1)
            .expect_err("truncated witness step must be rejected");

        assert!(
            error.contains("truncated witness step at index 0"),
            "{error}"
        );
    }

    #[test]
    fn witness_replay_v1_rejects_terminal_mismatch() {
        let (position, mut line) = draw_witness_fixture();
        line.terminal = SearchTerminalLabel::Win;

        let error = super::replay_combat_search_witness_line_v1(&position, &line, 20)
            .expect_err("terminal mismatch must be rejected");

        assert!(
            error.contains("witness terminal mismatch: expected Win, replayed Unresolved"),
            "{error}"
        );
    }

    #[test]
    fn witness_replay_v1_rejects_incomplete_action_preview() {
        let (position, mut line) = draw_witness_fixture();
        line.action_count = Some(2);

        let error = super::replay_combat_search_witness_line_v1(&position, &line, 20)
            .expect_err("incomplete witness preview must be rejected");

        assert!(
            error.contains("witness action preview incomplete: expected 2, found 1"),
            "{error}"
        );
    }

    #[test]
    fn witness_replay_v0_json_shape_is_unchanged() {
        let (position, line) = draw_witness_fixture();
        let replay = replay_combat_search_witness_line_v0(&position, &line);

        let json = serde_json::to_string(&(line, replay)).expect("V0 witness should serialize");

        assert!(!json.contains("drawn_cards"), "unexpected V1 field: {json}");
    }
}
