pub(crate) fn syntax_message(
    locale: sts_presentation::Locale,
    key: &str,
    parameters: &[(&str, &str)],
) -> String {
    locale
        .text("selector", key, parameters)
        .expect("action syntax message")
}

use sts_oracle_learning::agent::information::action::{
    PublicCombatAtomicActionV1, PublicCombatIndexedChoiceCandidateV1,
    PublicCombatSelectionDomainCandidateV1,
};
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicChoiceV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskStrategicActionV1, TaskStrategicCardV1,
};
use sts_oracle_learning::ai::planner_core::PlannerRewardDescriptor;
use sts_oracle_learning::content::identity::GameplayIdentitySurface;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunPlayActionSelectorV1 {
    OrdinalPath(Vec<usize>),
    /// Select one uniquely routine progression action. This is deliberately
    /// narrower than "pick the only legal candidate": card, reward, event,
    /// and shop purchase choices remain explicit even when only one exists.
    Advance,
    EndTurn,
    Proceed,
    Cancel,
    ChooseIndexed(NamedOccurrenceV1),
    PlayCard {
        card: NamedOccurrenceV1,
        target_monster_slot: Option<u8>,
    },
    UsePotion {
        potion: NamedOccurrenceV1,
        target_monster_slot: Option<u8>,
    },
    DiscardPotion(NamedOccurrenceV1),
    ChooseRoute {
        x: i32,
        y: i32,
    },
    OpenCardReward {
        occurrence: Option<usize>,
    },
    SingingBowl {
        occurrence: Option<usize>,
    },
    ClaimReward(NamedOccurrenceV1),
    TakeCard(NamedOccurrenceV1),
    SkipCardReward {
        occurrence: Option<usize>,
    },
    Buy(NamedOccurrenceV1),
    RemoveCard(NamedOccurrenceV1),
    Smith(NamedOccurrenceV1),
    Rest,
    Dig,
    Lift,
    Toke(NamedOccurrenceV1),
    Recall,
    SelectCards(Vec<NamedOccurrenceV1>),
    LeaveShop,
    OpenPendingRewards,
    TakeBossRelic(NamedOccurrenceV1),
    SkipBossRelic,
    OpenChest,
    EventOption {
        event: String,
        option_index: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedOccurrenceV1 {
    pub name: String,
    pub upgrades: Option<u8>,
    pub occurrence: Option<usize>,
}

pub fn parse_action_line_v1(
    spec: &str,
    command: &str,
    locale: sts_presentation::Locale,
) -> Result<Vec<RunPlayActionSelectorV1>, String> {
    if spec.is_empty() {
        return Err(syntax_message(
            locale,
            "emptyLine",
            &[("command", &(command.to_owned()))],
        ));
    }
    spec.split('/')
        .map(|step| parse_action_selector_v1(step, command, locale))
        .collect()
}

pub fn parse_action_selector_v1(
    spec: &str,
    command: &str,
    locale: sts_presentation::Locale,
) -> Result<RunPlayActionSelectorV1, String> {
    if spec.is_empty() {
        return Err(syntax_message(
            locale,
            "emptyAction",
            &[("command", &(command.to_owned()))],
        ));
    }
    if spec == "open-card" {
        return Ok(RunPlayActionSelectorV1::OpenCardReward { occurrence: None });
    }
    if let Some(occurrence) = spec.strip_prefix("open-card#") {
        return Ok(RunPlayActionSelectorV1::OpenCardReward {
            occurrence: Some(parse_number(occurrence, "occurrence", command, locale)?),
        });
    }
    if spec == "singing-bowl" {
        return Ok(RunPlayActionSelectorV1::SingingBowl { occurrence: None });
    }
    if let Some(occurrence) = spec.strip_prefix("singing-bowl#") {
        return Ok(RunPlayActionSelectorV1::SingingBowl {
            occurrence: Some(parse_number(occurrence, "occurrence", command, locale)?),
        });
    }
    if let Some(occurrence) = spec.strip_prefix("skip-card#") {
        return Ok(RunPlayActionSelectorV1::SkipCardReward {
            occurrence: Some(parse_number(occurrence, "occurrence", command, locale)?),
        });
    }
    match spec {
        "advance" => return Ok(RunPlayActionSelectorV1::Advance),
        "end" => return Ok(RunPlayActionSelectorV1::EndTurn),
        "proceed" => return Ok(RunPlayActionSelectorV1::Proceed),
        "cancel" => return Ok(RunPlayActionSelectorV1::Cancel),
        "skip-card" => return Ok(RunPlayActionSelectorV1::SkipCardReward { occurrence: None }),
        "rest" => return Ok(RunPlayActionSelectorV1::Rest),
        "dig" => return Ok(RunPlayActionSelectorV1::Dig),
        "lift" => return Ok(RunPlayActionSelectorV1::Lift),
        "recall" => return Ok(RunPlayActionSelectorV1::Recall),
        "leave" => return Ok(RunPlayActionSelectorV1::LeaveShop),
        "open-rewards" => return Ok(RunPlayActionSelectorV1::OpenPendingRewards),
        "skip-boss-relic" => return Ok(RunPlayActionSelectorV1::SkipBossRelic),
        "open-chest" => return Ok(RunPlayActionSelectorV1::OpenChest),
        _ => {}
    }
    let (verb, payload) = spec.split_once(':').ok_or_else(|| {
        syntax_message(
            locale,
            "unknownAction",
            &[
                ("spec", &(format!("{spec:?}"))),
                ("command", &(command.to_owned())),
            ],
        )
    })?;
    match verb {
        "ordinal" => Ok(RunPlayActionSelectorV1::OrdinalPath(parse_ordinal_path(
            payload, command, locale,
        )?)),
        "play" => {
            let (name, target_monster_slot) = parse_optional_target(payload, command, locale)?;
            Ok(RunPlayActionSelectorV1::PlayCard {
                card: parse_named_occurrence(name, command, locale)?,
                target_monster_slot,
            })
        }
        "use-potion" => {
            let (name, target_monster_slot) = parse_optional_target(payload, command, locale)?;
            Ok(RunPlayActionSelectorV1::UsePotion {
                potion: parse_named_occurrence(name, command, locale)?,
                target_monster_slot,
            })
        }
        "discard-potion" => Ok(RunPlayActionSelectorV1::DiscardPotion(
            parse_named_occurrence(payload, command, locale)?,
        )),
        "choose" => Ok(RunPlayActionSelectorV1::ChooseIndexed(
            parse_named_occurrence(payload, command, locale)?,
        )),
        "route" => {
            // PowerShell coerces an unquoted `2,2` argument passed to a
            // string-array parameter into `2 2`. Accept both spellings so
            // the map's displayed `go 2,2` action can be pasted unchanged.
            let coordinates = payload
                .split(|character: char| {
                    matches!(character, '.' | '/' | ',') || character.is_whitespace()
                })
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let [x, y] = coordinates.as_slice() else {
                return Err(syntax_message(
                    locale,
                    "route",
                    &[("command", &(command.to_owned()))],
                ));
            };
            Ok(RunPlayActionSelectorV1::ChooseRoute {
                x: parse_number(x, "routeX", command, locale)?,
                y: parse_number(y, "routeY", command, locale)?,
            })
        }
        "claim" => Ok(RunPlayActionSelectorV1::ClaimReward(
            parse_named_occurrence(payload, command, locale)?,
        )),
        "take" => Ok(RunPlayActionSelectorV1::TakeCard(parse_named_occurrence(
            payload, command, locale,
        )?)),
        "buy" => Ok(RunPlayActionSelectorV1::Buy(parse_named_occurrence(
            payload, command, locale,
        )?)),
        "remove" => Ok(RunPlayActionSelectorV1::RemoveCard(parse_named_occurrence(
            payload, command, locale,
        )?)),
        "smith" => Ok(RunPlayActionSelectorV1::Smith(parse_named_occurrence(
            payload, command, locale,
        )?)),
        "toke" => Ok(RunPlayActionSelectorV1::Toke(parse_named_occurrence(
            payload, command, locale,
        )?)),
        "select" => Ok(RunPlayActionSelectorV1::SelectCards(if payload == "none" {
            Vec::new()
        } else {
            payload
                .split(',')
                .map(|card| parse_named_occurrence(card, command, locale))
                .collect::<Result<Vec<_>, _>>()?
        })),
        "boss-relic" => Ok(RunPlayActionSelectorV1::TakeBossRelic(
            parse_named_occurrence(payload, command, locale)?,
        )),
        "event" => {
            let (event, option) = payload.rsplit_once(':').ok_or_else(|| {
                syntax_message(locale, "event", &[("command", &(command.to_owned()))])
            })?;
            if event.is_empty() {
                return Err(syntax_message(
                    locale,
                    "eventName",
                    &[("command", &(command.to_owned()))],
                ));
            }
            Ok(RunPlayActionSelectorV1::EventOption {
                event: event.to_string(),
                option_index: parse_number(option, "eventOption", command, locale)?,
            })
        }
        _ => Err(syntax_message(
            locale,
            "verb",
            &[("verb", &(format!("{verb:?}")))],
        )),
    }
}

pub fn resolve_action_selector_v1(
    decision: &TaskPublicDecisionV1,
    selector: &RunPlayActionSelectorV1,
) -> Result<TaskPublicChoiceV1, String> {
    if let RunPlayActionSelectorV1::OrdinalPath(ordinal_path) = selector {
        let choice = TaskPublicChoiceV1 {
            ordinal_path: ordinal_path.clone(),
        };
        decision.validate_choice(&choice)?;
        return Ok(choice);
    }

    let matches = match selector {
        RunPlayActionSelectorV1::Advance => return resolve_routine_advance_v1(decision),
        RunPlayActionSelectorV1::EndTurn => atomic_matches(decision, |action| {
            matches!(action, PublicCombatAtomicActionV1::EndTurn)
        }),
        RunPlayActionSelectorV1::Proceed => decision
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| match candidate {
                TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::Proceed,
                }
                | TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::Proceed { .. },
                } => Some(ordinal),
                _ => None,
            })
            .collect(),
        RunPlayActionSelectorV1::Cancel => decision
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| match candidate {
                TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::Cancel,
                }
                | TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::Cancel { .. },
                } => Some(ordinal),
                _ => None,
            })
            .collect(),
        RunPlayActionSelectorV1::PlayCard {
            card,
            target_monster_slot,
        } => return resolve_play_card(decision, card, *target_monster_slot),
        RunPlayActionSelectorV1::UsePotion {
            potion,
            target_monster_slot,
        } => return resolve_use_potion(decision, potion, *target_monster_slot),
        RunPlayActionSelectorV1::DiscardPotion(potion) => {
            return resolve_discard_potion(decision, potion)
        }
        RunPlayActionSelectorV1::ChooseIndexed(candidate) => {
            let matches = decision
                .candidates
                .iter()
                .enumerate()
                .filter_map(|(ordinal, public_candidate)| {
                    let TaskPublicCandidateV1::CombatIndexedChoice {
                        candidate: indexed, ..
                    } = public_candidate
                    else {
                        return None;
                    };
                    let (identity_matches, upgrades) = match indexed {
                        PublicCombatIndexedChoiceCandidateV1::Card { card_id, upgrades } => (
                            card_id.matches_input_alias_v1(&candidate.name),
                            Some(*upgrades),
                        ),
                        PublicCombatIndexedChoiceCandidateV1::Stance { stance } => {
                            (stance.matches_input_alias_v1(&candidate.name), None)
                        }
                    };
                    (identity_matches
                        && candidate
                            .upgrades
                            .is_none_or(|expected| upgrades == Some(expected)))
                    .then_some(ordinal)
                })
                .collect();
            return select_named_match(decision, selector, candidate, matches);
        }
        RunPlayActionSelectorV1::ChooseRoute { x, y } => strategic_matches(decision, |action| {
            matches!(
                action,
                TaskStrategicActionV1::ChooseRouteNode {
                    x: candidate_x,
                    y: candidate_y,
                    ..
                } if candidate_x == x && candidate_y == y
            )
        }),
        RunPlayActionSelectorV1::OpenCardReward { occurrence } => {
            let matches = strategic_matches(decision, |action| {
                matches!(action, TaskStrategicActionV1::OpenCardReward { .. })
            });
            return select_occurrence_match(decision, selector, *occurrence, matches);
        }
        RunPlayActionSelectorV1::SingingBowl { occurrence } => {
            let matches = strategic_matches(decision, |action| {
                matches!(action, TaskStrategicActionV1::SingingBowl { .. })
            });
            return select_occurrence_match(decision, selector, *occurrence, matches);
        }
        RunPlayActionSelectorV1::ClaimReward(reward) => {
            let matches = named_strategic_matches(decision, reward, |action| match action {
                TaskStrategicActionV1::ClaimReward {
                    reward: candidate_reward,
                    ..
                } => Some((
                    reward_matches_input_alias_v1(candidate_reward, &reward.name),
                    None,
                )),
                _ => None,
            })?;
            return select_named_match(decision, selector, reward, matches);
        }
        RunPlayActionSelectorV1::TakeCard(card) => {
            let matches = named_strategic_matches(decision, card, |action| match action {
                TaskStrategicActionV1::TakeCard {
                    card: candidate_card,
                    upgrades,
                    ..
                } => Some((
                    candidate_card.matches_input_alias_v1(&card.name),
                    Some(*upgrades),
                )),
                _ => None,
            })?;
            return select_named_match(decision, selector, card, matches);
        }
        RunPlayActionSelectorV1::SkipCardReward { occurrence } => {
            let matches = strategic_matches(decision, |action| {
                matches!(action, TaskStrategicActionV1::SkipCardReward { .. })
            });
            return select_occurrence_match(decision, selector, *occurrence, matches);
        }
        RunPlayActionSelectorV1::Buy(item) => {
            let matches = named_strategic_matches(decision, item, |action| match action {
                TaskStrategicActionV1::BuyCard { card, upgrades, .. } => {
                    Some((card.matches_input_alias_v1(&item.name), Some(*upgrades)))
                }
                TaskStrategicActionV1::BuyRelic { relic, .. } => {
                    Some((relic.matches_input_alias_v1(&item.name), None))
                }
                TaskStrategicActionV1::BuyPotion { potion, .. } => {
                    Some((potion.matches_input_alias_v1(&item.name), None))
                }
                _ => None,
            })?;
            return select_named_match(decision, selector, item, matches);
        }
        RunPlayActionSelectorV1::RemoveCard(card) => {
            let matches = named_strategic_matches(decision, card, |action| match action {
                TaskStrategicActionV1::RemoveCard {
                    card: candidate_card,
                    upgrades,
                    ..
                } => Some((
                    candidate_card.matches_input_alias_v1(&card.name),
                    Some(*upgrades),
                )),
                _ => None,
            })?;
            return select_named_match(decision, selector, card, matches);
        }
        RunPlayActionSelectorV1::Smith(card) => {
            let matches = named_strategic_matches(decision, card, |action| match action {
                TaskStrategicActionV1::Smith {
                    card: candidate_card,
                    upgrades,
                    ..
                } => Some((
                    candidate_card.matches_input_alias_v1(&card.name),
                    Some(*upgrades),
                )),
                _ => None,
            })?;
            return select_named_match(decision, selector, card, matches);
        }
        RunPlayActionSelectorV1::Rest => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::Rest)
        }),
        RunPlayActionSelectorV1::Dig => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::Dig)
        }),
        RunPlayActionSelectorV1::Lift => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::Lift)
        }),
        RunPlayActionSelectorV1::Toke(card) => {
            let matches = named_strategic_matches(decision, card, |action| match action {
                TaskStrategicActionV1::Toke {
                    card: candidate_card,
                    upgrades,
                    ..
                } => Some((
                    candidate_card.matches_input_alias_v1(&card.name),
                    Some(*upgrades),
                )),
                _ => None,
            })?;
            return select_named_match(decision, selector, card, matches);
        }
        RunPlayActionSelectorV1::Recall => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::Recall)
        }),
        RunPlayActionSelectorV1::SelectCards(cards) => match decision.observation {
            TaskPublicObservationV1::Strategic { .. } => {
                return resolve_run_card_selection_v1(decision, selector, cards)
            }
            TaskPublicObservationV1::Combat { .. } => {
                return resolve_combat_card_selection_v1(decision, selector, cards)
            }
        },
        RunPlayActionSelectorV1::LeaveShop => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::LeaveShop)
        }),
        RunPlayActionSelectorV1::OpenPendingRewards => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::OpenPendingRewards)
        }),
        RunPlayActionSelectorV1::TakeBossRelic(relic) => {
            let matches = named_strategic_matches(decision, relic, |action| match action {
                TaskStrategicActionV1::TakeBossRelic {
                    relic: candidate_relic,
                    ..
                } => Some((candidate_relic.matches_input_alias_v1(&relic.name), None)),
                _ => None,
            })?;
            return select_named_match(decision, selector, relic, matches);
        }
        RunPlayActionSelectorV1::SkipBossRelic => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::SkipBossRelic)
        }),
        RunPlayActionSelectorV1::OpenChest => strategic_matches(decision, |action| {
            matches!(action, TaskStrategicActionV1::OpenChest)
        }),
        RunPlayActionSelectorV1::EventOption {
            event,
            option_index,
        } => strategic_matches(decision, |action| {
            matches!(
                action,
                TaskStrategicActionV1::ChooseEventOption {
                    event: candidate_event,
                    option_index: candidate_option,
                    ..
                } if candidate_event.matches_input_alias_v1(event)
                    && candidate_option == option_index
            )
        }),
        RunPlayActionSelectorV1::OrdinalPath(_) => unreachable!(),
    };
    select_unique_match(decision, selector, matches)
}

