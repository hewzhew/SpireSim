//! A tactical handoff reading of public facts. No search, action selection or
//! private draw-order access occurs while displaying a decision.

use std::fmt::Write;

use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::information::state::{
    CombatLearningCardV1, CombatLearningEnemyIdentityV1, CombatLearningIntentV1,
    CombatLearningPowerV1, PublicCombatStateV1,
};
use sts_oracle_learning::content::identity::gameplay_display_label_v1;
use sts_oracle_learning::content::powers::PowerId;
use sts_oracle_learning::runtime::combat::{CombatPhase, Intent};

use crate::render::{format_card, render_powers};

use super::{
    DecisionOptionV1, DecisionSelectionMemberV1, DecisionSelectionV1, RunPlayDecisionReadingV1,
};
use sts_oracle_learning::agent::information::action::{
    PublicCombatAtomicActionV1 as Atomic, PublicCombatIndexedChoiceCandidateV1 as Indexed,
    PublicCombatSelectionDomainCandidateV1 as Domain,
};
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1 as Candidate, TaskPublicChoiceV1, TaskPublicDecisionV1,
};

pub(crate) fn is_pending_choice(candidate: &Candidate) -> bool {
    matches!(
        candidate,
        Candidate::CombatIndexedChoice { .. }
            | Candidate::CombatSelectionFamily { .. }
            | Candidate::CombatAtomic {
                action: Atomic::Cancel
            }
    )
}

