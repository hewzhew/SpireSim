//! Compact projections of existing search evidence. No ranking or execution.
use std::fmt::Write as _;
use sts_presentation::Locale;

use sha2::{Digest, Sha256};

use sts_oracle_learning::agent::information::state::PublicCombatStateV1;
use sts_oracle_learning::agent::task::{
    TaskResearchPlanAttemptExitV1, TaskResearchPlanMetaChangeV1, TaskResearchPlanPendingRewardV1,
};
use sts_oracle_learning::ai::combat_witness_contract::CombatWitnessSatisfactionV1;
use sts_oracle_learning::content::identity::gameplay_display_label_v1;
use sts_oracle_learning::content::relics::{
    persistent_state::{post_combat_persistent_relic_state_v1, relic_counter_is_combat_local_v1},
    RelicState,
};
use sts_oracle_learning::eval::run_control::WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1;

use crate::agent_protocol::{AgentCombatResearchBoundedProgressV1, AgentCombatResearchCandidateV1};
use crate::combat_trace::RunPlayCombatTraceV1;
use crate::render::format_card;

pub(crate) fn research_message(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("combat", &format!("research.{key}"), parameters)
        .expect("combat research message")
}

fn word(locale: Locale, key: &str) -> &'static str {
    locale
        .entry("combat", &format!("research.{key}"))
        .expect("combat research text")
}

/// Display selectors bind a candidate to the report that produced it. Wire
/// references remain separate and travel with the complete report identity.
pub(crate) fn plan_id(report_id: &str, reference: &str) -> String {
    let stamp = format!("{:x}", Sha256::digest(report_id.as_bytes()));
    format!("{reference}@{}", &stamp[..12])
}

