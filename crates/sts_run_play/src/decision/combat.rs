//! A tactical handoff reading of public facts. No search, action selection or
//! private draw-order access occurs while displaying a decision.

use std::fmt::Write;
use sts_presentation::Locale;

use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::information::state::{
    CombatLearningCardV1, CombatLearningEnemyIdentityV1, CombatLearningIntentV1,
    CombatLearningPowerV1, PublicCombatStateV1,
};
use sts_oracle_learning::content::identity::gameplay_display_label_v1;
use sts_oracle_learning::content::powers::PowerId;
use sts_oracle_learning::runtime::combat::{CombatPhase, Intent};

use crate::render::render_powers;

fn text(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("combat", key, parameters)
        .expect("combat reading message")
}

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
    locale: Locale,
) -> Result<(), String> {
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        let label = match candidate {
            Candidate::CombatIndexedChoice { candidate, .. } => match candidate {
                Indexed::Card { card_id, upgrades } => {
                    locale.card_name(card_id.stable_id(), *upgrades)
                }
                Indexed::Stance { stance } => {
                    locale.entity("stance", stance.stable_id()).to_owned()
                }
            },
            Candidate::CombatAtomic {
                action: Atomic::Cancel,
            } => text(locale, "cancel", &[]),
            Candidate::CombatSelectionFamily { family } => {
                let label = sts_complete_turn_view::combat_selection_reason_label_v1(
                    &family.reason,
                    locale,
                );
                let members = selection_members(&family.domain, locale);
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
    super::render_selections(
        &mut reading.view,
        decision,
        &reading.selections,
        bound,
        locale,
    )?;
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
            indexed.map_or_else(
                || text(locale, "options", &[]),
                |reason| indexed_prompt(reason, locale)
            )
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
                locale.entry("combat", "choiceCardsBrief").unwrap()
            } else {
                locale.entry("combat", "choiceCardsFull").unwrap()
            }
        )
        .unwrap();
        let mut seen = std::collections::HashSet::new();
        for (card, upgrades) in cards {
            if seen.insert((card, upgrades)) {
                if brief {
                    writeln!(
                        reading.view,
                        "  {}",
                        locale.card_name(card.stable_id(), upgrades)
                    )
                    .unwrap();
                    crate::mechanics_info::write_card_offer(
                        &mut reading.view,
                        card,
                        upgrades,
                        locale,
                    );
                } else {
                    crate::mechanics_info::write_card_rule(
                        &mut reading.view,
                        card,
                        upgrades,
                        locale,
                    );
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn selection_members(
    domain: &[Domain],
    locale: Locale,
) -> Vec<DecisionSelectionMemberV1> {
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
                label: card_id.map_or_else(
                    || text(locale, "unknownCard", &[]),
                    |id| locale.card_name(id.stable_id(), upgrades),
                ),
            })
        })
        .collect::<Vec<_>>()
}

fn indexed_prompt(
    reason: &sts_oracle_learning::agent::information::action::PublicCombatIndexedChoiceReasonV1,
    locale: Locale,
) -> String {
    use sts_oracle_learning::agent::information::action::PublicCombatIndexedChoiceReasonV1 as Reason;
    match reason {
        Reason::Discovery { amount, .. } => {
            if *amount == 1 {
                text(
                    locale,
                    "indexed.discovery",
                    &[("name", locale.entity("card", "Discovery"))],
                )
            } else {
                text(
                    locale,
                    "indexed.discoveryCopies",
                    &[
                        ("amount", &amount.to_string()),
                        ("name", locale.entity("card", "Discovery")),
                    ],
                )
            }
        }
        Reason::CardReward { .. } => text(locale, "indexed.cardReward", &[]),
        Reason::ForeignInfluence { .. } => text(
            locale,
            "indexed.foreignInfluence",
            &[("name", locale.entity("card", "ForeignInfluence"))],
        ),
        Reason::ChooseOne => text(locale, "indexed.chooseOne", &[]),
        Reason::Stance => text(locale, "indexed.stance", &[]),
    }
}

pub(crate) fn hand_card_label(
    card: sts_oracle_learning::content::cards::CardId,
    upgrades: u8,
    effective_cost: i32,
    locale: Locale,
) -> String {
    let cost = match effective_cost {
        -1 => text(locale, "handXCost", &[]),
        -2 => locale
            .entry("mechanics", "card.unplayable")
            .unwrap()
            .to_owned(),
        cost => text(locale, "handCost", &[("cost", &cost.to_string())]),
    };
    format!("{}[{cost}]", locale.card_name(card.stable_id(), upgrades))
}

