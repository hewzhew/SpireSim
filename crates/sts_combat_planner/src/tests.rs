use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use sts_core::content::cards::CardId;
use sts_core::content::monsters::EnemyId;
use sts_core::content::potions::{Potion, PotionId};
use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::{CombatCard, CombatPhase, Power, PowerPayload};
use sts_core::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepResult, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use sts_core::sim::combat_action_surface::{
    combat_legal_action_surface_v2, pending_choice_input_is_legal, CombatLegalActionSurfaceV2,
    CombatSelectionActionFamilyV2,
};
use sts_core::state::core::{ClientInput, EngineState, HandSelectReason, PendingChoice};

use super::*;
use crate::generator::TurnOptionGeneratorPreferredLane;
use crate::types::exact_hash;

mod combat_draft;
mod depth_beam;
mod generator_behavior;
mod local_turn_graph;
mod policy_discrepancy;
mod replay;
mod selection_intent;

const PLAY: ClientInput = ClientInput::PlayCard {
    card_index: 0,
    target: None,
};

#[derive(Clone, Copy)]
struct PreferPlayPolicy;

#[derive(Clone, Copy)]
struct PreferEndTurnPolicy;

impl CombatActionPolicy for PreferPlayPolicy {
    fn weights(&self, _position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        choices
            .iter()
            .map(|choice| match choice {
                CombatPolicyChoice::Atomic(input) if **input == PLAY => 100.0,
                _ => 1.0,
            })
            .collect()
    }

    fn state_guide_rank(&self, position: &CombatPosition) -> Option<CombatStateGuideRank> {
        Some(CombatStateGuideRank::new(vec![
            i32::from(matches!(
                position.engine,
                EngineState::GameOver(sts_core::state::core::RunResult::Victory)
            )),
            position.combat.turn.turn_count as i32,
            -position
                .combat
                .entities
                .monsters
                .iter()
                .map(|monster| monster.current_hp.max(0))
                .sum::<i32>(),
        ]))
    }
}

impl CombatActionPolicy for PreferEndTurnPolicy {
    fn weights(&self, _position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        choices
            .iter()
            .map(|choice| match choice {
                CombatPolicyChoice::Atomic(ClientInput::EndTurn) => 100.0,
                _ => 1.0,
            })
            .collect()
    }

    fn state_guide_rank(&self, position: &CombatPosition) -> Option<CombatStateGuideRank> {
        Some(CombatStateGuideRank::new(vec![i32::from(matches!(
            position.engine,
            EngineState::GameOver(sts_core::state::core::RunResult::Victory)
        ))]))
    }
}

#[derive(Clone, Copy)]
struct PreferSelection22Policy;

impl CombatActionPolicy for PreferSelection22Policy {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        PreferPlayPolicy.weights(position, choices)
    }

    fn structured_selection_member_weights(
        &self,
        _position: &CombatPosition,
        _family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        members
            .iter()
            .map(|input| match input {
                ClientInput::SubmitSelection(resolution)
                    if resolution.selected_card_lineages()
                        == [sts_core::test_support::scenario_lineage(22)] =>
                {
                    100.0
                }
                _ => 1.0,
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
struct SharedGuidePolicy;

const SHARED_TEST_GUIDE: CombatGuideLaneId = CombatGuideLaneId::new(77);

impl CombatActionPolicy for SharedGuidePolicy {
    fn weights(&self, _position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        choices
            .iter()
            .map(|choice| match choice {
                CombatPolicyChoice::Atomic(input) if **input == PLAY => 100.0,
                _ => 1.0,
            })
            .collect()
    }

    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        vec![CombatStateGuide::new(
            SHARED_TEST_GUIDE,
            vec![i32::from(position.combat.turn.energy == 0)],
        )]
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.state_guides(position)
    }
}

#[derive(Clone)]
struct TinyTurnStepper {
    selection_size: Option<u8>,
    winning_selection_22: bool,
    duplicate_unclassified_surface: bool,
    opens_awakened_transition_window: bool,
    activates_finite_skill_conversion: bool,
    lethal_from_turn: Option<u32>,
    terminal_loss: bool,
    terminal_escape: bool,
    timeout_next_selection: Arc<std::sync::atomic::AtomicBool>,
    calls: Arc<Mutex<Vec<ClientInput>>>,
    successor_salt: Arc<AtomicI32>,
}

impl TinyTurnStepper {
    fn plain() -> Self {
        Self {
            selection_size: None,
            winning_selection_22: false,
            duplicate_unclassified_surface: false,
            opens_awakened_transition_window: false,
            activates_finite_skill_conversion: false,
            lethal_from_turn: None,
            terminal_loss: false,
            terminal_escape: false,
            timeout_next_selection: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            calls: Arc::new(Mutex::new(Vec::new())),
            successor_salt: Arc::new(AtomicI32::new(0)),
        }
    }

    fn with_selection() -> Self {
        Self {
            selection_size: Some(2),
            ..Self::plain()
        }
    }

    fn with_single_selection() -> Self {
        Self {
            selection_size: Some(1),
            ..Self::plain()
        }
    }

    fn with_winning_single_selection() -> Self {
        Self {
            selection_size: Some(1),
            winning_selection_22: true,
            ..Self::plain()
        }
    }

    fn with_duplicate_unclassified_surface() -> Self {
        Self {
            duplicate_unclassified_surface: true,
            ..Self::plain()
        }
    }

    fn activating_finite_skill_conversion() -> Self {
        Self {
            activates_finite_skill_conversion: true,
            ..Self::plain()
        }
    }

    fn lethal() -> Self {
        Self {
            lethal_from_turn: Some(1),
            ..Self::plain()
        }
    }

    fn lethal_after_current_turn() -> Self {
        Self {
            lethal_from_turn: Some(2),
            ..Self::plain()
        }
    }

    fn escaping() -> Self {
        Self {
            terminal_escape: true,
            ..Self::plain()
        }
    }

    fn call_count(&self, input: &ClientInput) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|called| *called == input)
            .count()
    }
}

