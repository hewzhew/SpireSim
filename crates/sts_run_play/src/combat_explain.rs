use std::fmt::Write as _;

use sts_oracle_learning::agent::information::action::PublicCombatAtomicActionV1;
use sts_oracle_learning::agent::information::state::{
    CombatLearningCardV1, CombatLearningPowerV1, PublicCombatStateV1,
};
use sts_oracle_learning::agent::task::{TaskPublicCandidateV1, TaskPublicObservationV1};
use sts_oracle_learning::content::cards::mechanics::{
    card_is_ethereal, card_mechanic_profile, end_turn_hand_damage_v1,
    unconditional_card_draw_request_v1, CardMechanicRole, EndTurnHandDamageV1,
};
use sts_oracle_learning::content::cards::{get_card_definition, CardId, CardType};
use sts_oracle_learning::content::identity::gameplay_display_label_v1;
use sts_oracle_learning::content::powers::PowerId;
use sts_oracle_learning::eval::run_control::{ExactRunPlayProbeStepV1, RunActionResultChangeV1};

use crate::render::format_card;

#[derive(Clone, Copy)]
struct PendingDrawRequestV1 {
    source_card: CardId,
    requested: usize,
}

#[derive(Default)]
pub(crate) struct CombatLineExplanationStateV1 {
    pending_draw_request: Option<PendingDrawRequestV1>,
}

impl CombatLineExplanationStateV1 {
    pub(crate) fn render_step_v1(&mut self, output: &mut String, step: &ExactRunPlayProbeStepV1) {
        self.pending_draw_request =
            render_combat_step_evidence_v1(output, step, self.pending_draw_request);
    }
}

fn render_combat_step_evidence_v1(
    output: &mut String,
    step: &ExactRunPlayProbeStepV1,
    pending_draw_request: Option<PendingDrawRequestV1>,
) -> Option<PendingDrawRequestV1> {
    let TaskPublicObservationV1::Combat {
        observation: before,
        ..
    } = &step.source_decision.observation
    else {
        return None;
    };
    let after = step.next_decision.as_ref().and_then(|decision| {
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            return None;
        };
        Some(observation)
    });
    let after = after?;

    writeln!(
        output,
        "    observed turn={}->{} hp={}->{} ({:+}) block={}->{} ({:+}) energy={}->{} ({:+})",
        before.turn.turn_count,
        after.turn.turn_count,
        before.player.hp,
        after.player.hp,
        after.player.hp - before.player.hp,
        before.player.block,
        after.player.block,
        after.player.block - before.player.block,
        before.turn.energy,
        after.turn.energy,
        i32::from(after.turn.energy) - i32::from(before.turn.energy),
    )
    .expect("string write");

    let drawn = step
        .action_result
        .iter()
        .flat_map(|result| &result.changes)
        .filter_map(|change| match change {
            RunActionResultChangeV1::CombatCardDrawn { card } => Some((card.id, card.upgrades)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let added_to_exhaust =
        added_combat_cards_v1(&before.cards.exhaust.cards, &after.cards.exhaust.cards);
    let played = played_card_v1(step, before);
    let direct_draw_request = played.and_then(fixed_card_draw_request_v1);
    let applied_draw_request = match &step.selected_candidate {
        TaskPublicCandidateV1::CombatSelectionFamily { .. } => pending_draw_request,
        _ => direct_draw_request,
    };
    if !drawn.is_empty() || applied_draw_request.is_some() {
        let request = applied_draw_request
            .map(|request| {
                format!(
                    "{} by {}",
                    request.requested,
                    gameplay_display_label_v1(request.source_card)
                )
            })
            .unwrap_or_else(|| "not_from_selected_action".to_string());
        let shortfall = applied_draw_request
            .map(|request| request.requested.saturating_sub(drawn.len()))
            .unwrap_or(0);
        writeln!(
            output,
            "    observed draw_request={} actual={} [{}] hand={}->{} draw_pile={}->{}{}",
            request,
            drawn.len(),
            drawn
                .iter()
                .map(|(card, upgrades)| format_card(*card, *upgrades))
                .collect::<Vec<_>>()
                .join(", "),
            before.cards.hand.cards.len(),
            after.cards.hand.cards.len(),
            before.cards.draw.cards.len(),
            after.cards.draw.cards.len(),
            if shortfall > 0 && after.cards.hand.cards.len() == 10 {
                format!(" hand_limit_shortfall={shortfall}")
            } else {
                String::new()
            },
        )
        .expect("string write");
    }
    if !added_to_exhaust.is_empty() {
        writeln!(
            output,
            "    observed exhaust_added=[{}] exhaust_pile={}->{}",
            added_to_exhaust
                .iter()
                .map(|card| format_card(card.card_id, card.upgrades))
                .collect::<Vec<_>>()
                .join(", "),
            before.cards.exhaust.cards.len(),
            after.cards.exhaust.cards.len(),
        )
        .expect("string write");
    }
    render_orientation_change_v1(output, step, before, after);
    let is_end_turn = matches!(
        step.selected_candidate,
        TaskPublicCandidateV1::CombatAtomic {
            action: PublicCombatAtomicActionV1::EndTurn
        }
    );
    if !is_end_turn && !added_to_exhaust.is_empty() {
        let feel_no_pain = power_amount_v1(&before.player.powers, PowerId::FeelNoPain).max(0)
            * added_to_exhaust.len() as i32;
        if feel_no_pain > 0 {
            writeln!(
                output,
                "    explained exhaust_count={} FeelNoPain=+{} block",
                added_to_exhaust.len(),
                feel_no_pain,
            )
            .expect("string write");
        }
    }
    if is_end_turn {
        render_end_turn_explanation_v1(output, before, after, &added_to_exhaust, &drawn);
    }

    match direct_draw_request {
        Some(request)
            if drawn.is_empty()
                && card_mechanic_profile(request.source_card, played?.upgrades)
                    .roles
                    .contains(CardMechanicRole::SelectFromHand) =>
        {
            Some(request)
        }
        _ => None,
    }
}

fn played_card_v1<'a>(
    step: &ExactRunPlayProbeStepV1,
    before: &'a PublicCombatStateV1,
) -> Option<&'a CombatLearningCardV1> {
    let action = match &step.selected_candidate {
        TaskPublicCandidateV1::CombatAtomic { action }
        | TaskPublicCandidateV1::CombatIndexedChoice { action, .. } => action,
        _ => return None,
    };
    let PublicCombatAtomicActionV1::PlayCard { hand_index, .. } = action else {
        return None;
    };
    before.cards.hand.cards.get(*hand_index)
}