fn current_hand_card_label(
    card: &CombatLearningCardV1,
    state: &PublicCombatStateV1,
    locale: Locale,
) -> String {
    use sts_oracle_learning::content::cards::mechanics::{
        card_base_damage_delivery_v1, CardAttackHitsV1 as Hits, CardAttackTargetsV1 as Targets,
        CardBaseDamageDeliveryV1 as Delivery,
    };
    let mut label = hand_card_label(card.card_id, card.upgrades, card.effective_cost, locale);
    label.pop();
    if let Some(Delivery::Attack { hits, targets, .. }) =
        card_base_damage_delivery_v1(card.card_id, card.upgrades)
    {
        // The public projection already applies Strength, Weak and each target's
        // modifiers. These are per-hit requests before block, not total HP loss.
        let hit = |damage: i32| match hits {
            Hits::Fixed { count: 1 } => {
                text(locale, "handDamage", &[("damage", &damage.to_string())])
            }
            Hits::Fixed { count } => text(
                locale,
                "handMultiDamage",
                &[
                    ("damage", &damage.to_string()),
                    ("hits", &count.to_string()),
                ],
            ),
            _ => text(
                locale,
                "handVariableDamage",
                &[("damage", &damage.to_string())],
            ),
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
                write!(label, " {}", hit(*first)).unwrap();
            } else {
                for (slot, damage) in damage {
                    write!(label, " M{slot}:{}", hit(damage)).unwrap();
                }
            }
        } else if card.damage_by_monster_order.is_empty() {
            write!(label, " {}", hit(card.current_damage)).unwrap();
        }
        match targets {
            Targets::EachEnemy => write!(label, " {}", text(locale, "handAll", &[])).unwrap(),
            Targets::RandomEnemyPerHit => {
                write!(label, " {}", text(locale, "handRandom", &[])).unwrap()
            }
            Targets::SelectedEnemy => {}
        }
    }
    if card.current_block > 0 {
        write!(
            label,
            " {}",
            text(
                locale,
                "handBlock",
                &[("block", &card.current_block.to_string())]
            )
        )
        .unwrap();
    }
    label.push(']');
    label
}

pub(crate) fn reading(
    context: &PublicCombatRunContextV1,
    state: &PublicCombatStateV1,
    brief: bool,
    locale: Locale,
) -> String {
    let mut out = String::new();
    write_run_context(&mut out, context, locale);
    out.push_str(&state_reading_with_rules(state, !brief, locale));
    if let PublicCombatRunContextV1::Available { run_goal, keys, .. } = context {
        if !brief {
            crate::mechanics_info::write_key_status(
                &mut out,
                *keys,
                *run_goal == sts_oracle_learning::ai::planner_core::PlannerRunGoal::HeartVictory,
                locale,
            );
        }
    }
    out
}

pub(crate) fn summary(
    context: &PublicCombatRunContextV1,
    state: &PublicCombatStateV1,
    locale: Locale,
) -> String {
    let mut out = String::new();
    write_run_context(&mut out, context, locale);
    out.push_str(&state_summary(state, locale));
    out
}

fn write_run_context(out: &mut String, context: &PublicCombatRunContextV1, locale: Locale) {
    if let PublicCombatRunContextV1::Available {
        act,
        floor,
        encounter_id,
        ..
    } = context
    {
        out.push_str(&text(
            locale,
            "location",
            &[("act", &act.to_string()), ("floor", &floor.to_string())],
        ));
        if let Some(encounter) = encounter_id {
            write!(
                out,
                "{} | ",
                locale.entity("encounter", encounter.stable_id())
            )
            .unwrap();
        }
    }
}

pub(crate) fn state_reading(state: &PublicCombatStateV1, locale: Locale) -> String {
    state_reading_with_rules(state, true, locale)
}

