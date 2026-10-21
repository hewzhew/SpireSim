//! Exact grounding of public combat agendas against complete-turn candidates.
//!
//! This crate neither generates agendas nor searches future turns. It verifies
//! already generated complete-turn successors and measures their observable
//! agenda progress. The batch keeps every grounded candidate for diagnostics;
//! only an explicit decision owner may opt into its semantic frontier.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use sts_agent::agent::information::state::{
    public_combat_state_v1, CombatLearningMonsterStateV1, PublicCombatStateV1,
};
use sts_agent::agent::task::{
    public_combat_agenda_for_plan_v1, PublicCombatAgendaOptionV1, PublicCombatAgendaPriorityV1,
    PublicCombatAgendaV1, PublicCombatPlanStageV1, PublicCombatPlanStateV1,
    PUBLIC_COMBAT_PLAN_SCHEMA_NAME, PUBLIC_COMBAT_PLAN_SCHEMA_VERSION,
};
use sts_core::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_core::content::cards::mechanics::{card_mechanic_profile, CardMechanicRole};
use sts_core::content::cards::{get_card_definition, CardType};
use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::Intent;
use sts_core::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_core::state::core::{ClientInput, EngineState, HandSelectReason, PendingChoice};

pub const PUBLIC_COMBAT_AGENDA_GROUNDING_SCHEMA_NAME: &str = "PublicCombatAgendaGrounding";
pub const PUBLIC_COMBAT_AGENDA_GROUNDING_SCHEMA_VERSION: u32 = 1;
const PUBLIC_COMBAT_AGENDA_ESTABLISHED_COMMITMENT_GUIDE_LANE_V1: u32 = 0x4147_0001;
const PUBLIC_COMBAT_AGENDA_FEASIBLE_COMMITMENT_GUIDE_LANE_V1: u32 = 0x4147_0002;

#[derive(Clone)]
pub struct GroundableCompleteTurnV1 {
    pub plan_index: usize,
    pub actions: Vec<ClientInput>,
    pub successor: CombatPosition,
}

#[derive(Clone)]
pub struct PublicCombatAgendaGroundedTurnV1 {
    pub candidate: GroundableCompleteTurnV1,
    pub report: PublicCombatAgendaGroundedCandidateV1,
}

/// One fully grounded agenda batch.
///
/// Grounding intentionally retains every verified candidate for diagnostics,
/// while execution owners may opt into the semantic non-dominated frontier.
/// The fields stay private and this type deliberately does not implement
/// `IntoIterator`: callers must say which of those two contracts they mean.
#[derive(Clone)]
pub struct PublicCombatAgendaGroundedBatchV1 {
    report: PublicCombatAgendaGroundingV1,
    all_candidates: Vec<PublicCombatAgendaGroundedTurnV1>,
}

impl PublicCombatAgendaGroundedBatchV1 {
    pub fn report(&self) -> &PublicCombatAgendaGroundingV1 {
        &self.report
    }

    /// Every replay-verified grounded candidate, including dominated ones.
    /// This view is for diagnostics or a downstream owner with independent
    /// continuation semantics.
    pub fn all_candidates(&self) -> &[PublicCombatAgendaGroundedTurnV1] {
        &self.all_candidates
    }

    pub fn into_all_candidates(self) -> Vec<PublicCombatAgendaGroundedTurnV1> {
        self.all_candidates
    }

    /// Candidates admitted by this agenda's semantic dominance contract.
    pub fn frontier_candidates(&self) -> impl Iterator<Item = &PublicCombatAgendaGroundedTurnV1> {
        self.all_candidates
            .iter()
            .filter(|candidate| candidate.report.on_non_dominated_frontier)
    }