impl CombatStepper for TinyTurnStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        match position.engine {
            EngineState::CombatPlayerTurn if position.combat.turn.energy > 0 => {
                if self.duplicate_unclassified_surface {
                    vec![PLAY, ClientInput::EndTurn, ClientInput::EndTurn]
                } else {
                    vec![PLAY, ClientInput::EndTurn]
                }
            }
            EngineState::CombatPlayerTurn => vec![ClientInput::EndTurn],
            EngineState::PendingChoice(_) => {
                combat_legal_action_surface_v2(&position.engine, &position.combat).atomic_actions
            }
            _ => Vec::new(),
        }
    }

    fn legal_action_surface(&self, position: &CombatPosition) -> CombatLegalActionSurfaceV2 {
        match position.engine {
            EngineState::PendingChoice(_) => {
                combat_legal_action_surface_v2(&position.engine, &position.combat)
            }
            _ => CombatLegalActionSurfaceV2 {
                atomic_actions: self.atomic_actions(position),
                selection_families: Vec::new(),
                indexed_choice: None,
            },
        }
    }

    fn supports_canonical_pending_choice_actions(&self) -> bool {
        true
    }

    fn is_legal_action(&self, position: &CombatPosition, input: &ClientInput) -> bool {
        match &position.engine {
            EngineState::PendingChoice(choice) => {
                pending_choice_input_is_legal(choice, &position.combat, input)
            }
            _ => self.atomic_actions(position).contains(input),
        }
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> CombatStepResult {
        self.calls.lock().unwrap().push(input.clone());
        if matches!(input, ClientInput::SubmitSelection(_))
            && self.timeout_next_selection.swap(false, Ordering::SeqCst)
        {
            return CombatStepResult {
                position: position.clone(),
                terminal: CombatTerminal::Unresolved,
                alive: true,
                truncated: false,
                timed_out: true,
                engine_steps: 1,
            };
        }
        let mut next = position.clone();
        if self.terminal_loss {
            next.engine = EngineState::GameOver(sts_core::state::core::RunResult::Defeat);
        } else if self.terminal_escape {
            next.engine =
                EngineState::RewardScreen(sts_core::state::rewards::RewardState::with_context(
                    sts_core::state::rewards::RewardScreenContext::SmokedCombat,
                ));
            next.combat.runtime.combat_smoked = true;
            next.combat.turn.current_phase = CombatPhase::TurnTransition;
            next.combat.turn.mark_player_escaping();
            next.combat.turn.mark_victory_triggered();
        } else {
            match input {
                ClientInput::PlayCard {
                    card_index: 0,
                    target: None,
                } => {
                    next.combat.turn.energy = 0;
                    next.combat.turn.turn_start_draw_modifier +=
                        self.successor_salt.load(Ordering::SeqCst);
                    if self.opens_awakened_transition_window {
                        let awakened = &mut next.combat.entities.monsters[0];
                        awakened.awakened_one.form1 = false;
                        awakened.half_dead = true;
                        awakened.current_hp = 0;
                    }
                    if self.activates_finite_skill_conversion {
                        let player = next.combat.entities.player.id;
                        sts_core::content::powers::store::set_powers_for(
                            &mut next.combat,
                            player,
                            vec![Power {
                                power_type: PowerId::Corruption,
                                instance_id: None,
                                amount: -1,
                                extra_data: 0,
                                payload: PowerPayload::None,
                                just_applied: false,
                            }],
                        );
                    }
                    if let Some(selection_size) = self.selection_size {
                        next.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
                            candidate_lineages: sts_core::test_support::scenario_lineages([11, 22]),
                            min_cards: selection_size,
                            max_cards: selection_size,
                            can_cancel: false,
                            reason: HandSelectReason::Discard,
                        });
                    } else if self
                        .lethal_from_turn
                        .is_some_and(|turn| next.combat.turn.turn_count >= turn)
                    {
                        next.engine =
                            EngineState::GameOver(sts_core::state::core::RunResult::Victory);
                    }
                }
                ClientInput::SubmitSelection(resolution) => {
                    let selected = resolution.selected_card_lineages();
                    next.combat.turn.turn_start_draw_modifier =
                        i32::try_from(selected[0].ordinal()).unwrap();
                    next.engine = if self.winning_selection_22
                        && selected == [sts_core::test_support::scenario_lineage(22)]
                    {
                        EngineState::GameOver(sts_core::state::core::RunResult::Victory)
                    } else {
                        EngineState::CombatPlayerTurn
                    };
                }
                ClientInput::EndTurn => {
                    next.combat.turn.turn_count += 1;
                    next.engine = EngineState::CombatPlayerTurn;
                }
                _ => panic!("tiny stepper received unsupported input"),
            }
        }
        let terminal = self.terminal(&next);
        CombatStepResult {
            position: next,
            terminal,
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        match position.engine {
            EngineState::RewardScreen(_) if position.combat.runtime.combat_smoked => {
                CombatTerminal::Win
            }
            EngineState::GameOver(sts_core::state::core::RunResult::Victory) => CombatTerminal::Win,
            EngineState::GameOver(sts_core::state::core::RunResult::Defeat) => CombatTerminal::Loss,
            _ => CombatTerminal::Unresolved,
        }
    }
}

