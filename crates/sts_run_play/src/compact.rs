use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use sts_presentation::Locale;

use sts_oracle_learning::agent::information::action::PublicCombatAtomicActionV1;
use sts_oracle_learning::agent::task::{
    GameOutcomeV1, TaskEpisodeFailureV1, TaskEpisodeOutcomeV1, TaskPublicCandidateV1,
    TaskPublicDecisionV1, TaskPublicObservationV1, TaskStrategicActionV1, TaskStrategicCardV1,
};
use sts_oracle_learning::ai::planner_core::{
    PlannerDecisionContext, PlannerShopInventoryV1, PlannerShopPurchaseStatusV1,
};
use sts_oracle_learning::content::identity::gameplay_display_label_v1;
use sts_oracle_learning::content::{cards, relics};
use sts_oracle_learning::eval::run_control::{ExactRunPlayProbeStepV1, RunActionResultChangeV1};

use crate::combat_explain::CombatLineExplanationStateV1;
use crate::decision::RunPlayDecisionReadingV1;
use crate::driver::{RunPlayComparisonV1, RunPlayControllerV1};
use crate::model::{RunPlayEvidenceIdentityV1, RunPlayHeadV1, RunPlayPositionV1};
use crate::render::short_id;

#[cfg(test)]
#[path = "compact/inventory_tests.rs"]
mod inventory_tests;
#[cfg(test)]
#[path = "compact/status_tests.rs"]
mod status_tests;
/// Compact, typed resume context for humans and task handoffs. `show` remains
/// the full evidence surface; this projection deliberately favors one-screen
/// orientation without reading archive JSON or reconstructing private state.
pub fn render_status_v1(head: &RunPlayHeadV1, locale: Locale) -> Result<String, String> {
    if let RunPlayPositionV1::Terminal { outcome, .. } = &head.position {
        return Ok(compact_text(
            locale,
            "status.terminal",
            &[("outcome", &render_outcome_compact(outcome, locale))],
        ));
    }
    // The normal reading already owns human labels for combat, rewards, and
    // events. Keep status on that surface; `show` retains the exact diagnostic
    // ids, source proof and engine candidate dump when they are needed.
    let decision = head
        .current_decision
        .as_ref()
        .ok_or_else(|| "run-play status has no cached decision".to_string())?
        .public_decision_v1()?;
    let reading = crate::decision::read_compact_decision_v1(
        &decision,
        Some(crate::decision::decision_source(head)?),
        false,
        None,
        locale,
    )?;
    let mut output =
        if let TaskPublicObservationV1::Strategic { observation } = &decision.observation {
            if let PlannerDecisionContext::Shop { inventory, .. } = &observation.context {
                render_shop_status_v1(&decision, &reading, observation, inventory, None, locale)?
            } else {
                reading.view
            }
        } else {
            reading.view
        };
    let compatibility =
        RunPlayEvidenceIdentityV1::compatibility_label_v1(head.evidence_identity.as_ref());
    if !head.scratch && compatibility != "current" {
        writeln!(
            output,
            "{}",
            compact_text(
                locale,
                "status.savedDecision",
                &[("compatibility", compatibility)]
            )
        )
        .expect("string write");
    }
    Ok(output)
}

pub(crate) fn compact_text(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("compact", key, parameters)
        .expect("compact presentation message")
}

fn position_site_label_v1(
    site: sts_oracle_learning::ai::planner_core::PlannerDecisionSite,
    locale: Locale,
) -> String {
    use sts_oracle_learning::ai::planner_core::PlannerDecisionSite;
    let key = match site {
        PlannerDecisionSite::Map => "site.Map",
        PlannerDecisionSite::CardReward => "site.CardReward",
        PlannerDecisionSite::Neow => "site.Neow",
        PlannerDecisionSite::Event => "site.Event",
        PlannerDecisionSite::Shop => "site.Shop",
        PlannerDecisionSite::Campfire => "site.Campfire",
        PlannerDecisionSite::BossRelic => "site.BossRelic",
        PlannerDecisionSite::Reward => "site.Reward",
        PlannerDecisionSite::RunChoice => "site.RunChoice",
        PlannerDecisionSite::Treasure => "site.Treasure",
        PlannerDecisionSite::RunTransition => "site.RunTransition",
    };
    compact_text(locale, key, &[])
}