    pub fn into_frontier_candidates(self) -> Vec<PublicCombatAgendaGroundedTurnV1> {
        self.all_candidates
            .into_iter()
            .filter(|candidate| candidate.report.on_non_dominated_frontier)
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatAgendaGroundingV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub agenda: PublicCombatAgendaV1,
    pub candidate_count: usize,
    pub frontier_plan_indices: Vec<usize>,
    pub candidates: Vec<PublicCombatAgendaGroundedCandidateV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatAgendaGroundedCandidateV1 {
    pub plan_index: usize,
    pub action_count: usize,
    pub cards_played: usize,
    pub attacks_played: usize,
    pub terminal: PublicCombatAgendaTerminalV1,
    pub player_hp: i32,
    pub living_enemies: usize,
    pub total_enemy_hp: i32,
    pub option_progress: Vec<PublicCombatAgendaOptionProgressV1>,
    pub on_non_dominated_frontier: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatAgendaTerminalV1 {
    Loss,
    Unresolved,
    Win,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatAgendaOptionProgressV1 {
    pub priority: PublicCombatAgendaPriorityV1,
    pub option: PublicCombatAgendaOptionV1,
    pub achieved: i32,
    pub required: i32,
    pub satisfied: bool,
}

/// One public, same-turn progress rank for an agenda commitment while an
/// exact complete-turn proposal is still being assembled.
///
/// The rank deliberately stops at feasibility.  It does not compare damage,
/// predict a terminal outcome, or interpret hidden continuation state.  Once
/// the persistent mechanic is established, every later prefix keeps the same
/// maximal rank so ordinary follow-up actions cannot erase the milestone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicCombatAgendaPartialCommitmentRankV1 {
    pub established: bool,
    pub immediately_playable: bool,
    pub pending_choice_advances_feasibility: bool,
    pub cheapest_visible_energy_cost: Option<i32>,
}

impl PublicCombatAgendaPartialCommitmentRankV1 {
    /// Returns separate incomparable guide lanes for an already established
    /// commitment and for the best still-undeployed feasible realization.
    pub fn guide_ranks(&self) -> Vec<(u32, Vec<i32>)> {
        if self.established {
            vec![(
                PUBLIC_COMBAT_AGENDA_ESTABLISHED_COMMITMENT_GUIDE_LANE_V1,
                vec![1],
            )]
        } else {
            self.cheapest_visible_energy_cost
                .filter(|_| self.immediately_playable || self.pending_choice_advances_feasibility)
                .map(|cost| {
                    vec![(
                        PUBLIC_COMBAT_AGENDA_FEASIBLE_COMMITMENT_GUIDE_LANE_V1,
                        vec![-cost],
                    )]
                })
                .unwrap_or_default()
        }
    }
}

/// Projects the active commitment in an already-derived public agenda onto a
/// partial exact position.  Exact state is used only to verify present action
/// legality; the semantic role and cost come from the public observation.
pub fn public_combat_agenda_partial_commitment_rank_v1(
    agenda: &PublicCombatAgendaV1,
    partial: &CombatPosition,
) -> Option<PublicCombatAgendaPartialCommitmentRankV1> {
    let role = agenda.entries.iter().find_map(|entry| {
        (entry.priority == PublicCombatAgendaPriorityV1::Commitment)
            .then_some(&entry.option)
            .and_then(|option| match option {
                PublicCombatAgendaOptionV1::EstablishPersistentMechanic { role } => Some(*role),
                _ => None,
            })
    })?;
    let public = public_combat_state_v1(&partial.combat);
    if persistent_role_active(&public, role) {
        return Some(PublicCombatAgendaPartialCommitmentRankV1 {
            established: true,
            immediately_playable: false,
            pending_choice_advances_feasibility: false,
            cheapest_visible_energy_cost: None,
        });
    }

    let immediately_playable_cost = public
        .cards
        .hand
        .cards
        .iter()
        .enumerate()
        .filter(|(_, card)| {
            card_mechanic_profile(card.card_id, card.upgrades)
                .roles
                .contains(role)
        })
        .filter_map(|(card_index, card)| {
            let input = ClientInput::PlayCard {
                card_index,
                target: None,
            };
            (matches!(partial.engine, EngineState::CombatPlayerTurn)
                && EngineCombatStepper.is_legal_action(partial, &input))
            .then_some(if card.free_to_play_once {
                0
            } else {
                card.effective_cost
            })
        })
        .filter(|cost| *cost >= 0)
        .min();
    let pending_upgrade_cost = match &partial.engine {
        EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages,
            reason: HandSelectReason::Upgrade,
            ..
        }) => partial
            .combat
            .zones
            .hand
            .iter()
            .filter(|card| {
                candidate_lineages.contains(&card.lineage)
                    && card_mechanic_profile(card.id, card.upgrades)
                        .roles
                        .contains(role)
            })
            .filter_map(|card| {
                let mut upgraded = card.clone();
                sts_core::content::cards::upgrade_card_once_java(&mut upgraded).then_some(upgraded)
            })
            .map(|card| {
                if card.free_to_play_once {
                    0
                } else {
                    card.cost_for_turn_java()
                }
            })
            .filter(|cost| *cost >= 0 && *cost <= i32::from(public.turn.energy))
            .min(),
        _ => None,
    };
    let cheapest_visible_energy_cost = immediately_playable_cost
        .into_iter()
        .chain(pending_upgrade_cost)
        .min();
    Some(PublicCombatAgendaPartialCommitmentRankV1 {
        established: false,
        immediately_playable: immediately_playable_cost.is_some(),
        pending_choice_advances_feasibility: pending_upgrade_cost.is_some(),
        cheapest_visible_energy_cost,
    })
}

pub fn ground_public_combat_agenda_v1(
    root: &CombatPosition,
    plan: &PublicCombatPlanStateV1,
    agenda: &PublicCombatAgendaV1,
    candidates: Vec<GroundableCompleteTurnV1>,
    max_engine_steps_per_action: usize,
) -> Result<PublicCombatAgendaGroundedBatchV1, String> {
    if max_engine_steps_per_action == 0 {
        return Err("combat agenda grounding requires a positive engine-step limit".into());
    }
    if plan.schema_name != PUBLIC_COMBAT_PLAN_SCHEMA_NAME
        || plan.schema_version != PUBLIC_COMBAT_PLAN_SCHEMA_VERSION
    {
        return Err("combat plan schema is not supported by this agenda grounder".into());
    }
    let root_public = public_combat_state_v1(&root.combat);
    if *agenda != public_combat_agenda_for_plan_v1(&root_public, plan) {
        return Err(
            "combat agenda was not derived from this exact root and persistent plan state".into(),
        );
    }

    let mut grounded = candidates
        .into_iter()
        .map(|candidate| {
            let action_summary =
                verify_and_summarize_actions(root, &candidate, max_engine_steps_per_action)?;
            let successor_public = public_combat_state_v1(&candidate.successor.combat);
            let report = candidate_report(
                &root_public,
                &successor_public,
                agenda,
                &candidate,
                &action_summary,
                EngineCombatStepper.terminal(&candidate.successor),
            );
            Ok(PublicCombatAgendaGroundedTurnV1 { candidate, report })
        })
        .collect::<Result<Vec<_>, String>>()?;

    for index in 0..grounded.len() {
        let dominated = (0..grounded.len()).any(|other| {
            index != other
                && strictly_dominates(plan.stage, &grounded[other].report, &grounded[index].report)
        });
        grounded[index].report.on_non_dominated_frontier = !dominated;
    }
    grounded.sort_by(|left, right| compare_for_display(&left.report, &right.report));
    let frontier_plan_indices = grounded
        .iter()
        .filter(|candidate| candidate.report.on_non_dominated_frontier)
        .map(|candidate| candidate.report.plan_index)
        .collect::<Vec<_>>();
    let report = PublicCombatAgendaGroundingV1 {
        schema_name: PUBLIC_COMBAT_AGENDA_GROUNDING_SCHEMA_NAME.to_string(),
        schema_version: PUBLIC_COMBAT_AGENDA_GROUNDING_SCHEMA_VERSION,
        agenda: agenda.clone(),
        candidate_count: grounded.len(),
        frontier_plan_indices,
        candidates: grounded.iter().map(|item| item.report.clone()).collect(),
    };
    Ok(PublicCombatAgendaGroundedBatchV1 {
        report,
        all_candidates: grounded,
    })
}

#[derive(Default)]
struct ActionSummaryV1 {
    cards_played: usize,
    attacks_played: usize,
    roles_played: Vec<CardMechanicRole>,
}

fn verify_and_summarize_actions(
    root: &CombatPosition,
    candidate: &GroundableCompleteTurnV1,
    max_engine_steps_per_action: usize,
) -> Result<ActionSummaryV1, String> {
    let mut position = root.clone();
    let mut summary = ActionSummaryV1::default();
    for input in &candidate.actions {
        if let ClientInput::PlayCard { card_index, .. } = input {
            let card = position.combat.zones.hand.get(*card_index).ok_or_else(|| {
                format!(
                    "agenda plan {} refers to absent hand index {card_index}",
                    candidate.plan_index
                )
            })?;
            summary.cards_played += 1;
            if get_card_definition(card.id).card_type == CardType::Attack {
                summary.attacks_played += 1;
            }
            summary
                .roles_played
                .extend(card_mechanic_profile(card.id, card.upgrades).roles.iter());
        }
        if !EngineCombatStepper.is_legal_action(&position, input)
            && EngineCombatStepper
                .choice_for_legal_input(&position, input)
                .is_none()
        {
            return Err(format!(
                "agenda plan {} contains an illegal exact action: {input:?}",
                candidate.plan_index
            ));
        }
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_action,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "agenda plan {} did not reach a stable exact boundary",
                candidate.plan_index
            ));
        }
        position = step.position;
    }
    if combat_exact_state_hash_v2(&position.engine, &position.combat)
        != combat_exact_state_hash_v2(&candidate.successor.engine, &candidate.successor.combat)
    {
        return Err(format!(
            "agenda plan {} successor disagrees with exact replay",
            candidate.plan_index
        ));
    }
    Ok(summary)
}