pub(crate) fn research(
    report_id: &str,
    root: &PublicCombatStateV1,
    candidates: &[AgentCombatResearchCandidateV1],
    progress: &[AgentCombatResearchBoundedProgressV1],
    max_search_ms: Option<u64>,
    lanes: &[sts_oracle_learning::agent::task::TaskResearchPlanLaneAttemptV1],
    target: CombatWitnessSatisfactionV1,
    work_limit: Option<usize>,
    diagnostics: bool,
    locale: Locale,
) -> String {
    let message =
        |key: &str, parameters: &[(&str, &str)]| research_message(locale, key, parameters);
    let mut out = String::new();
    let whole_combat = lanes
        .iter()
        .any(|lane| lane.provenance.producer_name == WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1);
    let owner = if whole_combat {
        "CombatSolver"
    } else {
        word(locale, "owner")
    };
    let result = if candidates.is_empty() {
        message("noWin", &[])
    } else {
        let mugged = candidates.iter().any(|c| c.outcome.combat_mugged);
        let key = match (mugged, candidates.len() == 1) {
            (true, true) => "oneOutcome",
            (true, false) => "manyOutcomes",
            (false, true) => "oneWin",
            (false, false) => "manyWins",
        };
        message(key, &[("count", &candidates.len().to_string())])
    };
    let mut stops = Vec::new();
    for lane in lanes {
        let stop = short_exit_description(lane.exit, locale);
        if !stops.contains(&stop) {
            stops.push(stop);
        }
    }
    writeln!(
        out,
        "{}",
        message(
            "heading",
            &[
                ("owner", owner),
                ("turn", &root.turn.turn_count.saturating_add(1).to_string()),
                ("hp", &root.player.hp.to_string()),
                ("maxHp", &root.player.max_hp.to_string()),
                ("result", &result),
                (
                    "stops",
                    &if stops.is_empty() {
                        String::new()
                    } else {
                        format!(" | {}", stops.join(", "))
                    }
                )
            ]
        )
    )
    .unwrap();
    if diagnostics {
        render_search_root(&mut out, root, locale);
        writeln!(
            out,
            "{}",
            message("target", &[("target", &target_description(target, locale))])
        )
        .unwrap();
        if let Some(work) = work_limit {
            writeln!(
                out,
                "{}",
                message("workLimit", &[("work", &work.to_string())])
            )
            .unwrap();
        }
        if let Some(ms) = max_search_ms {
            writeln!(out, "{}", message("timeLimit", &[("ms", &ms.to_string())])).unwrap();
        }
        for (index, lane) in lanes.iter().enumerate() {
            writeln!(
                out,
                "{}",
                message(
                    "lane",
                    &[
                        ("index", &index.to_string()),
                        ("work", &lane.work_consumed.to_string()),
                        ("exit", exit_description(lane.exit, locale))
                    ]
                )
            )
            .unwrap();
        }
        if let Some(turn) = progress.iter().map(|p| p.max_player_turn).max() {
            writeln!(
                out,
                "{}",
                message("deepest", &[("turn", &turn.saturating_add(1).to_string())])
            )
            .unwrap();
        }
    }
    if candidates.is_empty() {
        return out;
    }
    let theft_outcomes = candidates.iter().any(|candidate| {
        let outcome = &candidate.outcome;
        outcome.combat_mugged
            || outcome.persistent.unrecovered_stolen_gold > 0
            || outcome
                .pending_rewards
                .iter()
                .any(|reward| matches!(reward, TaskResearchPlanPendingRewardV1::StolenGold { .. }))
    });
    let first = &candidates[0].outcome;
    let shared_gold = candidates.iter().all(|candidate| {
        candidate.outcome.final_gold == first.final_gold
            && candidate.outcome.claimable_gold_v1() == first.claimable_gold_v1()
    });
    let is_money = |reward: &TaskResearchPlanPendingRewardV1| {
        matches!(
            reward,
            TaskResearchPlanPendingRewardV1::Gold { .. }
                | TaskResearchPlanPendingRewardV1::StolenGold { .. }
        )
    };
    let first_non_gold_rewards = first
        .pending_rewards
        .iter()
        .filter(|reward| !is_money(reward))
        .collect::<Vec<_>>();
    let shared_non_gold_rewards = candidates.iter().all(|candidate| {
        candidate
            .outcome
            .pending_rewards
            .iter()
            .filter(|reward| !is_money(reward))
            .eq(first_non_gold_rewards.iter().copied())
    });
    if !diagnostics && shared_non_gold_rewards {
        let potions = first_non_gold_rewards
            .iter()
            .filter_map(|reward| match reward {
                TaskResearchPlanPendingRewardV1::Potion { potion_id } => {
                    Some(locale.entity("potion", potion_id.stable_id()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !potions.is_empty() {
            writeln!(
                out,
                "{}",
                message("sharedPotion", &[("potions", &potions.join(", "))])
            )
            .unwrap();
        }
    }
    if shared_gold && (diagnostics || theft_outcomes) {
        writeln!(
            out,
            "{}",
            message(
                "sharedGold",
                &[
                    ("after", &first.claimable_gold_v1().to_string()),
                    ("before", &first.final_gold.to_string())
                ]
            )
        )
        .unwrap();
    }
    let opening = opening_actions(&candidates[0].action_line, locale);
    let shared_opening = candidates
        .iter()
        .all(|candidate| opening_actions(&candidate.action_line, locale) == opening);
    if diagnostics && shared_opening && !opening.is_empty() {
        writeln!(
            out,
            "{}",
            message("sharedActions", &[("actions", &opening)])
        )
        .unwrap();
    }
    let shared_counters: Vec<_> = candidates[0]
        .outcome
        .final_relics
        .iter()
        .filter(|relic| relic.counter >= 0 && !relic_counter_is_combat_local_v1(relic.id))
        .filter(|relic| {
            candidates
                .iter()
                .all(|candidate| candidate.outcome.final_relics.contains(relic))
        })
        .cloned()
        .collect();
    if !shared_counters.is_empty() {
        writeln!(out, "{}", message("sharedHeading", &[])).unwrap();
        render_persistent_counters_at_finish(&mut out, &shared_counters, locale);
    }
    let shared_growth = candidates.iter().all(|candidate| {
        candidate.outcome.persistent.ritual_dagger_value
            == candidates[0].outcome.persistent.ritual_dagger_value
            && candidate.outcome.persistent.genetic_algorithm_value
                == candidates[0].outcome.persistent.genetic_algorithm_value
    });
    if shared_growth
        && (candidates[0].outcome.persistent.ritual_dagger_value > 0
            || candidates[0].outcome.persistent.genetic_algorithm_value > 0)
    {
        let growth = [
            (
                locale.entity("card", "RitualDagger"),
                first.persistent.ritual_dagger_value,
            ),
            (
                locale.entity("card", "GeneticAlgorithm"),
                first.persistent.genetic_algorithm_value,
            ),
        ]
        .into_iter()
        .filter(|(_, value)| *value > 0)
        .map(|(name, value)| format!("{name} {value}"))
        .collect::<Vec<_>>();
        writeln!(
            out,
            "{}",
            message("sharedGrowth", &[("cards", &growth.join(", "))])
        )
        .unwrap();
    }
    let mut potion_groups = std::collections::BTreeMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let mut slots = candidate.outcome.consumed_entry_potion_slots.clone();
        slots.sort_unstable();
        potion_groups
            .entry((candidate.outcome.total_potions_consumed, slots))
            .or_insert_with(Vec::new)
            .push(index);
    }
    let groups = if diagnostics {
        vec![(0..candidates.len()).collect::<Vec<_>>()]
    } else {
        potion_groups.into_values().collect::<Vec<_>>()
    };
    let grouped = groups.len() > 1;
    let no_spend = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.outcome.total_potions_consumed == 0 && !c.outcome.combat_mugged)
        .min_by_key(|(_, c)| std::cmp::Reverse(c.outcome.final_hp));
    if grouped && no_spend.is_none() {
        writeln!(out, "{}", message("noPotionFreeWin", &[])).unwrap();
    }
    for index in groups.into_iter().flatten() {
        let candidate = &candidates[index];
        let o = &candidate.outcome;
        let details = if diagnostics {
            message(
                "actions",
                &[
                    ("count", &candidate.plan_steps.to_string()),
                    (
                        "unmet",
                        if candidate.satisfies_requested_quality {
                            ""
                        } else {
                            word(locale, "targetUnmet")
                        },
                    ),
                ],
            )
        } else {
            String::new()
        };
        let spending = message(
            "potions",
            &[
                ("count", &o.total_potions_consumed.to_string()),
                (
                    "names",
                    &spent_potion_names(root, &o.consumed_entry_potion_slots, locale),
                ),
            ],
        );
        let comparison = if grouped && o.total_potions_consumed > 0 && !o.combat_mugged {
            no_spend.map_or_else(String::new, |(reference, baseline)| {
                message(
                    "hpComparison",
                    &[
                        (
                            "delta",
                            &format!(
                                "{:+}",
                                i64::from(o.final_hp) - i64::from(baseline.outcome.final_hp)
                            ),
                        ),
                        ("reference", &plan_id(report_id, &format!("c{reference}"))),
                    ],
                )
            })
        } else {
            String::new()
        };
        let selector = plan_id(report_id, &format!("c{index}"));
        writeln!(
            out,
            "{}",
            message(
                "candidate",
                &[
                    ("selector", &selector),
                    ("hp", &o.final_hp.to_string()),
                    ("maxHp", &o.persistent.max_hp.to_string()),
                    (
                        "minimum",
                        &candidate
                            .minimum_observed_hp
                            .map_or_else(|| word(locale, "unknown").into(), |hp| hp.to_string())
                    ),
                    ("spending", &spending),
                    ("comparison", &comparison),
                    ("details", &details)
                ]
            ),
        )
        .unwrap();
        if !shared_gold {
            writeln!(
                out,
                "{}",
                message(
                    "gold",
                    &[
                        ("after", &o.claimable_gold_v1().to_string()),
                        ("before", &o.final_gold.to_string())
                    ]
                )
            )
            .unwrap();
        }
        let distinct_counters: Vec<_> = o
            .final_relics
            .iter()
            .filter(|relic| !shared_counters.contains(relic))
            .cloned()
            .collect();
        render_persistent_counters_at_finish(&mut out, &distinct_counters, locale);
        let growth = [
            (
                locale.entity("card", "RitualDagger"),
                o.persistent.ritual_dagger_value,
            ),
            (
                locale.entity("card", "GeneticAlgorithm"),
                o.persistent.genetic_algorithm_value,
            ),
        ]
        .into_iter()
        .filter(|(_, value)| *value > 0)
        .map(|(name, value)| format!("{name} {value}"))
        .collect::<Vec<_>>();
        if !growth.is_empty() && !shared_growth {
            writeln!(
                out,
                "{}",
                message("growth", &[("cards", &growth.join(", "))])
            )
            .unwrap();
        }
        render_used_up_relics_at_finish(
            &mut out,
            &o.final_relics,
            (!diagnostics).then_some(root.player.relics.as_slice()),
            locale,
        );
        if o.combat_mugged || o.persistent.unrecovered_stolen_gold > 0 {
            writeln!(
                out,
                "{}",
                message(
                    "theft",
                    &[
                        ("gold", &o.persistent.unrecovered_stolen_gold.to_string()),
                        (
                            "escaped",
                            if o.combat_mugged {
                                word(locale, "escaped")
                            } else {
                                ""
                            }
                        )
                    ]
                )
            )
            .unwrap();
        }
        if diagnostics || !shared_non_gold_rewards {
            let rewards = o
                .pending_rewards
                .iter()
                .map(|reward| match reward {
                    TaskResearchPlanPendingRewardV1::Gold { amount } => {
                        message("rewardGold", &[("amount", &amount.to_string())])
                    }
                    TaskResearchPlanPendingRewardV1::StolenGold { amount } => {
                        message("rewardStolenGold", &[("amount", &amount.to_string())])
                    }
                    TaskResearchPlanPendingRewardV1::Potion { potion_id } => message(
                        "rewardPotion",
                        &[("name", locale.entity("potion", potion_id.stable_id()))],
                    ),
                    TaskResearchPlanPendingRewardV1::Relic { relic_id } => message(
                        "rewardRelic",
                        &[("name", locale.entity("relic", relic_id.stable_id()))],
                    ),
                    TaskResearchPlanPendingRewardV1::Card { cards } => message(
                        "rewardCards",
                        &[(
                            "cards",
                            &cards
                                .iter()
                                .map(|card| {
                                    locale.card_name(card.card_id.stable_id(), card.upgrades)
                                })
                                .collect::<Vec<_>>()
                                .join(", "),
                        )],
                    ),
                    TaskResearchPlanPendingRewardV1::EmeraldKey => {
                        word(locale, "emeraldKey").to_string()
                    }
                    TaskResearchPlanPendingRewardV1::SapphireKey => {
                        word(locale, "sapphireKey").to_string()
                    }
                })
                .collect::<Vec<_>>();
            writeln!(
                out,
                "{}",
                message(
                    "rewards",
                    &[(
                        "rewards",
                        &if rewards.is_empty() {
                            word(locale, "none").to_string()
                        } else {
                            rewards.join(", ")
                        }
                    )]
                )
            )
            .unwrap();
        }
        if diagnostics && !candidate.action_line.is_empty() && !shared_opening {
            writeln!(
                out,
                "{}",
                message(
                    "nextActions",
                    &[("actions", &opening_actions(&candidate.action_line, locale))]
                )
            )
            .unwrap();
        }
        if !o.generated_potions.is_empty() {
            writeln!(
                out,
                "{}",
                message(
                    "generated",
                    &[(
                        "potions",
                        &o.generated_potions
                            .iter()
                            .map(|p| format!(
                                "P{} {}",
                                p.slot,
                                locale.entity("potion", p.potion_id.stable_id())
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )]
                )
            )
            .unwrap();
        }
        for change in &o.meta_changes {
            let label = match change {
                TaskResearchPlanMetaChangeV1::AddCardToMasterDeck { card_id } => message(
                    "deckAdded",
                    &[("card", &locale.card_name(card_id.stable_id(), 0))],
                ),
                TaskResearchPlanMetaChangeV1::ModifyCardMisc {
                    master_deck_ordinal,
                    amount,
                } => message(
                    "deckMisc",
                    &[
                        ("ordinal", &master_deck_ordinal.to_string()),
                        ("amount", &format!("{amount:+}")),
                    ],
                ),
                TaskResearchPlanMetaChangeV1::UpgradeMasterDeckCard {
                    master_deck_ordinal,
                } => message(
                    "deckUpgraded",
                    &[("ordinal", &master_deck_ordinal.to_string())],
                ),
            };
            writeln!(out, "  {label}").unwrap();
        }
    }

    out
}

fn spent_potion_names(root: &PublicCombatStateV1, slots: &[usize], locale: Locale) -> String {
    if slots.is_empty() {
        return String::new();
    }
    let names = slots
        .iter()
        .map(|slot| {
            let name = root
                .potions
                .get(*slot)
                .and_then(Option::as_ref)
                .map_or(word(locale, "unknown"), |p| {
                    locale.entity("potion", p.potion_id.stable_id())
                });
            format!("P{slot} {name}")
        })
        .collect::<Vec<_>>();
    format!(" ({})", names.join(", "))
}

fn short_exit_description(exit: TaskResearchPlanAttemptExitV1, locale: Locale) -> &'static str {
    use TaskResearchPlanAttemptExitV1::*;
    let key = match exit {
        Witness => "shortExit.Witness",
        WitnessAtWorkLimit => "shortExit.WitnessAtWorkLimit",
        WitnessAtWallLimit => "shortExit.WitnessAtWallLimit",
        GenerationWorkExhausted => "shortExit.GenerationWorkExhausted",
        ExpandedNodeBudgetExhausted => "shortExit.ExpandedNodeBudgetExhausted",
        WallTimeExhausted => "shortExit.WallTimeExhausted",
        AllowanceExhausted => "shortExit.AllowanceExhausted",
        DeadlineReached => "shortExit.DeadlineReached",
        PortfolioCompleteWithoutWitness => "shortExit.PortfolioCompleteWithoutWitness",
        SearchFrontierExhaustedWithoutWitness => "shortExit.SearchFrontierExhaustedWithoutWitness",
        ActionPrefixBudgetExhausted => "shortExit.ActionPrefixBudgetExhausted",
        ActionSurfaceIncomplete => "shortExit.ActionSurfaceIncomplete",
        FrontierOpenWithoutWitness => "shortExit.FrontierOpenWithoutWitness",
        MilestoneCandidates => "shortExit.MilestoneCandidates",
    };
    word(locale, key)
}

fn render_search_root(out: &mut String, root: &PublicCombatStateV1, locale: Locale) {
    use crate::decision::combat::{hand_card_label, power_labels};
    writeln!(
        out,
        "{}",
        research_message(
            locale,
            "root",
            &[
                ("turn", &root.turn.turn_count.saturating_add(1).to_string()),
                ("hp", &root.player.hp.to_string()),
                ("maxHp", &root.player.max_hp.to_string()),
                ("energy", &root.turn.energy.to_string()),
                ("block", &root.player.block.to_string()),
                (
                    "played",
                    &root.turn.counters.cards_played_this_turn.to_string()
                )
            ]
        )
    )
    .unwrap();
    writeln!(
        out,
        "{}",
        research_message(
            locale,
            "rootHand",
            &[(
                "cards",
                &root
                    .cards
                    .hand
                    .cards
                    .iter()
                    .map(|c| hand_card_label(c.card_id, c.upgrades, c.effective_cost, locale))
                    .collect::<Vec<_>>()
                    .join(", ")
            )]
        )
    )
    .unwrap();
    writeln!(
        out,
        "{}",
        research_message(
            locale,
            "rootPowers",
            &[(
                "powers",
                &if root.player.powers.is_empty() {
                    word(locale, "none").into()
                } else {
                    power_labels(&root.player.powers, locale)
                }
            )]
        )
    )
    .unwrap();
    writeln!(
        out,
        "{}",
        research_message(
            locale,
            "rootPotions",
            &[(
                "potions",
                &root
                    .potions
                    .iter()
                    .enumerate()
                    .map(|(slot, potion)| format!(
                        "P{slot} {}",
                        potion.as_ref().map_or(word(locale, "empty"), |p| locale
                            .entity("potion", p.potion_id.stable_id()))
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )]
        )
    )
    .unwrap();
}

fn target_description(target: CombatWitnessSatisfactionV1, locale: Locale) -> String {
    use CombatWitnessSatisfactionV1::*;
    let (key, limit) = match target {
        BudgetOrExhaustion => return word(locale, "targetRule.budget").into(),
        ZeroLossOrBudget => ("zeroLoss", None),
        FirstCompleteWin => ("firstWin", None),
        HpLossAtMost(limit) => ("hpLoss", Some(limit)),
        MaterializedPersistentPayoffGain => ("payoff", None),
        FirstCompleteWinWithoutNewExternalBurden => ("noBurden", None),
        HpLossAtMostWithoutNewExternalBurden(limit) => ("hpNoBurden", Some(limit)),
        PotionFreeHpLossAtMostWithoutNewExternalBurden(limit) => ("potionFree", Some(limit)),
    };
    let requested = research_message(
        locale,
        &format!("targetRule.{key}"),
        &[("limit", &limit.unwrap_or(0).to_string())],
    );
    research_message(
        locale,
        "targetRule.recoverGold",
        &[("requested", &requested)],
    )
}

fn exit_description(exit: TaskResearchPlanAttemptExitV1, locale: Locale) -> &'static str {
    use TaskResearchPlanAttemptExitV1::*;
    let key = match exit {
        Witness => "exit.Witness",
        WitnessAtWorkLimit => "exit.WitnessAtWorkLimit",
        WitnessAtWallLimit => "exit.WitnessAtWallLimit",
        GenerationWorkExhausted => "exit.GenerationWorkExhausted",
        ExpandedNodeBudgetExhausted => "exit.ExpandedNodeBudgetExhausted",
        WallTimeExhausted => "exit.WallTimeExhausted",
        AllowanceExhausted => "exit.AllowanceExhausted",
        DeadlineReached => "exit.DeadlineReached",
        PortfolioCompleteWithoutWitness => "exit.PortfolioCompleteWithoutWitness",
        SearchFrontierExhaustedWithoutWitness => "exit.SearchFrontierExhaustedWithoutWitness",
        ActionPrefixBudgetExhausted => "exit.ActionPrefixBudgetExhausted",
        ActionSurfaceIncomplete => "exit.ActionSurfaceIncomplete",
        FrontierOpenWithoutWitness => "exit.FrontierOpenWithoutWitness",
        MilestoneCandidates => "exit.MilestoneCandidates",
    };
    word(locale, key)
}

fn render_used_up_relics_at_finish(
    out: &mut String,
    relics: &[RelicState],
    root_relics: Option<&[RelicState]>,
    locale: Locale,
) {
    // Default search summaries describe spending from this root. Explicit
    // diagnostics and reviews retain full final availability.
    // Combat-local activations such as Ancient Tea Set reset next combat.
    let used_up = relics
        .iter()
        .filter(|relic| post_combat_persistent_relic_state_v1(relic).used_up)
        .filter(|relic| {
            !root_relics.is_some_and(|root| {
                root.iter().any(|entry| {
                    entry.id == relic.id && post_combat_persistent_relic_state_v1(entry).used_up
                })
            })
        })
        .map(|relic| locale.entity("relic", relic.id.stable_id()))
        .collect::<Vec<_>>();
    if !used_up.is_empty() {
        let label = if root_relics.is_some() {
            word(locale, "newlyUsedUp")
        } else {
            word(locale, "usedUp")
        };
        writeln!(out, "  {label}: {}", used_up.join(", ")).unwrap();
    }
}

fn render_persistent_counters_at_finish(out: &mut String, relics: &[RelicState], locale: Locale) {
    let counters = relics
        .iter()
        .filter(|relic| relic.counter >= 0 && !relic_counter_is_combat_local_v1(relic.id))
        .map(|relic| {
            let label =
                crate::compact::relic_label_v1(relic.id, relic.counter, relic.used_up, locale);
            if relic.id == sts_oracle_learning::content::relics::RelicId::IncenseBurner {
                if let Some(turn) =
                    sts_oracle_learning::content::relics::incense_burner::next_combat_trigger_turn(
                        relic.counter,
                    )
                {
                    return research_message(
                        locale,
                        "nextCombatTurn",
                        &[("relic", &label), ("turn", &turn.to_string())],
                    );
                }
            }
            label
        })
        .collect::<Vec<_>>();
    if !counters.is_empty() {
        writeln!(
            out,
            "{}",
            research_message(locale, "counters", &[("relics", &counters.join(", "))])
        )
        .unwrap();
    }
}

fn opening_actions(actions: &[String], locale: Locale) -> String {
    const MAX_SHOWN: usize = 6;
    let opening_len = actions
        .iter()
        .position(|action| action == "end")
        .map_or(actions.len(), |index| index + 1);
    let shown = opening_len.min(MAX_SHOWN);
    let mut preview = actions[..shown].join(" -> ");
    if shown < opening_len {
        preview.push_str(&research_message(
            locale,
            "moreActions",
            &[("count", &(opening_len - shown).to_string())],
        ));
    }
    preview
}

#[cfg(test)]
mod tests {
    fn root() -> super::PublicCombatStateV1 {
        sts_oracle_learning::agent::information::state::public_combat_state_v1(
            &sts_oracle_learning::test_support::blank_test_combat(),
        )
    }

    fn candidate(hp: i32, slots: Vec<usize>) -> super::AgentCombatResearchCandidateV1 {
        use sts_oracle_learning::agent::task::{
            TaskResearchPlanOutcomeV1, TaskResearchPlanPersistentOutcomeV1,
        };
        super::AgentCombatResearchCandidateV1 {
            candidate_id: format!("hp-{hp}-{slots:?}"),
            report_candidate_index: None,
            supporting_provenances: vec![],
            plan_steps: 1,
            minimum_observed_hp: Some(hp),
            satisfies_requested_quality: true,
            action_line: vec!["end".into()],
            outcome: TaskResearchPlanOutcomeV1 {
                final_hp: hp,
                final_gold: 0,
                persistent: TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold: 0,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                total_potions_consumed: slots.len() as u32,
                consumed_entry_potion_slots: slots,
                generated_potions: vec![],
                final_relics: vec![],
                meta_changes: vec![],
                pending_rewards: vec![],
                combat_mugged: false,
            },
        }
    }

    fn render_candidates(candidates: &[super::AgentCombatResearchCandidateV1]) -> String {
        super::research(
            "test-report",
            &root(),
            candidates,
            &[],
            None,
            &[],
            super::CombatWitnessSatisfactionV1::FirstCompleteWin,
            None,
            false,
            sts_presentation::Locale::En,
        )
    }

    #[test]
    fn bilingual_research_keeps_numeric_outcomes_commands_and_original_evidence() {
        use sts_oracle_learning::agent::task::{
            TaskResearchPlanGeneratedPotionV1, TaskResearchPlanMetaChangeV1 as Change,
            TaskResearchPlanPendingRewardV1 as Reward, TaskResearchPlanRewardCardV1,
        };
        use sts_oracle_learning::content::{
            cards::CardId,
            potions::PotionId,
            relics::{RelicId, RelicState},
        };
        use sts_presentation::Locale;
        let mut root = root();
        root.potions = vec![Some(
            sts_oracle_learning::agent::information::state::CombatLearningPotionV1 {
                potion_id: PotionId::SteroidPotion,
                can_use: true,
                can_discard: true,
                requires_target: false,
            },
        )];
        let mut paid = candidate(70, vec![0]);
        paid.minimum_observed_hp = None;
        paid.satisfies_requested_quality = false;
        paid.outcome.final_gold = 12;
        paid.outcome.persistent.ritual_dagger_value = 50;
        paid.outcome.persistent.unrecovered_stolen_gold = 4;
        paid.outcome.final_relics = vec![
            RelicState {
                counter: 4,
                ..RelicState::new(RelicId::IncenseBurner)
            },
            RelicState {
                used_up: true,
                ..RelicState::new(RelicId::LizardTail)
            },
        ];
        paid.outcome.pending_rewards = vec![
            Reward::Gold { amount: 13 },
            Reward::StolenGold { amount: 5 },
            Reward::Potion {
                potion_id: PotionId::FirePotion,
            },
            Reward::Relic {
                relic_id: RelicId::Shovel,
            },
            Reward::Card {
                cards: vec![TaskResearchPlanRewardCardV1 {
                    card_id: CardId::ShrugItOff,
                    upgrades: 1,
                }],
            },
            Reward::EmeraldKey,
            Reward::SapphireKey,
        ];
        paid.outcome.generated_potions = vec![TaskResearchPlanGeneratedPotionV1 {
            slot: 1,
            potion_id: PotionId::FirePotion,
        }];
        paid.outcome.meta_changes = vec![
            Change::AddCardToMasterDeck {
                card_id: CardId::Shiv,
            },
            Change::ModifyCardMisc {
                master_deck_ordinal: 2,
                amount: 3,
            },
            Change::UpgradeMasterDeckCard {
                master_deck_ordinal: 0,
            },
        ];
        let candidates = [paid, candidate(66, vec![])];
        let before = serde_json::to_value((&root, &candidates)).unwrap();
        for diagnostics in [false, true] {
            for locale in [Locale::En, Locale::ZhCn] {
                let view = super::research(
                    "test-report",
                    &root,
                    &candidates,
                    &[],
                    Some(500),
                    &[],
                    super::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(17),
                    Some(64),
                    diagnostics,
                    locale,
                );
                for id in ["c0@4bf5ca407322", "c1@4bf5ca407322"] {
                    assert!(view.contains(&format!("./p commit {id}")));
                }
                assert!(!view.contains("{{") && !view.contains('·'));
                if locale == Locale::ZhCn {
                    for expected in [
                        "生命 70/80",
                        "最低观测生命 未知",
                        "P0 类固醇药水",
                        "领取后 30，领取前 12",
                        "仪式匕首 50",
                        "香炉 [4]（下场战斗第 2 回合触发）",
                        "蜥蜴尾巴",
                        "13 金币",
                        "追回 5 金币",
                        "火焰药水",
                        "遗物 铲子",
                        "卡牌 [耸肩无视+]",
                        "绿宝石钥匙",
                        "蓝宝石钥匙",
                        "牌组 +小刀",
                        "牌组卡牌 2：附加值 +3",
                        "牌组卡牌 0：已升级",
                    ] {
                        assert!(view.contains(expected), "missing {expected}: {view}");
                    }
                    if diagnostics {
                        assert!(view.contains("生命净损失 <= 17，不增加永久诅咒牌；追回被偷金币"));
                        assert!(view.contains("未达到请求目标"));
                    } else {
                        assert!(view.contains("相比 c1@4bf5ca407322，生命 +4"));
                    }
                }
            }
        }
        assert_eq!(serde_json::to_value((&root, &candidates)).unwrap(), before);
    }

    #[test]
    fn every_research_exit_and_target_has_bilingual_semantics() {
        use super::CombatWitnessSatisfactionV1::*;
        use super::TaskResearchPlanAttemptExitV1::*;
        use sts_presentation::Locale;
        for exit in [
            Witness,
            WitnessAtWorkLimit,
            WitnessAtWallLimit,
            GenerationWorkExhausted,
            ExpandedNodeBudgetExhausted,
            WallTimeExhausted,
            AllowanceExhausted,
            DeadlineReached,
            PortfolioCompleteWithoutWitness,
            SearchFrontierExhaustedWithoutWitness,
            ActionPrefixBudgetExhausted,
            ActionSurfaceIncomplete,
            FrontierOpenWithoutWitness,
            MilestoneCandidates,
        ] {
            for locale in [Locale::En, Locale::ZhCn] {
                for text in [
                    super::short_exit_description(exit, locale),
                    super::exit_description(exit, locale),
                ] {
                    assert!(!text.is_empty());
                    if locale == Locale::ZhCn {
                        assert!(text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
                    }
                }
            }
        }
        for target in [
            BudgetOrExhaustion,
            ZeroLossOrBudget,
            FirstCompleteWin,
            HpLossAtMost(17),
            MaterializedPersistentPayoffGain,
            FirstCompleteWinWithoutNewExternalBurden,
            HpLossAtMostWithoutNewExternalBurden(17),
            PotionFreeHpLossAtMostWithoutNewExternalBurden(17),
        ] {
            for locale in [Locale::En, Locale::ZhCn] {
                let text = super::target_description(target, locale);
                assert!(!text.contains("{{"));
                if locale == Locale::ZhCn {
                    assert!(text.contains('胜') || text.contains("预算"));
                }
            }
        }
    }

    #[test]
    fn potion_groups_expose_hp_tradeoffs_without_rebinding_candidate_references() {
        let candidates = [
            candidate(70, vec![0]),
            candidate(68, vec![]),
            candidate(69, vec![1]),
            candidate(66, vec![]),
        ];
        let view = render_candidates(&candidates);
        assert!(
            view.find("c1@4bf5ca407322:").unwrap() < view.find("c0@4bf5ca407322:").unwrap(),
            "{view}"
        );
        assert!(
            view.find("c3@4bf5ca407322:").unwrap() < view.find("c0@4bf5ca407322:").unwrap(),
            "{view}"
        );
        assert_eq!(view.matches("Additional potions 0").count(), 2, "{view}");
        assert_eq!(view.matches("Additional potions 1").count(), 2, "{view}");
        assert!(
            view.contains(
                "c0@4bf5ca407322: HP 70/80 after combat | min observed 70 | Additional potions 1 (P0 Unknown) | HP +2 vs c1@4bf5ca407322 | ./p commit c0@4bf5ca407322"
            ),
            "{view}"
        );
        assert!(
            view.contains(
                "c2@4bf5ca407322: HP 69/80 after combat | min observed 69 | Additional potions 1 (P1 Unknown) | HP +1 vs c1@4bf5ca407322 | ./p commit c2@4bf5ca407322"
            ),
            "{view}"
        );
        assert!(
            view.contains("P0 Unknown") && view.contains("P1 Unknown"),
            "{view}"
        );
        assert_eq!(candidates[0].outcome.final_hp, 70);
        assert!(!view.contains("recommended"));

        // A spent potion can accompany less HP; differences are signed outcome
        // comparisons, not a potion price or a promise that spending helps.
        let negative = render_candidates(&[candidate(65, vec![0]), candidate(68, vec![])]);
        assert!(negative.contains("HP -3 vs c1@4bf5ca407322"), "{negative}");
    }

    #[test]
    fn missing_potion_free_win_does_not_use_an_escape_as_the_baseline() {
        let mut escaped = candidate(80, vec![]);
        escaped.outcome.combat_mugged = true;
        let view = render_candidates(&[candidate(70, vec![0]), escaped]);
        assert!(
            view.contains("No potion-free win in this report."),
            "{view}"
        );
        assert!(!view.contains("HP -10 vs"), "{view}");
        let only_spending = render_candidates(&[candidate(70, vec![0]), candidate(69, vec![1])]);
        assert!(
            only_spending.contains("No potion-free win in this report."),
            "{only_spending}"
        );
        assert!(!only_spending.contains(" vs c"), "{only_spending}");
        let one_group = render_candidates(&[candidate(68, vec![])]);
        assert!(
            one_group.contains("Additional potions 0 | ./p commit c0@4bf5ca407322"),
            "{one_group}"
        );
        assert!(!one_group.contains("No potion-free"), "{one_group}");
    }

    #[test]
    fn search_candidate_resources_include_persistent_counters_and_card_values() {
        use sts_oracle_learning::agent::task::{
            TaskResearchPlanOutcomeV1, TaskResearchPlanPendingRewardV1,
            TaskResearchPlanPersistentOutcomeV1,
        };
        use sts_oracle_learning::content::relics::{RelicId, RelicState};
        let candidate = super::AgentCombatResearchCandidateV1 {
            candidate_id: "gold-candidate".into(),
            report_candidate_index: None,
            supporting_provenances: vec![],
            plan_steps: 1,
            minimum_observed_hp: Some(12),
            satisfies_requested_quality: true,
            action_line: vec!["end".into()],
            outcome: TaskResearchPlanOutcomeV1 {
                final_hp: 40,
                final_gold: 112,
                persistent: TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold: 0,
                    ritual_dagger_value: 50,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                consumed_entry_potion_slots: vec![],
                total_potions_consumed: 0,
                generated_potions: vec![],
                final_relics: vec![
                    RelicState {
                        counter: 4,
                        ..RelicState::new(RelicId::IncenseBurner)
                    },
                    RelicState {
                        counter: 9,
                        ..RelicState::new(RelicId::PenNib)
                    },
                    RelicState {
                        counter: 2,
                        ..RelicState::new(RelicId::Kunai)
                    },
                ],
                meta_changes: vec![],
                pending_rewards: vec![TaskResearchPlanPendingRewardV1::Gold { amount: 96 }],
                combat_mugged: false,
            },
        };
        let mut root = root();
        root.player.gold = 192;
        root.potions = vec![Some(
            sts_oracle_learning::agent::information::state::CombatLearningPotionV1 {
                potion_id: sts_oracle_learning::content::potions::PotionId::CultistPotion,
                can_use: true,
                can_discard: true,
                requires_target: false,
            },
        )];
        let mut alternative = candidate.clone();
        alternative.outcome.final_relics[0].counter = 0;
        alternative.outcome.persistent.ritual_dagger_value = 45;
        alternative.outcome.consumed_entry_potion_slots = vec![0];
        alternative.outcome.total_potions_consumed = 1;
        let view = super::research(
            "test-report",
            &root,
            &[candidate.clone(), alternative.clone()],
            &[],
            None,
            &[],
            super::CombatWitnessSatisfactionV1::FirstCompleteWin,
            None,
            false,
            sts_presentation::Locale::En,
        );
        assert!(!view.contains("Candidate Gold after claims"), "{view}");
        assert!(!view.contains("(now "), "{view}");
        assert_eq!(view.matches("Pen Nib [9]").count(), 1);
        assert!(!view.contains("Next actions:"), "{view}");
        assert_eq!(view.matches("min observed 12").count(), 2);
        assert!(view.contains("Additional potions 0"), "{view}");
        assert!(
            view.contains("Additional potions 1 (P0 Cultist Potion)")
                && view.contains("HP +0 vs c0@4bf5ca407322 | ./p commit c1@4bf5ca407322"),
            "{view}"
        );
        let first = view.split("c1@4bf5ca407322:").next().unwrap();
        assert!(
            first.contains("Post-combat counters: Incense Burner [4] (next combat Turn 2)"),
            "{view}"
        );
        assert!(
            first.contains("Permanent card values (sum across copies): Ritual Dagger 50"),
            "{view}"
        );
        let second = view.split("c1@4bf5ca407322:").nth(1).unwrap();
        assert!(
            second.contains("Post-combat counters: Incense Burner [0] (next combat Turn 6)"),
            "{view}"
        );
        assert!(
            second.contains("Permanent card values (sum across copies): Ritual Dagger 45"),
            "{view}"
        );
        assert!(
            !view.contains("Kunai"),
            "combat-local counters are not carried resources: {view}"
        );
        // Recovered-gold differences must not duplicate the same potion/cards,
        // or hide a robbery while the settled HP happens to be identical.
        use sts_oracle_learning::content::potions::PotionId;
        let mut recovered = candidate.clone();
        recovered.outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 80 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 10 },
            TaskResearchPlanPendingRewardV1::Potion {
                potion_id: PotionId::FirePotion,
            },
            TaskResearchPlanPendingRewardV1::Card { cards: vec![] },
        ];
        let mut robbed = recovered.clone();
        robbed.outcome.pending_rewards[0] =
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 40 };
        robbed.outcome.persistent.unrecovered_stolen_gold = 40;
        robbed.outcome.combat_mugged = true;
        let render = |candidates: &[super::AgentCombatResearchCandidateV1], diagnostics| {
            super::research(
                "test-report",
                &root,
                candidates,
                &[],
                None,
                &[],
                super::CombatWitnessSatisfactionV1::FirstCompleteWin,
                None,
                diagnostics,
                sts_presentation::Locale::En,
            )
        };
        let theft = render(&[recovered.clone(), robbed.clone()], false);
        assert_eq!(theft.matches("Fire Potion").count(), 1, "{theft}");
        assert!(theft.contains("not yet claimed"), "{theft}");
        assert!(
            theft.contains("Gold after claims 202 (before claims 112)"),
            "{theft}"
        );
        assert!(
            theft.contains("Gold after claims 162 (before claims 112)"),
            "{theft}"
        );
        assert!(
            theft.contains("Theft: 40 gold unrecovered | enemy escaped"),
            "{theft}"
        );
        assert!(!theft.contains("Pending rewards:"), "{theft}");
        let theft_full = render(&[recovered.clone(), robbed.clone()], true);
        assert_eq!(theft_full.matches("Pending rewards:").count(), 2);
        assert!(theft_full.contains("80 recovered gold"));
        assert!(theft_full.contains("40 recovered gold"));
        // Non-money rewards that actually differ remain on their own lines.
        robbed
            .outcome
            .pending_rewards
            .push(TaskResearchPlanPendingRewardV1::EmeraldKey);
        let different = render(&[recovered, robbed], false);
        assert_eq!(different.matches("Pending rewards:").count(), 2);
        assert!(different.contains("emerald key"));
        let full = super::research(
            "test-report",
            &root,
            &[candidate, alternative],
            &[],
            None,
            &[],
            super::CombatWitnessSatisfactionV1::FirstCompleteWin,
            None,
            true,
            sts_presentation::Locale::En,
        );
        assert_eq!(
            full.matches("Candidate Gold after claims 208 (before claims 112)")
                .count(),
            1
        );
        assert_eq!(full.matches("Next actions: end").count(), 1);
    }

    #[test]
    fn whole_combat_report_identifies_owner_and_does_not_claim_survival_at_max_depth() {
        use sts_oracle_learning::agent::task::{
            TaskPrivilegedSupportAccessV1, TaskPrivilegedSupportProvenanceV1,
            TaskResearchPlanLaneAttemptV1, TaskResearchPlanWorkUnitV1,
        };
        let provenance = TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: super::WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1.into(),
            producer_revision: 1,
        };
        let progress = super::AgentCombatResearchBoundedProgressV1 {
            provenance: provenance.clone(),
            bounded_search_only: true,
            absence_means_unknown: true,
            max_player_turn: 6,
            recent_turn_survival_envelope: vec![],
            paths: vec![],
        };
        let lane = TaskResearchPlanLaneAttemptV1 {
            provenance,
            work_unit: TaskResearchPlanWorkUnitV1::WholeCombatTransitions,
            work_consumed: 1234,
            exit: super::TaskResearchPlanAttemptExitV1::WallTimeExhausted,
        };
        let view = super::research(
            "test-report",
            &root(),
            &[],
            &[progress.clone()],
            Some(8000),
            &[lane.clone()],
            super::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(0),
            Some(200000),
            false,
            sts_presentation::Locale::En,
        );
        assert!(
            view.starts_with("CombatSolver | from T2 HP 80/80"),
            "{view}"
        );
        assert!(
            view.contains("Unknown: no complete win found | time limit"),
            "{view}"
        );
        assert_eq!(view.lines().count(), 1);
        let mut work_lane = lane.clone();
        work_lane.exit = super::TaskResearchPlanAttemptExitV1::GenerationWorkExhausted;
        let work_view = super::research(
            "test-report",
            &root(),
            &[],
            &[],
            Some(8000),
            &[work_lane],
            super::CombatWitnessSatisfactionV1::FirstCompleteWin,
            Some(200000),
            false,
            sts_presentation::Locale::En,
        );
        assert!(work_view.contains("work limit"), "{work_view}");
        assert!(!work_view.contains("time limit"), "{work_view}");
        let full = super::research(
            "test-report",
            &root(),
            &[],
            &[progress],
            Some(8000),
            &[lane],
            super::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(0),
            Some(200000),
            true,
            sts_presentation::Locale::En,
        );
        assert!(full.contains("Deepest simulated turn: 7 (includes losing states)"));
        assert!(full.contains("Lane 0: work 1234 | time limit reached"));
        assert!(!full.contains("Explored through turn"));
    }

    #[test]
    fn finished_search_frontiers_are_not_presented_as_defeat_proofs() {
        use sts_oracle_learning::agent::task::TaskResearchPlanAttemptExitV1::*;
        for exit in [
            PortfolioCompleteWithoutWitness,
            SearchFrontierExhaustedWithoutWitness,
        ] {
            assert!(super::exit_description(exit, sts_presentation::Locale::En).contains("not"));
        }
        let view = super::research(
            "test-report",
            &root(),
            &[],
            &[],
            None,
            &[],
            super::CombatWitnessSatisfactionV1::FirstCompleteWin,
            None,
            false,
            sts_presentation::Locale::En,
        );
        assert!(view.contains("Unknown: no complete win found"));
    }

    #[test]
    fn final_relic_availability_does_not_hide_revives_or_count_combat_local_activations() {
        use sts_oracle_learning::content::relics::{RelicId, RelicState};

        let mut tail = RelicState::new(RelicId::LizardTail);
        let mut tea_set = RelicState::new(RelicId::AncientTeaSet);
        tea_set.used_up = true;
        let mut view = String::new();
        super::render_used_up_relics_at_finish(
            &mut view,
            &[tail.clone(), tea_set.clone()],
            None,
            sts_presentation::Locale::En,
        );
        assert!(view.is_empty());

        tail.used_up = true;
        tail.counter = -2;
        super::render_used_up_relics_at_finish(
            &mut view,
            &[tail, tea_set],
            None,
            sts_presentation::Locale::En,
        );
        assert_eq!(view, "  Used-up relics at finish: Lizard Tail\n");
    }

    #[test]
    fn compact_relic_spending_ignores_resources_already_used_at_search_start() {
        use sts_oracle_learning::content::relics::{RelicId, RelicState};

        let neow = RelicState {
            used_up: true,
            counter: 0,
            ..RelicState::new(RelicId::NeowsLament)
        };
        let tail = RelicState::new(RelicId::LizardTail);
        let root = [neow.clone(), tail.clone()];
        let mut view = String::new();
        super::render_used_up_relics_at_finish(
            &mut view,
            &root,
            Some(&root),
            sts_presentation::Locale::En,
        );
        assert!(view.is_empty());

        let spent_tail = RelicState {
            used_up: true,
            counter: -2,
            ..tail
        };
        let finish = [neow, spent_tail];
        super::render_used_up_relics_at_finish(
            &mut view,
            &finish,
            Some(&root),
            sts_presentation::Locale::En,
        );
        assert_eq!(view, "  Newly used-up relics: Lizard Tail\n");

        view.clear();
        super::render_used_up_relics_at_finish(
            &mut view,
            &finish,
            None,
            sts_presentation::Locale::En,
        );
        assert!(view.contains("Neow's Lament"), "{view}");
        assert!(view.contains("Lizard Tail"), "{view}");
    }

    #[test]
    fn offsetting_action_and_end_turn_changes_remain_visible_once() {
        use crate::combat_trace::*;
        let mut turn = RunPlayCombatTurnReviewV1 {
            turn: 0,
            first_step: 0,
            last_step: 1,
            actions: vec![],
            start: RunPlayCombatTurnStartV1 {
                player_hp: 50,
                player_max_hp: 75,
                player_block: 0,
                energy: 3,
                visible_incoming_damage: None,
                player_powers: vec![],
                hand: vec![],
                monsters: vec![],
            },
            end_turn: Some(RunPlayCombatEndTurnReviewV1 {
                visible_incoming_damage: None,
                player_block: 5,
                observed_player_hp_delta: 6,
            }),
            observed_player_hp_delta: 0,
            observed_player_max_hp_delta: 4,
            end_boundary: RunPlayCombatNextBoundaryV1::Strategic,
        };
        let mut view = String::new();
        super::render_turn_changes(&mut view, &turn);
        assert!(view.contains("Actions: HP -6"));
        assert!(view.contains("Incoming unknown | HP +6"));
        assert!(view.contains("Max HP +4"));
        assert_eq!(view.matches("HP -6").count(), 1);
        assert_eq!(view.matches("HP +6").count(), 1);
        turn.observed_player_hp_delta = -16;
        turn.end_turn.as_mut().unwrap().observed_player_hp_delta = -10;
        view.clear();
        super::render_turn_changes(&mut view, &turn);
        assert!(view.contains("Actions: HP -6"));
        assert!(view.contains("Incoming unknown | HP -10"));
        assert!(
            !view.contains("HP -16"),
            "do not print the overlapping total as another change"
        );
    }

    #[test]
    fn a_bounded_miss_stays_unknown() {
        let view = super::research(
            "test-report",
            &root(),
            &[],
            &[],
            Some(100),
            &[],
            super::CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(7),
            Some(64),
            true,
            sts_presentation::Locale::En,
        );
        assert!(view.contains("Unknown"));
        assert!(!view.contains("Unwinnable"));
        assert!(view.contains("Time limit: 100 ms"));
        assert!(view.contains("Work limit: 64 transitions"));
        assert!(view.contains("net HP loss <= 7"));
        assert!(view.contains("without adding a permanent Curse"));
    }

    #[test]
    fn opening_preview_stops_at_the_first_turn_boundary_and_marks_truncation() {
        let actions = [
            "play:PommelStrike+#0@0",
            "play:Metallicize+0#0",
            "end",
            "play:FiendFire+0#0@1",
        ]
        .map(str::to_string);
        assert_eq!(
            super::opening_actions(&actions, sts_presentation::Locale::En,),
            "play:PommelStrike+#0@0 -> play:Metallicize+0#0 -> end"
        );
        let long_opening = vec!["play:Anger+0#0@0".to_string(); 7];
        assert_eq!(
            super::opening_actions(&long_opening,sts_presentation::Locale::En,),
            "play:Anger+0#0@0 -> play:Anger+0#0@0 -> play:Anger+0#0@0 -> play:Anger+0#0@0 -> play:Anger+0#0@0 -> play:Anger+0#0@0 -> … +1 actions"
        );
    }
}

pub(crate) fn review(
    trace: &RunPlayCombatTraceV1,
    refs: &[String],
    turn: Option<u32>,
    compact: bool,
) -> Result<String, String> {
    if refs.len() != trace.candidates.len() {
        return Err("Combat review references and traces disagree".into());
    }
    if turn == Some(0) {
        return Err("Review turn starts at 1".into());
    }
    let mut out = String::new();
    for (reference, candidate) in refs.iter().zip(&trace.candidates) {
        writeln!(
            out,
            "{reference} | Settled HP {}/{} | Additional potions {} | Actions {}",
            candidate.final_hp,
            candidate.final_max_hp,
            candidate.total_potions_consumed,
            candidate.action_count
        )
        .unwrap();
        if let Some(first) = candidate.turn_reviews.first() {
            let mut hp = first.start.player_hp;
            let minimum = candidate
                .steps
                .iter()
                .map(|step| {
                    hp = hp.saturating_add(step.delta.player_hp);
                    hp
                })
                .chain(std::iter::once(first.start.player_hp))
                .min()
                .unwrap();
            writeln!(out, "Minimum observed HP: {minimum}").unwrap();
        }
        writeln!(out, "Block before End Turn excludes end-turn triggers; HP change includes their settlement and enemy actions.").unwrap();
        render_persistent_counters_at_finish(&mut out, &candidate.final_relics, Locale::En);
        render_used_up_relics_at_finish(&mut out, &candidate.final_relics, None, Locale::En);
        let turns = candidate
            .turn_reviews
            .iter()
            .filter(|r| turn.is_none_or(|t| r.turn.saturating_add(1) == t))
            .collect::<Vec<_>>();
        if turns.is_empty() {
            return Err(format!("{reference} has no requested turn"));
        }
        for r in turns
            .iter()
            .take(if turn.is_some() { usize::MAX } else { 8 })
        {
            let actions = candidate
                .steps
                .iter()
                .filter(|s| s.step >= r.first_step && s.step <= r.last_step)
                .map(|s| s.label.clone())
                .collect::<Vec<_>>();
            let shown = if turn.is_some() {
                actions.len()
            } else {
                actions.len().min(12)
            };
            if compact && turn.is_none() {
                use crate::decision::combat::power_labels;
                let end_context = r.end_turn.map_or_else(String::new, |end| {
                    format!(" | Block before End Turn {}", end.player_block)
                });
                let powers = if r.start.player_powers.is_empty() {
                    String::new()
                } else {
                    format!(
                        " | Powers: {}",
                        power_labels(&r.start.player_powers, Locale::En)
                    )
                };
                writeln!(
                    out,
                    "T{} | Start HP {}/{} | HP change {:+} | Start Block {}{}{} | {}{}",
                    r.turn.saturating_add(1),
                    r.start.player_hp,
                    r.start.player_max_hp,
                    r.observed_player_hp_delta,
                    r.start.player_block,
                    end_context,
                    powers,
                    actions[..shown].join(" · "),
                    if shown < actions.len() {
                        format!(" · … +{} more", actions.len() - shown)
                    } else {
                        String::new()
                    },
                )
                .unwrap();
                continue;
            }
            writeln!(
                out,
                "Turn {} | Start HP {}/{} | Start Energy {}",
                r.turn.saturating_add(1),
                r.start.player_hp,
                r.start.player_max_hp,
                r.start.energy
            )
            .unwrap();
            if turn.is_some() {
                render_turn_context(&mut out, &r.start);
            }
            writeln!(
                out,
                "  {}{}",
                actions[..shown].join(" -> "),
                if shown < actions.len() {
                    format!(" -> ... ({} more actions)", actions.len() - shown)
                } else {
                    String::new()
                }
            )
            .unwrap();
            render_turn_changes(&mut out, r);
        }
        if turn.is_none() {
            writeln!(
                out,
                "Expand a turn: review {reference} {}",
                turns[0].turn.saturating_add(1)
            )
            .unwrap();
            if let Some(next) = turns.get(8) {
                writeln!(
                    out,
                    "Continue: review {reference} {}",
                    next.turn.saturating_add(1)
                )
                .unwrap();
            }
        }
    }
    Ok(out)
}

fn render_turn_changes(out: &mut String, turn: &crate::combat_trace::RunPlayCombatTurnReviewV1) {
    let end_hp = turn.end_turn.map_or(0, |end| end.observed_player_hp_delta);
    let action_hp = turn.observed_player_hp_delta - end_hp;
    if action_hp != 0 {
        writeln!(out, "  Actions: HP {action_hp:+}").unwrap();
    }
    if let Some(end) = turn.end_turn {
        write!(
            out,
            "  End turn: Block {} | Incoming {}",
            end.player_block,
            end.visible_incoming_damage
                .map_or_else(|| "unknown".into(), |n| n.to_string()),
        )
        .unwrap();
        if end_hp != 0 {
            write!(out, " | HP {end_hp:+}").unwrap();
        }
        out.push('\n');
    }
    if turn.observed_player_max_hp_delta != 0 {
        writeln!(out, "  Max HP {:+}", turn.observed_player_max_hp_delta).unwrap();
    }
}

fn render_turn_context(out: &mut String, start: &crate::combat_trace::RunPlayCombatTurnStartV1) {
    use crate::decision::combat::{hand_card_label, intent_label, power_labels};
    use sts_oracle_learning::agent::information::state::CombatLearningEnemyIdentityV1;
    writeln!(
        out,
        "  Block {} | Incoming {}",
        start.player_block,
        start
            .visible_incoming_damage
            .map_or_else(|| "unknown".into(), |n| n.to_string())
    )
    .unwrap();
    writeln!(
        out,
        "  Hand: {}",
        start
            .hand
            .iter()
            .map(|c| hand_card_label(c.card_id, c.upgrades, c.effective_cost, Locale::En))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    if !start.player_powers.is_empty() {
        writeln!(
            out,
            "  Powers: {}",
            power_labels(&start.player_powers, Locale::En)
        )
        .unwrap();
    }
    for monster in &start.monsters {
        let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        write!(
            out,
            "  M{} {}: HP {}/{}",
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
                "Reviving".into()
            } else {
                intent_label(&monster.intent, Locale::En)
            }
        )
        .unwrap();
        if !monster.powers.is_empty() {
            writeln!(
                out,
                "    Powers: {}",
                power_labels(&monster.powers, Locale::En)
            )
            .unwrap();
        }
    }
}

pub(crate) fn selected_action_label(
    observed: &sts_oracle_learning::eval::run_control::ExactRunPlayProbeStepV1,
    card: Option<&crate::combat_trace::RunPlayPlayedCardV1>,
    potion: Option<&crate::combat_trace::RunPlayPotionUseV1>,
) -> Result<String, String> {
    use sts_oracle_learning::agent::information::action::{
        PublicCombatAtomicActionV1 as Atomic, PublicCombatIndexedChoiceCandidateV1 as Indexed,
        PublicCombatSelectionDomainCandidateV1 as Domain,
    };
    use sts_oracle_learning::agent::task::TaskPublicCandidateV1 as Candidate;
    if let Some(card) = card {
        return Ok(format!(
            "{}{}",
            format_card(card.card_id, card.upgrades),
            card.target_slot
                .map_or_else(String::new, |s| format!(" @M{s}"))
        ));
    }
    if let Some(potion) = potion {
        return Ok(format!(
            "P{} {}{}",
            potion.slot,
            gameplay_display_label_v1(potion.potion_id),
            potion
                .target_slot
                .map_or_else(String::new, |s| format!(" @M{s}"))
        ));
    }
    Ok(match &observed.selected_candidate {
        Candidate::CombatAtomic {
            action: Atomic::EndTurn,
        } => "End turn".into(),
        Candidate::CombatAtomic {
            action: Atomic::Proceed,
        } => "Proceed".into(),
        Candidate::CombatAtomic {
            action: Atomic::Cancel,
        } => "Cancel".into(),
        Candidate::CombatAtomic {
            action: Atomic::DiscardPotion { potion_index },
        } => format!("Discard P{potion_index}"),
        Candidate::CombatIndexedChoice { candidate, .. } => match candidate {
            Indexed::Card { card_id, upgrades } => {
                format!("Choose {}", format_card(*card_id, *upgrades))
            }
            Indexed::Stance { stance } => format!("Choose {stance:?}"),
        },
        Candidate::CombatSelectionFamily { family } => {
            let selected = observed
                .choice
                .ordinal_path
                .iter()
                .skip(1)
                .map(|index| {
                    let item = family
                        .domain
                        .get(*index)
                        .ok_or("Review selection outside its domain")?;
                    let (card, upgrades) = match item {
                        Domain::Card {
                            card_id, upgrades, ..
                        } => (*card_id, upgrades.unwrap_or(0)),
                        Domain::Scry { card_id, .. } => (*card_id, 0),
                    };
                    Ok(card.map_or_else(|| "Unknown card".into(), |c| format_card(c, upgrades)))
                })
                .collect::<Result<Vec<_>, String>>()?;
            if selected.is_empty() {
                "Select none".into()
            } else {
                format!("Select {}", selected.join(", "))
            }
        }
        _ => return Err("Review action lacks a typed display projection".into()),
    })
}