pub(super) fn pending_choices(
    decision: &TaskPublicDecisionV1,
    reading: &mut RunPlayDecisionReadingV1,
    brief: bool,
) -> Result<(), String> {
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        let label = match candidate {
            Candidate::CombatIndexedChoice { candidate, .. } => match candidate {
                Indexed::Card { card_id, upgrades } => format_card(*card_id, *upgrades),
                Indexed::Stance { stance } => gameplay_display_label_v1(*stance).to_owned(),
            },
            Candidate::CombatAtomic {
                action: Atomic::Cancel,
            } => "Cancel".to_owned(),
            Candidate::CombatSelectionFamily { family } => {
                let label =
                    sts_complete_turn_view::combat_selection_reason_label_v1(&family.reason);
                let members = selection_members(&family.domain);
                reading.selections.push(DecisionSelectionV1 {
                    label,
                    choice: TaskPublicChoiceV1::one(ordinal),
                    min: family.declared_min,
                    max: family.effective_max,
                    members,
                });
                continue;
            }
            _ => continue,
        };
        reading.options.push(DecisionOptionV1 {
            choice: TaskPublicChoiceV1::one(ordinal),
            label,
            candidate: candidate.clone(),
        });
    }
    let bound = reading.source.is_some();
    super::render_selections(&mut reading.view, decision, &reading.selections, bound)?;
    if !reading.options.is_empty() {
        let indexed = decision
            .candidates
            .iter()
            .find_map(|candidate| match candidate {
                Candidate::CombatIndexedChoice { reason, .. } => Some(reason),
                _ => None,
            });
        writeln!(
            reading.view,
            "{}",
            indexed.map_or_else(|| "Options".into(), indexed_prompt)
        )
        .unwrap();
        for option in &reading.options {
            let command = crate::selector::semantic_action_for_choice_v1(decision, &option.choice)?;
            writeln!(
                reading.view,
                "  {}{command}  |  {}",
                if bound { "./p " } else { "" },
                option.label
            )
            .unwrap();
        }
    }
    let mut cards = Vec::new();
    for candidate in &decision.candidates {
        match candidate {
            Candidate::CombatIndexedChoice {
                candidate: Indexed::Card { card_id, upgrades },
                ..
            } => {
                cards.push((*card_id, *upgrades));
            }
            Candidate::CombatSelectionFamily { family } => {
                for member in &family.domain {
                    if let Domain::Card {
                        card_id: Some(card_id),
                        upgrades,
                        eligible: true,
                        ..
                    } = member
                    {
                        cards.push((*card_id, upgrades.unwrap_or(0)));
                    }
                }
            }
            _ => {}
        }
    }
    if !cards.is_empty() {
        writeln!(
            reading.view,
            "{}",
            if brief {
                "Choice cards:"
            } else {
                "Choice card rules (base/upgraded):"
            }
        )
        .unwrap();
        let mut seen = std::collections::HashSet::new();
        for (card, upgrades) in cards {
            if seen.insert((card, upgrades)) {
                if brief {
                    writeln!(reading.view, "  {}", format_card(card, upgrades)).unwrap();
                    crate::mechanics_info::write_card_offer(&mut reading.view, card, upgrades);
                } else {
                    crate::mechanics_info::write_card_rule(&mut reading.view, card, upgrades);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn selection_members(domain: &[Domain]) -> Vec<DecisionSelectionMemberV1> {
    domain
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| {
            let (card_id, upgrades, available) = match candidate {
                Domain::Card {
                    card_id,
                    upgrades,
                    eligible,
                    ..
                } => (*card_id, upgrades.unwrap_or(0), *eligible),
                Domain::Scry {
                    card_id,
                    currently_present,
                    ..
                } => (*card_id, 0, *currently_present),
            };
            available.then(|| DecisionSelectionMemberV1 {
                ordinal,
                label: card_id
                    .map_or_else(|| "Unknown card".into(), |id| format_card(id, upgrades)),
            })
        })
        .collect::<Vec<_>>()
}

fn indexed_prompt(
    reason: &sts_oracle_learning::agent::information::action::PublicCombatIndexedChoiceReasonV1,
) -> String {
    use sts_oracle_learning::agent::information::action::PublicCombatIndexedChoiceReasonV1 as Reason;
    match reason {
        Reason::Discovery { amount, .. } => {
            if *amount == 1 {
                "Discovery | Choose 1".into()
            } else {
                format!("Discovery | Choose 1, receive {amount} copies")
            }
        }
        Reason::CardReward { .. } => "Card reward | Choose 1".into(),
        Reason::ForeignInfluence { .. } => "Foreign Influence | Choose 1".into(),
        Reason::ChooseOne => "Choose 1".into(),
        Reason::Stance => "Choose a stance".into(),
    }
}

pub(crate) fn hand_card_label(
    card: sts_oracle_learning::content::cards::CardId,
    upgrades: u8,
    effective_cost: i32,
) -> String {
    let cost = match effective_cost {
        -1 => "Xc".to_owned(),
        -2 => "unplayable".to_owned(),
        cost => format!("{cost}c"),
    };
    format!("{}[{cost}]", format_card(card, upgrades))
}

fn current_hand_card_label(card: &CombatLearningCardV1, state: &PublicCombatStateV1) -> String {
    use sts_oracle_learning::content::cards::mechanics::{
        card_base_damage_delivery_v1, CardAttackHitsV1 as Hits, CardAttackTargetsV1 as Targets,
        CardBaseDamageDeliveryV1 as Delivery,
    };
    let mut label = hand_card_label(card.card_id, card.upgrades, card.effective_cost);
    label.pop();
    if let Some(Delivery::Attack { hits, targets, .. }) =
        card_base_damage_delivery_v1(card.card_id, card.upgrades)
    {
        // The public projection already applies Strength, Weak and each target's
        // modifiers. These are per-hit requests before block, not total HP loss.
        let suffix = match hits {
            Hits::Fixed { count: 1 } => "d".to_owned(),
            Hits::Fixed { count } => format!("dx{count}"),
            _ => "d/hit".to_owned(),
        };
        let damage = state
            .monsters
            .iter()
            .zip(&card.damage_by_monster_order)
            .filter(|(m, _)| m.alive && !m.dying && !m.escaped && !m.half_dead)
            .map(|(m, damage)| (m.slot, *damage))
            .collect::<Vec<_>>();
        if let Some((_, first)) = damage.first() {
            if damage.iter().all(|(_, d)| d == first) {
                write!(label, " {first}{suffix}").unwrap();
            } else {
                for (slot, damage) in damage {
                    write!(label, " M{slot}:{damage}{suffix}").unwrap();
                }
            }
        } else if card.damage_by_monster_order.is_empty() {
            write!(label, " {}{suffix}", card.current_damage).unwrap();
        }
        match targets {
            Targets::EachEnemy => label.push_str(" all"),
            Targets::RandomEnemyPerHit => label.push_str(" random"),
            Targets::SelectedEnemy => {}
        }
    }
    if card.current_block > 0 {
        write!(label, " {}b", card.current_block).unwrap();
    }
    label.push(']');
    label
}

pub(crate) fn reading(
    context: &PublicCombatRunContextV1,
    state: &PublicCombatStateV1,
    brief: bool,
) -> String {
    let mut out = String::new();
    write_run_context(&mut out, context);
    out.push_str(&state_reading_with_rules(state, !brief));
    if let PublicCombatRunContextV1::Available { run_goal, keys, .. } = context {
        if !brief {
            crate::mechanics_info::write_key_status(
                &mut out,
                *keys,
                *run_goal == sts_oracle_learning::ai::planner_core::PlannerRunGoal::HeartVictory,
            );
        }
    }
    out
}

pub(crate) fn summary(context: &PublicCombatRunContextV1, state: &PublicCombatStateV1) -> String {
    let mut out = String::new();
    write_run_context(&mut out, context);
    out.push_str(&state_summary(state));
    out
}

fn write_run_context(out: &mut String, context: &PublicCombatRunContextV1) {
    if let PublicCombatRunContextV1::Available {
        act,
        floor,
        encounter_id,
        ..
    } = context
    {
        write!(out, "Act {act} F{floor} | ").unwrap();
        if let Some(encounter) = encounter_id {
            write!(out, "{} | ", gameplay_display_label_v1(*encounter)).unwrap();
        }
    }
}

pub(crate) fn state_reading(state: &PublicCombatStateV1) -> String {
    state_reading_with_rules(state, true)
}

fn state_summary(state: &PublicCombatStateV1) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "A{} | HP {}/{} | Gold {} | Combat",
        state.player.ascension_level, state.player.hp, state.player.max_hp, state.player.gold
    )
    .unwrap();
    if state.turn.player_escaping {
        writeln!(
            out,
            "Escape settlement pending | use act end to finish the escape. Card plays, other potion actions and combat search are unavailable."
        )
        .unwrap();
        return out;
    }
    let phase = match state.turn.phase {
        CombatPhase::PlayerTurn => "Player turn",
        CombatPhase::MonsterTurn => "Enemy turn",
        CombatPhase::TurnTransition => "Turn transition",
    };
    writeln!(
        out,
        "Turn {} | {phase} | Energy {} | Block {} | Played {} (attacks {})",
        state.turn.turn_count.saturating_add(1),
        state.turn.energy,
        state.player.block,
        state.turn.counters.cards_played_this_turn,
        state.turn.counters.attacks_played_this_turn
    )
    .unwrap();
    writeln!(
        out,
        "Hand: {}",
        state
            .cards
            .hand
            .cards
            .iter()
            .map(|card| current_hand_card_label(card, state))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    out
}

fn state_reading_with_rules(state: &PublicCombatStateV1, include_rules: bool) -> String {
    let mut out = state_summary(state);
    if state.turn.player_escaping {
        return out;
    }
    if include_rules {
        writeln!(
            out,
            "Damage: per-target hit after Strength/Weak/Vulnerable, before block."
        )
        .unwrap();
    }
    writeln!(
        out,
        "Draw {} | Discard {} | Exhaust {}",
        state.cards.draw.cards.len(),
        state.cards.discard.cards.len(),
        state.cards.exhaust.cards.len()
    )
    .unwrap();
    writeln!(
        out,
        "Potions: {}",
        state
            .potions
            .iter()
            .enumerate()
            .map(|(slot, potion)| {
                format!(
                    "P{slot} {}",
                    potion
                        .as_ref()
                        .map_or("empty", |p| gameplay_display_label_v1(p.potion_id))
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "Relics: {}",
        state
            .player
            .relics
            .iter()
            .map(|relic| crate::compact::relic_label_v1(
                relic.id,
                relic.counter,
                relic.used_up,
                sts_presentation::Locale::En
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    if !state.player.powers.is_empty() {
        writeln!(out, "Powers: {}", power_labels(&state.player.powers)).unwrap();
    }
    if state
        .player
        .powers
        .iter()
        .any(|power| power.power == PowerId::Surrounded)
    {
        writeln!(
            out,
            "Surrounded: shown attack damage includes current facing. Targeted cards turn you; recheck enemy intents before EndTurn."
        )
        .unwrap();
    }
    let mut inactive = 0;
    for monster in &state.monsters {
        if !monster.alive && !monster.half_dead {
            inactive += 1;
            continue;
        }
        let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        write!(
            out,
            "M{} {}: HP {}/{}",
            monster.slot,
            gameplay_display_label_v1(enemy_id),
            monster.hp,
            monster.max_hp
        )
        .unwrap();
        if monster.block != 0 {
            write!(out, ", Block {}", monster.block).unwrap();
        }
        writeln!(
            out,
            " | {}",
            if monster.half_dead {
                "Reviving".to_owned()
            } else {
                intent_label(&monster.intent)
            }
        )
        .unwrap();
        if !monster.powers.is_empty() {
            writeln!(out, "  Powers: {}", power_labels(&monster.powers)).unwrap();
        }
    }
    if inactive > 0 {
        writeln!(out, "Inactive enemies: {inactive}").unwrap();
    }
    if include_rules {
        crate::mechanics_info::write_combat_rules(&mut out, state);
    }
    out
}

pub(crate) fn power_labels(powers: &[CombatLearningPowerV1]) -> String {
    powers
        .iter()
        .map(|power| {
            if sts_oracle_learning::content::powers::uses_sentinel_amount(power.power) {
                let mut label = gameplay_display_label_v1(power.power).to_owned();
                if power.extra_data != 0 {
                    write!(label, "/{}", power.extra_data).unwrap();
                }
                if let Some(card) = &power.payload_card {
                    write!(label, "<{}>", format_card(card.card_id, card.upgrades)).unwrap();
                }
                label
            } else {
                render_powers(std::slice::from_ref(power))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn intent_label(public: &CombatLearningIntentV1) -> String {
    let Some(intent) = &public.intent else {
        return if public.hidden_reason.is_some() {
            "Intent hidden"
        } else {
            "Intent unknown"
        }
        .to_owned();
    };
    let label = match intent {
        Intent::Attack { .. } => "Attack",
        Intent::AttackBuff { .. } => "Attack + Buff",
        Intent::AttackDebuff { .. } => "Attack + Debuff",
        Intent::AttackDefend { .. } => "Attack + Defend",
        Intent::Buff => "Buff",
        Intent::Heal { .. } => "Heal",
        Intent::Debuff => "Debuff",
        Intent::StrongDebuff => "Strong Debuff",
        Intent::Debug => "Debug",
        Intent::Defend => "Defend",
        Intent::DefendDebuff => "Defend + Debuff",
        Intent::DefendBuff => "Defend + Buff",
        Intent::Escape => "Escape",
        Intent::Magic => "Magic",
        Intent::None => "None",
        Intent::Sleep => "Sleep",
        Intent::Stun => "Stun",
        Intent::Unknown => "Unknown",
    };
    match intent {
        Intent::Attack { hits, .. }
        | Intent::AttackBuff { hits, .. }
        | Intent::AttackDebuff { hits, .. }
        | Intent::AttackDefend { hits, .. } => {
            let damage = public
                .preview_damage_per_hit
                .map_or_else(|| "?".into(), |damage| damage.to_string());
            if *hits == 1 {
                format!("{label} {damage}")
            } else {
                format!("{label} {damage}/hit x{hits}")
            }
        }
        Intent::Heal {
            amount: Some(amount),
        } => format!("{label} {amount}"),
        _ => label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::agent::information::combat::{
        HiddenInformationReasonV1, ObservationEvidenceKindV1,
    };

    #[test]
    fn intent_reading_uses_visible_damage_and_keeps_missing_information_explicit() {
        let mut public = CombatLearningIntentV1 {
            evidence: ObservationEvidenceKindV1::VisibleExact,
            intent: Some(Intent::AttackBuff {
                damage: 900,
                hits: 2,
            }),
            preview_damage_per_hit: Some(7),
            hidden_reason: None,
        };
        assert_eq!(intent_label(&public), "Attack + Buff 7/hit x2");
        public.preview_damage_per_hit = None;
        assert_eq!(intent_label(&public), "Attack + Buff ?/hit x2");
        public.intent = Some(Intent::Attack {
            damage: 2,
            hits: 15,
        });
        public.preview_damage_per_hit = Some(0);
        assert_eq!(intent_label(&public), "Attack 0/hit x15");
        public.intent = None;
        assert_eq!(intent_label(&public), "Intent unknown");
        public.evidence = ObservationEvidenceKindV1::Hidden;
        public.hidden_reason = Some(HiddenInformationReasonV1::RunicDome);
        assert_eq!(intent_label(&public), "Intent hidden");
    }

    #[test]
    fn presence_powers_omit_sentinel_but_negative_strength_remains_visible() {
        use sts_oracle_learning::content::powers::PowerId;
        let presence = CombatLearningPowerV1 {
            power: PowerId::Corruption,
            amount: -1,
            extra_data: 0,
            just_applied: false,
            payload_card: None,
            source_creature: None,
        };
        let negative = CombatLearningPowerV1 {
            power: PowerId::Strength,
            ..presence.clone()
        };
        assert_eq!(
            power_labels(&[presence, negative]),
            "Corruption, Strength (-1)"
        );
    }

    #[test]
    fn power_labels_distinguish_duration_from_strength_and_per_hit_caps() {
        let power = |power, amount| CombatLearningPowerV1 {
            power,
            amount,
            extra_data: 0,
            just_applied: false,
            payload_card: None,
            source_creature: None,
        };
        assert_eq!(power_labels(&[
            power(PowerId::Vulnerable, 3), power(PowerId::Weak, 1),
            power(PowerId::Frail, 2), power(PowerId::Strength, 3),
            power(PowerId::Intangible, 1), power(PowerId::IntangiblePlayer, 1),
        ]), "Vulnerable (3 rounds), Weak (1 round), Frail (2 rounds), Strength (3), Intangible (1), Intangible (1 round)");
    }
}