fn candidate_report(
    root: &PublicCombatStateV1,
    successor: &PublicCombatStateV1,
    agenda: &PublicCombatAgendaV1,
    candidate: &GroundableCompleteTurnV1,
    actions: &ActionSummaryV1,
    terminal: CombatTerminal,
) -> PublicCombatAgendaGroundedCandidateV1 {
    let living_enemies = living_enemy_count(successor);
    PublicCombatAgendaGroundedCandidateV1 {
        plan_index: candidate.plan_index,
        action_count: candidate.actions.len(),
        cards_played: actions.cards_played,
        attacks_played: actions.attacks_played,
        terminal: match terminal {
            CombatTerminal::Loss => PublicCombatAgendaTerminalV1::Loss,
            CombatTerminal::Unresolved => PublicCombatAgendaTerminalV1::Unresolved,
            CombatTerminal::Win => PublicCombatAgendaTerminalV1::Win,
        },
        player_hp: successor.player.hp,
        living_enemies,
        total_enemy_hp: successor
            .monsters
            .iter()
            .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
            .map(|monster| monster.hp.max(0))
            .sum(),
        option_progress: agenda
            .entries
            .iter()
            .cloned()
            .map(|entry| option_progress(root, successor, actions, entry.priority, entry.option))
            .collect(),
        on_non_dominated_frontier: false,
    }
}