/// Converts one validated public choice back into an executable semantic
/// action. Advice, audit, and exact witness output use this instead of leaking
/// an ordinal whose meaning can change at the next decision surface.
pub fn semantic_action_for_choice_v1(
    decision: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
) -> Result<String, String> {
    decision.validate_choice(choice)?;
    let root_ordinal = *choice
        .ordinal_path
        .first()
        .ok_or_else(|| "semantic action requires a non-empty public choice".to_string())?;
    if !matches!(
        decision.candidates.get(root_ordinal),
        Some(TaskPublicCandidateV1::Strategic { .. })
    ) {
        let spec = combat_semantic_action_for_choice_v1(decision, choice, root_ordinal)?;
        validate_generated_semantic_action_v1(decision, choice, &spec)?;
        return Ok(spec);
    }
    let action = match decision.candidates.get(root_ordinal) {
        Some(TaskPublicCandidateV1::Strategic { action }) => action,
        Some(_) => unreachable!("combat choices returned above"),
        None => return Err("semantic action root ordinal is out of range".to_string()),
    };
    let spec = match action {
        TaskStrategicActionV1::ChooseRouteNode { x, y, .. } => format!("route:{x}.{y}"),
        TaskStrategicActionV1::ChooseEventOption {
            event,
            option_index,
            ..
        } => format!("event:{}:{option_index}", event.stable_id()),
        TaskStrategicActionV1::TakeCard { card, upgrades, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "take",
            card.stable_id(),
            Some(*upgrades),
            |candidate| matches!(candidate, TaskStrategicActionV1::TakeCard { card: other, upgrades: other_upgrades, .. } if other == card && other_upgrades == upgrades),
        )?,
        TaskStrategicActionV1::OpenCardReward { .. } => {
            occurrence_candidate_spec_v1(decision, root_ordinal, "open-card", |candidate| {
                matches!(candidate, TaskStrategicActionV1::OpenCardReward { .. })
            })?
        }
        TaskStrategicActionV1::SingingBowl { .. } => {
            occurrence_candidate_spec_v1(decision, root_ordinal, "singing-bowl", |candidate| {
                matches!(candidate, TaskStrategicActionV1::SingingBowl { .. })
            })?
        }
        TaskStrategicActionV1::SkipCardReward { .. } => {
            occurrence_candidate_spec_v1(decision, root_ordinal, "skip-card", |candidate| {
                matches!(candidate, TaskStrategicActionV1::SkipCardReward { .. })
            })?
        }
        TaskStrategicActionV1::ClaimReward { reward, .. } => {
            let name = reward_semantic_name_v1(reward);
            named_candidate_spec_v1(
                decision,
                root_ordinal,
                "claim",
                &name,
                None,
                // Occurrences belong to the same name-matched family as
                // resolution, not equal reward payloads (Gold amounts differ).
                |candidate| matches!(candidate, TaskStrategicActionV1::ClaimReward { reward: other, .. } if reward_matches_input_alias_v1(other, &name)),
            )?
        }
        TaskStrategicActionV1::Rest => "rest".to_string(),
        TaskStrategicActionV1::Smith { card, upgrades, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "smith",
            card.stable_id(),
            Some(*upgrades),
            |candidate| matches!(candidate, TaskStrategicActionV1::Smith { card: other, upgrades: other_upgrades, .. } if other == card && other_upgrades == upgrades),
        )?,
        TaskStrategicActionV1::Dig => "dig".to_string(),
        TaskStrategicActionV1::Lift => "lift".to_string(),
        TaskStrategicActionV1::Toke { card, upgrades, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "toke",
            card.stable_id(),
            Some(*upgrades),
            |candidate| matches!(candidate, TaskStrategicActionV1::Toke { card: other, upgrades: other_upgrades, .. } if other == card && other_upgrades == upgrades),
        )?,
        TaskStrategicActionV1::Recall => "recall".to_string(),
        TaskStrategicActionV1::BuyCard { card, upgrades, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "buy",
            card.stable_id(),
            Some(*upgrades),
            |candidate| matches!(candidate, TaskStrategicActionV1::BuyCard { card: other, upgrades: other_upgrades, .. } if other == card && other_upgrades == upgrades),
        )?,
        TaskStrategicActionV1::BuyRelic { relic, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "buy",
            relic.stable_id(),
            None,
            |candidate| matches!(candidate, TaskStrategicActionV1::BuyRelic { relic: other, .. } if other == relic),
        )?,
        TaskStrategicActionV1::BuyPotion { potion, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "buy",
            potion.stable_id(),
            None,
            |candidate| matches!(candidate, TaskStrategicActionV1::BuyPotion { potion: other, .. } if other == potion),
        )?,
        TaskStrategicActionV1::UseRunPotion { potion, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "use-potion",
            potion.stable_id(),
            None,
            |candidate| matches!(candidate, TaskStrategicActionV1::UseRunPotion { potion: other, .. } if other == potion),
        )?,
        TaskStrategicActionV1::DiscardRunPotion { potion, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "discard-potion",
            potion.stable_id(),
            None,
            |candidate| matches!(candidate, TaskStrategicActionV1::DiscardRunPotion { potion: other, .. } if other == potion),
        )?,
        TaskStrategicActionV1::RemoveCard { card, upgrades, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "remove",
            card.stable_id(),
            Some(*upgrades),
            |candidate| matches!(candidate, TaskStrategicActionV1::RemoveCard { card: other, upgrades: other_upgrades, .. } if other == card && other_upgrades == upgrades),
        )?,
        TaskStrategicActionV1::OpenPendingRewards => "open-rewards".to_string(),
        TaskStrategicActionV1::LeaveShop => "leave".to_string(),
        TaskStrategicActionV1::TakeBossRelic { relic, .. } => named_candidate_spec_v1(
            decision,
            root_ordinal,
            "boss-relic",
            relic.stable_id(),
            None,
            |candidate| matches!(candidate, TaskStrategicActionV1::TakeBossRelic { relic: other, .. } if other == relic),
        )?,
        TaskStrategicActionV1::SkipBossRelic => "skip-boss-relic".to_string(),
        TaskStrategicActionV1::BeginRunCardSelection {
            selectable_card_ordinals,
            ..
        } => {
            let selected = choice
                .ordinal_path
                .iter()
                .skip(1)
                .map(|selection_ordinal| {
                    selectable_card_ordinals
                        .get(*selection_ordinal)
                        .copied()
                        .ok_or_else(|| {
                            "owner run-card selection ordinal is out of range".to_string()
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            run_card_selection_spec_v1(decision, &selected)?
        }
        TaskStrategicActionV1::SubmitRunSelection {
            selected_card_ordinals,
            ..
        } => run_card_selection_spec_v1(decision, selected_card_ordinals)?,
        TaskStrategicActionV1::OpenChest => "open-chest".to_string(),
        TaskStrategicActionV1::Proceed { .. } => "proceed".to_string(),
        TaskStrategicActionV1::Cancel { .. } => "cancel".to_string(),
    };
    validate_generated_semantic_action_v1(decision, choice, &spec)?;
    Ok(spec)
}

pub fn transactional_play_alias_for_choice_v1(
    decision: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
) -> Result<Option<String>, String> {
    // A selection family needs member ordinals; its root alone is not an
    // executable choice. Only atomic card plays can have play aliases.
    if !matches!(
        choice
            .ordinal_path
            .first()
            .and_then(|ordinal| decision.candidates.get(*ordinal)),
        Some(TaskPublicCandidateV1::CombatAtomic {
            action: PublicCombatAtomicActionV1::PlayCard { .. }
        })
    ) {
        return Ok(None);
    }
    let exact = semantic_action_for_choice_v1(decision, choice)?;
    let RunPlayActionSelectorV1::PlayCard {
        mut card,
        target_monster_slot,
    } = parse_action_selector_v1(&exact, "transaction-alias", sts_presentation::Locale::En)?
    else {
        return Ok(None);
    };
    if card.occurrence.take().is_none() {
        return Ok(None);
    }
    let alias = RunPlayActionSelectorV1::PlayCard {
        card: card.clone(),
        target_monster_slot,
    };
    if resolve_action_selector_v1(decision, &alias).is_err() {
        return Ok(None);
    }
    Ok(Some(format!(
        "play:{}{}",
        canonical_upgraded_name_v1(&card.name, card.upgrades),
        target_monster_slot.map_or_else(String::new, |slot| format!("@{slot}"))
    )))
}

fn validate_generated_semantic_action_v1(
    decision: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
    spec: &str,
) -> Result<(), String> {
    let parsed = parse_action_selector_v1(
        spec,
        "generated-semantic-action",
        sts_presentation::Locale::En,
    )?;
    let resolved = resolve_action_selector_v1(decision, &parsed)?;
    if resolved != *choice {
        return Err(format!(
            "generated semantic action {spec:?} resolved to {:?}, expected {:?}",
            resolved.ordinal_path, choice.ordinal_path
        ));
    }
    Ok(())
}

fn combat_semantic_action_for_choice_v1(
    decision: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
    root_ordinal: usize,
) -> Result<String, String> {
    let candidate = decision
        .candidates
        .get(root_ordinal)
        .ok_or_else(|| "semantic action root ordinal is out of range".to_string())?;
    match candidate {
        TaskPublicCandidateV1::CombatIndexedChoice {
            candidate: indexed, ..
        } => {
            if choice.ordinal_path.len() != 1 {
                return Err("indexed combat choice requires one root ordinal".to_string());
            }
            let (name, upgrades) = indexed_choice_identity_v1(indexed);
            let occurrence = combat_candidate_occurrence_v1(decision, root_ordinal, |candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatIndexedChoice {
                        candidate: other,
                        ..
                    } if indexed_choice_identity_v1(other) == (name.clone(), upgrades)
                )
            })?;
            return Ok(format!(
                "choose:{}#{occurrence}",
                canonical_upgraded_name_v1(&name, upgrades)
            ));
        }
        TaskPublicCandidateV1::CombatSelectionFamily { family } => {
            return combat_selection_spec_v1(family, choice)
        }
        TaskPublicCandidateV1::Strategic { .. } => {
            unreachable!("strategic choices are handled by the caller")
        }
        TaskPublicCandidateV1::CombatAtomic { .. } => {}
    }
    if choice.ordinal_path.len() != 1 {
        return Err("atomic combat choice requires one root ordinal".to_string());
    }
    let TaskPublicCandidateV1::CombatAtomic { action } = candidate else {
        unreachable!("non-atomic combat candidates returned above")
    };
    match action {
        PublicCombatAtomicActionV1::PlayCard {
            hand_index,
            target_monster_index,
        } => {
            let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
                return Err("play-card candidate requires a combat observation".to_string());
            };
            let card = observation
                .cards
                .hand
                .cards
                .get(*hand_index)
                .ok_or_else(|| {
                    format!(
                        "play-card hand index {hand_index} is absent from its public observation"
                    )
                })?;
            let target_slot = target_monster_index
                .map(|index| {
                    observation
                        .monsters
                        .get(index)
                        .map(|monster| monster.slot)
                        .ok_or_else(|| {
                            format!(
                                "play-card target monster index {index} is absent from its public observation"
                            )
                        })
                })
                .transpose()?;
            // Number visible card occurrences from the complete ordered hand,
            // not from the subset that remains legally playable at the current
            // energy.  A semantic handle therefore does not silently change
            // merely because an earlier action spent energy.
            let occurrence = observation
                .cards
                .hand
                .cards
                .iter()
                .take(*hand_index)
                .filter(|candidate| {
                    candidate.card_id == card.card_id && candidate.upgrades == card.upgrades
                })
                .count();
            Ok(format!(
                "play:{}#{occurrence}{}",
                canonical_upgraded_name_v1(card.card_id.stable_id(), Some(card.upgrades)),
                target_slot.map_or_else(String::new, |slot| format!("@{slot}"))
            ))
        }
        PublicCombatAtomicActionV1::UsePotion {
            potion_index,
            target_monster_index,
        } => {
            let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
                return Err("use-potion candidate requires a combat observation".to_string());
            };
            let potion = observation
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    format!("use-potion slot {potion_index} is absent from its public observation")
                })?;
            let target_slot = target_monster_index
                .map(|index| {
                    observation
                        .monsters
                        .get(index)
                        .map(|monster| monster.slot)
                        .ok_or_else(|| {
                            format!(
                                "use-potion target monster index {index} is absent from its public observation"
                            )
                        })
                })
                .transpose()?;
            let occurrence = combat_candidate_occurrence_v1(decision, root_ordinal, |candidate| {
                let TaskPublicCandidateV1::CombatAtomic {
                    action:
                        PublicCombatAtomicActionV1::UsePotion {
                            potion_index: candidate_potion_index,
                            target_monster_index: candidate_target_index,
                        },
                } = candidate
                else {
                    return false;
                };
                let candidate_potion = observation
                    .potions
                    .get(*candidate_potion_index)
                    .and_then(Option::as_ref);
                let candidate_slot = candidate_target_index
                    .and_then(|index| observation.monsters.get(index))
                    .map(|monster| monster.slot);
                candidate_potion.is_some_and(|candidate_potion| {
                    candidate_potion.potion_id == potion.potion_id && candidate_slot == target_slot
                })
            })?;
            Ok(format!(
                "use-potion:{}#{occurrence}{}",
                potion.potion_id.stable_id(),
                target_slot.map_or_else(String::new, |slot| format!("@{slot}"))
            ))
        }
        PublicCombatAtomicActionV1::DiscardPotion { potion_index } => {
            let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
                return Err("discard-potion candidate requires a combat observation".to_string());
            };
            let potion = observation
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    format!(
                        "discard-potion slot {potion_index} is absent from its public observation"
                    )
                })?;
            let occurrence = combat_candidate_occurrence_v1(decision, root_ordinal, |candidate| {
                let TaskPublicCandidateV1::CombatAtomic {
                    action:
                        PublicCombatAtomicActionV1::DiscardPotion {
                            potion_index: candidate_potion_index,
                        },
                } = candidate
                else {
                    return false;
                };
                observation
                    .potions
                    .get(*candidate_potion_index)
                    .and_then(Option::as_ref)
                    .is_some_and(|candidate_potion| candidate_potion.potion_id == potion.potion_id)
            })?;
            Ok(format!(
                "discard-potion:{}#{occurrence}",
                potion.potion_id.stable_id()
            ))
        }
        PublicCombatAtomicActionV1::EndTurn => Ok("end".to_string()),
        PublicCombatAtomicActionV1::Proceed => Ok("proceed".to_string()),
        PublicCombatAtomicActionV1::Cancel => Ok("cancel".to_string()),
        PublicCombatAtomicActionV1::SubmitIndexedChoice { .. } => Err(
            "stable semantic combat actions do not yet cover indexed combat choices".to_string(),
        ),
    }
}

