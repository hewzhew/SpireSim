use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use sts_presentation::Locale;

use sts_oracle_learning::agent::information::action::PublicCombatAtomicActionV1;
use sts_oracle_learning::agent::information::state::{
    CombatLearningEnemyIdentityV1, PublicCombatStateV1,
};
use sts_oracle_learning::agent::task::{
    GameOutcomeV1, TaskEpisodeFailureV1, TaskEpisodeOutcomeV1, TaskPublicCandidateV1,
    TaskPublicDecisionV1, TaskPublicObservationV1, TaskStrategicActionV1, TaskStrategicCardV1,
};
use sts_oracle_learning::ai::planner_core::{
    PlannerDecisionContext, PlannerShopInventoryV1, PlannerShopPurchaseStatusV1,
};
use sts_oracle_learning::content::identity::gameplay_display_label_v1;
use sts_oracle_learning::content::{cards, potions, relics};
use sts_oracle_learning::eval::run_control::{ExactRunPlayProbeStepV1, RunActionResultChangeV1};

use crate::combat_explain::CombatLineExplanationStateV1;
use crate::decision::RunPlayDecisionReadingV1;
use crate::driver::{RunPlayComparisonV1, RunPlayControllerV1};
use crate::model::{RunPlayEvidenceIdentityV1, RunPlayHeadV1, RunPlayPositionV1};
use crate::render::{
    format_card, render_candidate_with_observation, render_event_structured_semantics_v1,
    render_monster_block_v1, render_player_block_v1, render_powers, render_route_options_v1,
    render_shop_inventory_v1, render_single_route_v1, render_strategic_run_header_v1, short_id,
};

#[cfg(test)]
#[path = "compact/inventory_tests.rs"]
mod inventory_tests;
/// Compact, typed resume context for humans and task handoffs. `show` remains
/// the full evidence surface; this projection deliberately favors one-screen
/// orientation without reading archive JSON or reconstructing private state.
pub fn render_status_v1(head: &RunPlayHeadV1) -> Result<String, String> {
    if let RunPlayPositionV1::Terminal { outcome, .. } = &head.position {
        return Ok(format!("Terminal: {}\n", render_outcome_compact(outcome)));
    }
    // The normal reading already owns human labels for combat, rewards, and
    // events. Keep status on that surface; `show` retains the exact diagnostic
    // ids, source proof and engine candidate dump when they are needed.
    let reading = crate::decision::read_decision(head, &[], None)?;
    let decision = head
        .current_decision
        .as_ref()
        .ok_or_else(|| "run-play status has no cached decision".to_string())?
        .public_decision_v1()?;
    let mut output = render_reading_status_v1(&decision, &reading, false, None)?;
    let compatibility =
        RunPlayEvidenceIdentityV1::compatibility_label_v1(head.evidence_identity.as_ref());
    if !head.scratch && compatibility != "current" {
        writeln!(
            output,
            "Saved decision only: {compatibility}; verify before acting."
        )
        .expect("string write");
    }
    Ok(output)
}

pub fn render_back_targets_v1(paths: &crate::storage::RunPlayPathsV1) -> Result<String, String> {
    let head = paths.load_head()?;
    if !head.scratch {
        return Err("floor return points and marks require a scratch session".into());
    }
    let floors = head
        .scratch_floor_ends
        .iter()
        .map(|end| end.floor.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!(
        "Retained floors (back floor <number>): {}\n",
        if floors.is_empty() { "none" } else { &floors }
    );
    let recent = paths.recent_scratch_heads_v1(&head, 12)?;
    if !recent.is_empty() {
        out.push_str("Recent return points (back <steps>; up to 12):\n");
        for (index, retained) in recent.iter().enumerate() {
            let decision = retained
                .current_decision
                .as_ref()
                .ok_or("scratch return point has no public decision")?;
            writeln!(
                out,
                "  back {} | {}",
                index + 1,
                render_position_resources_v1(&decision.observation)
            )
            .expect("string write");
        }
    }
    out.push_str(&render_mark_targets_v1(paths, &head)?);
    out.push_str("Each retained floor is its completed Map or first decision when no Map exists, not every earlier action. Save the current exact position with mark <name> BEFORE rewinding. Rewinding replaces the active branch and discards unmarked later floors; a named mark preserves its own floor return points. Do not guess node or revision IDs.\n");
    Ok(out)
}

pub fn render_mark_targets_v1(
    paths: &crate::storage::RunPlayPathsV1,
    head: &RunPlayHeadV1,
) -> Result<String, String> {
    if !head.scratch {
        return Err("marks require a scratch session".into());
    }
    if head.scratch_marks.is_empty() {
        return Ok("Named return points: none.\n".into());
    }
    let mut out =
        String::from("Named return points (back mark <name>; facts saved at each point):\n");
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
            render_position_resources_v1(&decision.observation)
        )
        .expect("string write");
    }
    out.push_str("These are saved branch facts. Read the restored game for its full inventory and current choices.\n");
    Ok(out)
}