fn root() -> CombatDecisionRoot {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.entities.monsters = vec![sts_core::test_support::test_monster(EnemyId::JawWorm)];
    combat.entities.monsters[0].max_hp = 60;
    combat.entities.monsters[0].current_hp = 40;
    combat.turn.turn_count = 1;
    combat.turn.energy = 1;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::Defend, 22),
    ];
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat)).unwrap()
}

fn double_thief_bridge_root() -> CombatDecisionRoot {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.turn.energy = 3;
    let mut looter = sts_core::test_support::planned_monster(EnemyId::Looter, 1);
    looter.id = 10;
    looter.current_hp = 43;
    looter.max_hp = 47;
    let mut mugger = sts_core::test_support::planned_monster(EnemyId::Mugger, 1);
    mugger.id = 20;
    mugger.current_hp = 14;
    mugger.max_hp = 48;
    combat.entities.monsters = vec![looter, mugger];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 3),
        CombatCard::for_scenario(CardId::DarkEmbrace, 10002),
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Defend, 6),
        CombatCard::for_scenario(CardId::PowerThrough, 10000),
    ];
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat)).unwrap()
}

fn escaping_thief_lethal_root() -> CombatDecisionRoot {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.turn.energy = 3;
    let mut looter = sts_core::test_support::planned_monster(EnemyId::Looter, 3);
    looter.id = 10;
    looter.current_hp = 4;
    looter.max_hp = 47;
    looter.thief.stolen_gold = 45;
    combat.entities.monsters = vec![looter];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::ThunderClap, 10003),
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Strike, 2),
    ];
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat)).unwrap()
}