fn indexed_choice_identity_v1(
    candidate: &PublicCombatIndexedChoiceCandidateV1,
) -> (String, Option<u8>) {
    match candidate {
        PublicCombatIndexedChoiceCandidateV1::Card { card_id, upgrades } => {
            (card_id.stable_id().to_string(), Some(*upgrades))
        }
        PublicCombatIndexedChoiceCandidateV1::Stance { stance } => {
            (stance.stable_id().to_string(), None)
        }
    }
}

fn combat_selection_spec_v1(
    family: &sts_oracle_learning::agent::information::action::PublicCombatSelectionFamilyV1,
    choice: &TaskPublicChoiceV1,
) -> Result<String, String> {
    if choice.ordinal_path.len() == 1 {
        return Ok("select:none".to_string());
    }
    let mut cards = Vec::with_capacity(choice.ordinal_path.len() - 1);
    for selection_ordinal in choice.ordinal_path.iter().skip(1) {
        let selected = family.domain.get(*selection_ordinal).ok_or_else(|| {
            format!("combat selection ordinal {selection_ordinal} is outside its public domain")
        })?;
        let (name, upgrades, available) = combat_selection_identity_v1(selected);
        if !available {
            return Err(format!(
                "combat selection ordinal {selection_ordinal} is not currently available"
            ));
        }
        let occurrence = family
            .domain
            .iter()
            .take(*selection_ordinal)
            .filter(|candidate| {
                let (candidate_name, candidate_upgrades, candidate_available) =
                    combat_selection_identity_v1(candidate);
                candidate_available && candidate_name == name && candidate_upgrades == upgrades
            })
            .count();
        cards.push(format!(
            "{}#{occurrence}",
            canonical_upgraded_name_v1(&name, upgrades)
        ));
    }
    Ok(format!("select:{}", cards.join(",")))
}