pub fn render_back_targets_v1(
    paths: &crate::storage::RunPlayPathsV1,
    locale: Locale,
) -> Result<String, String> {
    let head = paths.load_head()?;
    if !head.scratch {
        return Err(compact_text(locale, "returns.scratchRequired", &[]));
    }
    let floors = head
        .scratch_floor_ends
        .iter()
        .map(|end| end.floor.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = compact_text(
        locale,
        "returns.floors",
        &[(
            "floors",
            &if floors.is_empty() {
                compact_text(locale, "none", &[])
            } else {
                floors
            },
        )],
    );
    let recent = paths.recent_scratch_heads_v1(&head, 12)?;
    if !recent.is_empty() {
        out.push_str(&compact_text(locale, "returns.recent", &[]));
        for (index, retained) in recent.iter().enumerate() {
            let decision = retained
                .current_decision
                .as_ref()
                .ok_or("scratch return point has no public decision")?;
            writeln!(
                out,
                "  back {} | {}",
                index + 1,
                render_position_resources_v1(&decision.observation, locale)
            )
            .expect("string write");
        }
    }
    out.push_str(&render_mark_targets_v1(paths, &head, locale)?);
    out.push_str(&compact_text(locale, "returns.retention", &[]));
    Ok(out)
}

pub fn render_mark_targets_v1(
    paths: &crate::storage::RunPlayPathsV1,
    head: &RunPlayHeadV1,
    locale: Locale,
) -> Result<String, String> {
    if !head.scratch {
        return Err(compact_text(locale, "returns.marksScratchRequired", &[]));
    }
    if head.scratch_marks.is_empty() {
        return Ok(compact_text(locale, "returns.noMarks", &[]));
    }
    let mut out = compact_text(locale, "returns.marks", &[]);
    for mark in &head.scratch_marks {
        let retained = paths.load_scratch_mark_head_v1(head, &mark.name)?;
        let decision = retained
            .current_decision
            .as_ref()
            .ok_or("saved mark has no public decision")?;
        writeln!(
            out,
            "  {} | {}",
            mark.name,
            render_position_resources_v1(&decision.observation, locale)
        )
        .expect("string write");
    }
    out.push_str(&compact_text(locale, "returns.savedFacts", &[]));
    Ok(out)
}

pub(crate) fn render_position_resources_v1(
    observation: &TaskPublicObservationV1,
    locale: Locale,
) -> String {
    let (location, hp, max_hp, gold, held_potions) = match observation {
        TaskPublicObservationV1::Strategic { observation } => (
            compact_text(
                locale,
                "position.location",
                &[
                    ("act", &observation.run.act.to_string()),
                    ("floor", &observation.run.floor.to_string()),
                    (
                        "site",
                        &position_site_label_v1(observation.decision_site, locale),
                    ),
                ],
            ),
            observation.run.current_hp,
            observation.run.max_hp,
            observation.run.gold,
            observation
                .potions
                .iter()
                .filter_map(|slot| {
                    slot.potion
                        .as_ref()
                        .map(|potion| locale.entity("potion", potion.potion.stable_id()))
                })
                .collect::<Vec<_>>(),
        ),
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            let location = match run_context {
                sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Available {
                    act, floor, encounter_id, ..
                } => {
                    let encounter = encounter_id.as_ref().map_or_else(
                        || compact_text(locale, "position.combat", &[]),
                        |encounter| locale.entity("encounter", encounter.stable_id()).to_owned(),
                    );
                    compact_text(locale, "position.location", &[
                        ("act", &act.to_string()), ("floor", &floor.to_string()), ("site", &encounter),
                    ])
                }
                sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Unavailable { .. } => compact_text(locale, "position.unavailableCombat", &[]),
            };
            (
                format!(
                    "{location} | T{}",
                    observation.turn.turn_count.saturating_add(1)
                ),
                observation.player.hp,
                observation.player.max_hp,
                observation.player.gold,
                observation
                    .potions
                    .iter()
                    .filter_map(|slot| {
                        slot.as_ref()
                            .map(|potion| locale.entity("potion", potion.potion_id.stable_id()))
                    })
                    .collect::<Vec<_>>(),
            )
        }
    };
    compact_text(
        locale,
        "position.resources",
        &[
            ("location", &location),
            ("hp", &hp.to_string()),
            ("maxHp", &max_hp.to_string()),
            ("gold", &gold.to_string()),
            (
                "potions",
                &if held_potions.is_empty() {
                    compact_text(locale, "none", &[])
                } else {
                    held_potions.join(", ")
                },
            ),
        ],
    )
}

#[cfg(test)]
mod return_point_tests {
    use super::*;
    use sts_oracle_learning::agent::task::{TaskStrategicPotionSlotV1, TaskStrategicPotionV1};
    use sts_oracle_learning::content::potions::PotionId;

    #[test]
    fn saved_combat_turn_matches_the_player_visible_turn() {
        use sts_oracle_learning::agent::information::run::{
            PublicCombatRunContextGapV1, PublicCombatRunContextV1,
        };
        let mut combat = sts_oracle_learning::test_support::blank_test_combat();
        combat.turn.turn_count = 0;
        let public = TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Unavailable {
                reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
            },
            observation: sts_oracle_learning::agent::information::state::public_combat_state_v1(
                &combat,
            ),
        };
        assert!(render_position_resources_v1(&public, Locale::En).contains("| T1 |"));
        let zh = render_position_resources_v1(&public, Locale::ZhCn);
        assert!(zh.contains("战斗（幕层信息不可用） | T1 | 生命"), "{zh}");
    }

    #[test]
    fn same_hp_return_points_distinguish_their_actual_potion_reserves() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = crate::storage::RunPlayPathsV1::new(directory.0.join("return-facts"));
        let initial = crate::driver::create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            sts_oracle_learning::agent::task::TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            sts_oracle_learning::eval::run_control::CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        let mut public = initial.head.current_decision.unwrap().observation;
        let TaskPublicObservationV1::Strategic { observation } = &mut public else {
            panic!("natural start is strategic");
        };
        observation.run.current_hp = 10;
        observation.run.max_hp = 85;
        observation.potions.clear();
        let empty = render_position_resources_v1(&public, Locale::En);
        let TaskPublicObservationV1::Strategic { observation } = &mut public else {
            unreachable!();
        };
        observation.potions = [
            PotionId::EnergyPotion,
            PotionId::ExplosivePotion,
            PotionId::StrengthPotion,
        ]
        .into_iter()
        .enumerate()
        .map(|(slot, potion)| TaskStrategicPotionSlotV1 {
            slot,
            potion: Some(TaskStrategicPotionV1 {
                potion,
                can_use: true,
                can_discard: true,
                requires_target: false,
            }),
        })
        .collect();
        let supplied = render_position_resources_v1(&public, Locale::En);
        assert!(empty.contains("HP 10/85") && supplied.contains("HP 10/85"));
        assert!(empty.ends_with("Potions: none"));
        assert!(supplied.ends_with("Potions: Energy Potion, Explosive Potion, Strength Potion"));
        let chinese = render_position_resources_v1(&public, Locale::ZhCn);
        assert!(chinese.contains("生命 10/85"));
        assert!(
            chinese.ends_with("药水：能量药水, 爆炸药水, 力量药水"),
            "{chinese}"
        );
        assert_ne!(empty, supplied);
    }

    #[test]
    fn return_lists_share_saved_facts_without_changing_their_source_or_mark_names() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = crate::storage::RunPlayPathsV1::new(directory.0.join("return-locales"));
        crate::driver::create_scratch_natural_session_v1(
            &paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility:
                    sts_oracle_learning::content::events::neow::NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            sts_oracle_learning::agent::task::TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            sts_oracle_learning::eval::run_control::CombatResearchPlanOptionsV1::default(),
        )
        .unwrap();
        let initial = paths.load_head().unwrap();
        assert_eq!(
            render_mark_targets_v1(&paths, &initial, Locale::ZhCn).unwrap(),
            "命名返回点：无。\n"
        );
        crate::driver::save_scratch_mark_v1(&paths, "start").unwrap();
        let head = paths.load_head().unwrap();
        let points = crate::return_points::return_points_v2(&paths).unwrap();
        for locale in [Locale::En, Locale::ZhCn] {
            let marks = render_mark_targets_v1(&paths, &head, locale).unwrap();
            let back = render_back_targets_v1(&paths, locale).unwrap();
            assert!(marks.contains("  start | "));
            assert!(marks.contains("back mark <name>"));
            assert!(back.contains("back floor <number>"));
            assert!(back.contains(&marks));
            assert!(
                back.contains(if locale == Locale::En {
                    "HP 68/75"
                } else {
                    "生命 68/75"
                }),
                "{back}"
            );
            assert_eq!(paths.load_head().unwrap(), head);
            assert_eq!(
                crate::return_points::return_points_v2(&paths).unwrap(),
                points
            );
        }
    }
}