fn option_progress(
    root: &PublicCombatStateV1,
    successor: &PublicCombatStateV1,
    actions: &ActionSummaryV1,
    priority: PublicCombatAgendaPriorityV1,
    option: PublicCombatAgendaOptionV1,
) -> PublicCombatAgendaOptionProgressV1 {
    let (achieved, required) = match &option {
        PublicCombatAgendaOptionV1::EstablishPersistentMechanic { role } => {
            (i32::from(persistent_role_active(successor, *role)), 1)
        }
        PublicCombatAgendaOptionV1::ReduceLivingEnemyCount {
            current_living_enemies,
        } => (
            current_living_enemies.saturating_sub(living_enemy_count(successor)) as i32,
            1,
        ),
        PublicCombatAgendaOptionV1::ApplySharedEnemyControl {
            roles,
            minimum_living_targets,
        } => (
            root.monsters
                .iter()
                .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
                .filter(|monster| {
                    successor_monster(successor, monster).is_some_and(|target| {
                        roles
                            .iter()
                            .all(|role| enemy_control_role_active(target, *role))
                    })
                })
                .count() as i32,
            *minimum_living_targets as i32,
        ),
        PublicCombatAgendaOptionV1::ConvertCardThroughEstablishedPayoff {
            conversion_role, ..
        } => (
            i32::from(
                actions.roles_played.contains(conversion_role)
                    && successor.cards.exhaust.cards.len() > root.cards.exhaust.cards.len(),
            ),
            1,
        ),
        PublicCombatAgendaOptionV1::PreserveCardPlayThreshold {
            maximum_cards_played_this_turn,
        } => {
            let played = usize::from(root.turn.counters.cards_played_this_turn)
                .saturating_add(actions.cards_played);
            (
                usize::from(*maximum_cards_played_this_turn)
                    .saturating_add(1)
                    .saturating_sub(played) as i32,
                1,
            )
        }
        PublicCombatAgendaOptionV1::PreserveNoAttackTurn => (
            i32::from(
                usize::from(root.turn.counters.attacks_played_this_turn)
                    .saturating_add(actions.attacks_played)
                    == 0,
            ),
            1,
        ),
        PublicCombatAgendaOptionV1::ExerciseCardAccess { role } => {
            (i32::from(actions.roles_played.contains(role)), 1)
        }
        PublicCombatAgendaOptionV1::ApplyPersistentEnemyAttackReduction { role } => (
            i32::from(
                actions.roles_played.contains(role)
                    && total_enemy_strength(successor) < total_enemy_strength(root),
            ),
            1,
        ),
        PublicCombatAgendaOptionV1::ExploitVisiblePreparationWindow {
            deployment_role,
            minimum_sleeping_targets,
        } => (
            i32::from(
                actions.roles_played.contains(deployment_role)
                    && total_enemy_strength(successor) < total_enemy_strength(root)
                    && preserved_sleeping_enemy_count(root, successor) >= *minimum_sleeping_targets,
            ),
            1,
        ),
        PublicCombatAgendaOptionV1::PreservePlayerLife { current_player_hp } => {
            (successor.player.hp, *current_player_hp)
        }
        PublicCombatAgendaOptionV1::AdvanceLethalConversion {
            current_total_enemy_hp,
        } => (
            current_total_enemy_hp.saturating_sub(total_enemy_hp(successor)),
            *current_total_enemy_hp,
        ),
    };
    PublicCombatAgendaOptionProgressV1 {
        priority,
        option,
        achieved,
        required,
        satisfied: achieved >= required,
    }
}

