use std::collections::BTreeMap;

use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::content::cards::{get_card_definition, CardId};
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::super::combat_evidence_manifest::combat_action_sequence_hash;
use super::super::exact_turn_corridor::load_action_segments;
use super::{
    display_path, ActionObservation, CardObservation, EvidenceRecord, FiendFireClassification,
    FiendFireObservation, MonsterObservation, PairCandidate, PlayerObservation,
    PreviousCardBypassObservation, PreviousCardBypassStatus, ReplayFrame, StateObservation,
    EVIDENCE_SCHEMA_NAME,
};

pub(super) fn replay_pair(
    candidate: &PairCandidate,
    max_engine_steps_per_transition: usize,
) -> Result<EvidenceRecord, String> {
    let loaded = load_combat_case(&candidate.case_path).map_err(|error| {
        format!(
            "cannot load candidate case '{}': {error}",
            candidate.case_path.display()
        )
    })?;
    let case_identity = loaded.replay_identity_v2().map_err(|error| {
        format!(
            "candidate case '{}' has invalid replay identity: {error}",
            candidate.case_path.display()
        )
    })?;
    let inputs = load_action_segments(&candidate.action_paths)?;
    let action_hash = action_sequence_hash(&inputs)?;
    let stepper = EngineCombatStepper;
    let mut position = loaded.core.position;
    let root_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
    let record_id = record_id(&root_hash, &action_hash);
    let supplied_action_count = inputs.len();
    validate_manifest_input_identity(
        candidate,
        &case_identity,
        &root_hash,
        &action_hash,
        supplied_action_count,
    )?;
    let mut frames = Vec::with_capacity(inputs.len());

    for (index, input) in inputs.into_iter().enumerate() {
        if combat_terminal(&position.engine, &position.combat) != CombatTerminal::Unresolved {
            break;
        }
        let legal = stepper.atomic_actions(&position);
        let exact_input_is_legal = legal.iter().any(|candidate| candidate == &input)
            || stepper.choice_for_legal_input(&position, &input).is_some();
        if !exact_input_is_legal {
            return Err(format!(
                "pair replay rejected action {index} as illegal from exact state: {input:?}"
            ));
        }
        let before_position = position.clone();
        let before = snapshot(&position);
        let card = played_card(&position, &input);
        let card_type = card
            .as_ref()
            .map(|card| get_card_definition(card.id).card_type);
        let step = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "pair replay transition {index} exceeded the exact transition limit"
            ));
        }
        let observation = ActionObservation {
            index,
            input,
            card,
            card_type,
            before,
            after: snapshot(&step.position),
            terminal_after: step.terminal,
            previous_card_bypass: None,
        };
        position = step.position;
        frames.push(ReplayFrame {
            before_position,
            observation,
        });
    }

    for index in 0..frames.len() {
        if frames[index].observation.card.is_some() {
            let observation =
                previous_card_bypass_from_replay(&frames, index, max_engine_steps_per_transition);
            frames[index].observation.previous_card_bypass = Some(observation);
        }
    }

    let final_terminal = combat_terminal(&position.engine, &position.combat);
    let final_player_hp = position.combat.entities.player.current_hp;
    validate_manifest_outcome(candidate, final_terminal, final_player_hp, frames.len())?;
    let observations = frames
        .iter()
        .map(|frame| frame.observation.clone())
        .collect::<Vec<_>>();
    let fiend_fire_observations =
        fiend_fire_observations_from_replay(&record_id, &root_hash, &frames, final_terminal);
    Ok(EvidenceRecord {
        schema_name: EVIDENCE_SCHEMA_NAME.to_string(),
        schema_version: 3,
        record_id,
        root_exact_state_hash: root_hash,
        case_identity: Some(case_identity),
        action_sequence_blake2b_512: action_hash,
        provenance: candidate.provenance.clone(),
        source_paths: candidate.source_paths.clone(),
        case_path: Some(display_path(&candidate.case_path)),
        action_paths: candidate
            .action_paths
            .iter()
            .map(|path| display_path(path))
            .collect(),
        replay_exact: true,
        supplied_action_count,
        consumed_action_count: observations.len(),
        final_terminal,
        final_player_hp,
        actions: observations,
        fiend_fire_observations,
    })
}

