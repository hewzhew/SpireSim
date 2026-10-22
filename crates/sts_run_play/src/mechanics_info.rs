//! Read-only mechanics inspection from content definitions and public state.
//! No combat runtime, RNG, private move id, rollout or action resolution.
use std::collections::HashSet;
use std::fmt::Write;
use sts_oracle_learning::agent::information::run::PublicCombatRunContextV1;
use sts_oracle_learning::agent::information::state::{
    CombatLearningEnemyIdentityV1, CombatLearningPowerV1, PublicCombatStateV1,
};
use sts_oracle_learning::agent::task::{TaskPublicDecisionV1, TaskPublicObservationV1};
use sts_oracle_learning::content::identity::{
    gameplay_display_label_v1 as label, gameplay_identity_from_input_alias_v1 as resolve,
    GameplayIdentitySurface,
};
use sts_oracle_learning::content::monsters::encounter_pool::{
    public_encounter_pool, EncounterPoolTier,
};
use sts_oracle_learning::content::monsters::factory::EncounterId;
use sts_oracle_learning::content::{cards, monsters, potions, powers, relics};
use sts_oracle_learning::runtime::monster_move::{
    CardDestination, MoveStep, MoveTarget, SpawnHpValue, UtilityStep,
};
use sts_presentation::Locale;

mod deck_links;
pub(crate) use deck_links::write_owned_card_links;

#[derive(Clone, Copy)]
pub(crate) enum KeyRuleV1 {
    Timing,
    RubyOpportunities,
    ChestOpportunities,
    EmeraldOpportunities,
    EmeraldRewards,
    Ruby,
    Sapphire,
    Emerald,
    RubyDefinition,
    SapphireDefinition,
    EmeraldDefinition,
    Unavailable,
}

impl KeyRuleV1 {
    pub(crate) fn text(self, locale: Locale) -> &'static str {
        let key = match self {
            Self::Timing => "timing",
            Self::RubyOpportunities => "rubyOpportunities",
            Self::ChestOpportunities => "chestOpportunities",
            Self::EmeraldOpportunities => "emeraldOpportunities",
            Self::EmeraldRewards => "emeraldRewards",
            Self::Ruby => "ruby",
            Self::Sapphire => "sapphire",
            Self::Emerald => "emerald",
            Self::RubyDefinition => "rubyDefinition",
            Self::SapphireDefinition => "sapphireDefinition",
            Self::EmeraldDefinition => "emeraldDefinition",
            Self::Unavailable => "unavailable",
        };
        locale
            .entry("mechanics", &format!("key.{key}"))
            .expect("key rule message")
    }
}

fn render_key_rules(locale: Locale, rules: &[KeyRuleV1]) -> String {
    let mut out = rules
        .iter()
        .map(|rule| rule.text(locale))
        .collect::<Vec<_>>()
        .join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub(crate) fn write_key_status(
    out: &mut String,
    keys: [bool; 3],
    show_missing: bool,
    locale: Locale,
) {
    let message = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("decision", key, parameters)
            .expect("key status message")
    };
    let held: Vec<_> = ["ruby", "sapphire", "emerald"]
        .into_iter()
        .zip(keys)
        .filter_map(|(name, held)| held.then(|| message(&format!("keys.{name}"), &[])))
        .collect();
    writeln!(
        out,
        "{}",
        message(
            "keys.held",
            &[(
                "keys",
                &if held.is_empty() {
                    message("none", &[])
                } else {
                    held.join(", ")
                }
            )]
        )
    )
    .unwrap();
    if show_missing {
        let missing: Vec<_> = ["rubySource", "sapphireSource", "emeraldSource"]
            .into_iter()
            .zip(keys)
            .filter_map(|(source, held)| (!held).then(|| message(&format!("keys.{source}"), &[])))
            .collect();
        if !missing.is_empty() {
            writeln!(
                out,
                "{}",
                message("keys.missing", &[("keys", &missing.join("; "))])
            )
            .unwrap();
        }
    }
}
fn combat_rule_text(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("mechanics", &format!("combat.{key}"), parameters)
        .expect("combat rule message")
}

pub(crate) fn write_preparation_window(
    out: &mut String,
    observation: &sts_oracle_learning::agent::task::TaskStrategicObservationV1,
    locale: Locale,
) {
    use sts_oracle_learning::ai::planner_core::{PlannerDecisionContext, PlannerRunTransitionV1};
    let run = &observation.run;
    let message = |key, parameters: &[(&str, &str)]| {
        locale
            .text("decision", key, parameters)
            .expect("preparation window message")
    };
    if let Some(next_act) = run.pending_act_entry {
        if let Some(boss) = observation.public_map.boss {
            writeln!(
                out,
                "{}",
                message(
                    "preparation.completedBoss",
                    &[
                        ("act", &run.act.to_string()),
                        ("boss", locale.entity("encounter", boss.stable_id()))
                    ]
                )
            )
            .unwrap();
        }
        let entry_hp = sts_oracle_learning::state::run::RunState::dungeon_transition_hp(
            run.current_hp,
            run.max_hp,
            run.ascension_level,
        );
        writeln!(
            out,
            "{}",
            message(
                "preparation.entry",
                &[
                    ("act", &next_act.to_string()),
                    ("hp", &entry_hp.to_string()),
                    ("maxHp", &run.max_hp.to_string())
                ]
            )
        )
        .unwrap();
        if next_act <= 3 {
            writeln!(out, "{}", message("preparation.unknownMap", &[])).unwrap();
        } else {
            writeln!(
                out,
                "{}",
                message(
                    "preparation.act4",
                    &[
                        ("elites", locale.entity("encounter", "ShieldAndSpear")),
                        ("heart", locale.entity("encounter", "TheHeart"))
                    ]
                )
            )
            .unwrap();
        }
    } else if matches!(
        observation.context,
        PlannerDecisionContext::RunTransition {
            stage: PlannerRunTransitionV1::SecondBoss
        }
    ) {
        writeln!(out, "{}", message("preparation.secondBoss", &[])).unwrap();
    } else if let Some(boss) = observation.public_map.boss {
        writeln!(
            out,
            "{}",
            message(
                "preparation.boss",
                &[("boss", locale.entity("encounter", boss.stable_id()))]
            )
        )
        .unwrap();
    }
}

/// Rules accompany the facts that require them. This projection reads only the
/// supplied public state and difficulty-only content; it never resolves a move,
/// advances a simulation, accesses hidden pile order, or recommends an action.
pub(crate) fn write_combat_rules(out: &mut String, state: &PublicCombatStateV1, locale: Locale) {
    writeln!(
        out,
        "\n{}",
        locale
            .entry("combat", "rulesHeading")
            .expect("combat rule heading")
    )
    .unwrap();
    for key in ["handValues", "exhaustScope", "blockLifetime"] {
        writeln!(out, "{}", combat_rule_text(locale, key, &[])).unwrap();
    }
    let mut seen_cards = HashSet::new();
    for card in &state.cards.hand.cards {
        if seen_cards.insert((card.card_id, card.upgrades)) {
            write_card_rule(out, card.card_id, card.upgrades, locale);
        }
    }
    let mut seen_powers = HashSet::new();
    for (owner, entries) in std::iter::once((
        combat_rule_text(locale, "player", &[]),
        &state.player.powers,
    ))
    .chain(
        state
            .monsters
            .iter()
            .filter(|m| m.alive || m.half_dead)
            .map(|m| (format!("M{}", m.slot), &m.powers)),
    ) {
        for power in entries {
            if seen_powers.insert(power.power) {
                write_power(out, power.power, locale);
            }
            write_observed_amount(out, &owner, power, locale);
            if let Some(card) = &power.payload_card {
                writeln!(
                    out,
                    "{}",
                    combat_rule_text(
                        locale,
                        "payloadCard",
                        &[
                            ("owner", &owner),
                            (
                                "card",
                                &locale.card_name(card.card_id.stable_id(), card.upgrades)
                            )
                        ]
                    )
                )
                .unwrap();
            }
        }
    }
    for relic in &state.player.relics {
        writeln!(
            out,
            "{}",
            combat_rule_text(
                locale,
                "relic",
                &[
                    (
                        "name",
                        &crate::compact::relic_label_v1(
                            relic.id,
                            relic.counter,
                            relic.used_up,
                            locale
                        )
                    ),
                    (
                        "effect",
                        relics::mechanics::explanation(relic.id)
                            .unwrap_or(locale.entry("mechanics", "combat.unknownEffect").unwrap())
                    )
                ]
            )
        )
        .unwrap();
    }
    for (slot, potion) in state.potions.iter().enumerate() {
        if let Some(potion) = potion {
            writeln!(
                out,
                "{}",
                combat_rule_text(
                    locale,
                    "potion",
                    &[
                        ("slot", &slot.to_string()),
                        (
                            "name",
                            locale.entity("potion", potion.potion_id.stable_id())
                        ),
                        (
                            "usable",
                            locale
                                .entry("mechanics", &format!("potion.boolean.{}", potion.can_use))
                                .unwrap()
                        ),
                        (
                            "discardable",
                            locale
                                .entry(
                                    "mechanics",
                                    &format!("potion.boolean.{}", potion.can_discard)
                                )
                                .unwrap()
                        ),
                        (
                            "target",
                            locale
                                .entry(
                                    "mechanics",
                                    &format!("potion.boolean.{}", potion.requires_target)
                                )
                                .unwrap()
                        )
                    ]
                )
            )
            .unwrap();
            writeln!(
                out,
                "{}",
                combat_rule_text(
                    locale,
                    "potionRule",
                    &[(
                        "effect",
                        locale
                            .entry(
                                "game",
                                &format!("potionRule.{}", potion.potion_id.stable_id())
                            )
                            .expect("reviewed potion rule")
                    )]
                )
            )
            .unwrap();
        }
    }
    let mut seen_enemies = HashSet::new();
    for monster in state.monsters.iter().filter(|m| m.alive || m.half_dead) {
        let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        if seen_enemies.insert(enemy_id) {
            write_enemy(out, enemy_id, state.player.ascension_level, locale);
        }
    }
}

/// A short offer face; shared definitions still supply effects and upgrade facts.
/// Do not infer a complete effect from damage/block/draw alone.
pub(crate) fn write_card_offer(
    out: &mut String,
    card: cards::CardId,
    upgrades: u8,
    locale: Locale,
) {
    let definition = cards::get_card_definition(card);
    let message = |key, parameters: &[(&str, &str)]| {
        locale
            .text("mechanics", key, parameters)
            .expect("card offer message")
    };
    let cost =
        match cards::upgraded_base_cost_override_for(card, upgrades).unwrap_or(definition.cost) {
            -2 => message("card.unplayable", &[]),
            -1 => "X".into(),
            cost => message("card.paidCost", &[("cost", &cost.to_string())]),
        };
    let mut blueprint = sts_oracle_learning::runtime::combat::CardBlueprint::fresh(card);
    blueprint.upgrades = upgrades;
    let numbers = cards::mechanics::card_blueprint_persistent_base_numbers_v1(&blueprint);
    let mut face = vec![
        cost,
        card_type_name(definition.card_type, locale).to_owned(),
    ];
    if numbers.damage > 0 {
        face.push(message(
            "card.offerDamage",
            &[("amount", &numbers.damage.to_string())],
        ));
    }
    if numbers.block > 0 {
        face.push(message(
            "card.offerBlock",
            &[("amount", &numbers.block.to_string())],
        ));
    }
    let magic = definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic;
    if let Some(draw) = cards::mechanics::unconditional_card_draw_request_v1(card, magic) {
        face.push(message("card.offerDraw", &[("amount", &draw.to_string())]));
    }
    if cards::mechanics::card_exhausts_when_played(card, upgrades) {
        face.push(message("card.exhaustKeyword", &[]));
    }
    if cards::mechanics::card_is_ethereal(card, upgrades) {
        face.push(message("card.etherealKeyword", &[]));
    }
    writeln!(out, "  {}", face.join(" | ")).unwrap();
    if let Some(original) = cards::mechanics::original_printed_card_text(card, upgrades, locale)
        .expect("validated original card definition")
    {
        for line in original.lines() {
            writeln!(out, "  {line}").unwrap();
        }
    }
    match cards::mechanics::explanation(card) {
        Some(effect) => {
            writeln!(out, "{}", message("card.supplement", &[("effect", effect)])).unwrap()
        }
        None => writeln!(out, "  {}", message("card.ruleUnavailable", &[])).unwrap(),
    }
}

pub(crate) fn write_printed_card_rule(
    out: &mut String,
    card: cards::CardId,
    upgrades: u8,
    locale: Locale,
) {
    let message = |key, parameters: &[(&str, &str)]| {
        locale
            .text("mechanics", key, parameters)
            .expect("printed card rule message")
    };
    writeln!(
        out,
        "{}",
        message(
            "card.printedHeading",
            &[
                ("name", &locale.card_name(card.stable_id(), upgrades)),
                ("attributes", &card_attributes(card, upgrades, 0, locale)),
                (
                    "upgrade",
                    &message(
                        if upgrades == 0 {
                            "card.unupgraded"
                        } else {
                            "card.upgraded"
                        },
                        &[]
                    )
                )
            ]
        )
    )
    .unwrap();
    if let Some(original) = cards::mechanics::original_printed_card_text(card, upgrades, locale)
        .expect("validated original card definition")
    {
        for line in original.lines() {
            writeln!(out, "    {line}").unwrap();
        }
    }
    let effect = cards::mechanics::explanation(card)
        .map(str::to_owned)
        .unwrap_or_else(|| message("card.ruleUnavailable", &[]));
    writeln!(
        out,
        "{}",
        message("card.printedRule", &[("effect", &effect)])
    )
    .unwrap();
    write_card_fixed_requests(out, card, upgrades, locale);
}

pub(crate) fn write_card_rule(out: &mut String, card: cards::CardId, upgrades: u8, locale: Locale) {
    write_printed_card_rule(out, card, upgrades, locale);
    let message = |key, parameters: &[(&str, &str)]| {
        locale
            .text("mechanics", key, parameters)
            .expect("card lifecycle message")
    };
    let definition = cards::get_card_definition(card);
    let play = if definition.cost == -2 {
        "card.playUnplayable"
    } else if definition.card_type == cards::CardType::Power {
        "card.playPower"
    } else if cards::mechanics::card_exhausts_when_played(card, upgrades) {
        "card.playExhaust"
    } else {
        "card.playDiscard"
    };
    let unused = if cards::mechanics::card_is_ethereal(card, upgrades) {
        "card.unusedEthereal"
    } else {
        "card.unusedDiscard"
    };
    writeln!(
        out,
        "{}",
        message(
            "card.lifecycle",
            &[
                ("play", &message(play, &[])),
                ("unused", &message(unused, &[]))
            ]
        )
    )
    .unwrap();
}

pub(crate) fn write_card_fixed_requests(
    out: &mut String,
    card: cards::CardId,
    upgrades: u8,
    locale: Locale,
) {
    let definition = cards::get_card_definition(card);
    let magic = definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic;
    if let Some(draw) = cards::mechanics::unconditional_card_draw_request_v1(card, magic) {
        writeln!(
            out,
            "    {}",
            locale
                .text(
                    "mechanics",
                    "card.fixedDraw",
                    &[("amount", &draw.to_string())]
                )
                .expect("fixed draw message")
        )
        .unwrap();
    }
    if cards::mechanics::card_base_block_delivery_v1(card)
        == Some(cards::mechanics::CardBaseBlockDeliveryV1::OnceOnPlay)
    {
        let facts = sts_oracle_learning::ai::card_semantics_v1::card_reward_facts_v1(
            &sts_oracle_learning::state::rewards::RewardCard::new(card, upgrades),
        );
        if facts.block > 0 {
            writeln!(
                out,
                "    {}",
                locale
                    .text(
                        "mechanics",
                        "card.fixedBlock",
                        &[("amount", &facts.block.to_string())]
                    )
                    .expect("fixed block message")
            )
            .unwrap();
        }
    }
}

pub(crate) fn render(
    decision: Option<&TaskPublicDecisionV1>,
    query: &str,
    locale: Locale,
) -> Result<String, String> {
    let combat = decision.and_then(|d| match &d.observation {
        TaskPublicObservationV1::Combat { observation, .. } => Some(observation),
        _ => None,
    });
    let ascension = decision.map(|d| match &d.observation {
        TaskPublicObservationV1::Combat { observation, .. } => observation.player.ascension_level,
        TaskPublicObservationV1::Strategic { observation } => observation.run.ascension_level,
    });
    render_query(
        combat,
        ascension,
        decision.map(|d| &d.observation),
        query,
        locale,
    )
}