/// A display token for one member of a pending selection. The caller combines
/// tokens when the selection requires more than one card.
pub(crate) fn selection_member_token_v1(
    decision: &TaskPublicDecisionV1,
    root_ordinal: usize,
    member_ordinal: usize,
) -> Result<String, String> {
    let candidate = decision
        .candidates
        .get(root_ordinal)
        .ok_or_else(|| "selection root ordinal is out of range".to_string())?;
    let spec = match candidate {
        TaskPublicCandidateV1::CombatSelectionFamily { family } => {
            let mut choice = TaskPublicChoiceV1::one(root_ordinal);
            choice.ordinal_path.push(member_ordinal);
            combat_selection_spec_v1(family, &choice)?
        }
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::BeginRunCardSelection {
                    selectable_card_ordinals,
                    ..
                },
        } => {
            let card_ordinal = selectable_card_ordinals
                .get(member_ordinal)
                .ok_or_else(|| "selection member ordinal is out of range".to_string())?;
            run_card_selection_spec_v1(decision, &[*card_ordinal])?
        }
        _ => return Err("candidate is not a card selection".to_string()),
    };
    spec.strip_prefix("select:")
        .map(str::to_string)
        .ok_or_else(|| "selection member has no card token".to_string())
}

fn combat_selection_identity_v1(
    candidate: &PublicCombatSelectionDomainCandidateV1,
) -> (String, Option<u8>, bool) {
    match candidate {
        PublicCombatSelectionDomainCandidateV1::Card {
            card_id,
            upgrades,
            eligible,
            ..
        } => (
            card_id.map_or_else(
                || "unknown-card".to_string(),
                |card| card.stable_id().to_string(),
            ),
            *upgrades,
            *eligible,
        ),
        PublicCombatSelectionDomainCandidateV1::Scry {
            card_id,
            currently_present,
            ..
        } => (
            card_id.map_or_else(
                || "unknown-card".to_string(),
                |card| card.stable_id().to_string(),
            ),
            None,
            *currently_present,
        ),
    }
}

fn combat_selection_matches_input_v1(
    candidate: &PublicCombatSelectionDomainCandidateV1,
    requested: &str,
) -> bool {
    match candidate {
        PublicCombatSelectionDomainCandidateV1::Card { card_id, .. }
        | PublicCombatSelectionDomainCandidateV1::Scry { card_id, .. } => {
            card_id.is_some_and(|card_id| card_id.matches_input_alias_v1(requested))
        }
    }
}

fn combat_candidate_occurrence_v1(
    decision: &TaskPublicDecisionV1,
    root_ordinal: usize,
    matches: impl Fn(&TaskPublicCandidateV1) -> bool,
) -> Result<usize, String> {
    decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| matches(candidate).then_some(ordinal))
        .position(|ordinal| ordinal == root_ordinal)
        .ok_or_else(|| "selected combat candidate was absent from its semantic group".to_string())
}

fn occurrence_candidate_spec_v1(
    decision: &TaskPublicDecisionV1,
    root_ordinal: usize,
    verb: &str,
    matches: impl Fn(&TaskStrategicActionV1) -> bool,
) -> Result<String, String> {
    let occurrence = strategic_candidate_occurrence_v1(decision, root_ordinal, matches)?;
    Ok(format!("{verb}#{occurrence}"))
}

fn named_candidate_spec_v1(
    decision: &TaskPublicDecisionV1,
    root_ordinal: usize,
    verb: &str,
    name: &str,
    upgrades: Option<u8>,
    matches: impl Fn(&TaskStrategicActionV1) -> bool,
) -> Result<String, String> {
    let occurrence = strategic_candidate_occurrence_v1(decision, root_ordinal, matches)?;
    Ok(format!(
        "{verb}:{}#{occurrence}",
        canonical_upgraded_name_v1(name, upgrades)
    ))
}

fn strategic_candidate_occurrence_v1(
    decision: &TaskPublicDecisionV1,
    root_ordinal: usize,
    matches: impl Fn(&TaskStrategicActionV1) -> bool,
) -> Result<usize, String> {
    decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::Strategic { action } if matches(action) => Some(ordinal),
            _ => None,
        })
        .position(|ordinal| ordinal == root_ordinal)
        .ok_or_else(|| {
            "selected strategic candidate was absent from its semantic group".to_string()
        })
}

fn run_card_selection_spec_v1(
    decision: &TaskPublicDecisionV1,
    selected_card_ordinals: &[usize],
) -> Result<String, String> {
    if selected_card_ordinals.is_empty() {
        return Ok("select:none".to_string());
    }
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("run-card selection requires a strategic observation".to_string());
    };
    let cards = run_card_selection_tokens_v1(&observation.cards, selected_card_ordinals)?;
    Ok(format!("select:{}", cards.join(",")))
}

pub(crate) fn run_card_selection_tokens_v1(
    deck: &[TaskStrategicCardV1],
    selected_card_ordinals: &[usize],
) -> Result<Vec<String>, String> {
    let mut cards = Vec::with_capacity(selected_card_ordinals.len());
    for selected_ordinal in selected_card_ordinals {
        let card = deck
            .iter()
            .find(|card| card.ordinal == *selected_ordinal)
            .ok_or_else(|| format!("selected public deck ordinal {selected_ordinal} is missing"))?;
        let occurrence = deck
            .iter()
            .filter(|candidate| {
                candidate.card == card.card
                    && candidate.upgrades == card.upgrades
                    && candidate.ordinal < card.ordinal
            })
            .count();
        cards.push(format!(
            "{}#{occurrence}",
            canonical_upgraded_name_v1(card.card.stable_id(), Some(card.upgrades))
        ));
    }
    Ok(cards)
}

pub(crate) fn canonical_upgraded_name_v1(name: &str, upgrades: Option<u8>) -> String {
    match upgrades {
        None => name.to_string(),
        Some(0) => format!("{name}+0"),
        Some(1) => format!("{name}+"),
        Some(upgrades) => format!("{name}+{upgrades}"),
    }
}