fn validate_manifest_input_identity(
    candidate: &PairCandidate,
    case_identity: &sts_oracle_runtime::eval::combat_case_context::CombatCaseReplayIdentityV2,
    root_hash: &str,
    action_hash: &str,
    supplied_action_count: usize,
) -> Result<(), String> {
    let expected = &candidate.expectations;
    if expected
        .case_identities
        .iter()
        .any(|value| value != case_identity)
    {
        return Err(format!(
            "pair replay rejected manifest case identity: actual capability {:?}, root {}",
            case_identity.capability, case_identity.root_exact_state_hash
        ));
    }
    if expected
        .root_exact_state_hashes
        .iter()
        .any(|value| value != root_hash)
    {
        return Err(format!(
            "pair replay rejected manifest root identity: actual {root_hash}"
        ));
    }
    if expected
        .action_sequence_blake2b_512
        .iter()
        .any(|value| value != action_hash)
    {
        return Err(format!(
            "pair replay rejected manifest action identity: actual {action_hash}"
        ));
    }
    if expected
        .supplied_action_counts
        .iter()
        .any(|value| *value != supplied_action_count)
    {
        return Err(format!(
            "pair replay rejected manifest action count: actual {supplied_action_count}"
        ));
    }
    Ok(())
}

fn validate_manifest_outcome(
    candidate: &PairCandidate,
    final_terminal: CombatTerminal,
    final_player_hp: i32,
    consumed_action_count: usize,
) -> Result<(), String> {
    let expected = &candidate.expectations;
    if expected
        .supplied_action_counts
        .iter()
        .any(|value| *value != consumed_action_count)
    {
        return Err(format!(
            "pair replay rejected manifest consumed action count: actual {consumed_action_count}"
        ));
    }
    if expected
        .final_terminals
        .iter()
        .any(|value| *value != final_terminal)
    {
        return Err(format!(
            "pair replay rejected manifest terminal: actual {final_terminal:?}"
        ));
    }
    if expected
        .final_player_hps
        .iter()
        .any(|value| *value != final_player_hp)
    {
        return Err(format!(
            "pair replay rejected manifest final player HP: actual {final_player_hp}"
        ));
    }
    Ok(())
}

fn snapshot(position: &CombatPosition) -> StateObservation {
    StateObservation {
        turn: position.combat.turn.turn_count,
        energy: i32::from(position.combat.turn.energy),
        player: PlayerObservation {
            hp: position.combat.entities.player.current_hp,
            block: position.combat.entities.player.block,
        },
        hand: position
            .combat
            .zones
            .hand
            .iter()
            .map(card_observation)
            .collect(),
        monsters: position
            .combat
            .entities
            .monsters
            .iter()
            .map(|monster| MonsterObservation {
                id: monster.id,
                hp: monster.current_hp,
                max_hp: monster.max_hp,
                block: monster.block,
                slot: monster.slot,
                is_dying: monster.is_dying,
                half_dead: monster.half_dead,
                is_escaped: monster.is_escaped,
            })
            .collect(),
    }
}

fn played_card(position: &CombatPosition, input: &ClientInput) -> Option<CardObservation> {
    let ClientInput::PlayCard { card_index, .. } = input else {
        return None;
    };
    position
        .combat
        .zones
        .hand
        .get(*card_index)
        .map(card_observation)
}

fn card_observation(card: &sts_oracle_runtime::runtime::combat::CombatCard) -> CardObservation {
    CardObservation {
        id: card.id,
        lineage: card.lineage,
        upgrades: card.upgrades,
        cost_for_turn: card.cost_for_turn,
        free_to_play_once: card.free_to_play_once,
    }
}

fn fiend_fire_observations_from_replay(
    record_id: &str,
    root_hash: &str,
    frames: &[ReplayFrame],
    final_terminal: CombatTerminal,
) -> Vec<FiendFireObservation> {
    let mut observations = Vec::new();
    let actions = frames
        .iter()
        .map(|frame| frame.observation.clone())
        .collect::<Vec<_>>();
    for (index, frame) in frames.iter().enumerate() {
        if frame.observation.card.as_ref().map(|card| card.id) != Some(CardId::FiendFire) {
            continue;
        }
        let immediate = frame.observation.previous_card_bypass.clone().unwrap_or(
            PreviousCardBypassObservation {
                previous_action_index: None,
                status: PreviousCardBypassStatus::NoPreviousCardBoundary,
                terminal_after: None,
                after: None,
            },
        );
        observations.push(build_fiend_fire_observation(
            record_id,
            root_hash,
            &actions,
            index,
            immediate,
            final_terminal,
        ));
    }
    observations
}