fn persistent_role_active(state: &PublicCombatStateV1, role: CardMechanicRole) -> bool {
    match role {
        CardMechanicRole::ExhaustPayoff => state.player.powers.iter().any(|power| {
            matches!(power.power, PowerId::FeelNoPain | PowerId::DarkEmbrace) && power.amount > 0
        }),
        CardMechanicRole::StatusDrawPayoff => state.player.powers.iter().any(|power| {
            matches!(power.power, PowerId::Evolve | PowerId::FireBreathing) && power.amount > 0
        }),
        _ => false,
    }
}

fn enemy_control_role_active(
    monster: &CombatLearningMonsterStateV1,
    role: CardMechanicRole,
) -> bool {
    let power = match role {
        CardMechanicRole::ApplyWeak => PowerId::Weak,
        CardMechanicRole::ApplyVulnerable => PowerId::Vulnerable,
        _ => return false,
    };
    monster
        .powers
        .iter()
        .any(|candidate| candidate.power == power && candidate.amount > 0)
}

fn successor_monster<'a>(
    successor: &'a PublicCombatStateV1,
    root: &CombatLearningMonsterStateV1,
) -> Option<&'a CombatLearningMonsterStateV1> {
    successor
        .monsters
        .iter()
        .find(|monster| monster.slot == root.slot)
}

fn living_enemy_count(state: &PublicCombatStateV1) -> usize {
    state
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .count()
}

fn total_enemy_strength(state: &PublicCombatStateV1) -> i32 {
    state
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .flat_map(|monster| monster.powers.iter())
        .filter(|power| power.power == PowerId::Strength)
        .map(|power| power.amount)
        .sum()
}

fn preserved_sleeping_enemy_count(
    root: &PublicCombatStateV1,
    successor: &PublicCombatStateV1,
) -> usize {
    root.monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .filter(|monster| monster.intent.intent == Some(Intent::Sleep))
        .filter(|monster| {
            successor_monster(successor, monster).is_some_and(|successor| {
                successor.alive
                    && !successor.dying
                    && !successor.escaped
                    && successor.intent.intent == Some(Intent::Sleep)
            })
        })
        .count()
}

fn total_enemy_hp(state: &PublicCombatStateV1) -> i32 {
    state
        .monsters
        .iter()
        .filter(|monster| monster.alive && !monster.dying && !monster.escaped)
        .map(|monster| monster.hp.max(0))
        .sum()
}