/// Expand the run inventory on demand, including while the current decision is combat.
pub fn render_inventory_v1(
    controller: &mut RunPlayControllerV1,
    effects: bool,
    locale: Locale,
) -> Result<String, String> {
    let decision = controller
        .head()
        .current_decision
        .as_ref()
        .ok_or_else(|| "run-play inventory requires a decision head".to_string())?
        .public_decision_v1()?;
    let combat = if matches!(
        &decision.observation,
        TaskPublicObservationV1::Combat { .. }
    ) {
        Some(controller.current_combat_position_for_lab_v1()?)
    } else {
        None
    };
    render_inventory_decision_v1(decision, effects, combat, locale)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryCardReadingV1 {
    pub card: cards::CardId,
    pub upgrades: u8,
    pub count: usize,
    pub view: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryRelicReadingV1 {
    pub relic: relics::RelicId,
    pub view: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryReadingV1 {
    pub view: String,
    pub cards: Vec<InventoryCardReadingV1>,
    pub relics: Vec<InventoryRelicReadingV1>,
    pub resources: String,
}

pub(crate) fn render_inventory_decision_v1(
    decision: TaskPublicDecisionV1,
    effects: bool,
    combat: Option<sts_oracle_learning::sim::combat::CombatPosition>,
    locale: Locale,
) -> Result<String, String> {
    Ok(inventory_reading_decision_v1(decision, effects, combat, locale)?.view)
}

pub(crate) fn inventory_reading_decision_v1(
    decision: TaskPublicDecisionV1,
    effects: bool,
    combat: Option<sts_oracle_learning::sim::combat::CombatPosition>,
    locale: Locale,
) -> Result<InventoryReadingV1, String> {
    let potion_slots: Vec<_> = match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => observation
            .potions
            .iter()
            .map(|slot| (slot.slot, slot.potion.as_ref().map(|potion| potion.potion)))
            .collect(),
        TaskPublicObservationV1::Combat { observation, .. } => observation
            .potions
            .iter()
            .enumerate()
            .map(|(slot, potion)| (slot, potion.as_ref().map(|potion| potion.potion_id)))
            .collect(),
    };
    let (cards, relics) = match decision.observation {
        TaskPublicObservationV1::Strategic { observation } => (
            observation.cards,
            observation
                .relics
                .iter()
                .map(|relic| {
                    (
                        relic_label_v1(relic.relic, relic.counter, relic.used_up, locale),
                        relic.relic,
                    )
                })
                .collect::<Vec<_>>(),
        ),
        TaskPublicObservationV1::Combat { .. } => {
            let position = combat.ok_or("combat inventory requires its exact position")?;
            let bottle_bindings = position
                .combat
                .entities
                .player
                .relics
                .iter()
                .filter(|relic| {
                    matches!(
                        relic.id,
                        relics::RelicId::BottledFlame
                            | relics::RelicId::BottledLightning
                            | relics::RelicId::BottledTornado
                    )
                })
                .map(|relic| {
                    relics::persistent_state::persistent_relic_bound_card_lineage_v1(relic)
                        .map(|lineage| (relic.id, lineage))
                })
                .collect::<Result<Vec<_>, String>>()?;
            (
                position
                    .combat
                    .meta
                    .master_deck_snapshot
                    .iter()
                    .enumerate()
                    .map(|(ordinal, card)| TaskStrategicCardV1 {
                        ordinal,
                        card: card.id,
                        upgrades: card.upgrades,
                        misc_value: card.misc_value,
                        base_damage_override: card.base_damage_override,
                        base_block_override: card.base_block_override,
                        cost_modifier: card.cost_modifier,
                        bottled_by: bottle_bindings.iter().find_map(|(relic, lineage)| {
                            (*lineage == Some(card.lineage)).then_some(*relic)
                        }),
                    })
                    .collect::<Vec<_>>(),
                position
                    .combat
                    .entities
                    .player
                    .relics
                    .iter()
                    .map(|relic| {
                        (
                            relic_label_v1(relic.id, relic.counter, relic.used_up, locale),
                            relic.id,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }
    };
    let message = |key, parameters: &[(&str, &str)]| {
        locale
            .text("inventory", key, parameters)
            .expect("inventory message")
    };
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        message("deck", &[("count", &cards.len().to_string())])
    )
    .expect("string write");
    if effects {
        writeln!(output, "{}", message("permanent", &[])).expect("string write");
    }
    let mut name_ids = BTreeMap::<_, std::collections::BTreeSet<_>>::new();
    for card in &cards {
        name_ids
            .entry(locale.entity("card", card.card.stable_id()))
            .or_default()
            .insert(card.card.stable_id());
    }
    let groups = inventory_card_groups_v1(&cards);
    let mut card_readings = Vec::new();
    for &(count, card) in &groups {
        let id = card.card;
        let upgrades = card.upgrades;
        let start = output.len();
        let mut label = permanent_card_label_v1(&card, locale);
        if name_ids[locale.entity("card", id.stable_id())].len() > 1 {
            write!(label, " [{}]", id.stable_id()).expect("string write");
        }
        if count == 1 {
            write!(output, "  {label}").expect("string write");
        } else {
            write!(output, "  {count}× {label}").expect("string write");
        }
        if effects {
            write!(
                output,
                " | {}",
                crate::mechanics_info::card_attributes(id, upgrades, card.cost_modifier, locale)
            )
            .expect("string write");
        }
        writeln!(output).expect("string write");
        if effects {
            let blueprint = sts_oracle_learning::runtime::combat::CardBlueprint {
                id,
                upgrades,
                misc_value: card.misc_value,
                base_damage_override: card.base_damage_override,
                base_block_override: card.base_block_override,
                cost_modifier: card.cost_modifier,
                cost_for_turn: None,
                free_to_play_once: false,
            };
            if let Some(text) = cards::mechanics::original_card_blueprint_text(&blueprint, locale)?
            {
                for line in text.lines() {
                    writeln!(output, "    {line}").expect("string write");
                }
            }
            writeln!(
                output,
                "    {}",
                match cards::mechanics::get(id).map(|rule| rule.explanation(locale)) {
                    Some(effect) => message("supplement", &[("effect", &effect)]),
                    None => message("unknown", &[]),
                }
            )
            .expect("string write");
            // These are definition-level requests, distinct from the permanent
            // instance face above. Keep the complete existing rules together.
            crate::mechanics_info::write_card_fixed_requests(&mut output, id, upgrades, locale);
        }
        card_readings.push(InventoryCardReadingV1 {
            card: id,
            upgrades,
            count,
            view: output[start..].to_owned(),
        });
    }
    if effects {
        let mut links = String::new();
        crate::mechanics_info::write_owned_card_links(
            &mut links,
            groups
                .iter()
                .map(|&(count, card)| (card.card, card.upgrades, card.cost_modifier, count)),
            locale,
        );
        if !links.is_empty() {
            output.push_str(&links);
        }
    }
    let resources_start = output.len();
    let mut relic_readings = Vec::new();
    writeln!(
        output,
        "{}",
        message("relics", &[("count", &relics.len().to_string())])
    )
    .expect("string write");
    for (relic, id) in relics {
        let start = output.len();
        writeln!(output, "  {relic}").expect("string write");
        if effects {
            writeln!(
                output,
                "    {}",
                match relics::mechanics::get(id).map(|rule| rule.explanation(locale)) {
                    Some(effect) => message("supplement", &[("effect", &effect)]),
                    None => message("unknown", &[]),
                }
            )
            .expect("string write");
        }
        relic_readings.push(InventoryRelicReadingV1 {
            relic: id,
            view: output[start..].to_owned(),
        });
    }
    if effects {
        writeln!(
            output,
            "{}",
            message("potions", &[("count", &potion_slots.len().to_string())])
        )
        .expect("string write");
        for (slot, potion) in potion_slots {
            if let Some(id) = potion {
                writeln!(
                    output,
                    "  P{slot} {}",
                    locale.entity("potion", id.stable_id())
                )
                .expect("string write");
                writeln!(
                    output,
                    "    {}",
                    locale
                        .entry("game", &format!("potionRule.{}", id.stable_id()))
                        .unwrap_or_else(|| locale.entry("inventory", "unknown").unwrap())
                )
                .expect("string write");
            } else {
                writeln!(output, "  P{slot} {}", message("empty", &[])).expect("string write");
            }
        }
    }
    Ok(InventoryReadingV1 {
        resources: output[resources_start..].to_owned(),
        view: output,
        cards: card_readings,
        relics: relic_readings,
    })
}

/// Permanent instances group by identity and all public instance modifiers.
/// The result and its order do not depend on translated display names.
type PermanentCardVariantKeyV1 = (
    &'static str,
    u8,
    i32,
    Option<i32>,
    Option<i32>,
    i8,
    Option<&'static str>,
);

fn permanent_card_variant_key_v1(value: &TaskStrategicCardV1) -> PermanentCardVariantKeyV1 {
    // Ordinals locate a card on one page; they are not permanent identity.
    // Destructure exhaustively so a new public field needs an explicit policy.
    let TaskStrategicCardV1 {
        ordinal: _,
        card,
        upgrades,
        misc_value,
        base_damage_override,
        base_block_override,
        cost_modifier,
        bottled_by,
    } = *value;
    (
        card.stable_id(),
        upgrades,
        misc_value,
        base_damage_override,
        base_block_override,
        cost_modifier,
        bottled_by.map(|id| id.stable_id()),
    )
}

pub(crate) fn inventory_card_groups_v1(
    cards: &[TaskStrategicCardV1],
) -> Vec<(usize, TaskStrategicCardV1)> {
    let mut counts = BTreeMap::new();
    for &card in cards {
        let key = permanent_card_variant_key_v1(&card);
        counts.entry(key).or_insert((0, card)).0 += 1;
    }
    let mut groups = counts.into_values().collect::<Vec<_>>();
    groups.sort_by_key(|(_, card)| {
        (
            gameplay_display_label_v1(card.card),
            card.card.stable_id(),
            card.upgrades,
        )
    });
    groups
}

pub(crate) fn permanent_card_label_v1(card: &TaskStrategicCardV1, locale: Locale) -> String {
    let mut label = locale.card_name(card.card.stable_id(), card.upgrades);
    let modifier = |key, amount: String| {
        locale
            .text("inventory", key, &[("amount", &amount)])
            .expect("card modifier message")
    };
    if card.misc_value != 0 {
        write!(
            label,
            " ({})",
            modifier("misc", card.misc_value.to_string())
        )
        .expect("string write");
    }
    if card.cost_modifier != 0 {
        write!(
            label,
            " ({})",
            modifier("cost", format!("{:+}", card.cost_modifier))
        )
        .expect("string write");
    }
    if let Some(damage) = card.base_damage_override {
        write!(label, " ({})", modifier("damage", damage.to_string())).expect("string write");
    }
    if let Some(block) = card.base_block_override {
        write!(label, " ({})", modifier("block", block.to_string())).expect("string write");
    }
    if let Some(relic) = card.bottled_by {
        write!(label, " [{}]", locale.entity("relic", relic.stable_id())).expect("string write");
    }
    label
}

pub(crate) fn relic_label_v1(
    relic: relics::RelicId,
    counter: i32,
    used_up: bool,
    locale: Locale,
) -> String {
    let mut label = locale.entity("relic", relic.stable_id()).to_string();
    if let Some(state) = relics::mechanics::trigger_state_v1(relic, used_up) {
        let status = state.description(locale);
        write!(label, " ({status})").expect("string write");
    }
    if counter >= 0 {
        write!(label, " [{counter}]").expect("string write");
    }
    if relic == relics::RelicId::HappyFlower {
        let mut next = relics::RelicState::new(relic);
        next.counter = counter;
        let bonus: i32 = relics::happy_flower::HappyFlower::at_turn_start(&mut next)
            .iter()
            .map(|action| match &action.action {
                sts_oracle_learning::runtime::action::Action::GainEnergy { amount } => *amount,
                _ => 0,
            })
            .sum();
        if bonus > 0 {
            let status = locale
                .text(
                    "inventory",
                    "nextTurnEnergy",
                    &[("amount", &bonus.to_string())],
                )
                .expect("relic energy message");
            write!(label, " ({status})").expect("string write");
        }
    }
    label
}

pub(crate) fn relic_summary_v1(
    relics: impl IntoIterator<Item = (relics::RelicId, i32, bool)>,
    locale: Locale,
) -> String {
    let relics = relics.into_iter().collect::<Vec<_>>();
    let live = relics
        .iter()
        .filter(|(id, counter, used_up)| {
            *counter >= 0 || relics::mechanics::trigger_state_v1(*id, *used_up).is_some()
        })
        .map(|(id, counter, used_up)| relic_label_v1(*id, *counter, *used_up, locale))
        .collect::<Vec<_>>();
    compact_text(
        locale,
        if live.is_empty() {
            "status.relicCount"
        } else {
            "status.relicStates"
        },
        &[
            ("count", &relics.len().to_string()),
            ("states", &live.join(", ")),
        ],
    )
}

pub(crate) fn render_reading_status_v1(
    decision: &TaskPublicDecisionV1,
    reading: &RunPlayDecisionReadingV1,
    preview: bool,
    deck_change: Option<&str>,
    locale: Locale,
) -> Result<String, String> {
    if let TaskPublicObservationV1::Strategic { observation } = &decision.observation {
        if let PlannerDecisionContext::Shop { inventory, .. } = &observation.context {
            return render_shop_status_v1(
                decision,
                reading,
                observation,
                inventory,
                deck_change,
                locale,
            );
        }
    }
    Ok(crate::decision::read_compact_decision_v1(
        decision,
        reading.source.clone(),
        preview,
        deck_change,
        locale,
    )?
    .view)
}

fn shop_action_v1(
    decision: &TaskPublicDecisionV1,
    reading: &RunPlayDecisionReadingV1,
    expected: &TaskStrategicActionV1,
    status: &PlannerShopPurchaseStatusV1,
) -> Result<Option<String>, String> {
    let matches = reading
        .options
        .iter()
        .filter(|option| {
            matches!(&option.candidate, TaskPublicCandidateV1::Strategic { action } if action == expected)
        })
        .collect::<Vec<_>>();
    match (status, matches.as_slice()) {
        (PlannerShopPurchaseStatusV1::Purchasable, [option]) => {
            crate::selector::semantic_action_for_choice_v1(decision, &option.choice).map(Some)
        }
        (PlannerShopPurchaseStatusV1::Blocked { .. }, []) => Ok(None),
        _ => Err("Shop inventory and executable choices disagree".to_owned()),
    }
}

fn render_shop_offer_v1(
    output: &mut String,
    command: Option<&str>,
    name: &str,
    price: i32,
    detail: &str,
    status: &PlannerShopPurchaseStatusV1,
    locale: Locale,
) {
    if let Some(command) = command {
        writeln!(output, "  {command}  |  {price}g · {name} · {detail}").expect("string write");
    } else {
        writeln!(
            output,
            "  {name} {price}g{} · {detail}",
            crate::decision::purchase_suffix(status, locale)
        )
        .expect("string write");
    }
}

fn render_shop_status_v1(
    decision: &TaskPublicDecisionV1,
    reading: &RunPlayDecisionReadingV1,
    observation: &sts_oracle_learning::agent::task::TaskStrategicObservationV1,
    inventory: &PlannerShopInventoryV1,
    deck_change: Option<&str>,
    locale: Locale,
) -> Result<String, String> {
    let run = &observation.run;
    let mut output = compact_text(
        locale,
        "shop.header",
        &[
            ("act", &run.act.to_string()),
            ("floor", &run.floor.to_string()),
            ("ascension", &run.ascension_level.to_string()),
            ("hp", &run.current_hp.to_string()),
            ("maxHp", &run.max_hp.to_string()),
            ("gold", &run.gold.to_string()),
        ],
    );
    writeln!(
        output,
        "{}",
        compact_text(
            locale,
            "shop.potions",
            &[(
                "potions",
                &observation
                    .potions
                    .iter()
                    .map(|slot| format!(
                        "P{} {}",
                        slot.slot,
                        slot.potion.as_ref().map_or_else(
                            || locale.entry("decision", "empty").unwrap(),
                            |potion| locale.entity("potion", potion.potion.stable_id())
                        )
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )]
        )
    )
    .expect("string write");
    writeln!(
        output,
        "{}",
        relic_summary_v1(
            observation
                .relics
                .iter()
                .map(|r| (r.relic, r.counter, r.used_up)),
            locale
        )
    )
    .expect("string write");
    if let Some(boss) = observation.public_map.boss {
        writeln!(
            output,
            "{}",
            compact_text(
                locale,
                "shop.boss",
                &[("boss", locale.entity("encounter", boss.stable_id()))]
            )
        )
        .expect("string write");
    }

    writeln!(output, "{}", compact_text(locale, "shop.cards", &[])).expect("string write");
    for offer in &inventory.cards {
        let action = TaskStrategicActionV1::BuyCard {
            shop_slot: offer.slot,
            card: offer.card,
            upgrades: offer.upgrades,
            price: offer.price,
        };
        let command = shop_action_v1(decision, reading, &action, &offer.purchase_status)?;
        let definition = cards::get_card_definition(offer.card);
        let cost = cards::upgraded_base_cost_override_for(offer.card, offer.upgrades)
            .map(i32::from)
            .unwrap_or(i32::from(definition.cost));
        let cost = match cost {
            -1 => "X".to_owned(),
            -2 => compact_text(locale, "shop.unplayable", &[]),
            cost => format!("{cost}c"),
        };
        let detail = format!(
            "{cost} {}: {}",
            locale
                .entry(
                    "game",
                    &format!(
                        "cardType.{}",
                        match definition.card_type {
                            cards::CardType::Attack => "attack",
                            cards::CardType::Skill => "skill",
                            cards::CardType::Power => "power",
                            cards::CardType::Curse => "curse",
                            cards::CardType::Status => "status",
                        }
                    )
                )
                .expect("card type name"),
            cards::mechanics::get(offer.card)
                .map(|rule| rule.explanation(locale))
                .unwrap_or_else(|| compact_text(locale, "shop.unknown", &[]))
        );
        render_shop_offer_v1(
            &mut output,
            command.as_deref(),
            &locale.card_name(offer.card.stable_id(), offer.upgrades),
            offer.price,
            &detail,
            &offer.purchase_status,
            locale,
        );
    }
    writeln!(output, "{}", compact_text(locale, "shop.relics", &[])).expect("string write");
    for offer in &inventory.relics {
        let action = TaskStrategicActionV1::BuyRelic {
            shop_slot: offer.slot,
            relic: offer.relic,
            price: offer.price,
        };
        let command = shop_action_v1(decision, reading, &action, &offer.purchase_status)?;
        render_shop_offer_v1(
            &mut output,
            command.as_deref(),
            locale.entity("relic", offer.relic.stable_id()),
            offer.price,
            &relics::mechanics::get(offer.relic)
                .map(|rule| rule.explanation(locale))
                .unwrap_or_else(|| compact_text(locale, "shop.unknown", &[])),
            &offer.purchase_status,
            locale,
        );
    }
    writeln!(output, "{}", compact_text(locale, "shop.bottles", &[])).expect("string write");
    for offer in &inventory.potions {
        let action = TaskStrategicActionV1::BuyPotion {
            shop_slot: offer.slot,
            potion: offer.potion,
            price: offer.price,
        };
        let command = shop_action_v1(decision, reading, &action, &offer.purchase_status)?;
        render_shop_offer_v1(
            &mut output,
            command.as_deref(),
            locale.entity("potion", offer.potion.stable_id()),
            offer.price,
            &locale
                .entry("game", &format!("potionRule.{}", offer.potion.stable_id()))
                .map(str::to_owned)
                .unwrap_or_else(|| compact_text(locale, "shop.unknown", &[])),
            &offer.purchase_status,
            locale,
        );
    }
    if let Some(purge) = &inventory.purge {
        writeln!(
            output,
            "{}",
            compact_text(
                locale,
                "shop.remove",
                &[
                    ("price", &purge.price.to_string()),
                    (
                        "status",
                        &crate::decision::purchase_suffix(&purge.purchase_status, locale)
                    )
                ]
            )
        )
        .expect("string write");
        for (ordinal, candidate) in decision.candidates.iter().enumerate() {
            if let TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::RemoveCard { card, upgrades, .. },
            } = candidate
            {
                if cards::get_card_definition(*card).card_type == cards::CardType::Curse {
                    writeln!(
                        output,
                        "  {}  |  {}",
                        crate::selector::semantic_action_for_choice_v1(
                            decision,
                            &sts_oracle_learning::agent::task::TaskPublicChoiceV1::one(ordinal)
                        )?,
                        locale.card_name(card.stable_id(), *upgrades)
                    )
                    .expect("string write");
                }
            }
        }
        if matches!(
            purge.purchase_status,
            PlannerShopPurchaseStatusV1::Purchasable
        ) {
            writeln!(
                output,
                "{}",
                compact_text(locale, "shop.removeTargets", &[])
            )
            .expect("string write");
        }
    }
    if let Some(option) = reading.options.iter().find(|option| {
        matches!(
            &option.candidate,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::LeaveShop
            }
        )
    }) {
        writeln!(
            output,
            "{}",
            compact_text(
                locale,
                "shop.leave",
                &[(
                    "command",
                    &crate::selector::semantic_action_for_choice_v1(decision, &option.choice)?
                )]
            )
        )
        .expect("string write");
    }
    let mut other_heading = false;
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        let TaskPublicCandidateV1::Strategic { action } = candidate else {
            continue;
        };
        if !matches!(
            action,
            TaskStrategicActionV1::UseRunPotion { .. }
                | TaskStrategicActionV1::DiscardRunPotion { .. }
        ) {
            continue;
        }
        if !other_heading {
            writeln!(output, "{}", compact_text(locale, "shop.other", &[])).expect("string write");
            other_heading = true;
        }
        writeln!(
            output,
            "  {}",
            crate::selector::semantic_action_for_choice_v1(
                decision,
                &sts_oracle_learning::agent::task::TaskPublicChoiceV1::one(ordinal)
            )?
        )
        .expect("string write");
    }
    if let Some(change) = deck_change {
        writeln!(
            output,
            "{}",
            compact_text(locale, "status.deckChange", &[("change", change)])
        )
        .expect("string write");
    } else {
        writeln!(
            output,
            "{}",
            compact_text(
                locale,
                "status.deckCount",
                &[("count", &observation.cards.len().to_string())]
            )
        )
        .expect("string write");
    }
    Ok(output)
}

pub(crate) fn render_deck_change_v1(
    source: &TaskPublicDecisionV1,
    successor: &TaskPublicDecisionV1,
    locale: Locale,
) -> Option<String> {
    let (
        TaskPublicObservationV1::Strategic {
            observation: before,
        },
        TaskPublicObservationV1::Strategic { observation: after },
    ) = (&source.observation, &successor.observation)
    else {
        return None;
    };
    let mut changes = BTreeMap::<PermanentCardVariantKeyV1, (TaskStrategicCardV1, i32)>::new();
    for card in &before.cards {
        changes
            .entry(permanent_card_variant_key_v1(card))
            .or_insert((*card, 0))
            .1 -= 1;
    }
    for card in &after.cards {
        changes
            .entry(permanent_card_variant_key_v1(card))
            .or_insert((*card, 0))
            .1 += 1;
    }
    let mut names = BTreeMap::<_, std::collections::BTreeSet<_>>::new();
    for card in before.cards.iter().chain(&after.cards) {
        names
            .entry(locale.entity("card", card.card.stable_id()))
            .or_default()
            .insert(card.card.stable_id());
    }
    let changes = changes
        .into_values()
        .filter(|(_, count)| *count != 0)
        .map(|(card, count)| {
            let mut label = permanent_card_label_v1(&card, locale);
            if names[locale.entity("card", card.card.stable_id())].len() > 1 {
                write!(label, " [{}]", card.card.stable_id()).expect("string write");
            }
            match count {
                1 => format!("+{label}"),
                -1 => format!("-{label}"),
                n if n > 1 => format!("+{n}× {label}"),
                n => format!("-{}× {label}", -n),
            }
        })
        .collect::<Vec<_>>();
    (!changes.is_empty()).then(|| changes.join(", "))
}

pub fn render_inventory_change_v1(
    before: &TaskPublicDecisionV1,
    after: &TaskPublicDecisionV1,
    locale: Locale,
) -> String {
    let mut output = String::new();
    if let Some(change) = render_deck_change_v1(before, after, locale) {
        writeln!(
            output,
            "{}",
            compact_text(locale, "status.deckChange", &[("change", &change)])
        )
        .expect("string write");
    }
    if let (
        TaskPublicObservationV1::Strategic {
            observation: before,
        },
        TaskPublicObservationV1::Strategic { observation: after },
    ) = (&before.observation, &after.observation)
    {
        let held = before
            .relics
            .iter()
            .map(|relic| relic.relic)
            .collect::<Vec<_>>();
        for relic in &after.relics {
            if !held.contains(&relic.relic) {
                writeln!(
                    output,
                    "{}",
                    compact_text(
                        locale,
                        "transition.relicGained",
                        &[("relic", locale.entity("relic", relic.relic.stable_id()))]
                    )
                )
                .expect("string write");
            }
        }
    }
    output
}

/// Default private rehearsal output: the accepted public actions and the
/// hypothetical successor. Full comparison evidence remains behind --detail.
pub fn render_preview_v1(
    comparison: &RunPlayComparisonV1,
    detail: bool,
    locale: Locale,
) -> Result<String, String> {
    if detail {
        return render_comparison_v1(comparison, true, locale);
    }
    let mut output = compact_text(locale, "preview.heading", &[]);
    for (line_index, line) in comparison.lines.iter().enumerate() {
        if comparison.lines.len() > 1 {
            writeln!(
                output,
                "{}",
                compact_text(
                    locale,
                    "preview.line",
                    &[("index", &(line_index + 1).to_string())]
                )
            )
            .expect("string write");
        }
        let actions = line
            .steps
            .iter()
            .take(8)
            .map(|step| render_preview_action_v1(step, locale))
            .collect::<Result<Vec<_>, _>>()?;
        let remaining = line.steps.len().saturating_sub(actions.len());
        writeln!(
            output,
            "{}",
            compact_text(
                locale,
                "preview.actions",
                &[
                    ("count", &line.steps.len().to_string()),
                    ("actions", &actions.join(" → ")),
                    (
                        "remaining",
                        &if remaining > 0 {
                            compact_text(
                                locale,
                                "preview.more",
                                &[("count", &remaining.to_string())],
                            )
                        } else {
                            String::new()
                        }
                    ),
                ]
            )
        )
        .expect("string write");
        let Some(last) = line.steps.last() else {
            writeln!(output, "{}", compact_text(locale, "preview.empty", &[]))
                .expect("string write");
            continue;
        };
        if let Some(next) = &last.next_decision {
            let before = crate::decision::read_preview_decision_v1(
                &line.steps[0].source_decision,
                None,
                true,
                locale,
            )?;
            let after = crate::decision::read_preview_decision_v1(
                next,
                before.context.as_ref(),
                true,
                locale,
            )?;
            let deck_change = render_deck_change_v1(&line.steps[0].source_decision, next, locale);
            writeln!(output, "{}", compact_text(locale, "preview.result", &[]))
                .expect("string write");
            output.push_str(&render_reading_status_v1(
                next,
                &after,
                true,
                deck_change.as_deref(),
                locale,
            )?);
        } else {
            writeln!(
                output,
                "{}",
                compact_text(
                    locale,
                    "preview.terminalResult",
                    &[("result", &render_terminal_result_v1(last, locale))]
                )
            )
            .expect("string write");
        }
    }
    Ok(output)
}

fn render_preview_action_v1(
    step: &ExactRunPlayProbeStepV1,
    locale: Locale,
) -> Result<String, String> {
    let candidate = &step.selected_candidate;
    let public = &step.source_decision.observation;
    if let TaskPublicObservationV1::Strategic { observation } = public {
        return crate::decision::option_label(candidate, observation, public, false, locale);
    }
    use crate::combat_trace::{RunPlayCombatActionDisplayV1 as Display, RunPlayPlayedCardV1};
    let TaskPublicObservationV1::Combat { observation, .. } = public else {
        unreachable!()
    };
    let atomic = match candidate {
        TaskPublicCandidateV1::CombatAtomic { action }
        | TaskPublicCandidateV1::CombatIndexedChoice { action, .. } => Some(action),
        _ => None,
    };
    let target_slot = |target: Option<usize>| -> Result<Option<u8>, String> {
        target
            .map(|index| {
                observation
                    .monsters
                    .get(index)
                    .map(|monster| monster.slot)
                    .ok_or_else(|| {
                        compact_text(
                            locale,
                            "preview.targetUnavailable",
                            &[("index", &index.to_string())],
                        )
                    })
            })
            .transpose()
    };
    let display = match atomic {
        Some(PublicCombatAtomicActionV1::PlayCard {
            hand_index,
            target_monster_index,
        }) => {
            let card = observation
                .cards
                .hand
                .cards
                .get(*hand_index)
                .ok_or_else(|| {
                    compact_text(
                        locale,
                        "preview.cardUnavailable",
                        &[("index", &hand_index.to_string())],
                    )
                })?;
            Display::PlayCard(RunPlayPlayedCardV1 {
                card_id: card.card_id,
                upgrades: card.upgrades,
                target_slot: target_slot(*target_monster_index)?,
            })
        }
        Some(PublicCombatAtomicActionV1::UsePotion {
            potion_index,
            target_monster_index,
        }) => {
            let potion = observation
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    compact_text(
                        locale,
                        "preview.potionUnavailable",
                        &[("index", &potion_index.to_string())],
                    )
                })?;
            Display::UsePotion {
                slot: *potion_index,
                potion_id: potion.potion_id,
                target_slot: target_slot(*target_monster_index)?,
            }
        }
        _ => crate::combat_trace::selected_action_display(
            candidate,
            &step.choice.ordinal_path,
            None,
            None,
        )?,
    };
    Ok(crate::combat_reading::action_label(&display, locale))
}

pub fn render_comparison_v1(
    comparison: &RunPlayComparisonV1,
    detail: bool,
    locale: Locale,
) -> Result<String, String> {
    let text = |key: &str, parameters: &[(&str, &str)]| {
        compact_text(locale, &format!("compare.{key}"), parameters)
    };
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        text(
            "heading",
            &[
                ("root", short_id(&comparison.root_archive_node_id)),
                ("count", &comparison.lines.len().to_string())
            ]
        )
    )
    .expect("string write");
    for (line_index, line) in comparison.lines.iter().enumerate() {
        let mut explanation = CombatLineExplanationStateV1::default();
        let choices = line
            .steps
            .iter()
            .map(|step| {
                step.choice
                    .ordinal_path
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .collect::<Vec<_>>()
            .join("/");
        let actions = line
            .steps
            .iter()
            .map(|step| render_preview_action_v1(step, locale))
            .collect::<Result<Vec<_>, _>>()?
            .join(" > ");
        writeln!(
            output,
            "{}",
            text(
                "line",
                &[
                    ("index", &(line_index + 1).to_string()),
                    ("choices", &choices),
                    ("actions", &actions),
                    ("steps", &line.steps.len().to_string()),
                    (
                        "elapsed",
                        &format!("{:.3}", line.total_elapsed.as_secs_f64() * 1000.0)
                    ),
                    (
                        "fork",
                        &format!("{:.3}", line.fork_elapsed.as_secs_f64() * 1000.0)
                    )
                ]
            )
        )
        .expect("string write");
        if detail {
            for (step_index, step) in line.steps.iter().enumerate() {
                writeln!(
                    output,
                    "{}",
                    text(
                        "step",
                        &[
                            ("index", &(step_index + 1).to_string()),
                            (
                                "choice",
                                &step
                                    .choice
                                    .ordinal_path
                                    .iter()
                                    .map(usize::to_string)
                                    .collect::<Vec<_>>()
                                    .join(".")
                            ),
                            ("action", &render_preview_action_v1(step, locale)?),
                            (
                                "elapsed",
                                &format!("{:.3}", step.elapsed.as_secs_f64() * 1000.0)
                            ),
                            ("outcome", &render_outcome_compact(&step.outcome, locale))
                        ]
                    )
                )
                .expect("string write");
                if let TaskPublicCandidateV1::Strategic { action } = &step.selected_candidate {
                    crate::decision::write_choice_mechanics(&mut output, action, true, locale);
                    if let TaskPublicObservationV1::Strategic { observation } =
                        &step.source_decision.observation
                    {
                        crate::decision::write_event_resource_modifiers(
                            &mut output,
                            action,
                            observation,
                            locale,
                        );
                    }
                }
                explanation.render_step_v1(&mut output, step, locale);
            }
        }
        match line
            .steps
            .last()
            .and_then(|step| step.next_decision.as_ref())
        {
            Some(decision) => {
                let reading =
                    crate::decision::read_preview_decision_v1(decision, None, !detail, locale)?;
                output.push_str(&reading.view);
            }
            None => {
                let outcome = line
                    .steps
                    .last()
                    .map(|step| render_terminal_result_v1(step, locale))
                    .unwrap_or_else(|| text("empty", &[]));
                writeln!(output, "{}", text("final", &[("outcome", &outcome)]))
                    .expect("string write");
            }
        }
    }
    Ok(output)
}

pub(crate) fn render_terminal_result_v1(step: &ExactRunPlayProbeStepV1, locale: Locale) -> String {
    let outcome = match &step.outcome {
        TaskEpisodeOutcomeV1::Failure {
            game_outcome: Some(GameOutcomeV1::Defeat),
            ..
        } => compact_text(locale, "outcome.defeat", &[]),
        _ => render_outcome_compact(&step.outcome, locale),
    };
    let hp = step.action_result.as_ref().and_then(|result| {
        result.changes.iter().find_map(|change| match change {
            RunActionResultChangeV1::HpChanged {
                before_current,
                before_max,
                after_current,
                after_max,
            } => Some(compact_text(
                locale,
                "preview.health",
                &[
                    ("beforeHp", &before_current.to_string()),
                    ("beforeMax", &before_max.to_string()),
                    ("afterHp", &after_current.to_string()),
                    ("afterMax", &after_max.to_string()),
                ],
            )),
            _ => None,
        })
    });
    match hp {
        Some(hp) => format!("{outcome} | {hp}"),
        None => outcome,
    }
}

pub(crate) fn render_outcome_compact(outcome: &TaskEpisodeOutcomeV1, locale: Locale) -> String {
    match outcome {
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::HeartVictory,
        } => compact_text(locale, "outcome.heart", &[]),
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory,
        } => compact_text(locale, "outcome.act3", &[]),
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::Defeat,
        } => compact_text(locale, "outcome.ended", &[]),
        TaskEpisodeOutcomeV1::Paused { .. } => compact_text(locale, "outcome.paused", &[]),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::CommittedDefeat,
            ..
        } => compact_text(locale, "outcome.defeat", &[]),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::CommittedTerminalWithoutObjective,
            ..
        } => compact_text(locale, "outcome.beforeObjective", &[]),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::RetryBudgetExhausted,
            ..
        } => compact_text(locale, "outcome.retries", &[]),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::TransitionBudgetExhausted,
            ..
        } => compact_text(locale, "outcome.transitions", &[]),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::UnsupportedDecisionSurface,
            ..
        } => compact_text(locale, "outcome.unsupported", &[]),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::MechanicsOrProtocolError { message },
            ..
        } => compact_text(locale, "outcome.error", &[("message", message)]),
    }
}