#[test]
fn combat_root_accepts_a_stable_pre_turn_selection_boundary() {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::Defend, 22),
    ];
    let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([11, 22]),
        min_cards: 0,
        max_cards: 2,
        can_cancel: true,
        reason: HandSelectReason::GamblingChip,
    });
    let turn_count = combat.turn.turn_count;

    let root = CombatDecisionRoot::new(CombatPosition::new(engine, combat))
        .expect("Gambling Chip is a stable combat input boundary");

    assert_eq!(root.turn_count(), turn_count);
    assert!(matches!(
        root.position().engine,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::GamblingChip,
            ..
        })
    ));

    let mut generator = TurnOptionGeneratorSession::new(
        root,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 256,
            ..TurnOptionGeneratorConfig::default()
        },
    );
    let report = generator.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(1_000, 8_192),
    );
    assert_eq!(
        report.status,
        TurnOptionGenerationStatus::Complete,
        "{report:#?}"
    );
    let first_inputs = generator
        .completed_options()
        .iter()
        .filter_map(|option| option.actions().first())
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    assert!(
        generator.completed_options().iter().all(|option| {
            option.boundary() == CompleteTurnOptionBoundary::NextPlayerTurn
                && option
                    .actions()
                    .first()
                    .is_some_and(|action| {
                        matches!(
                            action.input,
                            ClientInput::Cancel | ClientInput::SubmitSelection(_)
                        )
                    })
        }),
        "every generated turn must resolve Gambling Chip before playing the combat turn: {first_inputs:?}"
    );
    assert!(
        generator.completed_options().len() >= 4,
        "all legal discard subsets must remain visible to combat planning"
    );
}

fn awakened_root() -> CombatDecisionRoot {
    let mut combat = sts_core::test_support::blank_test_combat();
    let mut awakened = sts_core::test_support::test_monster(EnemyId::AwakenedOne);
    awakened.id = 10;
    awakened.slot = 0;
    combat.entities.monsters = vec![awakened];
    combat.turn.turn_count = 1;
    combat.turn.energy = 1;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::Defend, 22),
    ];
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat)).unwrap()
}

fn awakened_conversion_root() -> CombatDecisionRoot {
    let mut combat = sts_core::test_support::blank_test_combat();
    let mut awakened = sts_core::test_support::test_monster(EnemyId::AwakenedOne);
    awakened.id = 10;
    awakened.slot = 0;
    combat.entities.monsters = vec![awakened];
    combat.turn.turn_count = 1;
    combat.turn.energy = 1;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Corruption, 11),
        CombatCard::for_scenario(CardId::Defend, 22),
    ];
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat)).unwrap()
}

fn threatened_awakened_conversion_root() -> CombatDecisionRoot {
    let mut combat = awakened_conversion_root().position().combat.clone();
    combat.entities.player.current_hp = 1;
    combat.entities.monsters[0].set_planned_move_id(1);
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat)).unwrap()
}

fn transition_window_conversion_root() -> CombatDecisionRoot {
    let mut combat = awakened_conversion_root().position().combat.clone();
    combat.entities.monsters[0].awakened_one.form1 = false;
    combat.entities.monsters[0].half_dead = true;
    combat.entities.monsters[0].current_hp = 0;
    CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat)).unwrap()
}

fn config() -> TurnOptionGeneratorConfig {
    TurnOptionGeneratorConfig {
        max_engine_steps_per_transition: 4,
        ..TurnOptionGeneratorConfig::default()
    }
}

fn exact_actions(
    stepper: &dyn CombatStepper,
    root: &CombatDecisionRoot,
    inputs: impl IntoIterator<Item = ClientInput>,
) -> Vec<TurnOptionAction> {
    let mut position = root.position().clone();
    inputs
        .into_iter()
        .map(|input| {
            let result = stepper.apply_to_stable(
                &position,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: 4,
                    deadline: None,
                },
            );
            let action = TurnOptionAction {
                input,
                expected_successor_hash: exact_hash(&result.position).into(),
                engine_steps: result.engine_steps,
            };
            position = result.position;
            action
        })
        .collect()
}

#[test]
fn exact_action_line_materialization_records_replayable_successors() {
    let root = root();
    let stepper = TinyTurnStepper::plain();
    let inputs = vec![PLAY, ClientInput::EndTurn];

    let line = materialize_exact_action_line(&stepper, root.position(), &inputs, 4)
        .expect("legal public inputs should materialize");
    let (replayed, engine_steps) =
        crate::atomic_witness::replay_atomic_actions(&stepper, root.position(), &line.actions, 4)
            .expect("materialized line should replay");

    assert_eq!(line.actions.len(), inputs.len());
    assert_eq!(line.replay_engine_steps, engine_steps);
    assert_eq!(exact_hash(&line.final_position), exact_hash(&replayed));
}

fn finish(
    session: &mut TurnOptionGeneratorSession,
    stepper: &TinyTurnStepper,
) -> TurnOptionGenerationReport {
    session.advance(stepper, CombatPlanningQuantum::deterministic(100, 100))
}