fn resolve_run_card_selection_v1(
    decision: &TaskPublicDecisionV1,
    selector: &RunPlayActionSelectorV1,
    requested_cards: &[NamedOccurrenceV1],
) -> Result<TaskPublicChoiceV1, String> {
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("select:<card> requires a strategic run-card selection".to_string());
    };
    let selected_card_ordinals = requested_cards
        .iter()
        .map(|requested| {
            let preferred = preferred_upgrade_v1(
                requested,
                observation
                    .cards
                    .iter()
                    .filter(|card| card.card.matches_input_alias_v1(&requested.name))
                    .map(|card| card.upgrades),
                "select",
            )?;
            let matches = observation
                .cards
                .iter()
                .filter(|card| {
                    card.card.matches_input_alias_v1(&requested.name)
                        && preferred.is_none_or(|level| level == card.upgrades)
                })
                .map(|card| card.ordinal)
                .collect::<Vec<_>>();
            match (requested.occurrence, matches.as_slice()) {
                (Some(occurrence), _) => matches.get(occurrence).copied().ok_or_else(|| {
                    format!(
                        "select:{} occurrence #{occurrence} is unavailable; matched {} deck card(s)",
                        requested.name,
                        matches.len()
                    )
                }),
                (None, [ordinal]) => Ok(*ordinal),
                (None, []) => Err(format!(
                    "select:{} has no matching public deck card",
                    requested.name
                )),
                (None, _) => Err(format!(
                    "select:{} is ambiguous across deck ordinals {matches:?}; add #<occurrence>",
                    requested.name
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut unique_card_ordinals = std::collections::HashSet::new();
    if !selected_card_ordinals
        .iter()
        .all(|ordinal| unique_card_ordinals.insert(*ordinal))
    {
        return Err("select:<card> repeats the same public deck card".to_string());
    }

    let submit_matches = decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::SubmitRunSelection {
                        selected_card_ordinals: candidate_ordinals,
                        ..
                    },
            } => (candidate_ordinals == &selected_card_ordinals).then_some(ordinal),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !submit_matches.is_empty() {
        return select_unique_match(decision, selector, submit_matches);
    }

    let parameterized_matches = decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::BeginRunCardSelection {
                        min_choices,
                        max_choices,
                        selectable_card_ordinals,
                        ..
                    },
            } if (*min_choices..=*max_choices).contains(&selected_card_ordinals.len()) => {
                let selection_ordinals = selected_card_ordinals
                    .iter()
                    .map(|selected| {
                        selectable_card_ordinals
                            .iter()
                            .position(|candidate| candidate == selected)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let mut choice = Vec::with_capacity(selection_ordinals.len() + 1);
                choice.push(ordinal);
                choice.extend(selection_ordinals);
                Some(choice)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match parameterized_matches.as_slice() {
        [ordinal_path] => Ok(TaskPublicChoiceV1 {
            ordinal_path: ordinal_path.clone(),
        }),
        [] => Err(no_match_message(decision, &format!("{selector:?}"))),
        _ => Err(format!(
            "semantic action {selector:?} is ambiguous across {} run-card selection families",
            parameterized_matches.len()
        )),
    }
}

fn resolve_combat_card_selection_v1(
    decision: &TaskPublicDecisionV1,
    selector: &RunPlayActionSelectorV1,
    requested_cards: &[NamedOccurrenceV1],
) -> Result<TaskPublicChoiceV1, String> {
    let mut matching_paths = Vec::new();
    for (root_ordinal, candidate) in decision.candidates.iter().enumerate() {
        let TaskPublicCandidateV1::CombatSelectionFamily { family } = candidate else {
            continue;
        };
        let requested_count = requested_cards.len() as u64;
        if requested_count < family.declared_min || requested_count > family.effective_max {
            continue;
        }
        let mut selected = Vec::with_capacity(requested_cards.len());
        let mut family_matches = true;
        for requested in requested_cards {
            let preferred = preferred_upgrade_v1(
                requested,
                family.domain.iter().filter_map(|candidate| {
                    let (_, candidate_upgrades, available) =
                        combat_selection_identity_v1(candidate);
                    (available && combat_selection_matches_input_v1(candidate, &requested.name))
                        .then_some(candidate_upgrades)
                        .flatten()
                }),
                "select",
            )?;
            let matches = family
                .domain
                .iter()
                .enumerate()
                .filter_map(|(ordinal, candidate)| {
                    let (_, candidate_upgrades, available) =
                        combat_selection_identity_v1(candidate);
                    (available
                        && combat_selection_matches_input_v1(candidate, &requested.name)
                        && preferred.is_none_or(|expected| candidate_upgrades == Some(expected)))
                    .then_some(ordinal)
                })
                .collect::<Vec<_>>();
            let selection = match (requested.occurrence, matches.as_slice()) {
                (Some(occurrence), _) => matches.get(occurrence).copied(),
                (None, [ordinal]) => Some(*ordinal),
                _ => None,
            };
            let Some(selection) = selection else {
                family_matches = false;
                break;
            };
            if selected.contains(&selection) {
                family_matches = false;
                break;
            }
            selected.push(selection);
        }
        if !family_matches {
            continue;
        }
        let mut ordinal_path = Vec::with_capacity(selected.len() + 1);
        ordinal_path.push(root_ordinal);
        ordinal_path.extend(selected);
        let choice = TaskPublicChoiceV1 { ordinal_path };
        if decision.validate_choice(&choice).is_ok() {
            matching_paths.push(choice);
        }
    }
    match matching_paths.as_slice() {
        [choice] => Ok(choice.clone()),
        [] => Err(no_match_message(decision, &format!("{selector:?}"))),
        _ => Err(format!(
            "semantic action {selector:?} is ambiguous across {} combat selection families",
            matching_paths.len()
        )),
    }
}

fn resolve_play_card(
    decision: &TaskPublicDecisionV1,
    selector: &NamedOccurrenceV1,
    target_monster_slot: Option<u8>,
) -> Result<TaskPublicChoiceV1, String> {
    let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
        return Err(format!(
            "play:{:?} requires a combat decision",
            selector.name
        ));
    };
    let has_play = decision.candidates.iter().any(|candidate| {
        matches!(
            candidate,
            TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::PlayCard { .. }
            }
        )
    });
    if !has_play
        && decision.candidates.iter().any(|candidate| {
            matches!(
                candidate,
                TaskPublicCandidateV1::CombatIndexedChoice { .. }
                    | TaskPublicCandidateV1::CombatSelectionFamily { .. }
            )
        })
    {
        return Err(format!(
            "play:{} cannot run while a combat choice is pending; insert choose:<card> or select:<card> immediately after the action opening that choice. Both act and preview support choices within the same line. To inspect a private choice, preview only the prefix through the opening action, then preview the full line with the selected token inserted. For an already committed choice, use options",
            selector.name
        ));
    }
    let preferred = preferred_upgrade_v1(
        selector,
        observation
            .cards
            .hand
            .cards
            .iter()
            .filter(|card| card.card_id.matches_input_alias_v1(&selector.name))
            .map(|card| card.upgrades),
        "play",
    )?;
    let matching_hand_indices = observation
        .cards
        .hand
        .cards
        .iter()
        .enumerate()
        .filter_map(|(hand_index, card)| {
            (card.card_id.matches_input_alias_v1(&selector.name)
                && preferred.is_none_or(|level| level == card.upgrades))
            .then_some(hand_index)
        })
        .collect::<Vec<_>>();
    if matching_hand_indices.is_empty() {
        let same_name_in_hand = observation
            .cards
            .hand
            .cards
            .iter()
            .any(|card| card.card_id.matches_input_alias_v1(&selector.name));
        return Err(if same_name_in_hand {
            format!(
                "play:{} has no card with the requested upgrade in the hand at this boundary",
                selector.name
            )
        } else {
            format!("play:{} is not in the hand at this boundary. Earlier draws, exhausts or turn changes may have changed it", selector.name)
        });
    }
    let requested_hand_index = selector
        .occurrence
        .map(|occurrence| {
            matching_hand_indices
                .get(occurrence)
                .copied()
                .ok_or_else(|| {
                    let transactional_alias = format!(
                        "play:{}{}",
                        canonical_upgraded_name_v1(&selector.name, preferred),
                        target_monster_slot
                            .map_or_else(String::new, |slot| format!("@{slot}"))
                    );
                    format!(
                        "play:{} occurrence #{occurrence} is absent from the visible hand; matched {} hand card(s). Occurrences are relative to each current successor; for a transactional line of publicly equivalent copies, repeat {transactional_alias} without #<occurrence> so every step resolves against its own successor",
                        selector.name,
                        matching_hand_indices.len()
                    )
                })
        })
        .transpose()?;

    let mut matches = Vec::new();
    let mut untargeted_legal_match = false;
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        let TaskPublicCandidateV1::CombatAtomic {
            action:
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                },
        } = candidate
        else {
            continue;
        };
        let Some(card) = observation.cards.hand.cards.get(*hand_index) else {
            continue;
        };
        if requested_hand_index.is_some_and(|requested| requested != *hand_index)
            || !card.card_id.matches_input_alias_v1(&selector.name)
            || preferred.is_some_and(|level| level != card.upgrades)
        {
            continue;
        }
        let candidate_slot = target_monster_index
            .and_then(|index| observation.monsters.get(index))
            .map(|monster| monster.slot);
        if target_monster_slot.is_some() && target_monster_slot != candidate_slot {
            untargeted_legal_match |= candidate_slot.is_none();
            continue;
        }
        matches.push((ordinal, *hand_index, card, candidate_slot));
    }
    if matches.is_empty() && untargeted_legal_match {
        return Err(format!(
            "play:{} has a legal play without a monster target; omit @{} and use options for its exact current command",
            selector.name,
            target_monster_slot.expect("mismatched target was supplied")
        ));
    }
    if let Some(hand_index) = requested_hand_index {
        return match matches.as_slice() {
            [(ordinal, ..)] => ordinal_choice(*ordinal),
            [] => {
                let card = observation
                    .cards
                    .hand
                    .cards
                    .get(hand_index)
                    .expect("requested visible hand index must remain present");
                Err(format!(
                    "play:{}#{} selects hand[{hand_index}] {} with cost {}, but it has no legal play{} at energy {}; inspect the current candidates instead of renumbering another copy",
                    selector.name,
                    selector.occurrence.expect("requested occurrence"),
                    canonical_upgraded_name_v1(card.card_id.stable_id(), Some(card.upgrades)),
                    card.effective_cost,
                    target_monster_slot.map_or_else(String::new, |slot| format!(" at monster slot {slot}")),
                    observation.turn.energy
                ))
            }
            _ => Err(format_play_ambiguity_v1(
                decision,
                observation,
                selector,
                &matches,
            )),
        };
    }
    let Some((first_ordinal, _, first_card, first_slot)) = matches.first() else {
        return Err(no_match_message(
            decision,
            &format!("play:{}", selector.name),
        ));
    };
    if matches
        .iter()
        .skip(1)
        .any(|(_, _, card, slot)| *card != *first_card || slot != first_slot)
    {
        return Err(format_play_ambiguity_v1(
            decision,
            observation,
            selector,
            &matches,
        ));
    }
    ordinal_choice(*first_ordinal)
}

fn format_play_ambiguity_v1(
    decision: &TaskPublicDecisionV1,
    observation: &sts_oracle_learning::agent::information::state::PublicCombatStateV1,
    selector: &NamedOccurrenceV1,
    matches: &[(
        usize,
        usize,
        &sts_oracle_learning::agent::information::state::CombatLearningCardV1,
        Option<u8>,
    )],
) -> String {
    let candidates = matches
        .iter()
        .map(|(ordinal, hand, card, slot)| {
            let occurrence = observation
                .cards
                .hand
                .cards
                .iter()
                .take(*hand)
                .filter(|candidate| {
                    candidate.card_id == card.card_id && candidate.upgrades == card.upgrades
                })
                .count();
            format!(
                "play:{}#{occurrence}{} [candidate={ordinal} hand={hand} cost={}]",
                canonical_upgraded_name_v1(card.card_id.stable_id(), Some(card.upgrades)),
                slot.map_or_else(String::new, |slot| format!("@{slot}")),
                card.effective_cost
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "play:{} is ambiguous at {:?}; use one exact visible-hand action: {candidates}",
        selector.name, decision.site
    )
}

fn resolve_routine_advance_v1(
    decision: &TaskPublicDecisionV1,
) -> Result<TaskPublicChoiceV1, String> {
    let routine_matches = decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::Proceed,
            }
            | TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::Proceed { .. }
                    | TaskStrategicActionV1::ChooseRouteNode { .. }
                    | TaskStrategicActionV1::OpenPendingRewards
                    | TaskStrategicActionV1::LeaveShop,
            } => Some(ordinal),
            _ => None,
        })
        .collect::<Vec<_>>();
    let explicit_choices = decision
        .candidates
        .iter()
        .enumerate()
        .filter(|(ordinal, candidate)| {
            !routine_matches.contains(ordinal) && !advance_side_action_is_ignorable_v1(candidate)
        })
        .map(|(ordinal, _)| {
            semantic_action_for_choice_v1(
                decision,
                &TaskPublicChoiceV1 {
                    ordinal_path: vec![ordinal],
                },
            )
            .unwrap_or_else(|_| format!("candidate:{ordinal}"))
        })
        .collect::<Vec<_>>();
    if !explicit_choices.is_empty() {
        return Err(format!(
            "advance would bypass explicit choice(s) at {:?}: {}; resolve them explicitly before routine progression",
            decision.site,
            explicit_choices.join(", ")
        ));
    }
    match routine_matches.as_slice() {
        [ordinal] => ordinal_choice(*ordinal),
        [] => Err(format!(
            "advance has no uniquely routine progression action at {:?}; card, reward, event, chest, campfire, and purchase choices must stay explicit",
            decision.site
        )),
        _ => Err(format!(
            "advance is ambiguous at {:?}; routine progression candidate ordinals are {:?}, so choose an explicit action",
            decision.site, routine_matches
        )),
    }
}

fn advance_side_action_is_ignorable_v1(candidate: &TaskPublicCandidateV1) -> bool {
    matches!(
        candidate,
        TaskPublicCandidateV1::CombatAtomic {
            action: PublicCombatAtomicActionV1::DiscardPotion { .. },
        } | TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::DiscardRunPotion { .. },
        }
    )
}