pub(crate) fn render_position_resources_v1(observation: &TaskPublicObservationV1) -> String {
    let (location, hp, max_hp, gold, held_potions) = match observation {
        TaskPublicObservationV1::Strategic { observation } => (
            format!(
                "Act {} F{} | {:?}",
                observation.run.act, observation.run.floor, observation.decision_site
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
                        .map(|potion| gameplay_display_label_v1(potion.potion))
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
                    let encounter = encounter_id.as_ref().map_or("Combat", |encounter| encounter.stable_id());
                    format!("Act {act} F{floor} | {encounter}")
                }
                sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Unavailable { .. } => "Combat (run context unavailable)".into(),
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
                            .map(|potion| gameplay_display_label_v1(potion.potion_id))
                    })
                    .collect::<Vec<_>>(),
            )
        }
    };
    format!(
        "{location} | HP {hp}/{max_hp} | Gold {gold} | Potions: {}",
        if held_potions.is_empty() {
            "none".into()
        } else {
            held_potions.join(", ")
        }
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
        assert!(render_position_resources_v1(&public).contains("| T1 |"));
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
        let empty = render_position_resources_v1(&public);
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
        let supplied = render_position_resources_v1(&public);
        assert!(empty.contains("HP 10/85") && supplied.contains("HP 10/85"));
        assert!(empty.ends_with("Potions: none"));
        assert!(supplied.ends_with("Potions: Energy Potion, Explosive Potion, Strength Potion"));
        assert_ne!(empty, supplied);
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
    let mut counts = BTreeMap::new();
    let mut name_ids = BTreeMap::<_, std::collections::BTreeSet<_>>::new();
    for card in cards {
        name_ids
            .entry(locale.entity("card", card.card.stable_id()))
            .or_default()
            .insert(card.card.stable_id());
        // Display names are neither identity nor instance grouping keys.
        let key = (
            card.card.stable_id(),
            card.upgrades,
            card.misc_value,
            card.base_damage_override,
            card.base_block_override,
            card.cost_modifier,
            card.bottled_by.map(|relic| relic.stable_id()),
        );
        counts.entry(key).or_insert((0, card)).0 += 1;
    }
    let mut groups = counts.values().collect::<Vec<_>>();
    // Keep a stable order across locales; sorting never merges identities.
    groups.sort_by_key(|(_, card)| {
        (
            gameplay_display_label_v1(card.card),
            card.card.stable_id(),
            card.upgrades,
        )
    });
    let mut card_readings = Vec::new();
    for &&(count, card) in &groups {
        let id = card.card;
        let upgrades = card.upgrades;
        let start = output.len();
        let mut label = inventory_card_label_v1(&card, locale);
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
                match cards::mechanics::explanation(id) {
                    Some(effect) => message("supplement", &[("effect", effect)]),
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
                .map(|&&(count, card)| (card.card, card.upgrades, card.cost_modifier, count)),
        );
        if !links.is_empty() {
            if locale != Locale::En {
                writeln!(output, "{}", message("linksEnglish", &[])).expect("string write");
            }
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
                match relics::mechanics::explanation(id) {
                    Some(effect) => message("supplement", &[("effect", effect)]),
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

fn inventory_card_label_v1(card: &TaskStrategicCardV1, locale: Locale) -> String {
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

fn relic_summary_v1(relics: &[String]) -> String {
    let live = relics
        .iter()
        .filter(|relic| relic.ends_with(']') || relic.ends_with(')'))
        .cloned()
        .collect::<Vec<_>>();
    if live.is_empty() {
        format!("Relics: {} held", relics.len())
    } else {
        format!("Relics: {} held; states: {}", relics.len(), live.join(", "))
    }
}

pub(crate) fn render_reading_status_v1(
    decision: &TaskPublicDecisionV1,
    reading: &RunPlayDecisionReadingV1,
    preview: bool,
    deck_change: Option<&str>,
) -> Result<String, String> {
    if let TaskPublicObservationV1::Strategic { observation } = &decision.observation {
        if let PlannerDecisionContext::Shop { inventory, .. } = &observation.context {
            return render_shop_status_v1(decision, reading, observation, inventory, deck_change);
        }
    }
    let actions = if matches!(
        &decision.observation,
        TaskPublicObservationV1::Strategic { .. }
    ) {
        reading
            .options
            .iter()
            .map(|option| {
                crate::selector::semantic_action_for_choice_v1(decision, &option.choice).ok()
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    Ok(wrap_status_lists_v1(
        &reading.view,
        &actions,
        preview,
        deck_change,
        match &decision.observation {
            TaskPublicObservationV1::Strategic { observation } => Some(observation.cards.len()),
            TaskPublicObservationV1::Combat { .. } => None,
        },
    ))
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
) {
    if let Some(command) = command {
        writeln!(output, "  {command}  |  {price}g · {name} · {detail}").expect("string write");
    } else {
        writeln!(
            output,
            "  {name} {price}g{} · {detail}",
            crate::decision::purchase_suffix(status)
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
) -> Result<String, String> {
    let run = &observation.run;
    let mut output = format!(
        "Act {} F{} | A{} | HP {}/{} | Gold {} | Shop\n",
        run.act, run.floor, run.ascension_level, run.current_hp, run.max_hp, run.gold
    );
    writeln!(
        output,
        "Potions: {}",
        observation
            .potions
            .iter()
            .map(|slot| format!(
                "P{} {}",
                slot.slot,
                slot.potion
                    .as_ref()
                    .map_or("empty", |potion| gameplay_display_label_v1(potion.potion))
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .expect("string write");
    let held_relics = observation
        .relics
        .iter()
        .map(|relic| relic_label_v1(relic.relic, relic.counter, relic.used_up, Locale::En))
        .collect::<Vec<_>>();
    writeln!(output, "{}", relic_summary_v1(&held_relics)).expect("string write");
    if let Some(boss) = observation.public_map.boss {
        writeln!(output, "Boss: {}", gameplay_display_label_v1(boss)).expect("string write");
    }

    writeln!(output, "Cards for sale (.\\scripts\\p.ps1 <action>):").expect("string write");
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
            -2 => "unplayable".to_owned(),
            cost => format!("{cost}c"),
        };
        let detail = format!(
            "{cost} {:?}: {}",
            definition.card_type,
            cards::mechanics::explanation(offer.card).unwrap_or("Effect description unavailable.")
        );
        render_shop_offer_v1(
            &mut output,
            command.as_deref(),
            &crate::render::format_card(offer.card, offer.upgrades),
            offer.price,
            &detail,
            &offer.purchase_status,
        );
    }
    writeln!(output, "Relics for sale:").expect("string write");
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
            gameplay_display_label_v1(offer.relic),
            offer.price,
            relics::mechanics::explanation(offer.relic)
                .unwrap_or("Effect description unavailable."),
            &offer.purchase_status,
        );
    }
    writeln!(output, "Potions for sale:").expect("string write");
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
            gameplay_display_label_v1(offer.potion),
            offer.price,
            potions::mechanics::explanation(offer.potion)
                .unwrap_or("Effect description unavailable."),
            &offer.purchase_status,
        );
    }
    if let Some(purge) = &inventory.purge {
        writeln!(
            output,
            "Remove card: {}g{}",
            purge.price,
            crate::decision::purchase_suffix(&purge.purchase_status)
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
                        crate::render::format_card(*card, *upgrades)
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
                "  Other targets: remove <CardId> (see .\\scripts\\p.ps1 inventory)."
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
            "Leave: {}",
            crate::selector::semantic_action_for_choice_v1(decision, &option.choice)?
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
            writeln!(output, "Other actions:").expect("string write");
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
        writeln!(output, "Deck change: {change}").expect("string write");
    } else {
        writeln!(
            output,
            "Deck: {} cards (see .\\scripts\\p.ps1 inventory)",
            observation.cards.len()
        )
        .expect("string write");
    }
    Ok(output)
}

fn wrap_status_lists_v1(
    view: &str,
    actions: &[Option<String>],
    preview: bool,
    deck_change: Option<&str>,
    deck_size: Option<usize>,
) -> String {
    let mut output = String::new();
    let mut action_heading = false;
    for line in view.lines() {
        if let Some(cards) = line.strip_prefix("Hand: ") {
            writeln!(output, "Hand:").expect("string write");
            for card in cards.split(", ") {
                writeln!(output, "  {card}").expect("string write");
            }
        } else if line.starts_with("Deck: ") {
            if let Some(change) = deck_change {
                writeln!(output, "Deck change: {change}").expect("string write");
            } else if let Some(size) = deck_size {
                writeln!(
                    output,
                    "Deck: {size} cards (see .\\scripts\\p.ps1 inventory)"
                )
                .expect("string write");
            }
        } else if let Some(relics) = line.strip_prefix("Relics: ") {
            let relics = relics.split(", ").map(str::to_owned).collect::<Vec<_>>();
            writeln!(output, "{}", relic_summary_v1(&relics)).expect("string write");
        } else if preview && line.starts_with("Unchanged: ") {
            continue;
        } else if let Some((number, label)) = line.split_once(": ") {
            if let Ok(index) = number.parse::<usize>() {
                if let Some(Some(action)) = actions.get(index) {
                    if !action_heading {
                        writeln!(output, "Actions:").expect("string write");
                        action_heading = true;
                    }
                    if let Some(coordinates) = action.strip_prefix("route:") {
                        let destination = coordinates.replace('.', ",");
                        let room = label
                            .strip_prefix(&format!("Go ({destination}) "))
                            .unwrap_or(label);
                        writeln!(output, "  go {destination}  |  {room}").expect("string write");
                    } else {
                        writeln!(output, "  {action}  |  {label}").expect("string write");
                    }
                    continue;
                }
            }
            writeln!(output, "{line}").expect("string write");
        } else if !line.starts_with("Damage: per-target hit") {
            writeln!(output, "{line}").expect("string write");
        }
    }
    output
}

pub(crate) fn preview_deck_change_v1(
    source: &TaskPublicDecisionV1,
    successor: &TaskPublicDecisionV1,
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
    let mut changes = BTreeMap::<String, i32>::new();
    for card in &before.cards {
        *changes
            .entry(crate::decision::deck_card_label(card))
            .or_default() -= 1;
    }
    for card in &after.cards {
        *changes
            .entry(crate::decision::deck_card_label(card))
            .or_default() += 1;
    }
    let changes = changes
        .into_iter()
        .filter(|(_, count)| *count != 0)
        .map(|(card, count)| match count {
            1 => format!("+{card}"),
            -1 => format!("-{card}"),
            n if n > 1 => format!("+{n}× {card}"),
            n => format!("-{}× {card}", -n),
        })
        .collect::<Vec<_>>();
    (!changes.is_empty()).then(|| changes.join(", "))
}

pub fn render_inventory_change_v1(
    before: &TaskPublicDecisionV1,
    after: &TaskPublicDecisionV1,
) -> String {
    let mut output = String::new();
    if let Some(change) = preview_deck_change_v1(before, after) {
        writeln!(output, "Deck change: {change}").expect("string write");
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
                    "Relic gained: {}",
                    gameplay_display_label_v1(relic.relic)
                )
                .expect("string write");
            }
        }
    }
    output
}

/// Default private rehearsal output: the accepted public actions and the
/// hypothetical successor. Full comparison evidence remains behind --detail.
pub fn render_preview_v1(comparison: &RunPlayComparisonV1, detail: bool) -> Result<String, String> {
    if detail {
        return render_comparison_v1(comparison, true);
    }
    let mut output = String::from("Preview only — current position unchanged\n");
    for (line_index, line) in comparison.lines.iter().enumerate() {
        if comparison.lines.len() > 1 {
            writeln!(output, "Line {}", line_index + 1).expect("string write");
        }
        let actions = line
            .steps
            .iter()
            .take(8)
            .map(|step| {
                render_preview_action_v1(
                    &step.selected_candidate,
                    &step.source_decision.observation,
                )
            })
            .collect::<Vec<_>>();
        let remaining = line.steps.len().saturating_sub(actions.len());
        writeln!(
            output,
            "Actions ({}): {}{}",
            line.steps.len(),
            actions.join(" → "),
            if remaining > 0 {
                format!(" → … (+{remaining}; --detail shows all)")
            } else {
                String::new()
            }
        )
        .expect("string write");
        let Some(last) = line.steps.last() else {
            writeln!(output, "Result: no action was executed").expect("string write");
            continue;
        };
        if let Some(next) = &last.next_decision {
            let before =
                crate::decision::read_preview_decision_v1(&line.steps[0].source_decision, None)?;
            let after = crate::decision::read_preview_decision_v1(next, before.context.as_ref())?;
            let deck_change = preview_deck_change_v1(&line.steps[0].source_decision, next);
            writeln!(output, "Result:").expect("string write");
            output.push_str(&render_reading_status_v1(
                next,
                &after,
                true,
                deck_change.as_deref(),
            )?);
        } else {
            writeln!(output, "Result: {}", render_terminal_result_v1(last)).expect("string write");
        }
    }
    Ok(output)
}

fn render_preview_action_v1(
    candidate: &TaskPublicCandidateV1,
    observation: &TaskPublicObservationV1,
) -> String {
    if let TaskPublicCandidateV1::Strategic {
        action:
            TaskStrategicActionV1::ChooseEventOption {
                event,
                option_index,
                public_semantics,
                ..
            },
    } = candidate
    {
        return public_semantics
            .public_label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} option {option_index}", event.stable_id()));
    }
    render_candidate_compact(candidate, observation)
}

pub fn render_comparison_v1(
    comparison: &RunPlayComparisonV1,
    detail: bool,
) -> Result<String, String> {
    let mut output = String::new();
    writeln!(
        output,
        "compare root=n{} lines={} durable_head_unchanged=true",
        short_id(&comparison.root_archive_node_id),
        comparison.lines.len()
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
        writeln!(
            output,
            "line {} choices={} actions=[{}] steps={} elapsed_ms={:.3} fork_ms={:.3}",
            line_index + 1,
            choices,
            line.steps
                .iter()
                .map(|step| render_candidate_compact(
                    &step.selected_candidate,
                    &step.source_decision.observation
                ))
                .collect::<Vec<_>>()
                .join(" > "),
            line.steps.len(),
            line.total_elapsed.as_secs_f64() * 1_000.0,
            line.fork_elapsed.as_secs_f64() * 1_000.0
        )
        .expect("string write");
        if detail {
            for (step_index, step) in line.steps.iter().enumerate() {
                writeln!(
                    output,
                    "  step {} [{}] {} elapsed_ms={:.3} outcome={}",
                    step_index + 1,
                    step.choice
                        .ordinal_path
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join("."),
                    render_candidate_with_observation(
                        &step.selected_candidate,
                        Some(&step.source_decision.observation)
                    ),
                    step.elapsed.as_secs_f64() * 1_000.0,
                    render_outcome_compact(&step.outcome)
                )
                .expect("string write");
                explanation.render_step_v1(&mut output, step);
            }
        }
        match line
            .steps
            .last()
            .and_then(|step| step.next_decision.as_ref())
        {
            Some(decision) => render_public_decision_status_v1(&mut output, decision, detail)?,
            None => {
                let outcome = line
                    .steps
                    .last()
                    .map(render_terminal_result_v1)
                    .unwrap_or_else(|| "empty".to_string());
                writeln!(output, "  final {outcome}").expect("string write");
            }
        }
    }
    Ok(output)
}

pub(crate) fn render_terminal_result_v1(step: &ExactRunPlayProbeStepV1) -> String {
    let outcome = match &step.outcome {
        TaskEpisodeOutcomeV1::Failure {
            game_outcome: Some(GameOutcomeV1::Defeat),
            ..
        } => "Defeat".to_string(),
        _ => render_outcome_compact(&step.outcome),
    };
    let hp = step.action_result.as_ref().and_then(|result| {
        result.changes.iter().find_map(|change| match change {
            RunActionResultChangeV1::HpChanged {
                before_current,
                before_max,
                after_current,
                after_max,
            } => Some(format!(
                "HP {before_current}/{before_max} → {after_current}/{after_max}"
            )),
            _ => None,
        })
    });
    match hp {
        Some(hp) => format!("{outcome} | {hp}"),
        None => outcome,
    }
}

fn render_public_decision_status_v1(
    output: &mut String,
    decision: &TaskPublicDecisionV1,
    shop_details: bool,
) -> Result<(), String> {
    match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => {
            writeln!(output, "{}", render_strategic_run_header_v1(observation))
                .expect("string write");
            writeln!(
                output,
                "deck [{}]",
                render_card_multiset(
                    observation
                        .cards
                        .iter()
                        .map(|card| { (card.card, card.upgrades) })
                )
            )
            .expect("string write");
            writeln!(
                output,
                "relics [{}] potions [{}]",
                observation
                    .relics
                    .iter()
                    .map(|relic| gameplay_display_label_v1(relic.relic).to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                observation
                    .potions
                    .iter()
                    .map(|slot| slot.potion.as_ref().map_or_else(
                        || "empty".to_string(),
                        |potion| gameplay_display_label_v1(potion.potion).to_string()
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            if shop_details {
                render_shop_inventory_v1(output, &observation.context);
            }
            if let Some(route) = render_single_route_v1(&observation.public_map) {
                writeln!(output, "{route}").expect("string write");
            }
            for route in render_route_options_v1(&observation.public_map, &decision.candidates) {
                writeln!(output, "{route}").expect("string write");
            }
        }
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            let context = match run_context {
                sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Available {
                    act,
                    floor,
                    encounter_id,
                    ..
                } => format!(
                    "act={act} floor={floor} encounter={}",
                    encounter_id
                        .as_ref()
                        .map(|encounter_id| encounter_id.stable_id())
                        .unwrap_or("unknown")
                ),
                sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Unavailable {
                    reason,
                } => format!("detached={reason:?}"),
            };
            writeln!(
                output,
                "combat {context} turn={} hp={}/{} block={} energy={}/{} played={} powers=[{}]",
                observation.turn.turn_count,
                observation.player.hp,
                observation.player.max_hp,
                render_player_block_v1(observation),
                observation.turn.energy,
                observation.player.energy_master,
                observation.turn.counters.cards_played_this_turn,
                render_powers(&observation.player.powers)
            )
            .expect("string write");
            writeln!(
                output,
                "enemies [{}]",
                observation
                    .monsters
                    .iter()
                    .map(|monster| format!(
                        "{}:{} {}/{} b{} intent={:?}/{:?} powers=[{}]",
                        monster.slot,
                        match monster.enemy {
                            CombatLearningEnemyIdentityV1::Known { enemy_id } => {
                                gameplay_display_label_v1(enemy_id)
                            }
                        },
                        monster.hp,
                        monster.max_hp,
                        render_monster_block_v1(monster.block, &monster.powers),
                        monster.intent.intent,
                        monster.intent.preview_damage_per_hit,
                        render_powers(&monster.powers)
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
            .expect("string write");
            writeln!(
                output,
                "hand [{}] piles=draw:{} discard:{} exhaust:{} nonmaster=[{}]",
                observation
                    .cards
                    .hand
                    .cards
                    .iter()
                    .enumerate()
                    .map(|(index, card)| format!(
                        "{index}:{}({})",
                        format_card(card.card_id, card.upgrades),
                        card.effective_cost
                    ))
                    .collect::<Vec<_>>()
                    .join(", "),
                observation.cards.draw.cards.len(),
                observation.cards.discard.cards.len(),
                observation.cards.exhaust.cards.len(),
                render_nonmaster_cards(observation)
            )
            .expect("string write");
            let potions = observation
                .potions
                .iter()
                .enumerate()
                .filter_map(|(slot, potion)| {
                    potion
                        .as_ref()
                        .map(|potion| format!("{slot}:{}", potion.potion_id.stable_id()))
                })
                .collect::<Vec<_>>();
            writeln!(
                output,
                "potions [{}]",
                if potions.is_empty() {
                    "empty".to_string()
                } else {
                    potions.join(", ")
                }
            )
            .expect("string write");
        }
    }
    let shop = matches!(
        &decision.observation,
        TaskPublicObservationV1::Strategic {
            observation: sts_oracle_learning::agent::task::TaskStrategicObservationV1 {
                context: sts_oracle_learning::ai::planner_core::PlannerDecisionContext::Shop { .. },
                ..
            }
        }
    );
    if shop && !shop_details {
        writeln!(
            output,
            "executable choices={} (inventory and choices not expanded; use --detail)",
            decision.candidates.len()
        )
        .expect("string write");
        return Ok(());
    }
    let choices_label = if shop {
        "executable choices"
    } else {
        "choices"
    };
    writeln!(
        output,
        "{choices_label} (engine indexes; ./p uses its own displayed rows):"
    )
    .expect("string write");
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        writeln!(
            output,
            "  {ordinal}: {}",
            render_candidate_with_observation(candidate, Some(&decision.observation))
        )
        .expect("string write");
    }
    Ok(())
}

fn render_card_multiset<I>(cards: I) -> String
where
    I: IntoIterator<Item = (sts_oracle_learning::content::cards::CardId, u8)>,
{
    let mut counts = BTreeMap::<String, usize>::new();
    for (card, upgrades) in cards {
        *counts.entry(format_card(card, upgrades)).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(card, count)| {
            if count == 1 {
                card
            } else {
                format!("{card}x{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_nonmaster_cards(observation: &PublicCombatStateV1) -> String {
    let mut surplus = BTreeMap::<String, i32>::new();
    for card in &observation.cards.master_deck.cards {
        *surplus
            .entry(format_card(card.card_id, card.upgrades))
            .or_default() -= 1;
    }
    for collection in [
        &observation.cards.hand,
        &observation.cards.draw,
        &observation.cards.discard,
        &observation.cards.exhaust,
        &observation.cards.limbo,
    ] {
        for card in &collection.cards {
            *surplus
                .entry(format_card(card.card_id, card.upgrades))
                .or_default() += 1;
        }
    }
    surplus
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(card, count)| {
            if count == 1 {
                card
            } else {
                format!("{card}x{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_candidate_compact(
    candidate: &TaskPublicCandidateV1,
    observation: &TaskPublicObservationV1,
) -> String {
    match candidate {
        TaskPublicCandidateV1::CombatAtomic {
            action:
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                },
        } => match observation {
            TaskPublicObservationV1::Combat { observation, .. } => observation
                .cards
                .hand
                .cards
                .get(*hand_index)
                .map(|card| {
                    let target = target_monster_index
                        .map(|target| format!("@{target}"))
                        .unwrap_or_default();
                    format!("{}{}", format_card(card.card_id, card.upgrades), target)
                })
                .unwrap_or_else(|| format!("PlayCard[{hand_index}]")),
            TaskPublicObservationV1::Strategic { .. } => format!("PlayCard[{hand_index}]"),
        },
        TaskPublicCandidateV1::CombatAtomic { action } => format!("{action:?}"),
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    event,
                    screen,
                    option_index,
                    action,
                    public_semantics,
                    ..
                },
        } => {
            let label = public_semantics
                .public_label
                .as_deref()
                .unwrap_or("<label-unavailable>");
            let mechanics = render_event_structured_semantics_v1(
                public_semantics.structured_semantics.as_ref(),
            );
            format!(
                "event:{}:s{screen}:o{option_index}:label={label:?}:{action:?}:mechanics=[{mechanics}]",
                event.stable_id()
            )
        }
        TaskPublicCandidateV1::Strategic { .. } => {
            render_candidate_with_observation(candidate, Some(observation))
        }
        TaskPublicCandidateV1::CombatIndexedChoice {
            reason, candidate, ..
        } => format!("{reason:?}:{candidate:?}"),
        TaskPublicCandidateV1::CombatSelectionFamily { family } => {
            format!("Select:{:?}", family.reason)
        }
    }
}

pub(crate) fn render_outcome_compact(outcome: &TaskEpisodeOutcomeV1) -> String {
    match outcome {
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::HeartVictory,
        } => "Heart victory".to_string(),
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory,
        } => "Act 3 victory".to_string(),
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::Defeat,
        } => "Run ended".to_string(),
        TaskEpisodeOutcomeV1::Paused { .. } => "Paused at a decision".to_string(),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::CommittedDefeat,
            ..
        } => "Defeat".to_string(),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::CommittedTerminalWithoutObjective,
            ..
        } => "Run ended before the objective".to_string(),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::RetryBudgetExhausted,
            ..
        } => "Stopped: retry budget exhausted".to_string(),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::TransitionBudgetExhausted,
            ..
        } => "Stopped: transition budget exhausted".to_string(),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::UnsupportedDecisionSurface,
            ..
        } => "Stopped: unsupported decision".to_string(),
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::MechanicsOrProtocolError { message },
            ..
        } => format!("Stopped: {message}"),
    }
}
