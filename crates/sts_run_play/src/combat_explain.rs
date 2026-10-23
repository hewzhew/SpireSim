use std::fmt::Write as _;
use sts_presentation::Locale;

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
use sts_oracle_learning::content::powers::PowerId;
use sts_oracle_learning::eval::run_control::{ExactRunPlayProbeStepV1, RunActionResultChangeV1};

fn text(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .definition_text_with("combat", &format!("explain.{key}"), parameters)
        .expect("combat explanation message")
}

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
    pub(crate) fn render_step_v1(
        &mut self,
        output: &mut String,
        step: &ExactRunPlayProbeStepV1,
        locale: Locale,
    ) {
        self.pending_draw_request =
            render_combat_step_evidence_v1(output, step, self.pending_draw_request, locale);
    }
}

fn render_combat_step_evidence_v1(
    output: &mut String,
    step: &ExactRunPlayProbeStepV1,
    pending_draw_request: Option<PendingDrawRequestV1>,
    locale: Locale,
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
        "{}",
        text(
            locale,
            "observed",
            &[
                ("beforeTurn", &before.turn.turn_count.to_string()),
                ("afterTurn", &after.turn.turn_count.to_string()),
                ("beforeHp", &before.player.hp.to_string()),
                ("afterHp", &after.player.hp.to_string()),
                (
                    "hpDelta",
                    &format!("{:+}", after.player.hp - before.player.hp)
                ),
                ("beforeBlock", &before.player.block.to_string()),
                ("afterBlock", &after.player.block.to_string()),
                (
                    "blockDelta",
                    &format!("{:+}", after.player.block - before.player.block)
                ),
                ("beforeEnergy", &before.turn.energy.to_string()),
                ("afterEnergy", &after.turn.energy.to_string()),
                (
                    "energyDelta",
                    &format!(
                        "{:+}",
                        i32::from(after.turn.energy) - i32::from(before.turn.energy)
                    )
                )
            ]
        ),
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
                text(
                    locale,
                    "requestBy",
                    &[
                        ("count", &request.requested.to_string()),
                        (
                            "card",
                            locale.entity("card", request.source_card.stable_id()),
                        ),
                    ],
                )
            })
            .unwrap_or_else(|| text(locale, "notFromAction", &[]));
        let shortfall = applied_draw_request
            .map(|request| request.requested.saturating_sub(drawn.len()))
            .unwrap_or(0);
        writeln!(
            output,
            "{}",
            text(
                locale,
                "draw",
                &[
                    ("request", &request),
                    ("actual", &drawn.len().to_string()),
                    (
                        "cards",
                        &drawn
                            .iter()
                            .map(|(card, upgrades)| locale.card_name(card.stable_id(), *upgrades))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    ("beforeHand", &before.cards.hand.cards.len().to_string()),
                    ("afterHand", &after.cards.hand.cards.len().to_string()),
                    ("beforeDraw", &before.cards.draw.cards.len().to_string()),
                    ("afterDraw", &after.cards.draw.cards.len().to_string()),
                    (
                        "shortfall",
                        &if shortfall > 0 && after.cards.hand.cards.len() == 10 {
                            text(
                                locale,
                                "handLimitSuffix",
                                &[("count", &shortfall.to_string())],
                            )
                        } else {
                            String::new()
                        }
                    )
                ]
            ),
        )
        .expect("string write");
    }
    if !added_to_exhaust.is_empty() {
        writeln!(
            output,
            "{}",
            text(
                locale,
                "exhaust",
                &[
                    (
                        "cards",
                        &added_to_exhaust
                            .iter()
                            .map(|card| locale.card_name(card.card_id.stable_id(), card.upgrades))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    ("before", &before.cards.exhaust.cards.len().to_string()),
                    ("after", &after.cards.exhaust.cards.len().to_string())
                ]
            ),
        )
        .expect("string write");
    }
    render_orientation_change_v1(output, step, before, after, locale);
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
                "{}",
                text(
                    locale,
                    "exhaustBlock",
                    &[
                        ("count", &added_to_exhaust.len().to_string()),
                        ("block", &feel_no_pain.to_string())
                    ]
                ),
            )
            .expect("string write");
        }
    }
    if is_end_turn {
        render_end_turn_explanation_v1(output, before, after, &added_to_exhaust, &drawn, locale);
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
    locale: Locale,
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
        "{}",
        text(
            locale,
            "orientation",
            &[
                (
                    "slot",
                    &target_slot
                        .map(|slot| slot.to_string())
                        .unwrap_or_else(|| "?".to_string())
                ),
                ("beforeFacing", &before.player.facing_left.to_string()),
                ("afterFacing", &after.player.facing_left.to_string()),
                ("beforeBack", &back_attack_slots_v1(before)),
                ("afterBack", &back_attack_slots_v1(after))
            ]
        ),
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
    locale: Locale,
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
        facts.push(text(
            locale,
            "ethereal",
            &[(
                "cards",
                &ethereal
                    .iter()
                    .map(|card| locale.card_name(card.card_id.stable_id(), card.upgrades))
                    .collect::<Vec<_>>()
                    .join(", "),
            )],
        ));
    }
    if feel_no_pain > 0 {
        facts.push(text(
            locale,
            "blockPower",
            &[
                (
                    "power",
                    locale.entity("power", PowerId::FeelNoPain.stable_id()),
                ),
                ("amount", &feel_no_pain.to_string()),
            ],
        ));
    }
    if metallicize > 0 {
        facts.push(text(
            locale,
            "blockPower",
            &[
                (
                    "power",
                    locale.entity("power", PowerId::Metallicize.stable_id()),
                ),
                ("amount", &metallicize.to_string()),
            ],
        ));
    }
    if plated_armor > 0 {
        facts.push(text(
            locale,
            "blockPower",
            &[
                (
                    "power",
                    locale.entity("power", PowerId::PlatedArmor.stable_id()),
                ),
                ("amount", &plated_armor.to_string()),
            ],
        ));
    }
    if hand_trigger_damage.blockable > 0 {
        facts.push(text(
            locale,
            "handDamage",
            &[("amount", &hand_trigger_damage.blockable.to_string())],
        ));
    }
    if hand_trigger_damage.hp_loss > 0 {
        facts.push(text(
            locale,
            "handHpLoss",
            &[("amount", &hand_trigger_damage.hp_loss.to_string())],
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
    facts.push(text(
        locale,
        "turnDraw",
        &[("amount", &base_draw_request.to_string())],
    ));
    if evolve_draw_request > 0 {
        facts.push(text(
            locale,
            "evolve",
            &[
                ("amount", &evolve_amount.to_string()),
                ("statuses", &drawn_statuses.to_string()),
                ("draw", &evolve_draw_request.to_string()),
            ],
        ));
    }
    facts.push(text(
        locale,
        "actualDraw",
        &[("amount", &drawn.len().to_string())],
    ));
    facts.push(text(
        locale,
        "drawResidual",
        &[("amount", &format!("{draw_residual:+}"))],
    ));
    if known_draw_request > drawn.len() && after.cards.hand.cards.len() == 10 {
        facts.push(text(
            locale,
            "handLimit",
            &[("count", &(known_draw_request - drawn.len()).to_string())],
        ));
    }
    if let Some(incoming) = incoming {
        let defense_before_attack = before.player.block + known_end_block;
        let expected_hp_loss = hand_trigger_damage.hp_loss
            + (hand_trigger_damage.blockable + incoming - defense_before_attack).max(0);
        facts.push(text(
            locale,
            "visibleAttack",
            &[("amount", &incoming.to_string())],
        ));
        facts.push(text(
            locale,
            "defense",
            &[("amount", &defense_before_attack.to_string())],
        ));
        facts.push(text(
            locale,
            "expectedLoss",
            &[("amount", &expected_hp_loss.to_string())],
        ));
        facts.push(text(
            locale,
            "observedLoss",
            &[("amount", &observed_hp_loss.to_string())],
        ));
        facts.push(text(
            locale,
            "residual",
            &[(
                "amount",
                &format!("{:+}", observed_hp_loss - expected_hp_loss),
            )],
        ));
    }
    if !facts.is_empty() {
        writeln!(
            output,
            "{}",
            text(locale, "endTurn", &[("facts", &facts.join("; "))])
        )
        .expect("string write");
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