fn resolve_use_potion(
    decision: &TaskPublicDecisionV1,
    selector: &NamedOccurrenceV1,
    target_monster_slot: Option<u8>,
) -> Result<TaskPublicChoiceV1, String> {
    if matches!(
        decision.observation,
        TaskPublicObservationV1::Strategic { .. }
    ) {
        if target_monster_slot.is_some() {
            return Err("a strategic run potion cannot have a combat target".to_string());
        }
        let matches = named_strategic_matches(decision, selector, |action| match action {
            TaskStrategicActionV1::UseRunPotion { potion, .. } => {
                Some((potion.matches_input_alias_v1(&selector.name), None))
            }
            _ => None,
        })?;
        return select_named_match(
            decision,
            &RunPlayActionSelectorV1::UsePotion {
                potion: selector.clone(),
                target_monster_slot,
            },
            selector,
            matches,
        );
    }
    let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
        return Err(format!(
            "use-potion:{:?} requires a combat decision",
            selector.name
        ));
    };
    let mut matches = Vec::new();
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        let TaskPublicCandidateV1::CombatAtomic {
            action:
                PublicCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                },
        } = candidate
        else {
            continue;
        };
        let Some(Some(potion)) = observation.potions.get(*potion_index) else {
            continue;
        };
        if !potion.potion_id.matches_input_alias_v1(&selector.name) {
            continue;
        }
        let candidate_slot = target_monster_index
            .and_then(|index| observation.monsters.get(index))
            .map(|monster| monster.slot);
        if target_monster_slot.is_some() && target_monster_slot != candidate_slot {
            continue;
        }
        matches.push(ordinal);
    }
    select_named_match(
        decision,
        &RunPlayActionSelectorV1::UsePotion {
            potion: selector.clone(),
            target_monster_slot,
        },
        selector,
        matches,
    )
}

fn resolve_discard_potion(
    decision: &TaskPublicDecisionV1,
    selector: &NamedOccurrenceV1,
) -> Result<TaskPublicChoiceV1, String> {
    if matches!(
        decision.observation,
        TaskPublicObservationV1::Strategic { .. }
    ) {
        let matches = named_strategic_matches(decision, selector, |action| match action {
            TaskStrategicActionV1::DiscardRunPotion { potion, .. } => {
                Some((potion.matches_input_alias_v1(&selector.name), None))
            }
            _ => None,
        })?;
        return select_named_match(
            decision,
            &RunPlayActionSelectorV1::DiscardPotion(selector.clone()),
            selector,
            matches,
        );
    }
    let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
        return Err(format!(
            "discard-potion:{:?} requires a combat decision",
            selector.name
        ));
    };
    let matches = decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::DiscardPotion { potion_index },
            } if observation
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .is_some_and(|potion| potion.potion_id.matches_input_alias_v1(&selector.name)) =>
            {
                Some(ordinal)
            }
            _ => None,
        })
        .collect();
    select_named_match(
        decision,
        &RunPlayActionSelectorV1::DiscardPotion(selector.clone()),
        selector,
        matches,
    )
}

fn atomic_matches(
    decision: &TaskPublicDecisionV1,
    matches: impl Fn(&PublicCombatAtomicActionV1) -> bool,
) -> Vec<usize> {
    decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::CombatAtomic { action } if matches(action) => Some(ordinal),
            _ => None,
        })
        .collect()
}

fn strategic_matches(
    decision: &TaskPublicDecisionV1,
    matches: impl Fn(&TaskStrategicActionV1) -> bool,
) -> Vec<usize> {
    decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::Strategic { action } if matches(action) => Some(ordinal),
            _ => None,
        })
        .collect()
}

fn named_strategic_matches(
    decision: &TaskPublicDecisionV1,
    selector: &NamedOccurrenceV1,
    describe: impl Fn(&TaskStrategicActionV1) -> Option<(bool, Option<u8>)>,
) -> Result<Vec<usize>, String> {
    let candidates = decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| {
            let TaskPublicCandidateV1::Strategic { action } = candidate else {
                return None;
            };
            let (identity_matches, upgrades) = describe(action)?;
            identity_matches.then_some((ordinal, upgrades))
        })
        .collect::<Vec<_>>();
    let preferred = preferred_upgrade_v1(
        selector,
        candidates.iter().filter_map(|(_, upgrades)| *upgrades),
        "named action",
    )?;
    Ok(candidates
        .into_iter()
        .filter_map(|(ordinal, candidate_upgrades)| {
            preferred
                .is_none_or(|expected| candidate_upgrades == Some(expected))
                .then_some(ordinal)
        })
        .collect())
}

fn preferred_upgrade_v1(
    selector: &NamedOccurrenceV1,
    upgrades: impl IntoIterator<Item = u8>,
    verb: &str,
) -> Result<Option<u8>, String> {
    if selector.upgrades.is_some() {
        return Ok(selector.upgrades);
    }
    let versions = upgrades
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    if versions.contains(&0) {
        return Ok(Some(0));
    }
    match versions.len() {
        0 => Ok(None),
        1 => Ok(versions.into_iter().next()),
        _ => Err(format!(
            "{verb}:{} has multiple upgraded levels but no unupgraded card; specify + or +<level>",
            selector.name
        )),
    }
}

fn select_named_match(
    decision: &TaskPublicDecisionV1,
    action: &RunPlayActionSelectorV1,
    named: &NamedOccurrenceV1,
    matches: Vec<usize>,
) -> Result<TaskPublicChoiceV1, String> {
    if let Some(occurrence) = named.occurrence {
        let Some(ordinal) = matches.get(occurrence) else {
            return Err(format!(
                "semantic action {action:?} occurrence #{occurrence} is unavailable at {:?}; matched {} candidate(s)",
                decision.site,
                matches.len()
            ));
        };
        return ordinal_choice(*ordinal);
    }
    select_unique_match(decision, action, matches)
}

fn select_occurrence_match(
    decision: &TaskPublicDecisionV1,
    action: &RunPlayActionSelectorV1,
    occurrence: Option<usize>,
    matches: Vec<usize>,
) -> Result<TaskPublicChoiceV1, String> {
    if let Some(occurrence) = occurrence {
        let Some(ordinal) = matches.get(occurrence) else {
            return Err(format!(
                "semantic action {action:?} occurrence #{occurrence} is unavailable at {:?}; matched {} candidate(s)",
                decision.site,
                matches.len()
            ));
        };
        return ordinal_choice(*ordinal);
    }
    select_unique_match(decision, action, matches)
}

fn reward_semantic_name_v1(reward: &PlannerRewardDescriptor) -> String {
    match reward {
        PlannerRewardDescriptor::Gold { .. } => "Gold".to_string(),
        PlannerRewardDescriptor::StolenGold { .. } => "StolenGold".to_string(),
        PlannerRewardDescriptor::CardReward { .. } => "CardReward".to_string(),
        PlannerRewardDescriptor::Relic { relic } => relic.stable_id().to_string(),
        PlannerRewardDescriptor::Potion { potion } => potion.stable_id().to_string(),
        PlannerRewardDescriptor::EmeraldKey => "EmeraldKey".to_string(),
        PlannerRewardDescriptor::SapphireKey => "SapphireKey".to_string(),
    }
}

fn reward_matches_input_alias_v1(reward: &PlannerRewardDescriptor, requested: &str) -> bool {
    match reward {
        PlannerRewardDescriptor::Relic { relic } => relic.matches_input_alias_v1(requested),
        PlannerRewardDescriptor::Potion { potion } => potion.matches_input_alias_v1(requested),
        other => name_matches(requested, &reward_semantic_name_v1(other)),
    }
}

fn select_unique_match(
    decision: &TaskPublicDecisionV1,
    selector: &RunPlayActionSelectorV1,
    matches: Vec<usize>,
) -> Result<TaskPublicChoiceV1, String> {
    match matches.as_slice() {
        [ordinal] => ordinal_choice(*ordinal),
        [] => Err(no_match_message(decision, &format!("{selector:?}"))),
        _ => Err(format!(
            "semantic action {selector:?} is ambiguous at {:?}; matching candidate ordinals are {:?}; add #<occurrence> or a target",
            decision.site, matches
        )),
    }
}

fn ordinal_choice(ordinal: usize) -> Result<TaskPublicChoiceV1, String> {
    Ok(TaskPublicChoiceV1 {
        ordinal_path: vec![ordinal],
    })
}

fn no_match_message(decision: &TaskPublicDecisionV1, selector: &str) -> String {
    let routes = decision
        .candidates
        .iter()
        .filter_map(|candidate| match candidate {
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
            } => Some(format!("route:{x}.{y}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    let hint = match routes.as_slice() {
        [route] => format!("; this Map has one route, so use advance or {route}"),
        [] => String::new(),
        _ => format!("; choose one explicit route from [{}]", routes.join(", ")),
    };
    format!(
        "semantic action {selector} has no legal match at {:?} ({} candidate(s)){hint}",
        decision.site,
        decision.candidates.len()
    )
}

fn parse_ordinal_path(
    payload: &str,
    command: &str,
    locale: sts_presentation::Locale,
) -> Result<Vec<usize>, String> {
    let ordinals = payload
        .split('.')
        .map(|value| parse_number(value, "ordinal", command, locale))
        .collect::<Result<Vec<_>, _>>()?;
    if ordinals.is_empty() {
        return Err(syntax_message(
            locale,
            "ordinalEmpty",
            &[("command", &(command.to_owned()))],
        ));
    }
    Ok(ordinals)
}

fn parse_optional_target<'a>(
    payload: &'a str,
    command: &str,
    locale: sts_presentation::Locale,
) -> Result<(&'a str, Option<u8>), String> {
    let Some((name, target)) = payload.rsplit_once('@') else {
        return Ok((payload, None));
    };
    Ok((
        name,
        Some(parse_number(target, "monsterSlot", command, locale)?),
    ))
}

fn parse_named_occurrence(
    payload: &str,
    command: &str,
    locale: sts_presentation::Locale,
) -> Result<NamedOccurrenceV1, String> {
    let (name_and_upgrade, occurrence) = match payload.rsplit_once('#') {
        Some((name, occurrence)) => (
            name,
            Some(parse_number(occurrence, "occurrence", command, locale)?),
        ),
        None => (payload, None),
    };
    let (name, upgrades) = parse_card_upgrade(name_and_upgrade, command, locale)?;
    Ok(NamedOccurrenceV1 {
        name: name.to_string(),
        upgrades,
        occurrence,
    })
}

pub(crate) fn parse_card_upgrade<'a>(
    payload: &'a str,
    command: &str,
    locale: sts_presentation::Locale,
) -> Result<(&'a str, Option<u8>), String> {
    if payload.contains('\u{ff0b}') {
        return Err(syntax_message(
            locale,
            "fullwidthPlus",
            &[("command", &(command.to_owned()))],
        ));
    }
    let (name, upgrades) = if let Some(name) = payload.strip_suffix('+') {
        (name, Some(1))
    } else if let Some((name, upgrades)) = payload.rsplit_once('+') {
        let upgrades = parse_number::<u8>(upgrades, "upgradeCount", command, locale)?;
        (name, Some(upgrades))
    } else {
        (payload, None)
    };
    if name.is_empty() {
        return Err(syntax_message(
            locale,
            "emptyName",
            &[("command", &(command.to_owned()))],
        ));
    }
    if name.contains('+') {
        return Err(syntax_message(
            locale,
            "upgradeSuffix",
            &[("command", &(command.to_owned()))],
        ));
    }
    Ok((name, upgrades))
}