fn fixed_card_draw_request_v1(card: &CombatLearningCardV1) -> Option<PendingDrawRequestV1> {
    let requested =
        unconditional_card_draw_request_v1(card.card_id, card.current_magic_number)? as usize;
    Some(PendingDrawRequestV1 {
        source_card: card.card_id,
        requested,
    })
}

fn added_combat_cards_v1(
    before: &[CombatLearningCardV1],
    after: &[CombatLearningCardV1],
) -> Vec<CombatLearningCardV1> {
    let mut remaining = before
        .iter()
        .map(|card| ((card.card_id, card.upgrades), 1usize))
        .collect::<Vec<_>>();
    let mut added = Vec::new();
    for card in after {
        if let Some((_, count)) = remaining
            .iter_mut()
            .find(|(identity, count)| *count > 0 && *identity == (card.card_id, card.upgrades))
        {
            *count -= 1;
        } else {
            added.push(card.clone());
        }
    }
    added
}

fn render_orientation_change_v1(
    output: &mut String,
    step: &ExactRunPlayProbeStepV1,
    before: &PublicCombatStateV1,
    after: &PublicCombatStateV1,
) {
    if before.player.facing_left == after.player.facing_left {
        return;
    }
    let Some(card) = played_card_v1(step, before) else {
        return;
    };
    if get_card_definition(card.card_id).card_type != CardType::Attack {
        return;
    }
    let target_slot = match &step.selected_candidate {
        TaskPublicCandidateV1::CombatAtomic {
            action:
                PublicCombatAtomicActionV1::PlayCard {
                    target_monster_index: Some(index),
                    ..
                },
        }
        | TaskPublicCandidateV1::CombatIndexedChoice {
            action:
                PublicCombatAtomicActionV1::PlayCard {
                    target_monster_index: Some(index),
                    ..
                },
            ..
        } => before.monsters.get(*index).map(|monster| monster.slot),
        _ => None,
    };
    writeln!(
        output,
        "    explained orientation target_slot={} facing_left={}->{} back_attack_slots=[{}]->[{}]",
        target_slot
            .map(|slot| slot.to_string())
            .unwrap_or_else(|| "?".to_string()),
        before.player.facing_left,
        after.player.facing_left,
        back_attack_slots_v1(before),
        back_attack_slots_v1(after),
    )
    .expect("string write");
}