fn state_summary(state: &PublicCombatStateV1, locale: Locale) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "{}",
        text(
            locale,
            "state",
            &[
                ("ascension", &state.player.ascension_level.to_string()),
                ("hp", &state.player.hp.to_string()),
                ("maxHp", &state.player.max_hp.to_string()),
                ("gold", &state.player.gold.to_string()),
            ]
        )
    )
    .unwrap();
    if state.turn.player_escaping {
        writeln!(out, "{}", text(locale, "escape", &[])).unwrap();
        return out;
    }
    let phase = match state.turn.phase {
        CombatPhase::PlayerTurn => "phase.player",
        CombatPhase::MonsterTurn => "phase.enemy",
        CombatPhase::TurnTransition => "phase.transition",
    };
    writeln!(
        out,
        "{}",
        text(
            locale,
            "turn",
            &[
                ("turn", &state.turn.turn_count.saturating_add(1).to_string()),
                ("phase", locale.entry("combat", phase).unwrap()),
                ("energy", &state.turn.energy.to_string()),
                ("block", &state.player.block.to_string()),
                (
                    "played",
                    &state.turn.counters.cards_played_this_turn.to_string()
                ),
                (
                    "attacks",
                    &state.turn.counters.attacks_played_this_turn.to_string()
                ),
            ]
        )
    )
    .unwrap();
    let hand = state
        .cards
        .hand
        .cards
        .iter()
        .map(|card| current_hand_card_label(card, state, locale))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "{}", text(locale, "hand", &[("cards", &hand)])).unwrap();
    out
}