fn strictly_dominates(
    stage: PublicCombatPlanStageV1,
    candidate: &PublicCombatAgendaGroundedCandidateV1,
    other: &PublicCombatAgendaGroundedCandidateV1,
) -> bool {
    let candidate_terminal = terminal_rank(candidate.terminal);
    let other_terminal = terminal_rank(other.terminal);
    if candidate_terminal != other_terminal {
        return candidate_terminal > other_terminal;
    }
    if candidate.terminal == PublicCombatAgendaTerminalV1::Loss {
        return false;
    }
    if stage == PublicCombatPlanStageV1::ConvertToLethal {
        return false;
    }
    for priority in [
        PublicCombatAgendaPriorityV1::Commitment,
        PublicCombatAgendaPriorityV1::ThreatResponse,
        PublicCombatAgendaPriorityV1::Development,
        PublicCombatAgendaPriorityV1::ResourcePreference,
    ] {
        let pairs = candidate
            .option_progress
            .iter()
            .zip(&other.option_progress)
            .filter(|(left, _)| left.priority == priority)
            .map(|(left, right)| (dominance_progress(left), dominance_progress(right)))
            .collect::<Vec<_>>();
        if pairs.iter().all(|(left, right)| left == right) {
            continue;
        }
        return pairs.iter().all(|(left, right)| left >= right)
            && pairs.iter().any(|(left, right)| left > right);
    }
    candidate.player_hp > other.player_hp
}

fn dominance_progress(progress: &PublicCombatAgendaOptionProgressV1) -> i32 {
    match progress.option {
        // Non-terminal damage is not a proof that one exact continuation is
        // better: it can spend a retained attack, alter the next hand, or
        // move an enemy into a different future phase. Terminal victory is
        // already handled by the terminal rank above.
        PublicCombatAgendaOptionV1::AdvanceLethalConversion { .. } => 0,
        _ => progress.achieved,
    }
}

fn compare_for_display(
    left: &PublicCombatAgendaGroundedCandidateV1,
    right: &PublicCombatAgendaGroundedCandidateV1,
) -> Ordering {
    right
        .on_non_dominated_frontier
        .cmp(&left.on_non_dominated_frontier)
        .then_with(|| terminal_rank(right.terminal).cmp(&terminal_rank(left.terminal)))
        .then_with(|| {
            let left_satisfied = left
                .option_progress
                .iter()
                .filter(|item| item.satisfied)
                .count();
            let right_satisfied = right
                .option_progress
                .iter()
                .filter(|item| item.satisfied)
                .count();
            right_satisfied.cmp(&left_satisfied)
        })
        .then_with(|| right.player_hp.cmp(&left.player_hp))
        .then_with(|| left.living_enemies.cmp(&right.living_enemies))
        .then_with(|| left.total_enemy_hp.cmp(&right.total_enemy_hp))
        .then_with(|| left.cards_played.cmp(&right.cards_played))
        .then_with(|| left.plan_index.cmp(&right.plan_index))
}