fn back_attack_slots_v1(state: &PublicCombatStateV1) -> String {
    state
        .monsters
        .iter()
        .filter(|monster| power_amount_v1(&monster.powers, PowerId::BackAttack) != 0)
        .map(|monster| monster.slot.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn render_end_turn_explanation_v1(
    output: &mut String,
    before: &PublicCombatStateV1,
    after: &PublicCombatStateV1,
    added_to_exhaust: &[CombatLearningCardV1],
    drawn: &[(CardId, u8)],
) {
    let ethereal = added_to_exhaust
        .iter()
        .filter(|card| card_is_ethereal(card.card_id, card.upgrades))
        .collect::<Vec<_>>();
    let feel_no_pain = power_amount_v1(&before.player.powers, PowerId::FeelNoPain).max(0)
        * added_to_exhaust.len() as i32;
    let metallicize = power_amount_v1(&before.player.powers, PowerId::Metallicize).max(0);
    let plated_armor = power_amount_v1(&before.player.powers, PowerId::PlatedArmor).max(0);
    let known_end_block = feel_no_pain + metallicize + plated_armor;
    let hand_trigger_damage = before
        .cards
        .hand
        .cards
        .iter()
        .map(|card| {
            end_turn_hand_damage_v1(card.card_id, card.upgrades, before.cards.hand.cards.len())
        })
        .fold(EndTurnHandDamageV1::default(), |mut total, damage| {
            total.blockable += damage.blockable;
            total.hp_loss += damage.hp_loss;
            total
        });
    let incoming = visible_incoming_damage_v1(before);
    let observed_hp_loss = (before.player.hp - after.player.hp).max(0);
    let mut facts = Vec::new();
    if !ethereal.is_empty() {
        facts.push(format!(
            "ethereal=[{}]",
            ethereal
                .iter()
                .map(|card| format_card(card.card_id, card.upgrades))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if feel_no_pain > 0 {
        facts.push(format!("FeelNoPain=+{feel_no_pain} block"));
    }
    if metallicize > 0 {
        facts.push(format!("Metallicize=+{metallicize} block"));
    }
    if plated_armor > 0 {
        facts.push(format!("PlatedArmor=+{plated_armor} block"));
    }
    if hand_trigger_damage.blockable > 0 {
        facts.push(format!(
            "end_turn_hand_damage={} blockable",
            hand_trigger_damage.blockable
        ));
    }
    if hand_trigger_damage.hp_loss > 0 {
        facts.push(format!(
            "end_turn_hand_hp_loss={}",
            hand_trigger_damage.hp_loss
        ));
    }
    let base_draw_request = (5 + before.turn.turn_start_draw_modifier).max(0) as usize;
    let evolve_amount = power_amount_v1(&before.player.powers, PowerId::Evolve).max(0) as usize;
    let drawn_statuses = drawn
        .iter()
        .filter(|(card, _)| get_card_definition(*card).card_type == CardType::Status)
        .count();
    let evolve_draw_request = evolve_amount.saturating_mul(drawn_statuses);
    let known_draw_request = base_draw_request.saturating_add(evolve_draw_request);
    let draw_residual = drawn.len() as i64 - known_draw_request as i64;
    facts.push(format!("turn_draw={base_draw_request}"));
    if evolve_draw_request > 0 {
        facts.push(format!(
            "Evolve={}x{} statuses=+{} draw",
            evolve_amount, drawn_statuses, evolve_draw_request
        ));
    }
    facts.push(format!("actual_draw={}", drawn.len()));
    facts.push(format!("draw_residual={draw_residual:+}"));
    if known_draw_request > drawn.len() && after.cards.hand.cards.len() == 10 {
        facts.push(format!(
            "hand_limit_shortfall={}",
            known_draw_request - drawn.len()
        ));
    }
    if let Some(incoming) = incoming {
        let defense_before_attack = before.player.block + known_end_block;
        let expected_hp_loss = hand_trigger_damage.hp_loss
            + (hand_trigger_damage.blockable + incoming - defense_before_attack).max(0);
        facts.push(format!("visible_enemy_attack={incoming}"));
        facts.push(format!("defense_before_attack={defense_before_attack}"));
        facts.push(format!("expected_hp_loss={expected_hp_loss}"));
        facts.push(format!("observed_hp_loss={observed_hp_loss}"));
        facts.push(format!(
            "residual={:+}",
            observed_hp_loss - expected_hp_loss
        ));
    }
    if !facts.is_empty() {
        writeln!(output, "    explained end_turn {}", facts.join("; ")).expect("string write");
    }
}

pub(crate) fn visible_incoming_damage_v1(state: &PublicCombatStateV1) -> Option<i32> {
    let mut total = 0i32;
    for monster in state.monsters.iter().filter(|monster| monster.alive) {
        match monster.intent.intent.as_ref()? {
            sts_oracle_learning::runtime::combat::Intent::Attack { hits, .. }
            | sts_oracle_learning::runtime::combat::Intent::AttackBuff { hits, .. }
            | sts_oracle_learning::runtime::combat::Intent::AttackDebuff { hits, .. }
            | sts_oracle_learning::runtime::combat::Intent::AttackDefend { hits, .. } => {
                total += monster.intent.preview_damage_per_hit? * i32::from(*hits);
            }
            _ => {}
        }
    }
    Some(total)
}

fn power_amount_v1(powers: &[CombatLearningPowerV1], power: PowerId) -> i32 {
    powers
        .iter()
        .filter(|candidate| candidate.power == power)
        .map(|candidate| candidate.amount)
        .sum()
}