fn state_reading_with_rules(
    state: &PublicCombatStateV1,
    include_rules: bool,
    locale: Locale,
) -> String {
    let mut out = state_summary(state, locale);
    if state.turn.player_escaping {
        return out;
    }
    if include_rules {
        writeln!(out, "{}", text(locale, "damageNote", &[])).unwrap();
    }
    writeln!(
        out,
        "{}",
        text(
            locale,
            "piles",
            &[
                ("draw", &state.cards.draw.cards.len().to_string()),
                ("discard", &state.cards.discard.cards.len().to_string()),
                ("exhaust", &state.cards.exhaust.cards.len().to_string()),
            ]
        )
    )
    .unwrap();
    let potions = state
        .potions
        .iter()
        .enumerate()
        .map(|(slot, potion)| {
            let name = potion.as_ref().map_or_else(
                || locale.entry("combat", "empty").unwrap(),
                |potion| locale.entity("potion", potion.potion_id.stable_id()),
            );
            format!("P{slot} {name}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "{}", text(locale, "potions", &[("potions", &potions)])).unwrap();
    let relics = state
        .player
        .relics
        .iter()
        .map(|relic| crate::compact::relic_label_v1(relic.id, relic.counter, relic.used_up, locale))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "{}", text(locale, "relics", &[("relics", &relics)])).unwrap();
    if !state.player.powers.is_empty() {
        writeln!(
            out,
            "{}",
            text(
                locale,
                "powers",
                &[("powers", &power_labels(&state.player.powers, locale))]
            )
        )
        .unwrap();
    }
    if state
        .player
        .powers
        .iter()
        .any(|power| power.power == PowerId::Surrounded)
    {
        writeln!(out, "{}", text(locale, "surrounded", &[])).unwrap();
    }
    let mut inactive = 0;
    for monster in &state.monsters {
        if !monster.alive && !monster.half_dead {
            inactive += 1;
            continue;
        }
        let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        out.push_str(&text(
            locale,
            "monster",
            &[
                ("slot", &monster.slot.to_string()),
                ("name", locale.entity("enemy", enemy_id.stable_id())),
                ("hp", &monster.hp.to_string()),
                ("maxHp", &monster.max_hp.to_string()),
            ],
        ));
        if monster.block != 0 {
            out.push_str(&text(
                locale,
                "monsterBlock",
                &[("block", &monster.block.to_string())],
            ));
        }
        let intent = if monster.half_dead {
            text(locale, "reviving", &[])
        } else {
            intent_label(&monster.intent, locale)
        };
        writeln!(out, " | {intent}").unwrap();
        if !monster.powers.is_empty() {
            writeln!(
                out,
                "  {}",
                text(
                    locale,
                    "powers",
                    &[("powers", &power_labels(&monster.powers, locale))]
                )
            )
            .unwrap();
        }
    }
    if inactive > 0 {
        writeln!(
            out,
            "{}",
            text(locale, "inactive", &[("count", &inactive.to_string())])
        )
        .unwrap();
    }
    if include_rules {
        crate::mechanics_info::write_combat_rules(&mut out, state, locale);
    }
    out
}

pub(crate) fn power_labels(powers: &[CombatLearningPowerV1], locale: Locale) -> String {
    powers
        .iter()
        .map(|power| {
            if sts_oracle_learning::content::powers::uses_sentinel_amount(power.power) {
                let mut label = locale
                    .entry("game", &format!("power.{}", power.power.stable_id()))
                    .unwrap_or_else(|| gameplay_display_label_v1(power.power))
                    .to_owned();
                if power.extra_data != 0 {
                    write!(label, "/{}", power.extra_data).unwrap();
                }
                if let Some(card) = &power.payload_card {
                    write!(
                        label,
                        "<{}>",
                        locale.card_name(card.card_id.stable_id(), card.upgrades)
                    )
                    .unwrap();
                }
                label
            } else {
                render_powers(std::slice::from_ref(power), locale)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn intent_label(public: &CombatLearningIntentV1, locale: Locale) -> String {
    let Some(intent) = &public.intent else {
        return text(
            locale,
            if public.hidden_reason.is_some() {
                "intent.hidden"
            } else {
                "intent.unavailable"
            },
            &[],
        );
    };
    let key = match intent {
        Intent::Attack { .. } => "attack",
        Intent::AttackBuff { .. } => "attackBuff",
        Intent::AttackDebuff { .. } => "attackDebuff",
        Intent::AttackDefend { .. } => "attackDefend",
        Intent::Buff => "buff",
        Intent::Heal { .. } => "heal",
        Intent::Debuff => "debuff",
        Intent::StrongDebuff => "strongDebuff",
        Intent::Debug => "debug",
        Intent::Defend => "defend",
        Intent::DefendDebuff => "defendDebuff",
        Intent::DefendBuff => "defendBuff",
        Intent::Escape => "escape",
        Intent::Magic => "magic",
        Intent::None => "none",
        Intent::Sleep => "sleep",
        Intent::Stun => "stun",
        Intent::Unknown => "unknown",
    };
    let label = text(locale, &format!("intent.{key}"), &[]);
    match intent {
        Intent::Attack { hits, .. }
        | Intent::AttackBuff { hits, .. }
        | Intent::AttackDebuff { hits, .. }
        | Intent::AttackDefend { hits, .. } => {
            let damage = public
                .preview_damage_per_hit
                .map_or_else(|| "?".into(), |damage| damage.to_string());
            text(
                locale,
                if *hits == 1 {
                    "intent.single"
                } else {
                    "intent.multi"
                },
                &[
                    ("label", &label),
                    ("damage", &damage),
                    ("hits", &hits.to_string()),
                ],
            )
        }
        Intent::Heal {
            amount: Some(amount),
        } => text(
            locale,
            "intent.healing",
            &[("label", &label), ("amount", &amount.to_string())],
        ),
        _ => label,
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
        assert_eq!(intent_label(&public, Locale::En), "Attack + Buff 7/hit x2");
        assert_eq!(
            intent_label(&public, Locale::ZhCn),
            "攻击并强化 7×2（每段伤害）"
        );
        assert!(!intent_label(&public, Locale::ZhCn).contains("900"));
        public.preview_damage_per_hit = None;
        assert_eq!(intent_label(&public, Locale::En), "Attack + Buff ?/hit x2");
        assert_eq!(
            intent_label(&public, Locale::ZhCn),
            "攻击并强化 ?×2（每段伤害）"
        );
        public.intent = Some(Intent::Attack {
            damage: 2,
            hits: 15,
        });
        public.preview_damage_per_hit = Some(0);
        assert_eq!(intent_label(&public, Locale::En), "Attack 0/hit x15");
        public.intent = None;
        assert_eq!(intent_label(&public, Locale::En), "Intent unknown");
        public.evidence = ObservationEvidenceKindV1::Hidden;
        public.hidden_reason = Some(HiddenInformationReasonV1::RunicDome);
        assert_eq!(intent_label(&public, Locale::En), "Intent hidden");
        assert_eq!(intent_label(&public, Locale::ZhCn), "意图已隐藏");
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
            power_labels(&[presence.clone(), negative.clone()], Locale::En),
            "Corruption, Strength (-1)"
        );
        assert_eq!(
            power_labels(&[presence, negative], Locale::ZhCn),
            "腐化, 力量（-1）"
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
        ], Locale::En), "Vulnerable (3 rounds), Weakened (1 round), Frail (2 rounds), Strength (3), Intangible (1), Intangible (1 round)");
        let cap = CombatLearningPowerV1 {
            extra_data: 200,
            ..power(PowerId::Invincible, 70)
        };
        let value = power_labels(
            &[cap, power(PowerId::Weak, 1), power(PowerId::Strength, -3)],
            Locale::ZhCn,
        );
        assert!(value.contains(&format!(
            "{}（剩余承伤 70/200）",
            Locale::ZhCn.entity("power", "Invincible")
        )));
        assert!(value.contains("虚弱（1 回合）"));
        assert!(value.contains("力量（-3）"));
    }
}