fn render_query(
    combat: Option<&PublicCombatStateV1>,
    ascension: Option<u8>,
    current_observation: Option<&TaskPublicObservationV1>,
    query: &str,
    locale: Locale,
) -> Result<String, String> {
    let query = query.trim();
    let lower = query.to_ascii_lowercase();
    let mut out = String::new();
    let message = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("mechanics", key, parameters)
            .expect("mechanics query message")
    };
    let run_keys = current_observation.and_then(|observation| match observation {
        TaskPublicObservationV1::Strategic { observation } => Some(observation.run.keys),
        TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Available { keys, .. },
            ..
        } => Some(*keys),
        _ => None,
    });
    if lower == "rules" || lower == "rules preparation" {
        let preparation = lower == "rules preparation";
        let asc = ascension.ok_or_else(|| message("query.ascensionUnavailable", &[]))?;
        let healing = if asc >= 5 {
            message("run.partialHeal", &[])
        } else {
            message("run.fullHeal", &[])
        };
        writeln!(
            out,
            "{}",
            message("run.heading", &[("ascension", &asc.to_string())])
        )
        .unwrap();
        for key in ["hand", "removeEthereal", "exhaustOnPlay"] {
            writeln!(out, "{}", message(&format!("run.{key}"), &[])).unwrap();
        }
        for key in ["exhaustScope", "blockLifetime"] {
            writeln!(out, "{}", combat_rule_text(locale, key, &[])).unwrap();
        }
        writeln!(out, "{}", message("run.actEntry", &[("healing", &healing)])).unwrap();
        writeln!(out, "{}", combat_rule_text(locale, "shopRemoval", &[])).unwrap();
        for key in ["a20Bosses", "transitionQuery"] {
            writeln!(out, "{}", message(&format!("run.{key}"), &[])).unwrap();
        }
        let goal = current_observation.and_then(|observation| match observation {
            TaskPublicObservationV1::Strategic { observation } => Some(observation.run_goal),
            TaskPublicObservationV1::Combat {
                run_context: PublicCombatRunContextV1::Available { run_goal, .. },
                ..
            } => Some(*run_goal),
            _ => None,
        });
        let heart_goal =
            goal == Some(sts_oracle_learning::ai::planner_core::PlannerRunGoal::HeartVictory);
        let public_run = current_observation.and_then(|observation| match observation {
            TaskPublicObservationV1::Strategic { observation } => {
                write_preparation_window(&mut out, observation, locale);
                Some(match observation.run.pending_act_entry {
                    Some(act) => (act, None),
                    None if matches!(observation.context, sts_oracle_learning::ai::planner_core::PlannerDecisionContext::RunTransition { .. }) => (observation.run.act, None),
                    None => (observation.run.act, observation.public_map.boss),
                })
            }
            TaskPublicObservationV1::Combat {
                run_context:
                    PublicCombatRunContextV1::Available {
                        act, public_map, ..
                    },
                ..
            } => Some((*act, public_map.boss)),
            _ => None,
        });
        let pending_act_entry = matches!(current_observation,
            Some(TaskPublicObservationV1::Strategic { observation })
                if observation.run.pending_act_entry.is_some());
        if let Some((act, boss)) = public_run {
            if !preparation && !pending_act_entry {
                let catalogue = crate::deck_lab::available_for_public_run(act, boss)
                    .iter()
                    .map(|spec| spec.encounter.stable_id())
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    out,
                    "\n{}",
                    message(
                        "run.catalogue",
                        &[("act", &act.to_string()), ("catalogue", &catalogue)]
                    )
                )
                .unwrap();
            } else if !preparation {
                writeln!(out, "{}", message("run.preEntryCatalogue", &[])).unwrap();
            }
            let elites = public_encounter_pool(act, EncounterPoolTier::Elite);
            if !elites.is_empty() {
                if act == 4 {
                    writeln!(out, "\n{}", message("run.fixedElite", &[])).unwrap();
                } else {
                    let names = elites
                        .iter()
                        .map(|entry| {
                            format!(
                                "{} [{}]",
                                locale.entity("encounter", entry.encounter.stable_id()),
                                entry.encounter.stable_id()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let key = if pending_act_entry {
                        "run.preparationElites"
                    } else {
                        "run.currentElites"
                    };
                    writeln!(
                        out,
                        "\n{}",
                        message(key, &[("act", &act.to_string()), ("names", &names)])
                    )
                    .unwrap();
                }
                write_encounter_preparation_links(
                    &mut out,
                    &elites
                        .iter()
                        .map(|entry| entry.encounter)
                        .collect::<Vec<_>>(),
                    asc,
                    locale,
                );
                if !preparation {
                    for entry in elites {
                        write_encounter(&mut out, entry.encounter, asc, locale)?;
                    }
                }
            }
            if let Some(boss) = boss {
                writeln!(
                    out,
                    "\n{}",
                    message(
                        "run.publicBoss",
                        &[
                            ("act", &act.to_string()),
                            ("boss", locale.entity("encounter", boss.stable_id()))
                        ]
                    )
                )
                .unwrap();
                if preparation {
                    if boss != EncounterId::TheHeart || !heart_goal {
                        write_encounter_preparation_links(&mut out, &[boss], asc, locale);
                    }
                } else if heart_goal && boss == EncounterId::TheHeart {
                    writeln!(out, "{}", message("run.objectiveReference", &[])).unwrap();
                } else {
                    write_encounter(&mut out, boss, asc, locale)?;
                }
            }
        }
        if heart_goal {
            if let Some(keys) = run_keys {
                out.push('\n');
                write_key_status(&mut out, keys, true, locale);
                if keys.contains(&false) {
                    writeln!(out, "{}", KeyRuleV1::Timing.text(locale)).unwrap();
                }
                for (held, opportunities) in keys.into_iter().zip([
                    KeyRuleV1::RubyOpportunities.text(locale),
                    KeyRuleV1::ChestOpportunities.text(locale),
                    KeyRuleV1::EmeraldOpportunities.text(locale),
                ]) {
                    if !held {
                        writeln!(out, "{opportunities}").unwrap();
                    }
                }
            }
            let route = sts_oracle_learning::state::map::generator::ENDING_ROOM_SEQUENCE
                .iter()
                .map(|room| {
                    use sts_oracle_learning::state::map::node::RoomType;
                    let key = match room {
                        RoomType::RestRoom => "rest",
                        RoomType::ShopRoom => "shop",
                        RoomType::MonsterRoomElite => "elite",
                        RoomType::MonsterRoomBoss => "boss",
                        RoomType::TrueVictoryRoom => "victory",
                        RoomType::EventRoom => "questionMark",
                        RoomType::MonsterRoom => "monster",
                        RoomType::TreasureRoom => "treasure",
                    };
                    locale
                        .entry("decision", &format!("room.{key}"))
                        .expect("room name")
                })
                .collect::<Vec<_>>()
                .join(" -> ");
            writeln!(
                out,
                "\n{}",
                message("run.heartAccess", &[("route", &route)])
            )
            .unwrap();
            if preparation {
                write_encounter_preparation_links(&mut out, &[EncounterId::TheHeart], asc, locale);
            } else {
                writeln!(out, "\n{}", message("run.objectiveHeading", &[])).unwrap();
                write_encounter(&mut out, EncounterId::TheHeart, asc, locale)?;
            }
        }
        return Ok(out);
    }
    if lower == "transition" || lower.starts_with("transition ") {
        let hp = query["transition".len()..]
            .trim()
            .parse::<i32>()
            .map_err(|_| message("query.transitionInteger", &[]))?;
        let max_hp = current_observation
            .map(|observation| match observation {
                TaskPublicObservationV1::Strategic { observation } => observation.run.max_hp,
                TaskPublicObservationV1::Combat { observation, .. } => observation.player.max_hp,
            })
            .ok_or_else(|| message("query.maxHpUnavailable", &[]))?;
        let asc = ascension.ok_or_else(|| message("query.ascensionUnavailable", &[]))?;
        if hp < 1 || hp > max_hp {
            return Err(message(
                "query.transitionRange",
                &[("maxHp", &max_hp.to_string())],
            ));
        }
        let after =
            sts_oracle_learning::state::run::RunState::dungeon_transition_hp(hp, max_hp, asc);
        let parameters = [
            ("ascension", asc.to_string()),
            ("hp", hp.to_string()),
            ("maxHp", max_hp.to_string()),
            ("missing", (max_hp - hp).to_string()),
            ("healed", (after - hp).to_string()),
            ("after", after.to_string()),
        ];
        let parameters: Vec<_> = parameters
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        for key in ["heading", "missing", "recovery", "scope"] {
            writeln!(
                out,
                "{}",
                message(&format!("transition.{key}"), &parameters)
            )
            .unwrap();
        }
        return Ok(out);
    }
    match lower.replace([' ', '_', '-'], "").as_str() {
        "keys" => {
            if let Some(keys) = run_keys {
                write_key_status(&mut out, keys, true, locale);
            } else if current_observation.is_some() {
                writeln!(out, "{}", KeyRuleV1::Unavailable.text(locale)).unwrap();
            }
            out.push_str(&render_key_rules(
                locale,
                &[
                    KeyRuleV1::Timing,
                    KeyRuleV1::Ruby,
                    KeyRuleV1::RubyOpportunities,
                    KeyRuleV1::Sapphire,
                    KeyRuleV1::Emerald,
                    KeyRuleV1::EmeraldRewards,
                    KeyRuleV1::EmeraldOpportunities,
                    KeyRuleV1::ChestOpportunities,
                ],
            ));
            return Ok(out);
        }
        "rubykey" => {
            return Ok(render_key_rules(
                locale,
                &[
                    KeyRuleV1::RubyDefinition,
                    KeyRuleV1::Timing,
                    KeyRuleV1::RubyOpportunities,
                ],
            ))
        }
        "sapphirekey" => {
            return Ok(render_key_rules(
                locale,
                &[KeyRuleV1::SapphireDefinition, KeyRuleV1::ChestOpportunities],
            ))
        }
        "emeraldkey" => {
            return Ok(render_key_rules(
                locale,
                &[
                    KeyRuleV1::EmeraldDefinition,
                    KeyRuleV1::EmeraldRewards,
                    KeyRuleV1::Timing,
                    KeyRuleV1::EmeraldOpportunities,
                ],
            ))
        }
        _ => {}
    }
    if lower == "help" {
        return Ok(message("query.help", &[]));
    }
    if lower == "event" || lower.starts_with("event ") {
        return Err(message("query.currentEvent", &[]));
    }
    if lower.starts_with("encounter ") {
        let name = query[10..].trim();
        let encounter = resolve::<EncounterId>(name)
            .ok_or_else(|| message("query.unknownEncounter", &[("name", name)]))?;
        write_encounter(&mut out, encounter, ascension.unwrap_or(0), locale)?;
        return Ok(out);
    }
    if lower.starts_with("enemy ") {
        let name = query[6..].trim();
        if resolve::<monsters::EnemyId>(name).is_none() {
            if let Some(encounter) = resolve::<EncounterId>(name) {
                write_encounter(&mut out, encounter, ascension.unwrap_or(0), locale)?;
                return Ok(out);
            }
        }
        let enemy = resolve::<monsters::EnemyId>(name)
            .ok_or_else(|| message("query.unknownEnemy", &[("name", name)]))?;
        write_enemy(&mut out, enemy, ascension.unwrap_or(0), locale);
        return Ok(out);
    }
    if lower.starts_with("power ") {
        let name = query[6..].trim();
        let matches = matching_powers(name);
        if matches.is_empty() {
            return Err(message("query.unknownPower", &[("name", name)]));
        }
        write_power_matches(&mut out, &matches, None, locale);
        return Ok(out);
    }
    if lower.starts_with("card ") {
        let (name, upgrades) = crate::selector::parse_card_upgrade(query[5..].trim(), query)
            .map_err(|_| message("query.cardSyntax", &[("query", query)]))?;
        // Exact simulator IDs are explicit identities, even when another card
        // shares their display name (e.g. the four starter Strikes).
        let card = cards::CardId::from_stable_id(name)
            .or_else(|| resolve::<cards::CardId>(name))
            .ok_or_else(|| message("query.unknownCard", &[("name", name)]))?;
        write_card(&mut out, card, upgrades.unwrap_or(0), locale)?;
        return Ok(out);
    }
    if lower.starts_with("relic ") {
        let name = query[6..].trim();
        let relic = resolve::<relics::RelicId>(name)
            .ok_or_else(|| message("query.unknownRelic", &[("name", name)]))?;
        write_relic(&mut out, relic, current_observation);
        return Ok(out);
    }
    if lower.starts_with("potion ") {
        let name = query[7..].trim();
        let potion = resolve::<potions::PotionId>(name).ok_or_else(|| {
            locale
                .text("mechanics", "potion.unknown", &[("name", name)])
                .expect("potion error message")
        })?;
        write_potion(&mut out, potion, locale);
        return Ok(out);
    }
    if lower == "player" || lower == "powers" {
        let Some(state) = combat else {
            return Err(message("query.powersNeedCombat", &[]));
        };
        writeln!(out, "{}", combat_rule_text(locale, "playerPowers", &[])).unwrap();
        write_current_powers(&mut out, &state.player.powers, locale);
        if lower == "powers" {
            for monster in &state.monsters {
                if !monster.alive && !monster.half_dead {
                    continue;
                }
                let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
                writeln!(
                    out,
                    "\n{}",
                    combat_rule_text(
                        locale,
                        "enemyPowers",
                        &[
                            ("slot", &monster.slot.to_string()),
                            ("name", locale.entity("enemy", enemy_id.stable_id()))
                        ]
                    )
                )
                .unwrap();
                write_current_powers(&mut out, &monster.powers, locale);
            }
        }
        return Ok(out);
    }
    if let Some(slot) = lower
        .strip_prefix('m')
        .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
    {
        let slot: u8 = slot.parse().map_err(|_| message("query.slotRange", &[]))?;
        let state = combat.ok_or_else(|| message("query.slotsNeedCombat", &[]))?;
        let monster = state
            .monsters
            .iter()
            .find(|m| m.slot == slot)
            .ok_or_else(|| message("query.slotAbsent", &[("slot", &slot.to_string())]))?;
        let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
        writeln!(
            out,
            "{}",
            combat_rule_text(
                locale,
                "enemyStatus",
                &[
                    ("slot", &slot.to_string()),
                    ("name", locale.entity("enemy", enemy_id.stable_id())),
                    ("hp", &monster.hp.to_string()),
                    ("maxHp", &monster.max_hp.to_string()),
                    ("block", &monster.block.to_string())
                ]
            )
        )
        .unwrap();
        if !monster.alive && !monster.half_dead {
            writeln!(out, "{}", combat_rule_text(locale, "inactive", &[])).unwrap();
        } else {
            writeln!(
                out,
                "{}",
                combat_rule_text(
                    locale,
                    "intent",
                    &[(
                        "intent",
                        &crate::decision::combat::intent_label(&monster.intent, locale)
                    )]
                )
            )
            .unwrap();
        }
        write_current_powers(&mut out, &monster.powers, locale);
        write_enemy(&mut out, enemy_id, state.player.ascension_level, locale);
        return Ok(out);
    }
    // An explicit card upgrade identifies a card definition even when the
    // unqualified name also denotes a Power, e.g. Corruption+.
    if query.contains(['+', '\u{ff0b}']) {
        let (name, upgrades) = crate::selector::parse_card_upgrade(query, query)
            .map_err(|_| message("query.cardSyntax", &[("query", query)]))?;
        let card = cards::CardId::from_stable_id(name)
            .or_else(|| resolve::<cards::CardId>(name))
            .ok_or_else(|| message("query.unknownCard", &[("name", name)]))?;
        write_card(&mut out, card, upgrades.unwrap_or(0), locale)?;
        return Ok(out);
    }
    let matches = matching_powers(query);
    if !matches.is_empty() {
        write_power_matches(&mut out, &matches, combat, locale);
        return Ok(out);
    }
    if let Some(enemy) = resolve::<monsters::EnemyId>(query) {
        write_enemy(
            &mut out,
            enemy,
            ascension.ok_or_else(|| message("query.enemyAscensionUnavailable", &[]))?,
            locale,
        );
        return Ok(out);
    }
    if let Some(card) = resolve::<cards::CardId>(query) {
        write_card(&mut out, card, 0, locale)?;
        return Ok(out);
    }
    if let Some(relic) = resolve::<relics::RelicId>(query) {
        write_relic(&mut out, relic, current_observation);
        return Ok(out);
    }
    if let Some(potion) = resolve::<potions::PotionId>(query) {
        write_potion(&mut out, potion, locale);
        return Ok(out);
    }
    if let Some(encounter) = resolve::<EncounterId>(query) {
        write_encounter(&mut out, encounter, ascension.unwrap_or(0), locale)?;
        return Ok(out);
    }
    Err(message("query.unknown", &[("query", query)]))
}

fn matching_powers(query: &str) -> Vec<powers::PowerId> {
    powers::PowerId::STABLE_IDS
        .iter()
        .filter_map(|id| powers::PowerId::from_stable_id(id))
        .filter(|power| power.matches_input_alias_v1(query))
        .collect()
}

fn write_power_matches(
    out: &mut String,
    matches: &[powers::PowerId],
    combat: Option<&PublicCombatStateV1>,
    locale: Locale,
) {
    if matches.len() > 1 {
        writeln!(out, "{}", combat_rule_text(locale, "multiplePowers", &[])).unwrap();
    }
    for &power in matches {
        write_power(out, power, locale);
        if let Some(state) = combat {
            for p in state.player.powers.iter().filter(|p| p.power == power) {
                write_observed_amount(out, &combat_rule_text(locale, "player", &[]), p, locale);
            }
            for monster in &state.monsters {
                if !monster.alive && !monster.half_dead {
                    continue;
                }
                for p in monster.powers.iter().filter(|p| p.power == power) {
                    write_observed_amount(out, &format!("M{}", monster.slot), p, locale);
                }
            }
        }
    }
}

fn write_potion(out: &mut String, potion: potions::PotionId, locale: Locale) {
    let definition = potions::get_potion_definition(potion);
    let rarity = match definition.rarity {
        potions::PotionRarity::Common => "Common",
        potions::PotionRarity::Uncommon => "Uncommon",
        potions::PotionRarity::Rare => "Rare",
    };
    let target = if definition.target_required {
        "true"
    } else {
        "false"
    };
    let header = locale
        .text(
            "mechanics",
            "potion.definition",
            &[
                ("name", locale.entity("potion", potion.stable_id())),
                ("id", potion.stable_id()),
                (
                    "rarity",
                    locale
                        .entry("mechanics", &format!("potion.rarity.{rarity}"))
                        .expect("potion rarity"),
                ),
                ("potency", &definition.base_potency.to_string()),
                (
                    "target",
                    locale
                        .entry("mechanics", &format!("potion.boolean.{target}"))
                        .expect("potion target flag"),
                ),
            ],
        )
        .expect("potion definition message");
    writeln!(out, "{header}").unwrap();
    let effect = locale
        .entry("game", &format!("potionRule.{}", potion.stable_id()))
        .expect("reviewed potion rule");
    writeln!(
        out,
        "{}",
        locale
            .text("mechanics", "potion.effect", &[("effect", effect)])
            .expect("potion effect message")
    )
    .unwrap();
}

fn write_relic(
    out: &mut String,
    relic: relics::RelicId,
    current_observation: Option<&TaskPublicObservationV1>,
) {
    writeln!(
        out,
        "Relic definition: {} | {:?}",
        label(relic),
        relics::get_relic_tier(relic)
    )
    .unwrap();
    if let Some(observation) = current_observation {
        let held = match observation {
            TaskPublicObservationV1::Strategic { observation } => observation
                .relics
                .iter()
                .find(|r| r.relic == relic)
                .map(|r| (r.counter, r.used_up)),
            TaskPublicObservationV1::Combat { observation, .. } => observation
                .player
                .relics
                .iter()
                .find(|r| r.id == relic)
                .map(|r| (r.counter, r.used_up)),
        };
        if let Some((counter, used_up)) = held {
            write!(out, "  Currently held: yes").unwrap();
            if counter >= 0 {
                write!(out, " | Counter: {counter}").unwrap();
            }
            if let Some(status) = relics::mechanics::trigger_state_description(relic, used_up) {
                write!(out, " | {status}").unwrap();
            }
            writeln!(out).unwrap();
        } else {
            writeln!(out, "  Currently held: no").unwrap();
        }
    }
    match relics::mechanics::explanation(relic) {
        Some(effect) => writeln!(out, "  Effect: {effect}").unwrap(),
        None => writeln!(out, "  Unknown: effect explanation not yet covered.").unwrap(),
    }
}

fn card_type_name(kind: cards::CardType, locale: Locale) -> &'static str {
    let key = match kind {
        cards::CardType::Attack => "attack",
        cards::CardType::Skill => "skill",
        cards::CardType::Power => "power",
        cards::CardType::Status => "status",
        cards::CardType::Curse => "curse",
    };
    locale
        .entry("game", &format!("cardType.{key}"))
        .expect("card type name")
}