fn previous_card_bypass_from_replay(
    frames: &[ReplayFrame],
    index: usize,
    max_engine_steps_per_transition: usize,
) -> PreviousCardBypassObservation {
    let actions = frames
        .iter()
        .map(|frame| frame.observation.clone())
        .collect::<Vec<_>>();
    let Some(previous_action_index) = previous_card_index(&actions, index) else {
        return PreviousCardBypassObservation {
            previous_action_index: None,
            status: PreviousCardBypassStatus::NoPreviousCardBoundary,
            terminal_after: None,
            after: None,
        };
    };
    previous_card_bypass_counterfactual(
        previous_action_index,
        &frames[previous_action_index].before_position,
        &frames[index].observation,
        max_engine_steps_per_transition,
    )
}

fn previous_card_bypass_counterfactual(
    previous_action_index: usize,
    previous_boundary: &CombatPosition,
    current_action: &ActionObservation,
    max_engine_steps_per_transition: usize,
) -> PreviousCardBypassObservation {
    let Some(card) = current_action.card.as_ref() else {
        return PreviousCardBypassObservation {
            previous_action_index: Some(previous_action_index),
            status: PreviousCardBypassStatus::MissingCardIdentity,
            terminal_after: None,
            after: None,
        };
    };
    let ClientInput::PlayCard { target, .. } = current_action.input else {
        return PreviousCardBypassObservation {
            previous_action_index: Some(previous_action_index),
            status: PreviousCardBypassStatus::NotCardPlay,
            terminal_after: None,
            after: None,
        };
    };
    let Some(card_index) = previous_boundary
        .combat
        .zones
        .hand
        .iter()
        .position(|candidate| candidate.lineage == card.lineage)
    else {
        return PreviousCardBypassObservation {
            previous_action_index: Some(previous_action_index),
            status: PreviousCardBypassStatus::CardNotInPreviousHand,
            terminal_after: None,
            after: None,
        };
    };
    let input = ClientInput::PlayCard { card_index, target };
    let stepper = EngineCombatStepper;
    let legal = stepper.atomic_actions(previous_boundary);
    if !legal.iter().any(|candidate| candidate == &input)
        && stepper
            .choice_for_legal_input(previous_boundary, &input)
            .is_none()
    {
        return PreviousCardBypassObservation {
            previous_action_index: Some(previous_action_index),
            status: PreviousCardBypassStatus::IllegalAtPreviousBoundary,
            terminal_after: None,
            after: None,
        };
    }
    let step = stepper.apply_to_stable(
        previous_boundary,
        input,
        CombatStepLimits {
            max_engine_steps: max_engine_steps_per_transition,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return PreviousCardBypassObservation {
            previous_action_index: Some(previous_action_index),
            status: PreviousCardBypassStatus::TransitionLimited,
            terminal_after: None,
            after: None,
        };
    }
    PreviousCardBypassObservation {
        previous_action_index: Some(previous_action_index),
        status: PreviousCardBypassStatus::Applied,
        terminal_after: Some(step.terminal),
        after: Some(snapshot(&step.position)),
    }
}

pub(super) fn previous_card_index(actions: &[ActionObservation], index: usize) -> Option<usize> {
    let turn = actions.get(index)?.before.turn;
    for candidate_index in (0..index).rev() {
        let candidate = &actions[candidate_index];
        if candidate.before.turn != turn {
            break;
        }
        if candidate.card.is_some() {
            return Some(candidate_index);
        }
    }
    None
}

pub(super) fn build_fiend_fire_observation(
    record_id: &str,
    root_hash: &str,
    actions: &[ActionObservation],
    index: usize,
    immediate: PreviousCardBypassObservation,
    final_terminal: CombatTerminal,
) -> FiendFireObservation {
    let fiend_fire = &actions[index];
    let previous_index = previous_card_index(actions, index);
    let previous = previous_index.map(|previous_index| &actions[previous_index]);
    let target = match fiend_fire.input {
        ClientInput::PlayCard { target, .. } => target,
        _ => None,
    };
    let target_before_previous = previous
        .and_then(|previous| target.and_then(|target| previous.before.monster(target)))
        .cloned();
    let target_after_previous = previous
        .and_then(|previous| target.and_then(|target| previous.after.monster(target)))
        .cloned();
    let target_before_fiend_fire = target
        .and_then(|target| fiend_fire.before.monster(target))
        .cloned();
    let target_after_fiend_fire = target
        .and_then(|target| fiend_fire.after.monster(target))
        .cloned();
    let previous_card = previous.and_then(|previous| previous.card.as_ref().map(|card| card.id));
    let previous_card_type = previous.and_then(|previous| previous.card_type.clone());
    let classification = classify_fiend_fire(
        previous,
        target,
        target_before_previous.as_ref(),
        target_after_previous.as_ref(),
        target_after_fiend_fire.as_ref(),
        &immediate,
        final_terminal,
    );
    FiendFireObservation {
        record_id: record_id.to_string(),
        root_exact_state_hash: root_hash.to_string(),
        turn: fiend_fire.before.turn,
        previous_action_index: previous_index,
        fiend_fire_action_index: index,
        previous_card,
        previous_card_type,
        target_id: target,
        target_before_previous,
        target_after_previous,
        target_before_fiend_fire,
        target_after_fiend_fire,
        immediate_fiend_fire: immediate,
        full_line_terminal: final_terminal,
        classification,
    }
}

fn classify_fiend_fire(
    previous: Option<&ActionObservation>,
    target: Option<usize>,
    target_before_previous: Option<&MonsterObservation>,
    target_after_previous: Option<&MonsterObservation>,
    target_after_fiend_fire: Option<&MonsterObservation>,
    immediate: &PreviousCardBypassObservation,
    final_terminal: CombatTerminal,
) -> FiendFireClassification {
    let Some(previous) = previous else {
        return FiendFireClassification::NoPreviousCard;
    };
    if previous.card_type != Some(sts_oracle_runtime::content::cards::CardType::Attack) {
        return FiendFireClassification::PreviousCardNotAttack;
    }
    if target.is_none() {
        return FiendFireClassification::FiendFireHasNoTarget;
    }
    let Some(before) = target_before_previous else {
        return FiendFireClassification::MissingPreviousTargetState;
    };
    let Some(after) = target_after_previous else {
        return FiendFireClassification::MissingPreviousTargetState;
    };
    if before.block <= 0 {
        return FiendFireClassification::NoPositiveBlockBeforePreviousAttack;
    }
    if after.block >= before.block {
        return FiendFireClassification::PreviousAttackDidNotReduceTargetBlock;
    }
    if !target_after_fiend_fire.is_some_and(MonsterObservation::terminal_like) {
        return FiendFireClassification::FiendFireNotTerminalLike;
    }
    match immediate.status {
        PreviousCardBypassStatus::Applied
            if immediate
                .after
                .as_ref()
                .and_then(|state| target.and_then(|target| state.monster(target)))
                .is_some_and(MonsterObservation::terminal_like) =>
        {
            FiendFireClassification::ImmediateFiendFireAlreadyTerminalLike
        }
        PreviousCardBypassStatus::Applied if final_terminal == CombatTerminal::Win => {
            FiendFireClassification::ConfirmedBlockConversionWindow
        }
        PreviousCardBypassStatus::Applied => {
            FiendFireClassification::LocalBlockConversionWithoutCompleteWin
        }
        PreviousCardBypassStatus::TraceOnlyUnavailable => {
            FiendFireClassification::ObservedBlockConversionCandidate
        }
        _ => FiendFireClassification::BlockConversionCounterfactualUnknown,
    }
}

pub(super) fn action_sequence_hash(inputs: &[ClientInput]) -> Result<String, String> {
    combat_action_sequence_hash(inputs)
}

pub(super) fn record_id(root_hash: &str, action_hash: &str) -> String {
    format!(
        "{}:{}",
        root_hash.get(..12).unwrap_or(root_hash),
        action_hash.get(..12).unwrap_or(action_hash)
    )
}

pub(super) fn deduplicate_records(records: Vec<EvidenceRecord>) -> Vec<EvidenceRecord> {
    let mut by_identity = BTreeMap::<String, EvidenceRecord>::new();
    for mut record in records {
        let key = format!(
            "{}|{}",
            record.root_exact_state_hash, record.action_sequence_blake2b_512
        );
        if let Some(existing) = by_identity.get_mut(&key) {
            if record.replay_exact && !existing.replay_exact {
                record.provenance.extend(existing.provenance.clone());
                record.source_paths.extend(existing.source_paths.clone());
                *existing = record;
            } else {
                existing.provenance.extend(record.provenance);
                existing.source_paths.extend(record.source_paths);
            }
        } else {
            by_identity.insert(key, record);
        }
    }
    by_identity.into_values().collect()
}