fn terminal_rank(terminal: PublicCombatAgendaTerminalV1) -> u8 {
    match terminal {
        PublicCombatAgendaTerminalV1::Loss => 0,
        PublicCombatAgendaTerminalV1::Unresolved => 1,
        PublicCombatAgendaTerminalV1::Win => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;
    use sts_core::content::monsters::EnemyId;
    use sts_core::runtime::combat::CombatCard;
    use sts_core::runtime::monster_move::MonsterMoveSpec;
    use sts_core::state::core::EngineState;
    use sts_core::test_support::{blank_test_combat, test_monster};

    #[test]
    fn grounded_batch_separates_all_candidates_from_the_semantic_frontier() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::FeelNoPain, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let root_public = public_combat_state_v1(&root.combat);
        let plan = sts_agent::agent::task::public_combat_plan_v1(&root_public);
        let agenda = public_combat_agenda_for_plan_v1(&root_public, &plan);
        let engine = candidate(
            &root,
            0,
            vec![ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }],
        );
        let pass = candidate(
            &root,
            1,
            vec![ClientInput::PlayCard {
                card_index: 1,
                target: None,
            }],
        );

        let grounded =
            ground_public_combat_agenda_v1(&root, &plan, &agenda, vec![engine, pass], 250)
                .expect("ground agenda");

        assert_eq!(grounded.report().frontier_plan_indices, vec![0]);
        assert_eq!(grounded.all_candidates().len(), 2);
        assert!(grounded
            .all_candidates()
            .iter()
            .any(|candidate| candidate.report.plan_index == 1));
        assert_eq!(
            grounded
                .frontier_candidates()
                .map(|candidate| candidate.report.plan_index)
                .collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            grounded
                .into_frontier_candidates()
                .into_iter()
                .map(|candidate| candidate.report.plan_index)
                .collect::<Vec<_>>(),
            vec![0]
        );
    }

    #[test]
    fn supported_status_draw_payoff_breaks_an_equal_survival_frontier() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::FireBreathing, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::RecklessCharge, 3)].into();
        combat.meta.master_deck_snapshot = combat
            .zones
            .hand
            .iter()
            .chain(combat.zones.draw_pile.iter())
            .cloned()
            .collect::<Vec<_>>()
            .into();
        combat.entities.monsters = vec![
            test_monster(EnemyId::JawWorm),
            test_monster(EnemyId::Cultist),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let root_public = public_combat_state_v1(&root.combat);
        let plan = sts_agent::agent::task::public_combat_plan_v1(&root_public);
        let agenda = public_combat_agenda_for_plan_v1(&root_public, &plan);
        let engine = candidate(
            &root,
            0,
            vec![ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }],
        );
        let defend = candidate(
            &root,
            1,
            vec![ClientInput::PlayCard {
                card_index: 1,
                target: None,
            }],
        );

        let grounded =
            ground_public_combat_agenda_v1(&root, &plan, &agenda, vec![engine, defend], 250)
                .expect("ground supported status payoff");

        assert_eq!(grounded.report().frontier_plan_indices, vec![0]);
        assert!(grounded.all_candidates()[0]
            .report
            .option_progress
            .iter()
            .any(|progress| matches!(
                progress,
                PublicCombatAgendaOptionProgressV1 {
                    priority: PublicCombatAgendaPriorityV1::Development,
                    option: PublicCombatAgendaOptionV1::EstablishPersistentMechanic {
                        role: CardMechanicRole::StatusDrawPayoff,
                    },
                    satisfied: true,
                    ..
                }
            )));
    }

    #[test]
    fn partial_commitment_rank_tracks_feasibility_then_keeps_establishment() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::DarkEmbrace, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
        ];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let root_public = public_combat_state_v1(&root.combat);
        let plan = sts_agent::agent::task::public_combat_plan_v1(&root_public);
        let agenda = public_combat_agenda_for_plan_v1(&root_public, &plan);

        let base = public_combat_agenda_partial_commitment_rank_v1(&agenda, &root)
            .expect("the exhaust-payoff commitment is active");
        assert_eq!(
            base.guide_ranks(),
            vec![(
                PUBLIC_COMBAT_AGENDA_FEASIBLE_COMMITMENT_GUIDE_LANE_V1,
                vec![-2]
            )]
        );

        let mut upgraded = root.clone();
        upgraded.combat.zones.hand[0].upgrades = 1;
        let upgraded_rank = public_combat_agenda_partial_commitment_rank_v1(&agenda, &upgraded)
            .expect("the upgraded payoff is still the active commitment");
        assert_eq!(
            upgraded_rank.guide_ranks(),
            vec![(
                PUBLIC_COMBAT_AGENDA_FEASIBLE_COMMITMENT_GUIDE_LANE_V1,
                vec![-1]
            )]
        );

        let deployed = candidate(
            &upgraded,
            0,
            vec![ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }],
        )
        .successor;
        let established = public_combat_agenda_partial_commitment_rank_v1(&agenda, &deployed)
            .expect("the deployed payoff satisfies the commitment");
        assert_eq!(
            established.guide_ranks(),
            vec![(
                PUBLIC_COMBAT_AGENDA_ESTABLISHED_COMMITMENT_GUIDE_LANE_V1,
                vec![1]
            )]
        );

        let followed_up = candidate(
            &deployed,
            0,
            vec![ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            }],
        )
        .successor;
        let after_follow_up =
            public_combat_agenda_partial_commitment_rank_v1(&agenda, &followed_up)
                .expect("later actions do not erase an established commitment");
        assert_eq!(after_follow_up.guide_ranks(), established.guide_ranks());
    }

    #[test]
    fn preparation_progress_requires_both_deployment_and_preserved_visible_sleep() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.set_planned_visible_spec(Some(MonsterMoveSpec::Sleep));
        combat.entities.monsters = vec![monster];
        let root = public_combat_state_v1(&combat);
        let mut preserved = root.clone();
        preserved.monsters[0].powers.push(
            sts_agent::agent::information::state::CombatLearningPowerV1 {
                source_creature: None,
                power: PowerId::Strength,
                amount: -2,
                extra_data: 0,
                just_applied: false,
                payload_card: None,
            },
        );
        let mut awakened = preserved.clone();
        awakened.monsters[0].intent.intent = Some(Intent::Buff);
        let option = PublicCombatAgendaOptionV1::ExploitVisiblePreparationWindow {
            deployment_role: CardMechanicRole::PersistentEnemyAttackReduction,
            minimum_sleeping_targets: 1,
        };
        let deployed = ActionSummaryV1 {
            roles_played: vec![CardMechanicRole::PersistentEnemyAttackReduction],
            ..ActionSummaryV1::default()
        };

        let satisfied = option_progress(
            &root,
            &preserved,
            &deployed,
            PublicCombatAgendaPriorityV1::ThreatResponse,
            option.clone(),
        );
        let woke_target = option_progress(
            &root,
            &awakened,
            &deployed,
            PublicCombatAgendaPriorityV1::ThreatResponse,
            option.clone(),
        );
        let did_not_deploy = option_progress(
            &root,
            &preserved,
            &ActionSummaryV1::default(),
            PublicCombatAgendaPriorityV1::ThreatResponse,
            option,
        );

        assert!(satisfied.satisfied);
        assert!(!woke_target.satisfied);
        assert!(!did_not_deploy.satisfied);
    }

    #[test]
    fn pending_upgrade_choice_preserves_a_commitment_feasibility_step() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Armaments, 1),
            CombatCard::for_scenario(CardId::DarkEmbrace, 2),
            CombatCard::for_scenario(CardId::Strike, 3),
        ];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let root_public = public_combat_state_v1(&root.combat);
        let plan = sts_agent::agent::task::public_combat_plan_v1(&root_public);
        let agenda = public_combat_agenda_for_plan_v1(&root_public, &plan);
        let pending = candidate(
            &root,
            0,
            vec![ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }],
        )
        .successor;
        assert!(
            matches!(pending.engine, EngineState::PendingChoice(_)),
            "Armaments must stop on its public upgrade choice, got {:?}",
            pending.engine
        );

        let rank = public_combat_agenda_partial_commitment_rank_v1(&agenda, &pending)
            .expect("the public upgrade choice can advance the payoff commitment");

        assert!(!rank.immediately_playable);
        assert!(rank.pending_choice_advances_feasibility);
        assert_eq!(
            rank.guide_ranks(),
            vec![(
                PUBLIC_COMBAT_AGENDA_FEASIBLE_COMMITMENT_GUIDE_LANE_V1,
                vec![-1]
            )]
        );
    }

    #[test]
    fn partial_progress_does_not_prune_a_distinct_continuation() {
        let progress = |achieved| PublicCombatAgendaGroundedCandidateV1 {
            plan_index: achieved as usize,
            action_count: 1,
            cards_played: 1,
            attacks_played: 1,
            terminal: PublicCombatAgendaTerminalV1::Unresolved,
            player_hp: 80,
            living_enemies: 1,
            total_enemy_hp: 100 - achieved,
            option_progress: vec![PublicCombatAgendaOptionProgressV1 {
                priority: PublicCombatAgendaPriorityV1::ThreatResponse,
                option: PublicCombatAgendaOptionV1::AdvanceLethalConversion {
                    current_total_enemy_hp: 100,
                },
                achieved,
                required: 100,
                satisfied: false,
            }],
            on_non_dominated_frontier: false,
        };

        assert!(!strictly_dominates(
            PublicCombatPlanStageV1::ConvertToLethal,
            &progress(30),
            &progress(20)
        ));
        assert!(!strictly_dominates(
            PublicCombatPlanStageV1::ConvertToLethal,
            &progress(20),
            &progress(30)
        ));
    }

    fn candidate(
        root: &CombatPosition,
        plan_index: usize,
        actions: Vec<ClientInput>,
    ) -> GroundableCompleteTurnV1 {
        let mut position = root.clone();
        for action in &actions {
            position = EngineCombatStepper
                .apply_to_stable(
                    &position,
                    action.clone(),
                    CombatStepLimits {
                        max_engine_steps: 250,
                        deadline: None,
                    },
                )
                .position;
        }
        GroundableCompleteTurnV1 {
            plan_index,
            actions,
            successor: position,
        }
    }
}