/// Printed card facts shared by inventory and strategic choices. Costs include
/// the actual upgrade and permanent modifier, never hypothetical combat powers.
pub(crate) fn card_attributes(
    card: cards::CardId,
    upgrades: u8,
    cost_modifier: i8,
    locale: Locale,
) -> String {
    let definition = cards::get_card_definition(card);
    let base_cost =
        cards::upgraded_base_cost_override_for(card, upgrades).unwrap_or(definition.cost);
    let cost = match base_cost {
        -2 => locale
            .entry("mechanics", "card.unplayable")
            .unwrap()
            .to_owned(),
        -1 => "X".to_owned(),
        n => locale
            .text(
                "mechanics",
                "card.paidCost",
                &[(
                    "cost",
                    &(i32::from(n) + i32::from(cost_modifier)).max(0).to_string(),
                )],
            )
            .expect("card cost message"),
    };
    let tags = if definition.tags.is_empty() {
        locale.entry("mechanics", "card.none").unwrap().to_owned()
    } else {
        definition
            .tags
            .iter()
            .map(|tag| format!("{tag:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let kind = card_type_name(definition.card_type, locale);
    locale
        .text(
            "mechanics",
            "card.inventoryAttributes",
            &[("type", kind), ("cost", &cost), ("tags", &tags)],
        )
        .expect("card attribute message")
}

fn write_card(
    out: &mut String,
    card: cards::CardId,
    upgrades: u8,
    locale: Locale,
) -> Result<(), String> {
    let definition = cards::get_card_definition(card);
    let original = cards::mechanics::original_printed_card_text(card, upgrades, locale)?;
    let message = |key, parameters: &[(&str, &str)]| {
        locale
            .text("mechanics", key, parameters)
            .expect("card query message")
    };
    let name = locale.card_name(card.stable_id(), upgrades);
    let kind = card_type_name(definition.card_type, locale);
    let upgrade = message(
        if upgrades == 0 {
            "card.unupgraded"
        } else {
            "card.upgraded"
        },
        &[],
    );
    writeln!(
        out,
        "{}",
        message(
            "card.definition",
            &[
                ("name", &name),
                ("id", card.stable_id()),
                ("type", kind),
                ("upgrade", &upgrade)
            ]
        )
    )
    .unwrap();
    let cost =
        match cards::upgraded_base_cost_override_for(card, upgrades).unwrap_or(definition.cost) {
            -2 => message("card.unplayable", &[]),
            -1 => "X".to_string(),
            cost => cost.to_string(),
        };
    let mut blueprint = sts_oracle_learning::runtime::combat::CardBlueprint::fresh(card);
    blueprint.upgrades = upgrades;
    let numbers = cards::mechanics::card_blueprint_persistent_base_numbers_v1(&blueprint);
    let boolean = |value: bool| message(if value { "card.yes" } else { "card.no" }, &[]);
    writeln!(
        out,
        "{}",
        message(
            "card.attributes",
            &[
                ("cost", &cost),
                ("damage", &numbers.damage.to_string()),
                ("block", &numbers.block.to_string()),
                (
                    "exhaust",
                    &boolean(cards::mechanics::card_exhausts_when_played(card, upgrades))
                ),
                (
                    "ethereal",
                    &boolean(cards::mechanics::card_is_ethereal(card, upgrades))
                )
            ]
        )
    )
    .unwrap();
    writeln!(
        out,
        "{}",
        message(
            "card.tags",
            &[(
                "tags",
                &if definition.tags.is_empty() {
                    message("card.none", &[])
                } else {
                    definition
                        .tags
                        .iter()
                        .map(|tag| format!("{tag:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )]
        )
    )
    .unwrap();
    if let Some(cards::mechanics::CardBaseDamageDeliveryV1::Attack { targets, .. }) =
        cards::mechanics::card_base_damage_delivery_v1(card, upgrades)
    {
        let targets = match targets {
            cards::mechanics::CardAttackTargetsV1::SelectedEnemy => "card.oneEnemy",
            cards::mechanics::CardAttackTargetsV1::EachEnemy => "card.allEnemies",
            cards::mechanics::CardAttackTargetsV1::RandomEnemyPerHit => "card.randomEnemy",
        };
        writeln!(
            out,
            "{}",
            message("card.targets", &[("targets", &message(targets, &[]))])
        )
        .unwrap();
    }
    if let Some(original) = original {
        writeln!(out, "{}", message("card.printed", &[])).unwrap();
        for line in original.lines() {
            writeln!(out, "    {line}").unwrap();
        }
    }
    match cards::mechanics::explanation(card) {
        Some(effect) => {
            writeln!(out, "{}", message("card.supplement", &[("effect", effect)])).unwrap()
        }
        None => writeln!(out, "{}", message("card.unknown", &[])).unwrap(),
    }
    Ok(())
}

fn write_power(out: &mut String, power: powers::PowerId, locale: Locale) {
    writeln!(
        out,
        "{}",
        combat_rule_text(
            locale,
            "power",
            &[
                ("name", locale.entity("power", power.stable_id())),
                ("id", power.stable_id())
            ]
        )
    )
    .unwrap();
    match powers::mechanics::get(power) {
        Some(fact) => {
            writeln!(
                out,
                "{}",
                combat_rule_text(
                    locale,
                    "powerSupplement",
                    &[
                        ("unit", &fact.amount_unit(locale)),
                        ("effect", &fact.effect(locale)),
                        ("timing", &fact.timing(locale))
                    ]
                )
            )
            .unwrap();
        }
        None => {
            writeln!(out, "{}", combat_rule_text(locale, "unknownPower", &[])).unwrap();
        }
    }
}

fn write_observed_amount(
    out: &mut String,
    owner: &str,
    power: &CombatLearningPowerV1,
    locale: Locale,
) {
    let name = locale.entity("power", power.power.stable_id());
    let base = [("owner", owner), ("name", name)];
    if powers::uses_sentinel_amount(power.power) {
        writeln!(out, "{}", combat_rule_text(locale, "present", &base)).unwrap();
        return;
    }
    if power.power == powers::PowerId::Invincible {
        writeln!(
            out,
            "{}",
            combat_rule_text(
                locale,
                "invincible",
                &[
                    ("owner", owner),
                    ("name", name),
                    ("amount", &power.amount.to_string()),
                    ("cap", &power.extra_data.to_string())
                ]
            )
        )
        .unwrap();
        return;
    }
    if power.power == powers::PowerId::Combust {
        writeln!(
            out,
            "{}",
            combat_rule_text(
                locale,
                "combust",
                &[
                    ("owner", owner),
                    ("name", name),
                    ("damage", &power.amount.to_string()),
                    ("hpLoss", &power.extra_data.to_string())
                ]
            )
        )
        .unwrap();
        return;
    }
    let duration = powers::mechanics::get(power.power).is_some_and(|p| p.round_duration);
    writeln!(
        out,
        "{}",
        combat_rule_text(
            locale,
            if duration { "duration" } else { "amount" },
            &[
                ("owner", owner),
                ("name", name),
                ("amount", &power.amount.to_string())
            ]
        )
    )
    .unwrap();
    if duration && power.just_applied {
        writeln!(out, "{}", combat_rule_text(locale, "justApplied", &[])).unwrap();
    }
}

fn write_current_powers(out: &mut String, entries: &[CombatLearningPowerV1], locale: Locale) {
    if entries.is_empty() {
        writeln!(out, "{}", combat_rule_text(locale, "noPowers", &[])).unwrap();
    }
    for p in entries {
        write_power(out, p.power, locale);
        write_observed_amount(out, &combat_rule_text(locale, "observed", &[]), p, locale);
    }
}

fn target(target: MoveTarget) -> &'static str {
    match target {
        MoveTarget::Player => "player",
        MoveTarget::SelfTarget => "self",
        MoveTarget::AllMonsters => "all monsters",
    }
}

fn spawn_hp(value: SpawnHpValue) -> String {
    match value {
        SpawnHpValue::Rolled => "rolled spawn HP".into(),
        SpawnHpValue::Fixed(hp) => hp.to_string(),
        SpawnHpValue::SourceCurrentHp => "parent's remaining HP at split execution".into(),
        SpawnHpValue::SourceMaxHp => "parent's Max HP".into(),
    }
}

fn card_addition(c: &sts_oracle_learning::runtime::monster_move::AddCardStep) -> String {
    format!(
        "add {} {}{} to {}",
        c.amount,
        label(c.card_id),
        if c.upgraded { "+" } else { "" },
        match c.destination {
            CardDestination::Hand => "hand",
            CardDestination::Discard => "discard pile",
            CardDestination::DrawPileTop => "top of draw pile",
            CardDestination::DrawPileRandom => "draw pile at random positions",
        }
    )
}

/// A small index into the same public definitions expanded below. It is a
/// pool reading, without choosing an encounter or forecasting a rolled move.
fn write_encounter_preparation_links(
    out: &mut String,
    encounters: &[EncounterId],
    ascension: u8,
    locale: Locale,
) {
    let mut lines = Vec::new();
    for &encounter in encounters {
        let Some(lineup) = monsters::factory::encounter_lineup(encounter) else {
            continue;
        };
        let mut seen = HashSet::new();
        for &enemy in lineup.fixed.iter().chain(lineup.random_pool.iter()) {
            if !seen.insert(enemy) {
                continue;
            }
            let source = format!("{} / {}", encounter.stable_id(), label(enemy));
            if let Some(fact) = monsters::mechanics::get(enemy, ascension) {
                for (name, plan) in fact.moves {
                    for step in plan.steps {
                        if let MoveStep::AddCard(c) = step {
                            lines.push(format!("{source} / {name}: {}", card_addition(&c)));
                        }
                    }
                }
            }
            for (power, amount) in monsters::mechanics::starting_powers(enemy, ascension) {
                if let Some(rule) = powers::mechanics::get(power) {
                    lines.push(format!(
                        "{source} / starting {} {amount}: {} {}",
                        label(power),
                        rule.effect(Locale::En),
                        rule.timing(Locale::En)
                    ));
                }
            }
        }
    }
    if !lines.is_empty() {
        writeln!(
            out,
            "\n{}",
            locale
                .entry("mechanics", "run.preparationLinks")
                .expect("preparation links heading")
        )
        .unwrap();
        for line in lines {
            writeln!(out, "  {line}").unwrap();
        }
        writeln!(
            out,
            "{}",
            locale
                .entry("mechanics", "run.preparationTiming")
                .expect("preparation timing rule")
        )
        .unwrap();
    }
}

fn write_encounter(
    out: &mut String,
    encounter: EncounterId,
    ascension: u8,
    locale: Locale,
) -> Result<(), String> {
    let message = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("mechanics", &format!("encounter.{key}"), parameters)
            .expect("encounter query message")
    };
    let lineup = monsters::factory::encounter_lineup(encounter)
        .ok_or_else(|| message("unavailable", &[("id", encounter.stable_id())]))?;
    writeln!(
        out,
        "{}",
        message(
            "heading",
            &[
                ("id", encounter.stable_id()),
                ("ascension", &ascension.to_string()),
                ("name", locale.entity("encounter", encounter.stable_id()))
            ]
        )
    )
    .unwrap();
    writeln!(
        out,
        "{}",
        message(
            "fixed",
            &[(
                "members",
                &lineup
                    .fixed
                    .iter()
                    .map(|id| format!(
                        "{} [{}]",
                        locale.entity("enemy", id.stable_id()),
                        id.stable_id()
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )]
        )
    )
    .unwrap();
    let mut seen_members = HashSet::new();
    let mut members: Vec<_> = lineup
        .fixed
        .iter()
        .copied()
        .filter(|enemy| seen_members.insert(*enemy))
        .collect();
    if lineup.random_slots > 0 {
        let mut pool = Vec::new();
        let mut seen_pool = HashSet::new();
        for &enemy in lineup.random_pool {
            if seen_pool.insert(enemy) {
                if !members.contains(&enemy) {
                    members.push(enemy);
                }
                let weight = lineup.random_pool.iter().filter(|&&id| id == enemy).count();
                pool.push(message(
                    "weightedMember",
                    &[
                        ("name", locale.entity("enemy", enemy.stable_id())),
                        ("id", enemy.stable_id()),
                        ("weight", &weight.to_string()),
                        ("total", &lineup.random_pool.len().to_string()),
                    ],
                ));
            }
        }
        writeln!(
            out,
            "{}",
            message(
                "random",
                &[
                    ("slots", &lineup.random_slots.to_string()),
                    ("pool", &pool.join(", "))
                ]
            )
        )
        .unwrap();
    }
    for enemy in members {
        write_enemy(out, enemy, ascension, locale);
    }
    Ok(())
}

fn write_enemy(out: &mut String, enemy: monsters::EnemyId, ascension: u8, locale: Locale) {
    if locale != Locale::En {
        writeln!(
            out,
            "{}",
            combat_rule_text(
                locale,
                "enemySupplement",
                &[("name", locale.entity("enemy", enemy.stable_id()))]
            )
        )
        .unwrap();
    }
    writeln!(out, "Enemy definition: {} | A{ascension}", label(enemy)).unwrap();
    let (min_hp, max_hp) = monsters::get_hp_range(enemy, ascension);
    writeln!(out, "  Base starting HP range: {min_hp}..{max_hp}").unwrap();
    let Some(fact) = monsters::mechanics::get(enemy, ascension) else {
        writeln!(out, "  Unknown: move explanations not yet covered. Current visible intent and powers remain authoritative.").unwrap();
        return;
    };
    let mut referenced_powers = Vec::new();
    for (power, amount) in monsters::mechanics::starting_powers(enemy, ascension) {
        writeln!(out, "  Starting power: {} {amount}", label(power)).unwrap();
        referenced_powers.push(power);
    }
    writeln!(
        out,
        "Possible moves (base values before powers; not a prediction):"
    )
    .unwrap();
    let mut referenced_cards = Vec::new();
    for (name, plan) in fact.moves {
        for step in &plan.steps {
            match step {
                MoveStep::ApplyPower(p) => {
                    if !referenced_powers.contains(&p.power_id) {
                        referenced_powers.push(p.power_id);
                    }
                }
                MoveStep::AddCard(c) => {
                    let entry = (c.card_id, u8::from(c.upgraded));
                    if !referenced_cards.contains(&entry) {
                        referenced_cards.push(entry);
                    }
                }
                _ => {}
            }
        }
        let effects: Vec<_> = plan
            .steps
            .iter()
            .map(|step| match step {
                MoveStep::Attack(a) => format!(
                    "attack {} for {} base damage/hit (hits: {})",
                    target(a.target),
                    a.attack.base_damage,
                    a.attack.hits
                ),
                MoveStep::ApplyPower(p) => format!(
                    "apply {} {} to {}",
                    p.amount,
                    label(p.power_id),
                    target(p.target)
                ),
                MoveStep::GainBlock(b) => {
                    format!("gain {} Block for {}", b.amount, target(b.target))
                }
                MoveStep::Heal(h) => format!(
                    "heal {} for {} HP, up to Max HP",
                    target(h.target),
                    h.amount
                ),
                MoveStep::GainBlockRandomMonster(b) => format!(
                    "give {} Block to a random eligible other monster, or self if none remains",
                    b.amount
                ),
                MoveStep::StealGold(s) => format!("steal up to {} player Gold", s.amount),
                MoveStep::UpgradeCards(c) => format!(
                    "upgrade existing {} cards in draw and discard piles",
                    label(c.card_id)
                ),
                MoveStep::SpawnMonster(s) => format!(
                    "spawn {} (current HP: {}; Max HP: {})",
                    label(s.enemy_id),
                    spawn_hp(s.hp.current),
                    spawn_hp(s.hp.max)
                ),
                MoveStep::Suicide => "remove parent without death relic triggers".into(),
                MoveStep::Sleep => {
                    "sleep; no attack on this enemy turn (wake conditions below)".into()
                }
                MoveStep::Stun => "stunned; no attack on this enemy turn".into(),
                MoveStep::Charge => {
                    "charge; no attack on this enemy turn (charge cycle below)".into()
                }
                MoveStep::Escape => "escape from combat".into(),
                MoveStep::RemovePower(p) => {
                    format!("remove {} from {}", label(p.power_id), target(p.target))
                }
                MoveStep::Utility(UtilityStep::RemoveAllDebuffs { target: t }) => {
                    format!("remove all debuffs from {}", target(*t))
                }
                MoveStep::AddCard(c) => card_addition(c),
                _ => "Unknown: this effect has no explanation yet".into(),
            })
            .collect();
        writeln!(out, "  {name}: {}", effects.join("; ")).unwrap();
    }
    for (name, effect) in fact.special_moves {
        writeln!(out, "  {name}: {effect}").unwrap();
    }
    writeln!(out, "Rules: {}", fact.notes).unwrap();
    if !referenced_powers.is_empty() || !referenced_cards.is_empty() {
        writeln!(
            out,
            "Referenced effect definitions (not evidence that they are currently active):"
        )
        .unwrap();
        for power in referenced_powers {
            write_power(out, power, Locale::En);
        }
        for (card, upgrades) in referenced_cards {
            write_card_rule(out, card, upgrades, Locale::En);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_boss_rules_use_the_upcoming_act_and_actual_entry_healing() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let mut strategic: TaskPublicObservationV1 =
            serde_json::from_value(fixture["payload"]["decision"]["observation"].clone()).unwrap();
        let TaskPublicObservationV1::Strategic { observation } = &mut strategic else {
            unreachable!()
        };
        observation.run.act = 1;
        observation.run.ascension_level = 20;
        observation.run.current_hp = 32;
        observation.run.max_hp = 80;
        observation.run.pending_act_entry = Some(2);
        observation.public_map.boss = Some(EncounterId::Hexaghost);
        let text =
            super::render_query(None, Some(20), Some(&strategic), "rules", Locale::En).unwrap();
        assert!(text.contains("Completed Act 1 Boss: Hexaghost"));
        assert!(text.contains("transition recovery gives 68/80 HP"));
        assert!(text.contains("Preparation Act 2 elite encounter pool"));
        assert!(!text.contains("Enemy definition: Hexaghost"));
        assert!(!text.contains("Current Act 1 elite encounter pool"));
        let TaskPublicObservationV1::Strategic { observation } = &mut strategic else {
            unreachable!()
        };
        observation.run.act = 3;
        observation.run.pending_act_entry = None;
        observation.context =
            sts_oracle_learning::ai::planner_core::PlannerDecisionContext::RunTransition {
                stage: sts_oracle_learning::ai::planner_core::PlannerRunTransitionV1::SecondBoss,
            };
        let text =
            super::render_query(None, Some(20), Some(&strategic), "rules", Locale::En).unwrap();
        assert!(text.contains("Current HP carries into that fight"));
        assert!(!text.contains("transition recovery gives"));
    }

    #[test]
    fn elite_preparation_links_follow_the_current_pool_and_difficulty() {
        let mut act2 = String::new();
        write_encounter_preparation_links(
            &mut act2,
            &public_encounter_pool(2, EncounterPoolTier::Elite)
                .iter()
                .map(|entry| entry.encounter)
                .collect::<Vec<_>>(),
            20,
            Locale::En,
        );
        assert!(act2.contains("possible moves and starting effects"));
        assert!(act2.contains("Taskmaster / Scouring Whip: add 3 Wound to discard pile"));
        assert!(act2.contains("Book of Stabbing / starting Painful Stabs -1"));
        assert!(act2.contains(
            powers::core::painful_stabs::MECHANICS
                .effect(Locale::En)
                .as_str()
        ));
        assert!(act2.contains("a later reshuffle and draw"));
        let mut low = String::new();
        write_encounter_preparation_links(
            &mut low,
            &public_encounter_pool(2, EncounterPoolTier::Elite)
                .iter()
                .map(|entry| entry.encounter)
                .collect::<Vec<_>>(),
            0,
            Locale::En,
        );
        assert!(low.contains("Scouring Whip: add 1 Wound to discard pile"));
        assert!(!low.contains("Scouring Whip: add 3 Wound"));
        let mut act1 = String::new();
        write_encounter_preparation_links(
            &mut act1,
            &public_encounter_pool(1, EncounterPoolTier::Elite)
                .iter()
                .map(|entry| entry.encounter)
                .collect::<Vec<_>>(),
            20,
            Locale::En,
        );
        assert!(act1.contains("Dazed to discard pile"));
        assert!(!act1.contains("Taskmaster"));
        assert!(!act1.contains("Painful Stabs"));
    }

    #[test]
    fn all_implemented_enemy_queries_explain_moves_and_referenced_effects() {
        for asc in [0, 2, 17, 20] {
            for id in monsters::EnemyId::STABLE_IDS {
                let view = render_query(None, Some(asc), &format!("enemy {id}")).unwrap();
                assert!(!view.contains("Unknown:"), "A{asc} {id}: {view}");
            }
        }
        let chosen = render_query(None, Some(20), "enemy Chosen").unwrap();
        assert!(chosen.contains("Drain: apply 3 Weak to player; apply 3 Strength to self"));
        assert!(chosen.contains("Hex: apply 1 Hex to player"));
        assert!(chosen.contains("including a Skill or Power"));
        assert!(chosen.contains("random positions"));
        let growth = render_query(None, Some(20), "enemy SpireGrowth").unwrap();
        assert!(growth.contains("Constrict: apply 12 Constricted to player"));
        assert!(growth.contains("Block can absorb it"));
        let mass = render_query(None, Some(20), "enemy WrithingMass").unwrap();
        assert!(mass.contains("Starting power: Reactive -1"));
        assert!(mass.contains("Starting power: Malleable 3"));
        assert!(mass.contains("permanent master deck"));
        assert!(mass.contains("Multi-hit attacks can cause multiple rolls"));
        assert!(render_query(None, Some(20), "enemy Spiker")
            .unwrap()
            .contains("Starting power: Thorns 7"));
    }

    #[test]
    fn card_definition_queries_honor_the_requested_upgrade() {
        for (query, heading, numbers) in [
            (
                "card Body Slam+",
                "Body Slam+ | Attack | upgraded base",
                "Cost: 0",
            ),
            (
                "BodySlam+1",
                "Body Slam+ | Attack | upgraded base",
                "Cost: 0",
            ),
            (
                "card BodySlam+0",
                "Body Slam | Attack | unupgraded base",
                "Cost: 1",
            ),
            (
                "card Impervious+",
                "Impervious+ | Skill | upgraded base",
                "Base Block: 40",
            ),
            (
                "card Hemokinesis+",
                "Hemokinesis+ | Attack | upgraded base",
                "Base damage: 20",
            ),
            (
                "Corruption+",
                "Corruption+ | Power | upgraded base",
                "Cost: 2",
            ),
            (
                "card Apparition+",
                "Apparition+ | Skill | upgraded base",
                "Ethereal: false",
            ),
            (
                "card SearingBlow+2",
                "Searing Blow+2 | Attack | upgraded base",
                "Base damage: 21",
            ),
        ] {
            let view = render_query(None, Some(20), query).unwrap();
            assert!(view.contains(heading), "{query}: {view}");
            assert!(view.contains(numbers), "{query}: {view}");
            assert!(!view.contains("current instance"));
        }
        assert!(render_query(None, Some(20), "Corruption")
            .unwrap()
            .starts_with("Power:"));
        for query in ["card BodySlam++", "BodySlam+256", "card BodySlam＋"] {
            assert!(render_query(None, Some(20), query).is_err(), "{query}");
        }
    }

    #[test]
    fn contextual_card_rules_show_printed_cost_and_explicit_upgrade_state() {
        for (card, upgrades, expected) in [
            (cards::CardId::Impervious, 0, "Impervious | Skill | 2c"),
            (cards::CardId::FeelNoPain, 0, "Feel No Pain | Power | 1c"),
            (cards::CardId::Barricade, 0, "Barricade | Power | 3c"),
            (cards::CardId::Barricade, 1, "Barricade+ | Power | 2c"),
            (cards::CardId::BodySlam, 1, "Body Slam+ | Attack | 0c"),
            (
                cards::CardId::AscendersBane,
                0,
                "Ascender's Bane | Curse | unplayable",
            ),
        ] {
            let mut out = String::new();
            write_card_rule(&mut out, card, upgrades, Locale::En);
            assert!(out.contains(expected), "{out}");
            assert!(
                out.contains(if upgrades == 0 {
                    "printed, unupgraded"
                } else {
                    "printed, upgraded"
                }),
                "{out}"
            );
        }
    }

    #[test]
    fn contextual_draw_requests_resolve_the_displayed_upgrade() {
        for (card, base, upgraded) in [
            (cards::CardId::Offering, 3, 5),
            (cards::CardId::PommelStrike, 1, 2),
            (cards::CardId::BurningPact, 2, 3),
            (cards::CardId::DeepBreath, 1, 2),
        ] {
            for (upgrades, draw) in [(0, base), (1, upgraded)] {
                let mut out = String::new();
                write_card_rule(&mut out, card, upgrades, Locale::En);
                assert!(
                    out.contains(&format!(
                        "Printed draw request at this upgrade: {draw} cards."
                    )),
                    "{out}"
                );
            }
        }
        let mut conditional = String::new();
        write_card_rule(&mut conditional, cards::CardId::Dropkick, 1, Locale::En);
        assert!(!conditional.contains("Printed draw request"));
        let mut triggered = String::new();
        write_card_rule(&mut triggered, cards::CardId::Evolve, 0, Locale::En);
        assert!(!triggered.contains("Printed draw request"));
    }

    #[test]
    fn contextual_block_requests_resolve_upgrades_and_keep_conditional_delivery_separate() {
        for (card, base, upgraded) in [
            (cards::CardId::PowerThrough, 15, 20),
            (cards::CardId::TrueGrit, 7, 9),
            (cards::CardId::ShrugItOff, 8, 11),
        ] {
            for (upgrades, block) in [(0, base), (1, upgraded)] {
                let mut out = String::new();
                write_card_rule(&mut out, card, upgrades, Locale::En);
                assert!(
                    out.contains(&format!(
                        "Printed Block request at this upgrade: {block} Block"
                    )),
                    "{out}"
                );
            }
        }
        for card in [
            cards::CardId::SecondWind,
            cards::CardId::AutoShields,
            cards::CardId::BodySlam,
        ] {
            let mut out = String::new();
            write_card_rule(&mut out, card, 1, Locale::En);
            assert!(!out.contains("Printed Block request"), "{out}");
        }
    }

    #[test]
    fn printed_lifecycle_separates_play_exhaust_ethereal_and_power_deployment() {
        let read = |card, upgrades| {
            let mut out = String::new();
            write_card_rule(&mut out, card, upgrades, Locale::En);
            out
        };
        let armor = read(cards::CardId::GhostlyArmor, 0);
        assert!(armor.contains("when played, enter Discard after resolving its effects"));
        assert!(armor.contains("when unplayed, if still in Hand without Retain, enter Exhaust at turn end via Ethereal"));
        let apparition = read(cards::CardId::Apparition, 1);
        assert!(apparition.contains("when played, enter Exhaust after resolving its effects"));
        assert!(
            apparition.contains("when unplayed, follow ordinary turn-end discard and Retain rules")
        );
        let form = read(cards::CardId::DemonForm, 0);
        assert!(form.contains("when played, deploy the Power"));
        let bane = read(cards::CardId::AscendersBane, 0);
        assert!(bane.contains("printed unplayable"));
        assert!(!bane.contains("when played, enter Discard"));
    }

    fn render_query(
        combat: Option<&PublicCombatStateV1>,
        ascension: Option<u8>,
        query: &str,
    ) -> Result<String, String> {
        super::render_query(combat, ascension, None, query, Locale::En)
    }

    #[test]
    fn original_card_queries_localize_faces_and_preserve_native_rules_and_state() {
        let dir = crate::decision_tests::TestDirectory::new();
        let game = crate::decision_tests::controller_with_session(
            &dir.0,
            "card-text",
            &crate::decision_tests::fixture("reward"),
        );
        let before = game.head().clone();
        let decision = before
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        for &id in cards::CardId::STABLE_IDS {
            let card = cards::CardId::from_stable_id(id).unwrap();
            for upgrades in [0, 1] {
                let query = format!("card {id}{}", if upgrades == 0 { "" } else { "+" });
                for locale in [Locale::En, Locale::ZhCn] {
                    let view = super::render(Some(&decision), &query, locale).unwrap();
                    assert!(view.contains(id), "{query}/{locale}: {view}");
                    if let Some(rule) = cards::mechanics::explanation(card) {
                        assert!(
                            view.contains(rule),
                            "complete supplementary rule: {query}/{locale}"
                        );
                    }
                    if card != cards::CardId::Impulse {
                        let face =
                            cards::mechanics::original_printed_card_text(card, upgrades, locale)
                                .unwrap()
                                .unwrap();
                        for line in face.lines() {
                            assert!(view.contains(line), "{query}/{locale}: {view}");
                        }
                    }
                }
            }
        }
        let armaments = super::render(Some(&decision), "card Armaments+", Locale::ZhCn).unwrap();
        assert!(armaments.starts_with("卡牌定义：武装+ | ID：Armaments"));
        assert!(armaments.contains("所有牌"));
        assert!(armaments.contains("机制补充（英文）"));
        assert_eq!(
            *game.head(),
            before,
            "card rendering must not alter the bound run"
        );
    }

    #[test]
    fn every_potion_query_uses_shared_rules_and_keeps_a_copyable_stable_id() {
        for id in potions::PotionId::STABLE_IDS {
            for locale in [Locale::En, Locale::ZhCn] {
                let reading = super::render(None, &format!("potion {id}"), locale).unwrap();
                assert!(reading.contains(id));
                assert!(reading.contains(locale.entity("potion", id)));
                assert!(
                    reading.contains(locale.entry("game", &format!("potionRule.{id}")).unwrap())
                );
                assert_eq!(reading, super::render(None, id, locale).unwrap());
                assert_eq!(reading.lines().count(), 2);
            }
        }
        let failure = super::render(None, "potion 未知 {{keep}}", Locale::ZhCn).unwrap_err();
        assert_eq!(
            failure,
            "未知药水：未知 {{keep}}。请使用完整英文名称或稳定 ID。"
        );
    }

    #[test]
    fn first_act_elite_encounters_expose_members_and_ascension_effects() {
        let nob = render_query(None, Some(20), "encounter Gremlin Nob").unwrap();
        assert!(nob.contains("Fixed starting enemies: Gremlin Nob [GremlinNob]"));
        assert!(nob.contains("apply 3 Anger to self"));
        let laga = render_query(None, Some(20), "encounter Lagavulin").unwrap();
        assert!(laga.contains("Fixed starting enemies: Lagavulin [Lagavulin]"));
        assert!(laga.contains("apply -2 Dexterity to player"));
        let sentries = render_query(None, Some(20), "encounter ThreeSentries").unwrap();
        assert!(sentries
            .contains("Fixed starting enemies: Sentry [Sentry], Sentry [Sentry], Sentry [Sentry]"));
        assert_eq!(sentries.matches("Enemy definition: Sentry").count(), 1);
        assert!(sentries.contains("add 3 Dazed to discard pile"));
        for name in ["Sentries", "3 Sentries"] {
            assert_eq!(
                render_query(None, Some(20), &format!("encounter {name}")).unwrap(),
                sentries
            );
        }
    }

    #[test]
    fn act_two_elite_queries_expose_lineups_dynamic_hits_and_ally_effects() {
        let slavers = render_query(None, Some(20), "encounter Slavers").unwrap();
        for enemy in ["Blue Slaver", "Taskmaster", "Red Slaver"] {
            assert!(slavers.contains(&format!("Enemy definition: {enemy}")));
        }
        assert_eq!(
            render_query(None, Some(20), "enemy Slavers").unwrap(),
            slavers
        );
        assert_eq!(render_query(None, Some(20), "Slavers").unwrap(), slavers);
        let book = render_query(None, Some(20), "enemy BookOfStabbing").unwrap();
        assert!(book.contains("24 base damage/hit (hits: 1)"));
        assert!(book.contains("7 base damage per hit") && book.contains("persistent stab counter"));
        assert!(book.contains("Big Stab also increases the counter by 1"));
        assert!(book.contains("add 1 Wound") && book.contains("discard pile"));
        let leader = render_query(None, Some(20), "encounter GremlinLeader").unwrap();
        assert!(leader.contains("Random starting slots: 2"));
        assert!(leader.contains("[GremlinWarrior]: 2/8 per slot"));
        assert!(leader.contains("6 base damage/hit (hits: 3)"));
        assert!(leader.contains("5 Strength and 10 Block"));
        assert!(leader.contains("at most 3 occupied slots"));
        assert!(
            !leader.contains("Unknown:")
                && !book.contains("Unknown:")
                && !slavers.contains("Unknown:")
        );
        assert!(render_query(None, Some(20), "encounter Misspelled")
            .unwrap_err()
            .contains("Unknown encounter name"));
    }

    #[test]
    fn centurion_and_mystic_queries_explain_targets_frail_and_ambiguous_buff_icon() {
        let centurion = render_query(None, Some(20), "enemy Centurion").unwrap();
        assert!(centurion.contains("give 20 Block to a random eligible other monster"));
        assert!(centurion.contains("Fury: attack player for 7 base damage/hit (hits: 3)"));
        let mystic = render_query(None, Some(20), "enemy Mystic").unwrap();
        assert!(mystic.contains("apply 2 Frail to player"));
        assert!(mystic.contains("Heal: heal all monsters for 20 HP"));
        assert!(mystic.contains("Strengthen: apply 4 Strength to all monsters"));
        assert!(mystic.contains("icon alone does not distinguish them"));
        assert!(
            mystic.contains("Power: Frail")
                && mystic.contains("not evidence that they are currently active")
        );
        assert!(!centurion.contains("Unknown:") && !mystic.contains("Unknown:"));
    }

    #[test]
    fn first_act_enemy_queries_explain_static_and_conditional_effects() {
        let enemies = [
            "Jaw Worm",
            "Cultist",
            "Red Louse",
            "Green Louse",
            "AcidSlimeS",
            "AcidSlimeM",
            "AcidSlimeL",
            "SpikeSlimeS",
            "SpikeSlimeM",
            "SpikeSlimeL",
            "Fungi Beast",
            "Looter",
            "Blue Slaver",
            "Red Slaver",
            "Fat Gremlin",
            "Sneaky Gremlin",
            "Shield Gremlin",
            "Mad Gremlin",
            "Gremlin Wizard",
            "Gremlin Nob",
            "Lagavulin",
            "Sentry",
            "Slime Boss",
            "Hexaghost",
            "The Guardian",
        ];
        for asc in [0, 2, 4, 7, 9, 17, 18, 19, 20] {
            for name in enemies {
                let view = render_query(None, Some(asc), &format!("enemy {name}")).unwrap();
                assert!(!view.contains("Unknown:"), "A{asc} {name}: {view}");
                assert!(view.contains("not a prediction"));
            }
        }
        let guardian = render_query(None, Some(20), "enemy The Guardian").unwrap();
        assert!(guardian.contains("starts at 40 HP damage"));
        assert!(guardian.contains("raises the next threshold by 10"));
        assert!(guardian.contains("hits: 2"));
        assert!(guardian.contains("once per card, not once per hit"));
        let slime = render_query(None, Some(20), "enemy Slime Boss").unwrap();
        assert!(slime.contains("spawn Spike Slime (L)"));
        assert!(slime.contains("parent's remaining HP at split execution"));
        assert!(slime.contains("plans Slam next"));
        let louse = render_query(None, Some(20), "enemy Red Louse").unwrap();
        assert!(louse.contains("6..8 inclusive and stays fixed"));
        assert!(louse.contains("9..12 inclusive"));
        let looter = render_query(None, Some(20), "enemy Looter").unwrap();
        assert!(looter.contains("steal up to 20 player Gold"));
        assert!(looter.contains("Escape does not recover"));
        let laga = render_query(None, Some(20), "enemy Lagavulin").unwrap();
        assert!(laga.contains("apply -2 Dexterity to player"));
        assert!(laga.contains("Blocked damage does not wake"));
        let hexa = render_query(None, Some(20), "enemy Hexaghost").unwrap();
        assert!(hexa.contains("locks Divider's base damage"));
        assert!(hexa.contains("upgrade existing Burn cards in draw and discard piles"));
        for power in ["Mode Shift", "Split", "Metallicize", "Thievery"] {
            assert!(!render_query(None, Some(20), &format!("power {power}"))
                .unwrap()
                .contains("Unknown:"));
        }
    }

    #[test]
    fn same_name_power_queries_explain_each_identity_without_changing_selector_resolution() {
        assert!(resolve::<powers::PowerId>("Intangible").is_none());
        for query in ["power Intangible", "power intangible", "Intangible"] {
            let view = render_query(None, Some(20), query).unwrap();
            assert_eq!(view.matches("Power: Intangible | ID:").count(), 2);
            assert!(view.contains("ID: Intangible\n"));
            assert!(view.contains("ID: IntangiblePlayer\n"));
            assert!(view.contains("owner's turn end"));
            assert!(view.contains("after the enemy turn"));
            assert!(!view.contains("Unknown:"));
        }
        let player = render_query(None, Some(20), "power IntangiblePlayer").unwrap();
        assert_eq!(player.matches("Power: Intangible | ID:").count(), 1);
        assert!(player.contains("remaining rounds of player Intangible"));
        assert!(render_query(None, Some(20), "power Intang").is_err());
        for id in powers::PowerId::STABLE_IDS {
            let view = render_query(None, Some(20), &format!("power {id}")).unwrap();
            assert!(view.contains(&format!("ID: {id}\n")), "{id}: {view}");
        }
    }

    #[test]
    fn held_relic_queries_cover_the_observed_player_run() {
        for relic in [
            "Sling of Courage",
            "Ancient Tea Set",
            "Bloody Idol",
            "Mummified Hand",
            "Lee's Waffle",
            "Red Mask",
            "Toy Ornithopter",
            "Peace Pipe",
            "Potion Belt",
            "Boot",
            "Centennial Puzzle",
            "Meat on the Bone",
            "The Courier",
            "Akabeko",
            "Oddly Smooth Stone",
            "Singing Bowl",
            "Magic Flower",
            "Strange Spoon",
        ] {
            let view = render_query(None, Some(20), &format!("relic {relic}")).unwrap();
            assert!(view.contains("Effect:"), "{view}");
            assert!(!view.contains("Unknown:"), "{view}");
        }
        let bowl = render_query(None, Some(20), "relic Singing Bowl").unwrap();
        let courier = render_query(None, Some(20), "relic TheCourier").unwrap();
        assert!(courier.contains("20% lower"));
        assert!(courier.contains("immediately replaced by new stock"));
        assert!(courier.contains("Card removal is also discounted"));
        assert!(courier.contains("limited to once per shop"));
        let spoon = render_query(None, Some(20), "relic Strange Spoon").unwrap();
        assert!(spoon.contains("50% chance to be discarded instead"));
        assert!(spoon.contains("do not trigger on-Exhaust effects"));
        assert!(spoon.contains("Ethereal at turn end"));
        let flower = render_query(None, Some(20), "relic Magic Flower").unwrap();
        assert!(flower.contains("Healing during combat is 50% more effective"));
        assert!(flower.contains("out-of-combat recovery"));
        assert!(bowl.contains("gain 2 Max HP instead of taking a card"));
        assert!(bowl.contains("consumes that card reward"));
        assert!(bowl.contains("healing restrictions"));
        let vigor = render_query(None, Some(20), "power Vigor").unwrap();
        assert!(vigor.contains("every normal damage hit and target"));
        assert!(vigor.contains("Skills and Powers do not consume it"));
    }

    #[test]
    fn relic_queries_use_current_inventory_in_both_observation_domains() {
        use sts_oracle_learning::agent::information::run::{
            PublicCombatRunContextGapV1, PublicCombatRunContextV1,
        };
        use sts_oracle_learning::agent::information::state::public_combat_state_v1;
        use sts_oracle_learning::ai::planner_core::PlannerRelicObservation;
        use sts_oracle_learning::content::relics::{RelicId, RelicState};

        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let strategic: TaskPublicObservationV1 =
            serde_json::from_value(fixture["payload"]["decision"]["observation"].clone()).unwrap();
        let combat = TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Unavailable {
                reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
            },
            observation: public_combat_state_v1(
                &sts_oracle_learning::test_support::blank_test_combat(),
            ),
        };
        for mut observation in [strategic, combat] {
            let absent = super::render_query(
                None,
                Some(20),
                Some(&observation),
                "relic Centennial Puzzle",
                Locale::En,
            )
            .unwrap();
            assert!(absent.contains("Currently held: no"), "{absent}");
            assert!(
                absent.contains("first time you lose HP"),
                "definitions remain queryable even when not held"
            );
            match &mut observation {
                TaskPublicObservationV1::Strategic { observation } => {
                    observation.relics.push(PlannerRelicObservation {
                        relic: RelicId::CentennialPuzzle,
                        counter: 0,
                        used_up: true,
                        amount: 1,
                    })
                }
                TaskPublicObservationV1::Combat { observation, .. } => {
                    let mut relic = RelicState::new(RelicId::CentennialPuzzle);
                    relic.counter = 0;
                    relic.used_up = true;
                    observation.player.relics.push(relic);
                }
            }
            let held = super::render_query(
                None,
                Some(20),
                Some(&observation),
                "Centennial Puzzle",
                Locale::En,
            )
            .unwrap();
            assert!(
                held.contains("Currently held: yes | Counter: 0 | draw trigger spent this combat"),
                "{held}"
            );
        }
        assert!(!render_query(None, None, "relic Centennial Puzzle")
            .unwrap()
            .contains("Currently held:"));
    }

    #[test]
    fn observed_city_queries_expose_hit_rules_and_effects_beyond_visible_intents() {
        let byrd = render_query(None, Some(20), "enemy Byrd").unwrap();
        assert!(byrd.contains("base damage/hit (hits: 6)"));
        assert!(byrd.contains("4 Flight"));
        assert!(byrd.contains("Go Airborne"));
        assert!(render_query(None, Some(16), "enemy Byrd")
            .unwrap()
            .contains("3 Flight"));
        let whip = render_query(None, Some(20), "enemy Taskmaster").unwrap();
        assert!(whip.contains("add 3 Wound to discard pile"));
        assert!(whip.contains("gain 1 Strength on self"));
        let low_whip = render_query(None, Some(17), "enemy Taskmaster").unwrap();
        assert!(low_whip.contains("add 2 Wound to discard pile"));
        assert!(!low_whip.contains("gain 1 Strength on self"));
        let plant = render_query(None, Some(20), "enemy Snake Plant").unwrap();
        assert!(plant.contains("8 base damage/hit (hits: 3)"));
        assert!(plant.contains("apply 2 Frail to player"));
        assert!(plant.contains("apply 2 Weak to player"));
        assert!(plant.contains("two intervening Chompy Chomps"));
        for (query, expected) in [
            (
                "power Flight",
                "multiple hits from one card count separately",
            ),
            ("power Malleable", "immediately increases the amount by 1"),
            (
                "relic Eternal Feather",
                "even if you then choose Smith or Recall",
            ),
            ("card Parasite", "Exhausting it during combat"),
            ("card Writhe", "occupying a hand slot"),
        ] {
            let view = render_query(None, Some(20), query).unwrap();
            assert!(view.contains(expected), "{query}: {view}");
            assert!(!view.contains("Unknown:"), "{query}: {view}");
        }
    }

    #[test]
    fn observed_late_run_queries_explain_phases_countdowns_and_revival() {
        let repulsor = render_query(None, Some(20), "enemy Repulsor").unwrap();
        assert!(
            repulsor.contains("Attack: attack player for 13 base damage/hit (hits: 1)"),
            "{repulsor}"
        );
        assert!(
            repulsor.contains("Daze: add 2 Dazed to draw pile at random positions"),
            "{repulsor}"
        );
        assert!(
            repulsor.contains("Attack cannot repeat immediately"),
            "{repulsor}"
        );
        assert!(!repulsor.contains("Unknown:"), "{repulsor}");
        for (query, expected) in [
            ("enemy The Champ", "Remove all debuffs and remove Shackled"),
            ("enemy Darkling", "Base damage is rolled 9..13 inclusive"),
            ("enemy Exploder", "30 Thorns damage"),
            ("enemy Transient", "Starts with 6 Fading"),
            ("enemy Nemesis", "apply 1 Intangible"),
            ("enemy GiantHead", "first It Is Time is on enemy turn 4"),
            ("power Slow", "card-play counter for the current round"),
            (
                "power Life Link",
                "Bringing all Darklings to 0 HP before any revives",
            ),
            ("power Explosive", "queued after its move actions"),
            ("power Fading", "current attack still executes"),
            ("power Shifting", "normal or Thorns damage"),
        ] {
            let view = render_query(None, Some(20), query).unwrap();
            assert!(view.contains(expected), "{query}: {view}");
            assert!(!view.contains("Unknown:"), "{query}: {view}");
        }
        assert!(render_query(None, Some(16), "enemy Transient")
            .unwrap()
            .contains("Starts with 5 Fading"));
        let champ = render_query(None, Some(20), "enemy Champ").unwrap();
        assert!(champ.contains("apply 12 Strength to self"));
        assert!(champ.contains("damage does not immediately interrupt"));
        assert!(champ.contains("apply 2 Frail to player; apply 2 Vulnerable to player"));
        let nemesis = render_query(None, Some(20), "enemy Nemesis").unwrap();
        assert!(nemesis.contains("add 5 Burn to discard pile"));
        assert!(nemesis.contains("45 base damage/hit"));
    }

    #[test]
    fn final_shop_queries_explain_shuffle_retention_and_delayed_free_play() {
        let abacus = render_query(None, Some(20), "relic TheAbacus").unwrap();
        assert!(abacus.contains("shuffled into your draw pile, gain 6 Block"));
        let calipers = render_query(None, Some(20), "relic Calipers").unwrap();
        assert!(calipers.contains("lose 15 Block instead of all Block"));
        assert!(calipers.contains("If Barricade is active"));
        let forethought = render_query(None, Some(20), "card Forethought").unwrap();
        assert!(forethought.contains("bottom of the draw pile"));
        assert!(forethought.contains("free for its next play"));
        assert!(forethought.contains("any number if upgraded, including none"));
        assert!(forethought.contains("X-cost and unplayable"));
        for view in [abacus, calipers, forethought] {
            assert!(!view.contains("Unknown:"), "{view}");
        }
        let dagger = render_query(None, Some(20), "enemy SnakeDagger").unwrap();
        assert!(dagger.contains("then lose own current HP"));
        assert!(!dagger.contains("without death relic triggers"));
    }

    #[test]
    fn boss_and_late_elite_encounters_expose_all_starting_members() {
        for (name, members) in [
            ("TheGuardian", vec!["TheGuardian"]),
            ("Hexaghost", vec!["Hexaghost"]),
            ("SlimeBoss", vec!["SlimeBoss"]),
            ("Automaton", vec!["BronzeAutomaton"]),
            ("TheChamp", vec!["Champ"]),
            ("Collector", vec!["TheCollector"]),
            ("GiantHead", vec!["GiantHead"]),
            ("TheNemesis", vec!["Nemesis"]),
            ("Reptomancer", vec!["SnakeDagger", "Reptomancer"]),
            ("AwakenedOne", vec!["Cultist", "AwakenedOne"]),
            ("TimeEater", vec!["TimeEater"]),
            ("Donu and Deca", vec!["Deca", "Donu"]),
            ("ShieldAndSpear", vec!["SpireShield", "SpireSpear"]),
            ("ColosseumSlavers", vec!["SlaverBlue", "SlaverRed"]),
            ("ColosseumNobs", vec!["Taskmaster", "GremlinNob"]),
        ] {
            let view = render_query(None, Some(20), &format!("encounter {name}")).unwrap();
            let lineup = view
                .lines()
                .find(|line| line.starts_with("Fixed starting enemies:"))
                .unwrap();
            for member in members {
                assert!(lineup.contains(&format!("[{member}]")), "{name}: {view}");
            }
            assert!(!view.contains("Unknown:"), "{name}: {view}");
        }
        let automaton = render_query(None, Some(20), "encounter Automaton").unwrap();
        assert!(automaton.contains("Starting power: Artifact 3"));
        assert!(automaton.contains("Hyper Beam: attack player for 50 base damage/hit"));
        assert!(automaton.contains("enemy turn 6"));
        let rept = render_query(None, Some(20), "encounter Reptomancer").unwrap();
        assert!(rept.contains("Summon up to 2 Snake Daggers"));
        assert!(rept.contains("Snake Strike: attack player for 16 base damage/hit (hits: 2)"));
        assert!(rept.contains("add 1 Wound to discard pile"));
        assert!(render_query(None, Some(17), "encounter Reptomancer")
            .unwrap()
            .contains("Summon up to 1 Snake Daggers"));
        let orb = render_query(None, Some(20), "enemy BronzeOrb").unwrap();
        assert!(orb.contains("Give Bronze Automaton 12 Block"));
        assert!(orb.contains("preferring Rare, then Uncommon, then Common"));
        assert!(!orb.contains("Unknown:"));
    }

    #[test]
    fn endgame_queries_explain_phases_dynamic_effects_and_exact_card_destinations() {
        let awakened = render_query(None, Some(20), "enemy AwakenedOne").unwrap();
        assert!(awakened.contains(
            "Dark Echo (phase 2 opening): attack player for 40 base damage/hit (hits: 1)"
        ));
        assert!(awakened.contains("Rebirth: revive self and heal to full Max HP"));
        assert!(awakened.contains("Curiosity 1 (2 at A19+)"));
        assert!(awakened.contains("add 1 Void to draw pile at random positions"));
        let time = render_query(None, Some(20), "enemy TimeEater").unwrap();
        assert!(time.contains("using HP at execution; gain 32 Block"));
        assert!(!time.contains("heal 0"));
        assert!(time.contains("apply 1 Frail to player"));
        assert!(!render_query(None, Some(18), "enemy TimeEater")
            .unwrap()
            .contains("apply 1 Frail to player"));
        let heart = render_query(None, Some(20), "enemy CorruptHeart").unwrap();
        assert!(heart.contains("Blood Shots: attack player for 2 base damage/hit (hits: 15)"));
        for card in ["Dazed", "Slimed", "Wound", "Burn", "Void"] {
            assert!(heart.contains(&format!("add 1 {card} to draw pile at random positions")));
        }
        let spear = render_query(None, Some(20), "enemy SpireSpear").unwrap();
        assert!(spear.contains("add 2 Burn to top of draw pile"));
        assert!(render_query(None, Some(17), "enemy SpireSpear")
            .unwrap()
            .contains("add 2 Burn to discard pile"));
        for enemy in [
            "AwakenedOne",
            "TimeEater",
            "Donu",
            "Deca",
            "CorruptHeart",
            "SpireShield",
            "SpireSpear",
        ] {
            let view = render_query(None, Some(20), &format!("enemy {enemy}")).unwrap();
            assert!(!view.contains("Unknown:"), "{view}");
        }
    }

    #[test]
    fn endgame_power_queries_distinguish_persistent_triggers_and_remaining_damage_cap() {
        for query in ["encounter TheHeart", "enemy TheHeart", "enemy CorruptHeart"] {
            let view = render_query(None, Some(20), query).unwrap();
            assert!(view.contains("Starting power: Invincible 200"));
            assert!(view.contains("Starting power: Beat of Death 2"));
            assert!(view.contains("Buff 1: gain 2 Artifact"));
            assert!(view.contains("Buff 2: gain 1 Beat of Death"));
            assert!(view.contains("Buff 3: gain Painful Stabs"));
            assert!(view.contains("Buff 4: gain 10 Strength"));
            assert!(view.contains("Buff 5 and every later Buff: gain 50 Strength"));
            assert!(view.contains("Block can absorb it"));
            assert!(view.contains("before that card's final exhaust"));
            assert!(
                !view.contains("300"),
                "A20 query includes another difficulty's cap: {view}"
            );
        }
        let low = render_query(None, Some(18), "enemy CorruptHeart").unwrap();
        assert!(low.contains("Starting power: Invincible 300"));
        assert!(low.contains("Starting power: Beat of Death 1"));
        assert!(!low.contains("200"));
        let apotheosis = render_query(None, Some(20), "card Apotheosis").unwrap();
        assert!(apotheosis.contains("for this combat only"));
        assert!(apotheosis.contains("Costs 2 Energy (1 if upgraded)"));
        assert!(!apotheosis.contains("Unknown:"));
        for power in [
            "Buffer",
            "Curiosity",
            "Unawakened",
            "Regenerate",
            "TimeWarp",
            "BeatOfDeath",
            "Invincible",
            "Thorns",
            "DrawReduction",
            "PlatedArmor",
            "PainfulStabs",
            "Surrounded",
            "BackAttack",
        ] {
            let view = render_query(None, Some(20), &format!("power {power}")).unwrap();
            assert!(!view.contains("Unknown:"), "{view}");
        }
        let mut out = String::new();
        write_observed_amount(
            &mut out,
            "M0",
            &CombatLearningPowerV1 {
                power: powers::PowerId::Invincible,
                amount: 17,
                extra_data: 200,
                just_applied: false,
                payload_card: None,
                source_creature: None,
            },
            Locale::En,
        );
        assert!(out.contains("remaining damage allowance 17; reset cap 200"));
        assert!(render_query(None, Some(20), "power BeatOfDeath")
            .unwrap()
            .contains("before that card's final exhaust"));
        assert!(render_query(None, Some(20), "power Regenerate")
            .unwrap()
            .contains("Does not decrement"));
        let buffer = render_query(None, Some(20), "power Buffer").unwrap();
        assert!(buffer.contains("Fully blocked damage does not consume Buffer"));
        assert!(buffer.contains("Unblocked Beat of Death can consume it"));
        assert!(buffer.contains("Direct HP loss also consumes a charge"));
    }

    #[test]
    fn independent_player_reported_relic_and_curse_rules_are_available() {
        for (query, expected) in [
            ("relic SelfFormingClay", "per HP-loss event"),
            ("relic Matryoshka", "1 extra relic reward"),
            ("relic WarPaint", "up to 2 randomly chosen"),
            (
                "relic NeowsBlessing",
                "decreases immediately at combat start",
            ),
            ("relic Omamori", "1 charge per prevented card"),
            ("relic Shovel", "Dig uses the campfire's action"),
            (
                "relic SmilingMask",
                "Each shop still permits only one removal",
            ),
            ("relic DreamCatcher", "After choosing Rest"),
            ("relic BloodVial", "heal 2 HP"),
            ("relic MawBank", "Spending Gold at a shop"),
            ("relic BagOfMarbles", "apply 1 Vulnerable"),
            ("relic PrismaticShard", "preserving the rolled rarity"),
            (
                "relic FossilizedHelix",
                "Fully blocked hits leave Buffer intact",
            ),
            ("card Normality", "before Normality reached your hand"),
        ] {
            let view = render_query(None, Some(20), query).unwrap();
            assert!(view.contains(expected), "{view}");
            assert!(!view.contains("Unknown:"), "{view}");
        }
    }

    #[test]
    fn observed_act_three_shop_and_relic_queries_explain_choices_instead_of_reporting_gaps() {
        for (query, expected) in [
            ("card Discovery", "healing-tagged cards are excluded"),
            ("card SecretWeapon", "draw pile has no Attack"),
            ("card SecretTechnique", "with its existing cost"),
            ("card Panacea", "one charge per application"),
            ("card FlashOfSteel", "then draw 1 card"),
            ("card Mayhem", "after your normal start-of-turn draw"),
            ("card Violence", "Move 3 randomly chosen Attack cards"),
            ("relic Brimstone", "including the first, gain 2 Strength"),
            ("relic Pocketwatch", "3 or fewer cards"),
            ("relic Orichalcum", "0 Block"),
            ("relic Vajra", "gain 1 Strength"),
            ("relic JuzuBracelet", "fixed combats within events"),
            ("relic BronzeScales", "back per hit"),
            ("relic IncenseBurner", "counter persists between combats"),
            (
                "relic ChampionBelt",
                "Artifact blocks the Vulnerable application",
            ),
            ("relic TungstenRod", "including Torii"),
            ("relic PenNib", "counter 9"),
            ("relic CeramicFish", "master deck"),
            ("relic PaperFrog", "75%"),
            (
                "relic MembershipCard",
                "immediately discounts the remaining inventory",
            ),
            ("relic Toolbox", "normal Energy cost"),
            ("relic Turnip", "Prevent new Frail applications"),
            ("relic Ginger", "Prevent new Weak applications"),
            ("relic LetterOpener", "third Skill played in the same turn"),
        ] {
            let view = render_query(None, Some(20), query).unwrap();
            assert!(view.contains(expected), "{view}");
            assert!(!view.contains("Unknown:"), "{view}");
        }
    }

    #[test]
    fn typed_power_mechanics_are_available_through_the_player_query() {
        for id in powers::PowerId::STABLE_IDS {
            let power = powers::PowerId::from_stable_id(id).unwrap();
            if powers::mechanics::typed_profile(power).is_some() {
                let view = render_query(None, Some(20), id).unwrap();
                assert!(view.contains("Effect:"), "{view}");
                assert!(!view.contains("Unknown:"), "{view}");
            }
        }
        let shackled = render_query(None, Some(20), "Shackled").unwrap();
        assert!(shackled.contains("only if amount is positive"));
        assert!(shackled.contains("owner's turn end"));
        let rage = render_query(None, Some(20), "Rage").unwrap();
        assert!(rage.contains("once per card, not once per hit"));
        assert!(rage.contains("end of the player turn"));
        let hide = render_query(None, Some(20), "Sharp Hide").unwrap();
        assert!(hide.contains("Skills and Powers do not trigger"));
        assert!(hide.contains("Player Block can absorb"));
    }

    #[test]
    fn named_definitions_work_outside_combat_and_do_not_guess_unknowns() {
        let keys = render_query(None, Some(20), "keys").unwrap();
        assert!(keys.contains("Ruby Key: choose Recall at a campfire"));
        assert!(keys.contains("Sapphire Key: take it instead of the linked chest relic"));
        assert!(keys.contains("Emerald Key: win against a burning elite"));
        assert!(keys.contains(KeyRuleV1::Timing.text(Locale::En)));
        assert!(keys.contains(KeyRuleV1::RubyOpportunities.text(Locale::En)));
        assert!(render_query(None, Some(20), "rubykey")
            .unwrap()
            .contains(KeyRuleV1::RubyOpportunities.text(Locale::En)));
        assert!(keys.contains(KeyRuleV1::EmeraldOpportunities.text(Locale::En)));
        assert!(keys.contains("Parallel nodes on that row are alternative paths"));
        assert!(keys.contains("including a chest reached through a question-mark room"));
        assert!(keys.contains("after the Act 3 chest"));
        for key in ["RubyKey", "Sapphire Key", "emerald_key"] {
            let definition = render_query(None, Some(20), key).unwrap();
            assert!(definition.contains("One of the three keys"), "{definition}");
        }
        let emerald = render_query(None, Some(20), "emerald_key").unwrap();
        assert!(emerald.contains(KeyRuleV1::EmeraldOpportunities.text(Locale::En)));
        let spore = render_query(None, Some(20), "Spore Cloud").unwrap();
        assert!(spore.contains("Vulnerable stacks applied on death"));
        assert!(spore.contains("does not apply Weak"));
        let fungi = render_query(None, Some(20), "Fungi Beast").unwrap();
        assert!(fungi.contains("Grow: apply 5 Strength to self"));
        assert!(render_query(None, Some(0), "FungiBeast")
            .unwrap()
            .contains("Grow: apply 3 Strength"));
        assert!(render_query(None, Some(20), "Corruption")
            .unwrap()
            .contains("While active, Skills cost 0 and Exhaust when played"));
        assert!(render_query(None, Some(20), "The Guardian")
            .unwrap()
            .contains("Mode Shift"));
        assert!(render_query(None, Some(20), "enemy The Maw")
            .unwrap()
            .contains("Nom Nom Nom"));
        assert!(render_query(None, Some(20), "Spore").is_err());
        assert!(render_query(None, Some(20), "M0").is_err());
    }

    #[test]
    fn orb_walker_queries_explain_both_burn_destinations_and_round_scaling() {
        for (asc, damage, gain) in [(0, 10, 3), (16, 11, 3), (20, 11, 5)] {
            let view = render_query(None, Some(asc), "enemy Orb Walker").unwrap();
            assert!(!view.contains("Unknown:"), "{view}");
            assert!(view.contains(&format!(
                "Laser: attack player for {damage} base damage/hit"
            )));
            assert!(view.contains("add 1 Burn to discard pile"));
            assert!(view.contains("add 1 Burn to draw pile at random positions"));
            assert!(view.contains(&format!("Generic Strength Up {gain}")));
            assert!(view.contains("after its attack"));
            assert!(view.contains("Neither move can occur three times consecutively"));
        }
        let power = render_query(None, Some(20), "power Generic Strength Up").unwrap();
        assert!(!power.contains("Unknown:"), "{power}");
        assert!(power.contains("does not decrement"));
        assert!(power.contains("after the monsters have executed their moves"));
    }

    #[test]
    fn ironclad_relic_pools_have_queryable_effects_and_restrictions() {
        for tier in [
            relics::RelicTier::Starter,
            relics::RelicTier::Common,
            relics::RelicTier::Uncommon,
            relics::RelicTier::Rare,
            relics::RelicTier::Boss,
            relics::RelicTier::Shop,
            relics::RelicTier::Special,
        ] {
            for relic in relics::build_relic_pool(tier, "Ironclad") {
                let query = format!("relic {}", relic.stable_id());
                let view = render_query(None, Some(20), &query).unwrap();
                assert!(!view.contains("Unknown:"), "{view}");
            }
        }
        let hammer = render_query(None, Some(20), "relic FusionHammer").unwrap();
        assert!(hammer.contains("Cannot choose Smith"));
        assert!(hammer.contains("upgrades from cards, events and other sources still work"));
        let collar = render_query(None, Some(20), "relic SlaversCollar").unwrap();
        assert!(collar.contains("elite and boss combats"));
        assert!(collar.contains("Ordinary combats do not receive"));
        let pyramid = render_query(None, Some(20), "relic RunicPyramid").unwrap();
        assert!(pyramid.contains("Ethereal cards still exhaust"));
        assert!(pyramid.contains("does not grant Retain"));
    }

    #[test]
    fn relic_readings_distinguish_active_bonus_from_spent_triggers() {
        use sts_oracle_learning::agent::information::state::public_combat_state_v1;
        use sts_oracle_learning::content::relics::{RelicId, RelicState};

        let mut state = sts_oracle_learning::test_support::blank_test_combat();
        for id in [RelicId::RedSkull, RelicId::Lantern, RelicId::LizardTail] {
            let mut relic = RelicState::new(id);
            relic.used_up = true;
            state.entities.player.relics.push(relic);
        }
        let public = public_combat_state_v1(&state);
        let mut reading = String::new();
        super::write_combat_rules(&mut reading, &public, Locale::En);
        assert!(
            reading.contains("Relic Red Skull (low-HP Strength bonus active)"),
            "{reading}"
        );
        assert!(reading.contains("Relic Lantern (opening Energy bonus spent this combat)"));
        assert!(reading.contains("Relic Lizard Tail (used up)"));
        let compact = crate::decision::combat::state_reading(&public, Locale::En);
        assert!(
            compact.contains("Red Skull (low-HP Strength bonus active)"),
            "{compact}"
        );
        assert!(compact.contains("Lantern (opening Energy bonus spent this combat)"));

        let observation = TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Unavailable {
                reason: sts_oracle_learning::agent::information::run::PublicCombatRunContextGapV1::DetachedExactCombatPosition,
            },
            observation: public,
        };
        let active = super::render_query(
            None,
            Some(20),
            Some(&observation),
            "relic Red Skull",
            Locale::En,
        )
        .unwrap();
        assert!(active.contains("low-HP Strength bonus active"), "{active}");
        assert!(active.contains("removed when healing crosses above it"));
        assert!(!active.contains("used up"));
    }

    #[test]
    fn sentry_and_card_queries_explain_the_rules_without_ambiguous_names() {
        let sentry = render_query(None, Some(20), "Sentry").unwrap();
        assert!(sentry.contains("Beam: attack player for 10 base damage/hit (hits: 1)"));
        assert!(sentry.contains("Bolt: add 3 Dazed to discard pile"));
        assert!(sentry.contains("Each Sentry alternates Beam and Bolt"));
        let dazed = render_query(None, Some(20), "Dazed").unwrap();
        assert!(dazed.contains("Cost: unplayable"));
        assert!(dazed.contains("Ethereal: true"));
        assert!(dazed.contains("draw or discard pile stays"));
        let uppercut = render_query(None, Some(20), "card Uppercut").unwrap();
        assert!(uppercut.contains("Deal 13 damage"));
        assert!(uppercut.contains("1 Weak and 1 Vulnerable (2 of each if upgraded)"));
        assert!(uppercut.contains("damage first, then applies Weak, then Vulnerable"));
        assert!(render_query(None, Some(20), "card Metallicize")
            .unwrap()
            .contains("end of each player turn, gain 3 Block"));
        assert!(render_query(None, Some(20), "card Bloodletting")
            .unwrap()
            .contains("Lose 3 HP, then gain 2 Energy"));
        assert!(render_query(None, Some(20), "card Sever Soul")
            .unwrap()
            .contains("Exhaust all non-Attack cards remaining in hand"));
        assert!(render_query(None, Some(20), "Artifact")
            .unwrap()
            .contains("consumes one charge"));
        assert!(render_query(None, Some(20), "Anger")
            .unwrap()
            .starts_with("Power:"));
        let anger_card = render_query(None, Some(20), "card Anger").unwrap();
        assert!(anger_card.starts_with("Card definition:"));
        assert!(anger_card.contains("add a copy of this card to your discard pile"));
        assert!(anger_card.contains("not added to your permanent master deck"));
        assert!(render_query(None, Some(20), "card NotACard").is_err());
    }

    #[test]
    fn ironclad_pool_and_generated_statuses_have_queryable_effects() {
        let flame = render_query(None, Some(20), "card FlameBarrier").unwrap();
        assert!(flame.contains("12 Block (16 if upgraded)"));
        assert!(flame.contains("4 retaliation damage per incoming attack hit (6 if upgraded)"));
        let retaliation = render_query(None, Some(20), "power FlameBarrier").unwrap();
        assert!(!retaliation.contains("Unknown:"));
        assert!(retaliation.contains("removed at the start of the owner's next turn"));
        for (card, targets) in [
            ("Carnage", "one selected enemy"),
            ("Cleave", "all enemies"),
            ("Sword Boomerang", "random enemy per hit"),
        ] {
            let reading = render_query(None, Some(20), &format!("card {card}")).unwrap();
            assert!(
                reading.contains(&format!("Attack targets: {targets}")),
                "{reading}"
            );
        }
        let cards = cards::IRONCLAD_COMMON_POOL
            .iter()
            .chain(cards::IRONCLAD_UNCOMMON_POOL)
            .chain(cards::IRONCLAD_RARE_POOL)
            .copied()
            .chain([
                cards::CardId::Strike,
                cards::CardId::Defend,
                cards::CardId::Bash,
                cards::CardId::AscendersBane,
                cards::CardId::Wound,
                cards::CardId::Dazed,
                cards::CardId::Burn,
                cards::CardId::Slimed,
                cards::CardId::Void,
            ]);
        for card in cards {
            let reading =
                render_query(None, Some(20), &format!("card {}", card.stable_id())).unwrap();
            assert!(reading.contains("Effect:"), "{reading}");
            assert!(!reading.contains("Unknown:"), "{reading}");
            assert!(
                reading.len() < 800,
                "mechanics query grew too large: {reading}"
            );
        }
        let wild = render_query(None, Some(20), "card WildStrike").unwrap();
        assert!(wild.contains("insert 1 Wound at a random position in your draw pile"));
        let disarm = render_query(None, Some(20), "card Disarm").unwrap();
        assert!(disarm.contains("each later attack hit"));
        assert!(disarm.contains("ordinary Strength gains add to the reduced amount"));
        assert!(disarm.contains("Artifact can block"));
        assert!(render_query(None, Some(20), "card Strike")
            .unwrap()
            .contains("Deal 6 damage"));
        assert!(render_query(None, Some(20), "card Strike_R")
            .unwrap()
            .contains("Deal 6 damage"));
        assert!(render_query(None, Some(20), "card Defend")
            .unwrap()
            .contains("Gain 5 Block"));
        assert!(render_query(None, Some(20), "card strike").is_err());
    }

    #[test]
    fn encountered_card_rewards_explain_their_decision_relevant_effects() {
        for (card, expected) in [
            ("Rage", "each time you play an Attack"),
            ("Sentinel", "If this card is Exhausted, gain 2 Energy"),
            (
                "Berserk",
                "apply 2 turns of Vulnerable to yourself once (1 turn if upgraded)",
            ),
            ("Immolate", "add 1 Burn to the discard pile"),
            ("Juggernaut", "Each separate Block gain triggers once"),
        ] {
            let reading = render_query(None, Some(20), &format!("card {card}")).unwrap();
            assert!(reading.contains(expected), "missing {card} rule: {reading}");
            assert!(!reading.contains("Unknown: effect explanation"));
        }
    }

    #[test]
    fn special_card_queries_describe_conditions_and_instance_effects() {
        for (card, expected) in [
            ("Warcry", "top-deck step still occurs"),
            ("Rampage", "rest of combat"),
            ("BloodForBlood", "positive HP-loss event"),
            ("Dropkick", "before damage"),
            ("DualWield", "multiple eligible cards"),
            ("Exhume", "other than Exhume"),
            ("Havoc", "shuffle discard first"),
            (
                "Havoc",
                "A playable Power is applied normally and stays active",
            ),
            ("Purity", "choosing zero is allowed"),
            ("Panache", "multiple groups of 5 can trigger"),
            ("Burning Pact", "Burning Pact itself does not Exhaust"),
            ("Burning Pact", "Pact grants no Energy"),
            ("Dark Embrace", "two plays draw 2 per Exhaust"),
            ("Pain", "lose 1 HP whenever you play another card"),
        ] {
            let view = render_query(None, Some(20), &format!("card {card}")).unwrap();
            assert!(view.contains(expected), "{view}");
            assert!(!view.contains("Unknown"), "{view}");
            assert!(
                view.len() < 700,
                "mechanics query should remain concise: {view}"
            );
        }
    }

    #[test]
    fn encountered_shop_cards_and_attack_potion_explain_their_payoffs() {
        for (card, expected) in [
            ("Body Slam", "current Block"),
            ("Spot Weakness", "has an Attack intent"),
            ("Clothesline", "then apply 2 Weak"),
            ("Pommel Strike", "then draw 1 card"),
            ("Iron Wave", "Block resolves before the hit"),
            ("J.A.X.", "Lose 3 HP, then gain 2 Strength"),
            ("Apparition", "upgrading removes Ethereal"),
            ("Swift Strike", "7 damage to one enemy (10 if upgraded)"),
            ("Panic Button", "Gain 30 Block (40 if upgraded)"),
            ("Transmutation", "Generated cards cost 0 this turn"),
            (
                "The Bomb",
                "40 non-Attack damage to all enemies (50 if upgraded)",
            ),
        ] {
            let reading = render_query(None, Some(20), &format!("card {card}")).unwrap();
            assert!(reading.contains(expected), "missing {card} rule: {reading}");
        }
        let potion = render_query(None, Some(20), "potion Attack Potion").unwrap();
        let no_block = render_query(None, Some(20), "power NoBlock").unwrap();
        assert!(no_block.contains("Feel No Pain and Metallicize"));
        assert!(no_block.contains("including the round it was applied"));
        assert!(!no_block.contains("Unknown:"));
        let bomb = render_query(None, Some(20), "power TheBombPower").unwrap();
        assert!(bomb.contains("Starts at 3"));
        assert!(bomb.contains("including the turn it was applied"));
        assert!(bomb.contains("Strength, Weak and Vulnerable do not modify it"));
        assert!(potion.contains("they cost 0 this turn"));
        let colorless = render_query(None, Some(20), "potion Colorless Potion").unwrap();
        assert!(colorless.contains("three generated Colorless cards"));
        assert!(colorless.contains("choice cannot be skipped"));
        for (potion, expected) in [
            ("Strength Potion", "Gain Strength equal to potency"),
            ("Blood Potion", "percent of max HP"),
            ("Heart of Iron", "Gain Metallicize equal to potency"),
            (
                "Blessing of the Forge",
                "Upgrade all cards currently in your hand",
            ),
            ("Fire Potion", "Deal damage equal to potency to one enemy"),
        ] {
            let reading = render_query(None, Some(20), &format!("potion {potion}")).unwrap();
            assert!(
                reading.contains(expected),
                "missing {potion} rule: {reading}"
            );
        }
        assert!(render_query(None, Some(20), "potion NotAPotion").is_err());
    }

    #[test]
    fn ironclad_colorless_status_and_curse_queries_have_complete_explanations() {
        use cards::CardId;
        let mut ids = HashSet::new();
        for pool in [
            cards::IRONCLAD_COMMON_POOL,
            cards::IRONCLAD_UNCOMMON_POOL,
            cards::IRONCLAD_RARE_POOL,
            cards::COLORLESS_UNCOMMON_POOL,
            cards::COLORLESS_RARE_POOL,
            cards::get_curse_pool(),
        ] {
            ids.extend(pool.iter().copied());
        }
        ids.extend([
            CardId::Strike,
            CardId::Defend,
            CardId::Bash,
            CardId::Burn,
            CardId::Dazed,
            CardId::Slimed,
            CardId::Wound,
            CardId::Void,
            CardId::AscendersBane,
            CardId::CurseOfTheBell,
            CardId::Necronomicurse,
            CardId::Pride,
            CardId::Miracle,
            CardId::Insight,
            CardId::Shiv,
            CardId::Bite,
            CardId::Apparition,
            CardId::RitualDagger,
            CardId::JAX,
        ]);
        assert_eq!(ids.len(), 136);
        let mut state = sts_oracle_learning::test_support::blank_test_combat();
        state.entities.monsters = vec![sts_oracle_learning::test_support::test_monster(
            monsters::EnemyId::JawWorm,
        )];
        state.zones.draw_pile =
            vec![sts_oracle_learning::runtime::combat::CombatCard::for_scenario(CardId::Strike, 1)]
                .into();
        for id in ids {
            let query = render_query(None, Some(20), &format!("card {}", id.stable_id())).unwrap();
            assert!(!query.contains("unavailable"), "{query}");
            for upgrade in [0, 1] {
                let mut reading = String::new();
                write_card_rule(&mut reading, id, upgrade, Locale::En);
                assert!(!reading.contains("unavailable"), "{reading}");
                let mut card =
                    sts_oracle_learning::runtime::combat::CombatCard::for_scenario(id, 2);
                card.upgrades = upgrade;
                let target = matches!(
                    cards::mechanics::effective_card_target(id, upgrade),
                    cards::CardTarget::Enemy | cards::CardTarget::SelfAndEnemy
                )
                .then_some(state.entities.monsters[0].id);
                for action in cards::resolve_card_play(id, &state, &card, target) {
                    if let sts_oracle_learning::runtime::action::Action::ApplyPower {
                        power_id,
                        ..
                    } = action.action
                    {
                        let view = render_query(
                            None,
                            Some(20),
                            &format!("power {}", power_id.stable_id()),
                        )
                        .unwrap();
                        assert!(!view.contains("Unknown:"), "{id:?}: {view}");
                    }
                }
            }
        }
        let dagger = render_query(None, Some(20), "card Ritual Dagger").unwrap();
        assert!(dagger.contains("stored misc value, initially 15"));
        assert!(dagger.contains("by 3 (5 if upgraded)"));
        assert!(render_query(None, Some(20), "card Bandage Up")
            .unwrap()
            .contains("Heal 4 HP (6 if upgraded)"));
        let dummy = render_query(None, Some(20), "relic Strike Dummy").unwrap();
        assert!(dummy.contains("actual Strike tag"));
        assert!(dummy.contains("3 additional damage per hit"));
    }

    #[test]
    fn all_potions_and_explicit_enemy_power_queries_are_available() {
        assert_eq!(potions::ALL_POTIONS.len(), 42);
        for potion in potions::ALL_POTIONS {
            let view =
                render_query(None, Some(20), &format!("potion {}", potion.stable_id())).unwrap();
            assert!(view.contains("Effect:"), "{view}");
            assert!(!view.contains("Unknown:"), "{view}");
            assert!(view.len() < 700, "{view}");
        }
        for power in [
            powers::PowerId::Poison,
            powers::PowerId::Focus,
            powers::PowerId::Regeneration,
            powers::PowerId::DuplicationPower,
        ] {
            let view =
                render_query(None, Some(20), &format!("power {}", power.stable_id())).unwrap();
            assert!(!view.contains("Unknown:"), "{view}");
        }
        let brew = render_query(None, Some(20), "potion Gambler's Brew").unwrap();
        assert!(brew.contains("discard, then draw that many"));
        assert!(brew.contains("choose none"));
        let ritual = render_query(None, Some(20), "potion Cultist Potion").unwrap();
        assert!(ritual.contains("including the turn it is used"));
        assert_eq!(
            render_query(None, Some(20), "enemy Sentry").unwrap(),
            render_query(None, Some(20), "Sentry").unwrap()
        );
        assert_eq!(
            render_query(None, Some(20), "power Strength").unwrap(),
            render_query(None, Some(20), "Strength").unwrap()
        );
        assert!(render_query(None, Some(20), "enemy SphericGuardian")
            .unwrap()
            .starts_with("Enemy definition:"));
        assert!(render_query(None, Some(20), "enemy NotAnEnemy").is_err());
        assert!(render_query(None, Some(20), "power NotAPower").is_err());
    }

    #[test]
    fn relic_queries_resolve_exact_names_and_show_coverage_honestly() {
        let cube = render_query(None, Some(20), "relic Runic Cube").unwrap();
        assert!(cube.contains("Relic definition: Runic Cube | Boss"));
        assert!(cube.contains("draw 1 card per HP-loss event"));
        assert!(render_query(None, Some(20), "Astrolabe")
            .unwrap()
            .contains("replacements are upgraded"));
        for (name, effect) in [
            ("Anchor", "At the start of each combat, gain 10 Block"),
            ("RegalPillow", "heal 15 additional HP"),
            ("ChemicalX", "X-cost card effects use X + 2"),
            ("Torii", "Checks each hit after Block"),
            ("UnceasingTop", "No Draw prevents it"),
        ] {
            let text = render_query(None, Some(20), &format!("relic {name}")).unwrap();
            assert!(text.contains(effect));
            assert!(!text.contains("Unknown:"));
        }
        assert!(render_query(None, Some(20), "relic NotARelic").is_err());
        for (name, rule) in [
            ("Molten Egg", "Attack card"),
            ("MoltenEgg2", "Attack card"),
            ("Toxic Egg", "Skill card"),
            ("Bottled Flame", "one opening-hand slot"),
            ("Bottled Lightning", "normal opening draw"),
            ("Bottled Tornado", "Energy cost must still be paid"),
            ("Preserved Insect", "Does not reduce Max HP"),
            ("Warped Tongs", "for this combat"),
            ("Kunai", "Counts cards, not damage hits or targets"),
            ("Odd Mushroom", "Does not prevent or remove Vulnerable"),
            ("Lantern", "first turn of every combat"),
        ] {
            assert!(
                render_query(None, Some(20), &format!("relic {name}"))
                    .unwrap()
                    .contains(rule),
                "{name}"
            );
        }
    }

    #[test]
    fn encounter_locales_preserve_all_lineup_weights_and_full_enemy_definitions() {
        for id in EncounterId::STABLE_IDS {
            let encounter = EncounterId::from_stable_id(id).unwrap();
            let Some(lineup) = monsters::factory::encounter_lineup(encounter) else {
                continue;
            };
            for ascension in [4, 5, 20] {
                let mut zh = String::new();
                write_encounter(&mut zh, encounter, ascension, Locale::ZhCn).unwrap();
                assert!(zh.contains(&format!("[{}] | 进阶 {ascension}", encounter.stable_id())));
                for &enemy in lineup.fixed.iter().chain(lineup.random_pool.iter()) {
                    let mut original = String::new();
                    write_enemy(&mut original, enemy, ascension, Locale::En);
                    assert!(
                        zh.contains(&original),
                        "{id}: localized headers must preserve the complete mechanism supplement"
                    );
                    if lineup.random_pool.contains(&enemy) {
                        let weight = lineup
                            .random_pool
                            .iter()
                            .filter(|&&member| member == enemy)
                            .count();
                        assert!(zh.contains(&format!(
                            "{} [{}]：每个随机位置的权重 {weight}/{}",
                            Locale::ZhCn.entity("enemy", enemy.stable_id()),
                            enemy.stable_id(),
                            lineup.random_pool.len()
                        )));
                    }
                }
                if lineup.random_slots > 0 {
                    assert!(zh.contains(&format!("随机初始位置：{}", lineup.random_slots)));
                }
            }
        }
    }

    #[test]
    fn run_rules_explain_the_observed_transition_and_persistent_shop_price() {
        let a20 = render_query(None, Some(20), "rules").unwrap();
        assert!(a20.contains("heal 75% of missing HP"));
        assert!(a20.contains("75 Gold + 25 per previous shop removal"));
        assert!(a20.contains("each shop permits only one removal"));
        assert!(a20.contains("Skipping removal leaves the count and base price unchanged"));
        assert!(a20.contains("Completing a shop removal increments the count by one"));
        assert!(
            a20.contains("Each shop's displayed price includes its current relic price modifiers")
        );
        assert!(render_query(None, Some(4), "rules")
            .unwrap()
            .contains("heal to Max HP"));
        assert!(render_query(None, None, "rules").is_err());
    }

    #[test]
    fn preparation_rules_share_facts_without_expanding_every_enemy_move() {
        use sts_oracle_learning::ai::planner_core::PlannerRunGoal;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let mut observation: TaskPublicObservationV1 =
            serde_json::from_value(fixture["payload"]["decision"]["observation"].clone()).unwrap();
        let TaskPublicObservationV1::Strategic { observation: state } = &mut observation else {
            unreachable!()
        };
        state.run_goal = PlannerRunGoal::HeartVictory;
        state.run.act = 2;
        state.run.keys = [true, false, true];
        state.public_map.boss = Some(EncounterId::TheChamp);
        let full =
            super::render_query(None, Some(20), Some(&observation), "rules", Locale::En).unwrap();
        let brief = super::render_query(
            None,
            Some(20),
            Some(&observation),
            "rules preparation",
            Locale::En,
        )
        .unwrap();
        for fact in [
            "heal 75% of missing HP",
            Locale::En.entry("mechanics", "combat.shopRemoval").unwrap(),
            KeyRuleV1::ChestOpportunities.text(Locale::En),
            "The Champ",
            "Scouring Whip: add 3 Wound to discard pile",
            "Painful Stabs",
            "add 1 Dazed to draw pile at random positions",
            "Beat of Death",
        ] {
            assert!(
                full.contains(fact) && brief.contains(fact),
                "{fact}: {brief}"
            );
        }
        assert!(!brief.contains(KeyRuleV1::RubyOpportunities.text(Locale::En)));
        assert!(!brief.contains(KeyRuleV1::EmeraldOpportunities.text(Locale::En)));
        assert!(!brief.contains("Base starting HP range"));
        assert!(!brief.contains("Possible moves (base values"));
        assert!(!brief.contains("deck-lab encounters"));
        assert!(brief.len() < full.len());
        assert!(render_query(None, None, "rules preparation").is_err());
    }

    #[test]
    fn wrongly_categorized_keys_direct_the_reader_to_the_existing_untyped_query() {
        for name in ["Ruby Key", "Emerald Key", "Sapphire Key"] {
            for category in ["card", "relic"] {
                let error =
                    render_query(None, Some(20), &format!("{category} {name}")).unwrap_err();
                assert!(error.contains(&format!("query info {name} without a category")));
                let mechanics = render_query(None, Some(20), name).unwrap();
                assert!(mechanics.contains(name));
                assert!(mechanics.contains("Heart"));
            }
        }
    }

    #[test]
    fn transition_query_uses_bound_max_hp_and_shared_act_entry_calculation() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let mut observation: TaskPublicObservationV1 =
            serde_json::from_value(fixture["payload"]["decision"]["observation"].clone()).unwrap();
        let TaskPublicObservationV1::Strategic { observation: state } = &mut observation else {
            unreachable!()
        };
        state.run.max_hp = 82;
        state.run.current_hp = 65;
        for (asc, assumed_hp, recovered, final_hp) in
            [(20, 25, 43, 68), (20, 42, 30, 72), (4, 25, 57, 82)]
        {
            let text = super::render_query(
                None,
                Some(asc),
                Some(&observation),
                &format!("transition {assumed_hp}"),
                Locale::En,
            )
            .unwrap();
            assert!(
                text.contains(&format!("assumed surviving HP {assumed_hp}/82")),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "Transition recovery: +{recovered} HP -> {final_hp}/82"
                )),
                "{text}"
            );
        }
        for invalid in [
            "transition",
            "transition 0",
            "transition -1",
            "transition 83",
            "transition 3.5",
            "transition 25 42",
        ] {
            assert!(
                super::render_query(None, Some(20), Some(&observation), invalid, Locale::En)
                    .is_err(),
                "{invalid}"
            );
        }
        assert!(render_query(None, Some(20), "transition 25").is_err());
        assert!(
            super::render_query(None, None, Some(&observation), "transition 25", Locale::En)
                .is_err()
        );

        let mut combat = sts_oracle_learning::test_support::blank_test_combat();
        combat.entities.player.max_hp = 82;
        let combat_observation = TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Unavailable {
                reason: sts_oracle_learning::agent::information::run::PublicCombatRunContextGapV1::DetachedExactCombatPosition,
            },
            observation: sts_oracle_learning::agent::information::state::public_combat_state_v1(
                &combat,
            ),
        };
        let text = super::render_query(
            None,
            Some(20),
            Some(&combat_observation),
            "transition 25",
            Locale::En,
        )
        .unwrap();
        assert!(text.contains("+43 HP -> 68/82"));
    }

    #[test]
    fn heart_goal_rules_include_shared_difficulty_facts_in_preparation_and_combat() {
        use sts_oracle_learning::agent::information::state::public_combat_state_v1;
        use sts_oracle_learning::ai::planner_core::PlannerRunGoal;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let mut strategic: TaskPublicObservationV1 =
            serde_json::from_value(fixture["payload"]["decision"]["observation"].clone()).unwrap();
        let TaskPublicObservationV1::Strategic { observation } = &mut strategic else {
            panic!("expected strategic fixture")
        };
        observation.run_goal = PlannerRunGoal::HeartVictory;
        observation.run.ascension_level = 20;
        observation.run.keys = [false; 3];
        let mut combat_state = sts_oracle_learning::test_support::blank_test_combat();
        combat_state.meta.ascension_level = 20;
        let combat = TaskPublicObservationV1::Combat {
            run_context: PublicCombatRunContextV1::Available {
                run_goal: observation.run_goal,
                ascension_level: 20,
                combat_victory_guarantees_full_heal: false,
                act: observation.run.act,
                floor: observation.run.floor,
                keys: observation.run.keys,
                public_map: observation.public_map.clone(),
                encounter_id: None,
            },
            observation: public_combat_state_v1(&combat_state),
        };
        for observation in [&strategic, &combat] {
            let view = super::render_query(None, Some(20), Some(observation), "rules", Locale::En)
                .unwrap();
            assert!(view.contains("Objective encounter definition"));
            assert!(
                view.contains("fixed route Campfire -> Shop -> Elite -> Boss -> Victory"),
                "{view}"
            );
            assert!(view.contains("Gold you carry into it"));
            assert!(view.contains(KeyRuleV1::Timing.text(Locale::En)));
            assert!(view.contains(KeyRuleV1::ChestOpportunities.text(Locale::En)));
            assert!(view.contains(KeyRuleV1::EmeraldOpportunities.text(Locale::En)));
            assert!(view.contains("Act 3 bosses advance without a normal reward screen"));
            assert!(view.contains("Base starting HP range: 800..800"));
            assert!(view.contains("Starting power: Beat of Death 2"));
            assert!(view.contains("Starting power: Invincible 200"));
            assert!(view.contains("hits: 15"));
            assert!(view.contains("Torii's normal-hit reduction does not apply"));
        }
        let mut combat = combat;
        for keys in [
            [true, true, false],
            [false, true, true],
            [true, false, true],
            [true; 3],
            [false; 3], // Rewinding also restores the missing-key guidance.
        ] {
            let TaskPublicObservationV1::Strategic { observation } = &mut strategic else {
                unreachable!()
            };
            observation.run.keys = keys;
            let TaskPublicObservationV1::Combat {
                run_context: PublicCombatRunContextV1::Available { keys: held, .. },
                ..
            } = &mut combat
            else {
                unreachable!()
            };
            *held = keys;
            for observation in [&strategic, &combat] {
                let view =
                    super::render_query(None, Some(20), Some(observation), "rules", Locale::En)
                        .unwrap();
                assert_eq!(
                    view.contains(KeyRuleV1::Timing.text(Locale::En)),
                    keys.contains(&false)
                );
                for (held, opportunities) in keys.into_iter().zip([
                    KeyRuleV1::RubyOpportunities.text(Locale::En),
                    KeyRuleV1::ChestOpportunities.text(Locale::En),
                    KeyRuleV1::EmeraldOpportunities.text(Locale::En),
                ]) {
                    assert_eq!(view.contains(opportunities), !held, "{view}");
                }
                // A deliberate query retains the complete rule catalogue.
                let full =
                    super::render_query(None, Some(20), Some(observation), "keys", Locale::En)
                        .unwrap();
                assert!(full.contains(KeyRuleV1::RubyOpportunities.text(Locale::En)));
                assert!(full.contains(KeyRuleV1::ChestOpportunities.text(Locale::En)));
                assert!(full.contains(KeyRuleV1::EmeraldOpportunities.text(Locale::En)));
            }
        }
        let TaskPublicObservationV1::Strategic { observation } = &mut strategic else {
            unreachable!()
        };
        observation.run_goal = PlannerRunGoal::ActThreeVictory;
        assert!(
            !super::render_query(None, Some(20), Some(&strategic), "rules", Locale::En)
                .unwrap()
                .contains("Objective encounter definition")
        );
        assert!(!render_query(None, Some(20), "rules")
            .unwrap()
            .contains("Objective encounter definition"));
    }

    #[test]
    fn run_rules_follow_the_public_map_boss_in_preparation_and_combat() {
        use monsters::EnemyId;
        use sts_oracle_learning::agent::information::state::public_combat_state_v1;
        use sts_oracle_learning::ai::planner_core::PlannerRunGoal;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let mut strategic: TaskPublicObservationV1 =
            serde_json::from_value(fixture["payload"]["decision"]["observation"].clone()).unwrap();
        for (act, boss) in [
            (1, EncounterId::TheGuardian),
            (2, EncounterId::Collector),
            (3, EncounterId::TimeEater),
            (4, EncounterId::TheHeart),
            (1, EncounterId::TheGuardian),
        ] {
            let TaskPublicObservationV1::Strategic { observation } = &mut strategic else {
                panic!("expected strategic fixture")
            };
            observation.run_goal = PlannerRunGoal::HeartVictory;
            observation.run.act = act;
            observation.public_map.boss = Some(boss);
            let mut state = sts_oracle_learning::test_support::blank_test_combat();
            state.meta.ascension_level = 20;
            let combat = TaskPublicObservationV1::Combat {
                run_context: PublicCombatRunContextV1::Available {
                    run_goal: observation.run_goal,
                    ascension_level: 20,
                    combat_victory_guarantees_full_heal: false,
                    act,
                    floor: observation.run.floor,
                    keys: observation.run.keys,
                    public_map: observation.public_map.clone(),
                    encounter_id: None,
                },
                observation: public_combat_state_v1(&state),
            };
            let mut expected = String::new();
            write_encounter(&mut expected, boss, 20, Locale::En).unwrap();
            for observation in [&strategic, &combat] {
                let view =
                    super::render_query(None, Some(20), Some(observation), "rules", Locale::En)
                        .unwrap();
                assert!(view.contains(&format!("Public map Boss definition | Act {act}:")));
                assert!(view.contains(&format!("Current Act {act} deck-lab encounters:")));
                assert!(view.contains("The catalogue updates with the Act and public Boss"));
                for entry in public_encounter_pool(act, EncounterPoolTier::Elite) {
                    let mut elite_definition = String::new();
                    write_encounter(&mut elite_definition, entry.encounter, 20, Locale::En)
                        .unwrap();
                    assert!(view.contains(&elite_definition));
                    assert_eq!(
                        view.matches(&format!(
                            "Encounter definition: {}",
                            entry.encounter.stable_id()
                        ))
                        .count(),
                        1
                    );
                }
                if act == 2 {
                    assert!(view.contains("add 3 Wound to discard pile"));
                }
                if act == 4 {
                    assert!(view.contains("Fixed Act 4 elite encounter:"));
                    assert!(view.contains("Enemy definition: Spire Shield | A20"));
                    assert!(view.contains("Enemy definition: Spire Spear | A20"));
                }
                assert!(view.contains(&expected));
                assert_eq!(
                    view.matches(&format!(
                        "Enemy definition: {}",
                        label(EnemyId::CorruptHeart)
                    ))
                    .count(),
                    1
                );
                assert_eq!(
                    view.matches(&format!("Encounter definition: {}", boss.stable_id()))
                        .count(),
                    1
                );
            }
        }
        let TaskPublicObservationV1::Strategic { observation } = &mut strategic else {
            unreachable!()
        };
        observation.public_map.boss = None;
        assert!(
            super::render_query(None, Some(20), Some(&strategic), "rules", Locale::En)
                .unwrap()
                .contains("Current Act 1 elite encounter pool")
        );
        assert!(
            !super::render_query(None, Some(20), Some(&strategic), "rules", Locale::En)
                .unwrap()
                .contains("Public map Boss definition")
        );
    }

    #[test]
    fn act_two_enemy_queries_explain_the_players_observed_mechanics() {
        let sphere = render_query(None, Some(20), "enemy SphericGuardian").unwrap();
        assert!(sphere.contains("20 HP, 40 Block, 3 Artifact and Barricade"));
        assert!(sphere.contains("gain 35 Block"));
        assert!(sphere.contains("apply 5 Frail to player"));
        let collector = render_query(None, Some(20), "enemy TheCollector").unwrap();
        assert!(collector.contains("apply 5 Strength to all monsters"));
        assert!(collector.contains("gain 23 Block for self"));
        for power in ["Weak", "Vulnerable", "Frail"] {
            assert!(collector.contains(&format!("apply 5 {power} to player")));
        }
        assert!(collector.contains("first turn summons two Torch Heads"));
        assert!(render_query(None, Some(20), "enemy TorchHead")
            .unwrap()
            .contains("7 base damage/hit (hits: 1)"));
        assert!(!sphere.contains("Unknown:"));
        assert!(!collector.contains("Unknown:"));
    }

    #[test]
    fn observed_remaining_duration_is_not_presented_as_application_size() {
        let p = CombatLearningPowerV1 {
            power: powers::PowerId::Vulnerable,
            amount: 1,
            extra_data: 0,
            just_applied: false,
            payload_card: None,
            source_creature: None,
        };
        let mut out = String::new();
        write_observed_amount(&mut out, "Player", &p, Locale::En);
        assert!(out.contains("remaining duration 1"));
        assert!(!out.contains("Newly applied"));
        write_observed_amount(
            &mut out,
            "Player",
            &CombatLearningPowerV1 {
                just_applied: true,
                ..p
            },
            Locale::En,
        );
        assert!(out.contains("decrement is skipped"));
    }

    #[test]
    fn bilingual_power_queries_keep_complete_rules_and_observed_amount_semantics() {
        for id in powers::PowerId::STABLE_IDS {
            let power = powers::PowerId::from_stable_id(id).unwrap();
            let Some(fact) = powers::mechanics::get(power) else {
                continue;
            };
            for locale in [Locale::En, Locale::ZhCn] {
                let reading =
                    super::render_query(None, Some(20), None, &format!("power {id}"), locale)
                        .unwrap();
                for text in [
                    fact.amount_unit(locale),
                    fact.effect(locale),
                    fact.timing(locale),
                ] {
                    assert!(reading.contains(&text), "{id}/{locale}: {reading}");
                }
                assert!(!reading.contains("（英文）") && !reading.contains("{{"));
            }
        }
        let mut state = sts_oracle_learning::test_support::blank_test_combat();
        let mut monster =
            sts_oracle_learning::test_support::test_monster(monsters::EnemyId::CorruptHeart);
        monster.slot = 0;
        state.entities.monsters.push(monster);
        let mut public =
            sts_oracle_learning::agent::information::state::public_combat_state_v1(&state);
        public.player.powers = [
            (powers::PowerId::Strength, -2, 0),
            (powers::PowerId::Vulnerable, 2, 0),
            (powers::PowerId::Confusion, -1, 0),
            (powers::PowerId::Combust, 7, 2),
        ]
        .into_iter()
        .map(|(power, amount, extra_data)| CombatLearningPowerV1 {
            power,
            amount,
            extra_data,
            just_applied: true,
            payload_card: None,
            source_creature: None,
        })
        .collect();
        public.monsters[0].powers = vec![CombatLearningPowerV1 {
            power: powers::PowerId::Invincible,
            amount: 17,
            extra_data: 200,
            just_applied: false,
            payload_card: None,
            source_creature: None,
        }];
        let before = serde_json::to_value(&public).unwrap();
        for locale in [Locale::En, Locale::ZhCn] {
            let current =
                super::render_query(Some(&public), Some(20), None, "powers", locale).unwrap();
            let expected = match locale {
                Locale::En => [
                    "current amount -2",
                    "remaining duration 2",
                    "present (not a numeric stack count)",
                    "damage to each enemy 7; self HP loss 2",
                    "remaining damage allowance 17; reset cap 200",
                    "decrement is skipped",
                ],
                Locale::ZhCn => [
                    "当前数值 -2",
                    "剩余持续时间 2",
                    "已生效，不是数值层数",
                    "对每个敌人造成 7 点伤害，自身失去 2 点生命",
                    "本回合剩余承伤额度 17；持有者回合开始时重置为 200",
                    "跳过下一次回合结束时的持续时间递减",
                ],
            };
            for text in expected {
                assert!(current.contains(text), "{locale}: {current}");
            }
        }
        assert_eq!(serde_json::to_value(&public).unwrap(), before);
    }

    #[test]
    fn slot_query_uses_visible_intent_without_revealing_a_hidden_move() {
        use sts_oracle_learning::agent::information::state::public_combat_state_v1;
        let mut state = sts_oracle_learning::test_support::blank_test_combat();
        let mut monster =
            sts_oracle_learning::test_support::test_monster(monsters::EnemyId::FungiBeast);
        monster.slot = 0;
        state.entities.monsters.push(monster);
        let mut public = public_combat_state_v1(&state);
        public.player.ascension_level = 20;
        public.monsters[0].intent.intent = None;
        public.monsters[0].intent.preview_damage_per_hit = Some(999);
        public.monsters[0].intent.hidden_reason = Some(
            sts_oracle_learning::agent::information::combat::HiddenInformationReasonV1::RunicDome,
        );
        let reading = render_query(Some(&public), Some(20), "M0").unwrap();
        assert!(reading.contains("Current visible intent: Intent hidden"));
        assert!(!reading.contains("999"));
        assert!(reading.contains("Possible moves (base values before powers; not a prediction)"));
        assert!(render_query(Some(&public), Some(20), "M1").is_err());
    }
}