fn parse_number<T>(
    value: &str,
    label: &str,
    command: &str,
    locale: sts_presentation::Locale,
) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|error| {
        syntax_message(
            locale,
            "number",
            &[
                (
                    "label",
                    &(syntax_message(locale, &format!("field.{label}"), &[])),
                ),
                ("value", &(format!("{value:?}"))),
                ("command", &(command.to_owned())),
                ("error", &(error.to_string())),
            ],
        )
    })
}

fn name_matches(requested: &str, actual: &str) -> bool {
    normalized_name(requested) == normalized_name(actual)
}

fn normalized_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, '-' | '_' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_languages_preserve_commands_and_typed_action_identity() {
        use sts_presentation::Locale;
        for command in [
            "advance",
            "end",
            "play:Strike+0#0@0",
            "use-potion:FirePotion#0@1",
            "discard-potion:BlockPotion",
            "choose:FlurryOfBlows#1",
            "route:2.2",
            "route:2,2",
            "route:2 2",
            "open-card#1",
            "singing-bowl#0",
            "claim:Gold#0",
            "take:ShrugItOff",
            "skip-card#0",
            "buy:Anchor#0",
            "remove:Strike#0",
            "smith:Bash#0",
            "toke:Strike",
            "select:Strike+0#0,Defend+0#0",
            "select:none",
            "boss-relic:RunicPyramid#0",
            "event:Neow:0",
            "play:Spot Weakness@0",
        ] {
            assert_eq!(
                parse_action_selector_v1(command, "test", Locale::En),
                parse_action_selector_v1(command, "test", Locale::ZhCn),
                "{command}"
            );
        }
        let line = "play:Strike+0#0@0/select:Defend+0#0/end";
        assert_eq!(
            parse_action_line_v1(line, "test", Locale::En),
            parse_action_line_v1(line, "test", Locale::ZhCn)
        );
        for command in [
            "",
            "42",
            "bad:thing",
            "route:1",
            "route:x.2",
            "route:1.x",
            "event:Neow",
            "event::0",
            "event:Neow:x",
            "play:Strike@bad",
            "open-card#bad",
            "ordinal:bad",
            "play:Strike+bad",
            "play:+",
            "play:Strike++",
            "play:Strike＋",
        ] {
            let english = parse_action_selector_v1(command, "test", Locale::En).unwrap_err();
            let chinese = parse_action_selector_v1(command, "test", Locale::ZhCn).unwrap_err();
            assert_ne!(english, chinese, "{command}");
            assert!(
                chinese
                    .chars()
                    .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character)),
                "{chinese}"
            );
        }
        for locale in [Locale::En, Locale::ZhCn] {
            assert!(parse_action_line_v1("", "act", locale).is_err());
            // Raw parameters are inserted once; template-like command input remains literal.
            assert!(parse_action_selector_v1("{{raw_input}}", "act", locale)
                .unwrap_err()
                .contains("{{raw_input}}"));
        }
    }
    use sts_oracle_learning::agent::task::{
        TaskDecisionSiteV1, TaskStrategicCardV1, TaskStrategicObservationV1,
    };
    use sts_oracle_learning::ai::planner_core::{
        PlannerDecisionContext, PlannerDecisionSite, PlannerMechanicsManifest, PlannerPlayerClass,
        PlannerPublicHistory, PlannerPublicMap, PlannerRunGoal, PlannerRunScalars,
    };
    use sts_oracle_learning::content::cards::CardId;
    use sts_oracle_learning::state::selection::{SelectionReason, SelectionScope};

    fn run_selection_card(ordinal: usize, card: CardId, upgrades: u8) -> TaskStrategicCardV1 {
        TaskStrategicCardV1 {
            ordinal,
            card,
            upgrades,
            misc_value: 0,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
            bottled_by: None,
        }
    }

    fn run_selection_decision(candidates: Vec<TaskPublicCandidateV1>) -> TaskPublicDecisionV1 {
        TaskPublicDecisionV1::new(
            TaskDecisionSiteV1::RunChoice,
            TaskPublicObservationV1::Strategic {
                observation: TaskStrategicObservationV1 {
                    mechanics: PlannerMechanicsManifest {
                        mechanics_id: "ordered-run-selection-test".to_string(),
                        mechanics_version: 1,
                    },
                    run_goal: PlannerRunGoal::ActThreeVictory,
                    decision_site: PlannerDecisionSite::RunChoice,
                    run: PlannerRunScalars {
                        player_class: PlannerPlayerClass::Ironclad,
                        ascension_level: 20,
                        act: 2,
                        floor: 34,
                        current_hp: 23,
                        max_hp: 78,
                        gold: 316,
                        keys: [false; 3],
                        potion_capacity: 2,
                        pending_act_entry: None,
                    },
                    cards: vec![
                        run_selection_card(0, CardId::Strike, 1),
                        run_selection_card(1, CardId::Strike, 1),
                        run_selection_card(7, CardId::Defend, 1),
                        run_selection_card(19, CardId::Shame, 0),
                    ],
                    relics: Vec::new(),
                    potions: Vec::new(),
                    public_map: PlannerPublicMap {
                        current_x: 0,
                        current_y: 15,
                        boss: None,
                        nodes: Vec::new(),
                    },
                    context: PlannerDecisionContext::RunChoice {
                        scope: SelectionScope::Deck,
                        reason: SelectionReason::Transform,
                        source: None,
                        min_choices: 1,
                        max_choices: 1,
                    },
                    public_history: PlannerPublicHistory {
                        shop_purge_count: 0,
                        recent_combat_hp_loss: None,
                        recent_combat_turns: None,
                        recent_combat_potions_used: None,
                    },
                },
            },
            candidates,
        )
        .expect("run selection decision")
    }

    #[test]
    fn pending_potion_choices_explain_why_hand_cards_cannot_be_played() {
        use sts_oracle_learning::agent::information::{
            action::project_public_combat_actions_v1,
            run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1},
            state::public_combat_state_v1,
        };
        use sts_oracle_learning::agent::task::task_public_combat_decision_v1;
        use sts_oracle_learning::content::potions::{Potion, PotionId};
        use sts_oracle_learning::sim::combat::{
            CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
        };
        use sts_oracle_learning::state::core::ClientInput;

        let decision = |position: &CombatPosition| {
            let actions =
                project_public_combat_actions_v1(&position.engine, &position.combat).unwrap();
            task_public_combat_decision_v1(
                PublicCombatRunContextV1::Unavailable {
                    reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
                },
                public_combat_state_v1(&position.combat),
                &actions.public,
            )
            .unwrap()
        };
        let selector = NamedOccurrenceV1 {
            name: "Defend".into(),
            upgrades: None,
            occurrence: Some(0),
        };
        for potion in [
            PotionId::ColorlessPotion,
            PotionId::PowerPotion,
            PotionId::GamblersBrew,
        ] {
            let active = crate::decision_tests::fixture("combat_multiturn")
                .active_combat
                .unwrap();
            let mut position = CombatPosition::new(active.engine_state, active.combat_state);
            position.combat.entities.potions[0] = Some(Potion::new(potion, 0));
            assert!(resolve_play_card(&decision(&position), &selector, None).is_ok());
            let limits = CombatStepLimits {
                max_engine_steps: 250,
                deadline: None,
            };
            let pending = EngineCombatStepper.apply_to_stable(
                &position,
                ClientInput::UsePotion {
                    potion_index: 0,
                    target: None,
                },
                limits,
            );
            assert!(!pending.truncated);
            let error =
                resolve_play_card(&decision(&pending.position), &selector, None).unwrap_err();
            assert!(
                error.contains("combat choice is pending"),
                "{potion:?}: {error}"
            );
            assert!(error.contains("use options"), "{error}");
            assert!(
                error.contains("Both act and preview support choices"),
                "{error}"
            );
            assert!(error.contains("preview only the prefix"), "{error}");
            assert!(!error.contains("at energy"), "{error}");
            let choice = EngineCombatStepper.atomic_actions(&pending.position)[0].clone();
            let resolved = EngineCombatStepper.apply_to_stable(&pending.position, choice, limits);
            assert!(!resolved.truncated);
            assert!(resolve_play_card(&decision(&resolved.position), &selector, None).is_ok());
        }
    }

    #[test]
    fn parses_semantic_combat_line() {
        assert_eq!(
            parse_action_line_v1(
                "play:Defend/play:Whirlwind/end",
                "compare",
                sts_presentation::Locale::En
            )
            .unwrap(),
            vec![
                RunPlayActionSelectorV1::PlayCard {
                    card: NamedOccurrenceV1 {
                        name: "Defend".to_string(),
                        upgrades: None,
                        occurrence: None,
                    },
                    target_monster_slot: None,
                },
                RunPlayActionSelectorV1::PlayCard {
                    card: NamedOccurrenceV1 {
                        name: "Whirlwind".to_string(),
                        upgrades: None,
                        occurrence: None,
                    },
                    target_monster_slot: None,
                },
                RunPlayActionSelectorV1::EndTurn,
            ]
        );
    }

    #[test]
    fn bare_ordinals_are_not_semantic_actions() {
        let error =
            parse_action_line_v1("5/7/0", "compare", sts_presentation::Locale::En).unwrap_err();
        assert!(error.contains("unknown semantic action"));
        assert!(error.contains("copy a semantic action from current options"));
        assert!(!error.contains("explicit ordinal"));
        assert_eq!(
            parse_action_line_v1(
                "ordinal:5/ordinal:7/ordinal:0",
                "compare",
                sts_presentation::Locale::En
            )
            .unwrap(),
            vec![
                RunPlayActionSelectorV1::OrdinalPath(vec![5]),
                RunPlayActionSelectorV1::OrdinalPath(vec![7]),
                RunPlayActionSelectorV1::OrdinalPath(vec![0]),
            ]
        );
    }

    #[test]
    fn parses_occurrence_qualified_skip_and_multi_upgrade_card() {
        assert_eq!(
            parse_action_line_v1(
                "skip-card#2/take:SearingBlow+5#1",
                "act",
                sts_presentation::Locale::En
            )
            .unwrap(),
            vec![
                RunPlayActionSelectorV1::SkipCardReward {
                    occurrence: Some(2),
                },
                RunPlayActionSelectorV1::TakeCard(NamedOccurrenceV1 {
                    name: "SearingBlow".to_string(),
                    upgrades: Some(5),
                    occurrence: Some(1),
                }),
            ]
        );
    }

    #[test]
    fn explicit_zero_upgrade_preserves_generated_card_identity() {
        assert_eq!(
            parse_action_selector_v1(
                "select:DoubleTap+0#0,DoubleTap+#0",
                "generated",
                sts_presentation::Locale::En
            )
            .unwrap(),
            RunPlayActionSelectorV1::SelectCards(vec![
                NamedOccurrenceV1 {
                    name: "DoubleTap".to_string(),
                    upgrades: Some(0),
                    occurrence: Some(0),
                },
                NamedOccurrenceV1 {
                    name: "DoubleTap".to_string(),
                    upgrades: Some(1),
                    occurrence: Some(0),
                },
            ])
        );
        assert_eq!(canonical_upgraded_name_v1("DoubleTap", None), "DoubleTap");
        assert_eq!(
            canonical_upgraded_name_v1("DoubleTap", Some(0)),
            "DoubleTap+0"
        );
        assert_eq!(
            canonical_upgraded_name_v1("DoubleTap", Some(1)),
            "DoubleTap+"
        );
    }

    #[test]
    fn numeric_one_upgrade_matches_the_canonical_suffix_and_rejects_repeated_plus() {
        for verb in ["play", "select", "take"] {
            let one = parse_action_selector_v1(
                &format!("{verb}:Strike+1#0"),
                "test",
                sts_presentation::Locale::En,
            )
            .unwrap();
            let canonical = parse_action_selector_v1(
                &format!("{verb}:Strike+#0"),
                "test",
                sts_presentation::Locale::En,
            )
            .unwrap();
            assert_eq!(one, canonical);
            assert!(parse_action_selector_v1(
                &format!("{verb}:Strike++#0"),
                "test",
                sts_presentation::Locale::En
            )
            .unwrap_err()
            .contains("invalid upgrade suffix"));
            let fullwidth = parse_action_selector_v1(
                &format!("{verb}:Strike\u{ff0b}#0"),
                "test",
                sts_presentation::Locale::En,
            )
            .unwrap_err();
            assert!(fullwidth.contains("use ASCII '+' (U+002B)"), "{fullwidth}");
        }
    }

    #[test]
    fn parameterized_run_card_selection_round_trips_in_requested_order() {
        let decision = run_selection_decision(vec![TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::BeginRunCardSelection {
                scope: SelectionScope::Deck,
                reason: SelectionReason::TransformUpgraded,
                min_choices: 3,
                max_choices: 3,
                selectable_card_ordinals: vec![0, 1, 7, 19],
            },
        }]);

        let owner_choice = TaskPublicChoiceV1 {
            ordinal_path: vec![0, 3, 0, 1],
        };
        let family = crate::run_selection::project_run_selection_family_v1(
            &decision,
            &decision.candidates[0],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            family.card_tokens,
            ["Strike+#0", "Strike+#1", "Defend+#0", "Shame+0#0"]
        );
        assert_eq!(
            semantic_action_for_choice_v1(&decision, &owner_choice).unwrap(),
            "select:Shame+0#0,Strike+#0,Strike+#1"
        );

        let manual = parse_action_selector_v1(
            "select:Strike+#0,Strike+#1,Defend+#0",
            "ordered-run-selection-test",
            sts_presentation::Locale::En,
        )
        .unwrap();
        assert_eq!(
            resolve_action_selector_v1(&decision, &manual).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0, 0, 1, 2],
            }
        );
    }

    #[test]
    fn bare_run_selection_prefers_base_cards_and_uses_upgraded_when_base_absent() {
        let mut decision = run_selection_decision(vec![TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::BeginRunCardSelection {
                scope: SelectionScope::Deck,
                reason: SelectionReason::TransformUpgraded,
                min_choices: 2,
                max_choices: 2,
                selectable_card_ordinals: vec![0, 1, 7, 19],
            },
        }]);
        let TaskPublicObservationV1::Strategic { observation } = &mut decision.observation else {
            unreachable!()
        };
        observation.cards[0].upgrades = 0;

        let bare_base = parse_action_selector_v1(
            "select:Strike#0,Strike+#0",
            "test",
            sts_presentation::Locale::En,
        )
        .unwrap();
        assert_eq!(
            resolve_action_selector_v1(&decision, &bare_base).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0, 0, 1]
            }
        );
        let missing_base_copy = parse_action_selector_v1(
            "select:Strike#1,Strike+#0",
            "test",
            sts_presentation::Locale::En,
        )
        .unwrap();
        assert!(resolve_action_selector_v1(&decision, &missing_base_copy)
            .unwrap_err()
            .contains("matched 1 deck card"));

        let exact = parse_action_selector_v1(
            "select:Strike+0#0,Strike+#0",
            "test",
            sts_presentation::Locale::En,
        )
        .unwrap();
        assert_eq!(
            resolve_action_selector_v1(&decision, &exact).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0, 0, 1]
            }
        );

        let TaskPublicObservationV1::Strategic { observation } = &mut decision.observation else {
            unreachable!()
        };
        observation.cards[0].upgrades = 1;
        let only_upgraded = parse_action_selector_v1(
            "select:Strike#0,Strike#1",
            "test",
            sts_presentation::Locale::En,
        )
        .unwrap();
        assert_eq!(
            resolve_action_selector_v1(&decision, &only_upgraded).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0, 0, 1]
            }
        );
    }

    #[test]
    fn bare_remove_prefers_base_and_falls_back_to_only_upgraded_copy() {
        let mut decision = run_selection_decision(vec![TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::BeginRunCardSelection {
                scope: SelectionScope::Deck,
                reason: SelectionReason::TransformUpgraded,
                min_choices: 1,
                max_choices: 1,
                selectable_card_ordinals: vec![0],
            },
        }]);
        decision.candidates = vec![
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::RemoveCard {
                    card_ordinal: 0,
                    card: CardId::Strike,
                    upgrades: 0,
                    price: 75,
                },
            },
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::RemoveCard {
                    card_ordinal: 1,
                    card: CardId::Strike,
                    upgrades: 1,
                    price: 75,
                },
            },
        ];
        let bare_base =
            parse_action_selector_v1("remove:Strike#0", "test", sts_presentation::Locale::En)
                .unwrap();
        assert_eq!(
            resolve_action_selector_v1(&decision, &bare_base).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0]
            }
        );
        let missing_base_copy =
            parse_action_selector_v1("remove:Strike#1", "test", sts_presentation::Locale::En)
                .unwrap();
        assert!(resolve_action_selector_v1(&decision, &missing_base_copy)
            .unwrap_err()
            .contains("matched 1 candidate"));
        let exact =
            parse_action_selector_v1("remove:Strike+#0", "test", sts_presentation::Locale::En)
                .unwrap();
        assert_eq!(
            resolve_action_selector_v1(&decision, &exact).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![1]
            }
        );

        decision.candidates.remove(0);
        assert_eq!(
            resolve_action_selector_v1(&decision, &bare_base).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0]
            }
        );
    }

    #[test]
    fn bare_name_with_multiple_nonzero_upgrade_levels_stays_explicit() {
        let selector = NamedOccurrenceV1 {
            name: "SearingBlow".to_string(),
            upgrades: None,
            occurrence: Some(0),
        };
        let error = preferred_upgrade_v1(&selector, [1, 2], "select").unwrap_err();
        assert!(error.contains("multiple upgraded levels"), "{error}");
        assert_eq!(
            preferred_upgrade_v1(&selector, [0, 1, 2], "select").unwrap(),
            Some(0)
        );
    }

    #[test]
    fn submitted_run_card_selection_matches_order_not_only_membership() {
        let decision = run_selection_decision(vec![
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::SubmitRunSelection {
                    scope: SelectionScope::Deck,
                    selected_card_ordinals: vec![19, 0, 1],
                },
            },
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::SubmitRunSelection {
                    scope: SelectionScope::Deck,
                    selected_card_ordinals: vec![0, 1, 19],
                },
            },
        ]);
        let selector = parse_action_selector_v1(
            "select:Shame+0#0,Strike+#0,Strike+#1",
            "ordered-run-selection-test",
            sts_presentation::Locale::En,
        )
        .unwrap();

        assert_eq!(
            resolve_action_selector_v1(&decision, &selector).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0],
            }
        );
    }

    #[test]
    fn parses_reward_and_run_progression_actions_without_ordinals() {
        assert_eq!(
            parse_action_line_v1(
                "claim:gold/open-card#1/take:PommelStrike+/singing-bowl/proceed",
                "anchor-line",
                sts_presentation::Locale::En
            )
            .unwrap(),
            vec![
                RunPlayActionSelectorV1::ClaimReward(NamedOccurrenceV1 {
                    name: "gold".to_string(),
                    upgrades: None,
                    occurrence: None,
                }),
                RunPlayActionSelectorV1::OpenCardReward {
                    occurrence: Some(1),
                },
                RunPlayActionSelectorV1::TakeCard(NamedOccurrenceV1 {
                    name: "PommelStrike".to_string(),
                    upgrades: Some(1),
                    occurrence: None,
                }),
                RunPlayActionSelectorV1::SingingBowl { occurrence: None },
                RunPlayActionSelectorV1::Proceed,
            ]
        );
        assert_eq!(
            parse_action_line_v1(
                "dig/lift/toke:Strike#2/recall/select:Hemokinesis,Strike#1/open-rewards/boss-relic:BlackBlood/skip-boss-relic/open-chest",
                "anchor-line",
             sts_presentation::Locale::En)
            .unwrap(),
            vec![
                RunPlayActionSelectorV1::Dig,
                RunPlayActionSelectorV1::Lift,
                RunPlayActionSelectorV1::Toke(NamedOccurrenceV1 {
                    name: "Strike".to_string(),
                    upgrades: None,
                    occurrence: Some(2),
                }),
                RunPlayActionSelectorV1::Recall,
                RunPlayActionSelectorV1::SelectCards(vec![
                    NamedOccurrenceV1 {
                        name: "Hemokinesis".to_string(),
                        upgrades: None,
                        occurrence: None,
                    },
                    NamedOccurrenceV1 {
                        name: "Strike".to_string(),
                        upgrades: None,
                        occurrence: Some(1),
                    },
                ]),
                RunPlayActionSelectorV1::OpenPendingRewards,
                RunPlayActionSelectorV1::TakeBossRelic(NamedOccurrenceV1 {
                    name: "BlackBlood".to_string(),
                    upgrades: None,
                    occurrence: None,
                }),
                RunPlayActionSelectorV1::SkipBossRelic,
                RunPlayActionSelectorV1::OpenChest,
            ]
        );
    }

    #[test]
    fn advance_refuses_to_bypass_an_explicit_choice() {
        let proceed = TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::RunChoice,
            },
        };
        let routine_only = run_selection_decision(vec![proceed.clone()]);
        assert_eq!(
            resolve_action_selector_v1(&routine_only, &RunPlayActionSelectorV1::Advance).unwrap(),
            TaskPublicChoiceV1 {
                ordinal_path: vec![0],
            }
        );

        let with_card = run_selection_decision(vec![
            proceed,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index: 0,
                    card: CardId::PommelStrike,
                    upgrades: 0,
                },
            },
        ]);
        let error = resolve_action_selector_v1(&with_card, &RunPlayActionSelectorV1::Advance)
            .expect_err("advance must not skip a visible card choice");
        assert!(error.contains("advance would bypass explicit choice"));
        assert!(error.contains("take:PommelStrike+0#0"));
    }
}
