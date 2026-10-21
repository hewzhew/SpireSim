use crate::transport::{JavaRuntimeActionV1, JavaRuntimeObservationV1};
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::{canonical_game_id, natural_combat_grid_kind_v1};
use sts_parity_lockstep::{
    canonical_selection_indices_v1, natural_campfire_option_v1, natural_card_reward_choice_v1,
    natural_deck_card_choice_v1, natural_event_option_v1, natural_hand_card_choice_v1,
    natural_map_node_id_v1, natural_reward_choice_v1, DecisionSurfaceV1, NaturalRewardCardV1,
    NaturalRunRewardItemV1, NaturalRunRewardsV2, SemanticActionV1, SemanticChoiceV1,
    SemanticScalarV1,
};

#[derive(Clone, Debug)]
pub(crate) struct ResolvedJavaActionV1 {
    pub semantic: SemanticActionV1,
    pub runtime_handle: String,
    pub execution: JavaActionExecutionV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum JavaActionExecutionV1 {
    Act,
    PurgeShopCard { selected_card_uuid: String },
    SubmitHandSelect { selected_card_uuids: Vec<String> },
    SubmitGridSelect { selected_card_uuids: Vec<String> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JavaHandSelectProfileV1 {
    SingleExhaust,
    CardDiscard { cards: usize },
    ToolsOfTheTrade { cards: usize },
    Retain { max_cards: usize },
    OptionalExhaust { max_cards: usize },
    SinglePutOnDrawPile,
    SingleUpgrade,
    SingleCopy,
    GamblingChip,
    PotionExhaust,
}

pub(crate) fn resolved_actions_v1(
    observation: &JavaRuntimeObservationV1,
    runtime_card_bindings: Option<&BTreeMap<String, String>>,
    hand_select_profile: Option<JavaHandSelectProfileV1>,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let decision = &observation.frame["public_base"]["decision"];
    match observation.frame["boundary"]["kind"].as_str() {
        Some("terminal") if decision["kind"].as_str() == Some("heart_victory") => {
            let mut lifecycle_actions = BTreeSet::new();
            for action in &observation.actions {
                if action.kind != "run_discard_to_main_menu" {
                    return Err("Java heart terminal exposes a non-discard action".to_owned());
                }
                validate_oracle_lifecycle_action(action, &mut lifecycle_actions)?;
            }
            Ok(Vec::new())
        }
        Some("combat_player_decision") => combat_actions(
            observation,
            runtime_card_bindings.ok_or_else(|| {
                "Java combat action space has no complete source-proven card bindings".to_owned()
            })?,
        ),
        Some("run_decision") => match decision["kind"].as_str() {
            Some("event") if decision["event_id"].as_str() == Some("Neow") => {
                neow_actions(observation)
            }
            Some("event") => event_actions(observation),
            Some("grid_select") => grid_select_actions(
                observation,
                runtime_card_bindings.ok_or_else(|| {
                    "Java grid selection has no complete source-proven card bindings".to_owned()
                })?,
            ),
            Some("hand_select") => hand_select_actions(
                observation,
                runtime_card_bindings.ok_or_else(|| {
                    "Java hand selection has no complete source-proven card bindings".to_owned()
                })?,
                hand_select_profile.ok_or_else(|| {
                    "Java hand selection has no source-proven predecessor profile".to_owned()
                })?,
            ),
            Some("map") => map_actions(observation),
            Some("shop" | "shop_entry" | "shop_purge_card" | "shop_purge_confirm") => {
                crate::shop::actions(
                    observation,
                    runtime_card_bindings.ok_or("Java shop has no source-proven master deck")?,
                )
            }
            Some("combat_reward") => reward_actions(observation),
            Some("double_boss_complete") => boss_completion_actions(
                observation,
                DecisionSurfaceV1::DoubleBoss,
                "proceed_double_boss",
            ),
            Some("act3_victory_complete") => boss_completion_actions(
                observation,
                DecisionSurfaceV1::Act3Victory,
                "proceed_act3_victory",
            ),
            Some("true_victory_complete") => boss_completion_actions(
                observation,
                DecisionSurfaceV1::TrueVictory,
                "proceed_true_victory",
            ),
            Some("card_reward") => card_reward_actions(observation),
            Some("treasure") => treasure_actions(observation),
            Some("boss_relic" | "boss_treasure" | "boss_treasure_complete") => {
                boss_relic_actions(observation)
            }
            Some("campfire") => campfire_actions(observation),
            Some("smith_card") => smith_card_actions(
                observation,
                runtime_card_bindings.ok_or_else(|| {
                    "Java campfire Smith selection has no complete source-proven card bindings"
                        .to_owned()
                })?,
                false,
            ),
            Some("smith_confirm") => smith_card_actions(
                observation,
                runtime_card_bindings.ok_or_else(|| {
                    "Java campfire Smith confirmation has no complete source-proven card bindings"
                        .to_owned()
                })?,
                true,
            ),
            Some("campfire_complete") => campfire_complete_actions(observation),
            Some(kind) => Err(format!(
                "Java lockstep v1 does not project decision kind {kind:?}"
            )),
            None => Err("Java lockstep frame omitted its decision kind".to_owned()),
        },
        Some(kind) => Err(format!(
            "Java lockstep v1 does not project boundary {kind:?}"
        )),
        None => Err("Java lockstep frame omitted its boundary kind".to_owned()),
    }
}

fn boss_completion_actions(
    observation: &JavaRuntimeObservationV1,
    surface: DecisionSurfaceV1,
    expected_action: &str,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let (actions, mut resolved) = run_decision_actions(observation)?;
    let [action] = actions.as_slice() else {
        return Err("Java boss completion must expose one Proceed action".to_owned());
    };
    require_parameter_free_primary_action(action)?;
    if action.kind != expected_action || action.action_id != expected_action {
        return Err("Java boss continuation identity disagrees".to_owned());
    }
    resolved.push(ResolvedJavaActionV1 {
        semantic: SemanticActionV1::Proceed { surface },
        runtime_handle: action.runtime_handle.clone(),
        execution: JavaActionExecutionV1::Act,
    });
    Ok(resolved)
}

pub(crate) fn boss_relic_selection(
    observation: &JavaRuntimeObservationV1,
) -> Result<sts_parity_lockstep::NaturalBossRelicChoiceV1, String> {
    let (actions, _) = run_decision_actions(observation)?;
    let mut relics = BTreeMap::new();
    let mut can_skip = false;
    for action in actions {
        require_parameter_free(action)?;
        require_no_potion_identity(action)?;
        require_primary_decision_identity(action)?;
        if action.reward_type.as_deref() != Some("BOSS_RELIC") || action.reward_gold.is_some() {
            return Err("boss relic action carries another reward identity".to_owned());
        }
        match action.kind.as_str() {
            "choose_boss_relic" => {
                let index = action
                    .reward_index
                    .ok_or("boss relic action omitted offer index")?;
                let id = action
                    .reward_id
                    .as_deref()
                    .ok_or("boss relic action omitted relic ID")?;
                if action.reward_type.as_deref() != Some("BOSS_RELIC")
                    || action.action_id != format!("boss_relic:{index}:{id}")
                    || relics.insert(index, canonical_game_id(id)).is_some()
                {
                    return Err("boss relic action disagrees with its typed offer".to_owned());
                }
            }
            "skip_boss_relic"
                if action.action_id == "skip_boss_relic"
                    && !can_skip
                    && action.reward_index.is_none()
                    && action.reward_id.is_none() =>
            {
                can_skip = true
            }
            _ => return Err("boss relic boundary exposed an unrelated action".to_owned()),
        }
    }
    if relics.len() != 3 || relics.keys().copied().ne(0..3) {
        return Err("boss relic boundary requires three ordered offers".to_owned());
    }
    Ok(sts_parity_lockstep::NaturalBossRelicChoiceV1 {
        relics: relics.into_values().collect(),
        can_skip,
    })
}

fn boss_relic_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let decision = observation.frame["public_base"]["decision"]["kind"].as_str();
    let (actions, mut resolved) = run_decision_actions(observation)?;
    if decision != Some("boss_relic") {
        let expected = if decision == Some("boss_treasure") {
            "open_boss_chest"
        } else {
            "proceed_boss_treasure"
        };
        let [action] = actions.as_slice() else {
            return Err("boss chest must expose one closed continuation".to_owned());
        };
        require_parameter_free_primary_action(action)?;
        if action.kind != expected || action.action_id != expected {
            return Err("boss chest continuation identity disagrees".to_owned());
        }
        resolved.push(ResolvedJavaActionV1 {
            semantic: SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::BossRelic,
            },
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
        return Ok(resolved);
    }
    let selection = boss_relic_selection(observation)?;
    let mut occurrences = BTreeMap::new();
    let choices = selection
        .relics
        .iter()
        .map(|id| {
            let occurrence = occurrences.entry(id.clone()).or_insert(0);
            let choice = sts_parity_lockstep::natural_boss_relic_choice_v1(id, *occurrence);
            *occurrence += 1;
            choice
        })
        .collect::<Vec<_>>();
    for action in actions {
        let semantic = if action.kind == "choose_boss_relic" {
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::BossRelic,
                choice: choices[action.reward_index.unwrap()].clone(),
            }
        } else {
            SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::BossRelic,
            }
        };
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    Ok(resolved)
}

fn treasure_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let (actions, mut resolved) = run_decision_actions(observation)?;
    let mut saw_open = false;
    let mut saw_proceed = false;
    for action in actions {
        require_parameter_free_primary_action(action)?;
        let semantic = match (action.kind.as_str(), action.action_id.as_str()) {
            ("open_chest", "open_chest") if !saw_open => {
                saw_open = true;
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Treasure,
                    choice: SemanticChoiceV1::new("treasure_chest", "open", 0),
                }
            }
            ("proceed_treasure", "proceed_treasure") if !saw_proceed => {
                saw_proceed = true;
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Treasure,
                }
            }
            _ => return Err("Java treasure boundary exposes an invalid primary action".to_owned()),
        };
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    if !saw_open || !saw_proceed {
        return Err("Java treasure boundary must expose open and proceed actions".to_owned());
    }
    Ok(resolved)
}

fn campfire_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let (actions, mut resolved) = run_decision_actions(observation)?;
    let mut semantics = resolved
        .iter()
        .map(|action| serde_json::to_string(&action.semantic))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| format!("failed to encode Java campfire action: {error}"))?;
    let mut option_ids = BTreeSet::new();
    for action in actions {
        require_parameter_free_primary_action(action)?;
        let option_id = match action.kind.as_str() {
            "begin_campfire_rest" if action.action_id == "begin_campfire_rest" => "rest",
            "begin_campfire_recall" if action.action_id == "begin_campfire_recall" => "recall",
            "begin_campfire_smith" if action.action_id == "begin_campfire_smith" => "smith",
            "begin_campfire_dig" if action.action_id == "begin_campfire_dig" => "dig",
            _ => return Err("Java campfire exposes an invalid primary action".to_owned()),
        };
        if !option_ids.insert(option_id) {
            return Err("Java campfire repeats one option".to_owned());
        }
        let semantic = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice: natural_campfire_option_v1(option_id)?,
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Java campfire action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Java campfire repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    if option_ids.is_empty() {
        return Err("Java campfire exposes no primary options".to_owned());
    }
    Ok(resolved)
}

fn smith_card_actions(
    observation: &JavaRuntimeObservationV1,
    runtime_card_bindings: &BTreeMap<String, String>,
    confirmation: bool,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let (actions, mut resolved) = run_decision_actions(observation)?;
    if actions.is_empty() || confirmation && actions.len() != 1 {
        return Err(if confirmation {
            "Java campfire Smith confirmation does not expose exactly one card".to_owned()
        } else {
            "Java campfire Smith selection exposes no cards".to_owned()
        });
    }
    let expected_kind = if confirmation {
        "confirm_smith_card"
    } else {
        "choose_smith_card"
    };
    let expected_prefix = if confirmation {
        "confirm_smith_card"
    } else {
        "smith_card"
    };
    let mut semantics = resolved
        .iter()
        .map(|action| serde_json::to_string(&action.semantic))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| format!("failed to encode Java Smith action: {error}"))?;
    let mut uuids = BTreeSet::new();
    for action in actions {
        require_parameter_free_primary_action(action)?;
        if action.kind != expected_kind {
            return Err("Java campfire Smith boundary exposes an unrelated action".to_owned());
        }
        let raw = unique_raw_action(observation, &action.action_id, &action.kind)?;
        let ordinal = usize::try_from(required_u64(raw, "card_ordinal")?)
            .map_err(|_| "Java Smith card ordinal exceeds usize".to_owned())?;
        let uuid = required_string(raw, "card_uuid")?;
        let raw_card_id = required_string(raw, "card_id")?;
        let card_id = canonical_game_id(raw_card_id);
        let upgrades = u8::try_from(required_u64(raw, "card_upgrades")?)
            .map_err(|_| "Java Smith card upgrades exceed u8".to_owned())?;
        if action.action_id
            != format!(
                "{expected_prefix}:{ordinal}:{uuid}:{raw_card_id}:{}",
                u64::from(upgrades)
            )
            || !uuids.insert(uuid)
        {
            return Err("Java campfire Smith action has an invalid typed card identity".to_owned());
        }
        let master_card = observation.frame["parity_internal"]["master_deck"]
            .as_array()
            .and_then(|deck| deck.get(ordinal))
            .ok_or_else(|| "Java Smith card points outside the master deck".to_owned())?;
        if master_card["card_uuid"].as_str() != Some(uuid)
            || master_card["card_id"]
                .as_str()
                .map(canonical_game_id)
                .as_deref()
                != Some(card_id.as_str())
            || master_card["upgrades"].as_u64() != Some(u64::from(upgrades))
        {
            return Err("Java Smith card disagrees with its master-deck instance".to_owned());
        }
        let binding = runtime_card_bindings.get(uuid).ok_or_else(|| {
            "Java Smith card UUID has no source-proven master-deck binding".to_owned()
        })?;
        let semantic = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice: natural_deck_card_choice_v1(binding, &card_id, upgrades)?,
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Java Smith action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Java campfire Smith repeats one semantic card".to_owned());
        }
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    Ok(resolved)
}

fn campfire_complete_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let (actions, mut resolved) = run_decision_actions(observation)?;
    let [action] = actions.as_slice() else {
        return Err(
            "Java completed campfire does not expose exactly one proceed action".to_owned(),
        );
    };
    require_parameter_free_primary_action(action)?;
    if action.kind != "proceed_rest_room" || action.action_id != "proceed_rest_room" {
        return Err("Java completed campfire exposes an invalid proceed action".to_owned());
    }
    resolved.push(ResolvedJavaActionV1 {
        semantic: SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::Campfire,
        },
        runtime_handle: action.runtime_handle.clone(),
        execution: JavaActionExecutionV1::Act,
    });
    Ok(resolved)
}

fn require_parameter_free_primary_action(action: &JavaRuntimeActionV1) -> Result<(), String> {
    require_parameter_free(action)?;
    require_no_reward_identity(action)?;
    require_no_potion_identity(action)?;
    require_primary_decision_identity(action)
}

fn require_primary_decision_identity(action: &JavaRuntimeActionV1) -> Result<(), String> {
    if action.event_id.is_none()
        && action.option_index.is_none()
        && action.neow_reward_type.is_none()
        && action.neow_drawback.is_none()
        && action.map_x.is_none()
        && action.map_y.is_none()
        && action.map_room_type.is_none()
    {
        Ok(())
    } else {
        Err("Java primary action carries an unrelated decision identity".to_owned())
    }
}

fn unique_raw_action<'a>(
    observation: &'a JavaRuntimeObservationV1,
    action_id: &str,
    kind: &str,
) -> Result<&'a serde_json::Value, String> {
    let actions = observation.frame["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| "Java frame omitted its raw action space".to_owned())?;
    let matching = actions
        .iter()
        .filter(|action| {
            action["action_id"].as_str() == Some(action_id) && action["kind"].as_str() == Some(kind)
        })
        .collect::<Vec<_>>();
    let [action] = matching.as_slice() else {
        return Err(format!(
            "Java action {action_id:?} resolves to {} raw actions instead of one",
            matching.len()
        ));
    };
    Ok(action)
}

pub(crate) struct JavaEventBoundaryIdentityV1 {
    pub raw_id: String,
    pub canonical_id: String,
    pub screen: i64,
}

pub(crate) fn java_event_boundary_identity_v1(
    observation: &JavaRuntimeObservationV1,
) -> Result<JavaEventBoundaryIdentityV1, String> {
    let decision = &observation.frame["public_base"]["decision"];
    let raw_id = decision["event_id"]
        .as_str()
        .filter(|event_id| !event_id.is_empty())
        .ok_or_else(|| "Java natural event omitted its stable ID".to_owned())?;
    let identity = &decision["event_identity"];
    if identity["status"].as_str() != Some("supported")
        || identity["event_id"].as_str() != Some(raw_id)
        || identity["runtime_class"]
            .as_str()
            .filter(|runtime_class| !runtime_class.is_empty())
            .is_none()
        || !identity["unsupported_reason"].is_null()
    {
        return Err("Java natural event has an invalid producer-owned identity".to_owned());
    }
    let screen = decision["event_screen_index"]
        .as_i64()
        .filter(|screen| *screen >= 0)
        .ok_or_else(|| "Java natural event omitted its non-negative screen".to_owned())?;
    let screen = if raw_id == "GremlinWheelGame" {
        java_wheel_state_v1(observation, screen)?.0
    } else {
        screen
    };
    Ok(JavaEventBoundaryIdentityV1 {
        raw_id: raw_id.to_owned(),
        canonical_id: canonical_game_id(raw_id),
        screen,
    })
}

pub(crate) fn java_wheel_state_v1(
    observation: &JavaRuntimeObservationV1,
    raw_screen: i64,
) -> Result<(i64, Option<i64>), String> {
    let state = &observation.frame["parity_internal"]["gremlin_wheel"];
    // The constructor's untouched introductory page has no pending result.
    // No old post-input page can acquire a result from screen number alone.
    if state.is_null()
        && raw_screen == 0
        && observation.frame["public_base"]["decision"]["kind"].as_str() == Some("event")
    {
        return Ok((0, None));
    }
    if state["schema"].as_str() != Some("sts-gremlin-wheel-state-v1") {
        return Err("Java Gremlin Wheel omitted its observed phase and result".to_owned());
    }
    let screen = match (
        raw_screen,
        state["screen"].as_str(),
        state["stage"].as_str(),
    ) {
        (0, Some("INTRO"), Some("intro")) if state["result"].is_null() => return Ok((0, None)),
        (0, Some("INTRO"), Some("spin")) => 3,
        // Original CUR_SCREEN order is INTRO, LEAVE, SPIN, COMPLETE.
        // Shared input screens follow the result lifecycle instead.
        (3, Some("COMPLETE"), Some("confirm")) => 1,
        (1, Some("LEAVE"), Some("leave")) => 2,
        _ => return Err("Java Gremlin Wheel is not at a consistent stable input phase".to_owned()),
    };
    let result = state["result"]
        .as_i64()
        .filter(|v| (0..=5).contains(v))
        .ok_or("Java Gremlin Wheel omitted its actual result in 0..=5")?;
    Ok((screen, Some(result)))
}

#[cfg(test)]
#[test]
fn wheel_phase_requires_observed_result_and_distinguishes_the_spin_button() {
    use serde_json::json;
    let mut observation = JavaRuntimeObservationV1 {
        producer: sts_parity_lockstep::ProducerIdentityV1 {
            implementation: "fixture".into(),
            version: "1".into(),
            session_epoch: "wheel".into(),
        },
        frame_id: 1,
        actions: Vec::new(),
        frame: json!({"public_base":{"decision":{"kind":"event"}}}),
    };
    assert_eq!(java_wheel_state_v1(&observation, 0).unwrap(), (0, None));
    assert!(java_wheel_state_v1(&observation, 1).is_err());
    for (raw, key, stage, expected) in [
        (0, "INTRO", "spin", 3),
        (3, "COMPLETE", "confirm", 1),
        (1, "LEAVE", "leave", 2),
    ] {
        observation.frame["parity_internal"] = json!({"gremlin_wheel": {
            "schema":"sts-gremlin-wheel-state-v1", "screen":key, "stage":stage, "result":4
        }});
        assert_eq!(
            java_wheel_state_v1(&observation, raw).unwrap(),
            (expected, Some(4))
        );
        assert!(java_wheel_state_v1(&observation, raw + 7).is_err());
        observation.frame["parity_internal"]["gremlin_wheel"]["result"] = json!(6);
        assert!(java_wheel_state_v1(&observation, raw).is_err());
    }
}

fn event_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let identity = java_event_boundary_identity_v1(observation)?;
    let mut option_indices = BTreeSet::new();
    let (actions, mut resolved) = run_decision_actions(observation)?;
    for action in actions {
        require_parameter_free(action)?;
        require_no_potion_identity(action)?;
        let option_index = action
            .option_index
            .ok_or_else(|| "Java natural event action omitted its option index".to_owned())?;
        if action.kind != "choose_event_option"
            || action.event_id.as_deref() != Some(identity.raw_id.as_str())
            || !option_indices.insert(option_index)
            || action.neow_reward_type.is_some()
            || action.neow_drawback.is_some()
            || action.map_x.is_some()
            || action.map_y.is_some()
            || action.map_room_type.is_some()
            || require_no_reward_identity(action).is_err()
        {
            return Err("Java natural event action has an invalid typed identity".to_owned());
        }
        let occurrence = u32::try_from(option_index)
            .map_err(|_| "Java natural event option index exceeds u32".to_owned())?;
        resolved.push(ResolvedJavaActionV1 {
            semantic: if identity.canonical_id == "matchandkeep" && identity.screen == 2 {
                let cell = u8::try_from(option_index).map_err(|_| "invalid match cell")?;
                if !crate::match_game::board(observation, identity.screen)?.can_flip(cell) {
                    return Err("Java exposes an unavailable Match and Keep cell".into());
                }
                sts_parity_lockstep::natural_match_flip_v1(cell)?
            } else {
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Event,
                    choice: natural_event_option_v1(
                        &identity.canonical_id,
                        identity.screen,
                        occurrence,
                    )?,
                }
            },
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    if resolved.is_empty() {
        return Err("Java natural event exposes no semantic actions".to_owned());
    }
    Ok(resolved)
}

pub(crate) struct JavaGridSelectBoundaryIdentityV1 {
    pub event: JavaEventBoundaryIdentityV1,
    pub selection_kind: &'static str,
}

pub(crate) fn java_grid_select_boundary_identity_v1(
    observation: &JavaRuntimeObservationV1,
) -> Result<JavaGridSelectBoundaryIdentityV1, String> {
    let decision = &observation.frame["public_base"]["decision"];
    if decision["kind"].as_str() != Some("grid_select") {
        return Err("Java natural grid selection has the wrong decision kind".to_owned());
    }
    let mut event = java_event_boundary_identity_v1(observation)?;
    let grid = decision["grid_select"]
        .as_object()
        .ok_or_else(|| "Java natural grid selection omitted its typed state".to_owned())?;
    if grid.get("source_pile").and_then(serde_json::Value::as_str) == Some("EventOffer")
        && grid
            .get("selection_reason")
            .and_then(serde_json::Value::as_str)
            == Some("TheLibrary")
        && event.raw_id == "TheLibrary"
        && event.screen == 1
    {
        return Ok(JavaGridSelectBoundaryIdentityV1 {
            event,
            selection_kind: "event_card_offer",
        });
    }
    if grid.get("source_pile").and_then(serde_json::Value::as_str) != Some("MasterDeck") {
        return Err("Java natural grid selection is not over the master deck".to_owned());
    }
    let selection_kind = match grid
        .get("selection_reason")
        .and_then(serde_json::Value::as_str)
    {
        Some("DeckRemove") => "deck_remove",
        Some("DeckUpgrade") => "deck_upgrade",
        Some("DeckTransform") => "deck_transform",
        _ => return Err("Java natural grid selection has an unsupported reason".to_owned()),
    };
    if event.raw_id == "Neow" {
        if !matches!(selection_kind, "deck_transform" | "deck_upgrade") || event.screen != 99 {
            return Err(
                "Java Neow grid selection has an unsupported deck operation boundary".to_owned(),
            );
        }
        // Neow marks its original event screen complete before opening the
        // grid. Canonically this is the simulator's pending selection page.
        event.screen = 2;
    }
    Ok(JavaGridSelectBoundaryIdentityV1 {
        event,
        selection_kind,
    })
}

fn grid_select_actions(
    observation: &JavaRuntimeObservationV1,
    runtime_card_bindings: &BTreeMap<String, String>,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let exhume = is_exhume_selection(observation);
    let combat_grid = is_supported_combat_grid_select(observation);
    let scry = combat_grid_kind(observation) == Some("scry");
    let discard = grid_selection_card_kind_v1(observation) == "discard_card";
    let draw = combat_grid && !exhume && !discard;
    let bottle = bottle_grid_identity_v1(observation)?;
    let astrolabe = astrolabe_grid_selection_v1(observation)?;
    let empty_cage = empty_cage_grid_v1(observation)?;
    let drug_dealer = !combat_grid
        && bottle.is_none()
        && astrolabe.is_none()
        && observation.frame["public_base"]["decision"]["event_id"].as_str() == Some("DrugDealer")
        && java_grid_select_boundary_identity_v1(observation)?
            .event
            .screen
            == 1
        && observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"].as_str()
            == Some("DeckTransform");
    let event_offer = !combat_grid
        && bottle.is_none()
        && astrolabe.is_none()
        && !empty_cage
        && java_grid_select_boundary_identity_v1(observation)?.selection_kind == "event_card_offer";
    let decision = &observation.frame["public_base"]["decision"];
    let expected_event = decision["event_id"].as_str();
    let source_pile = if exhume {
        "Exhaust"
    } else if discard {
        "Discard"
    } else if draw {
        "Draw"
    } else if event_offer {
        "EventOffer"
    } else {
        "MasterDeck"
    };
    let grid = decision["grid_select"]
        .as_object()
        .expect("grid identity validated the object");
    let grid_id = grid
        .get("grid_select_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Java natural grid selection omitted its identity".to_owned())?;
    let candidates = grid
        .get("candidate_cards")
        .and_then(serde_json::Value::as_array)
        .filter(|cards| !cards.is_empty())
        .ok_or("Java natural grid selection has no candidate cards")?;
    let requested = if scry {
        let requested = required_u64(&decision["grid_select"], "requested_cards")?;
        if requested == 0 || candidates.len() > 10 || candidates.len() as u64 > requested {
            return Err("Java Scry has an unsupported count contract".into());
        }
        requested
    } else if astrolabe.is_some() {
        3
    } else if drug_dealer || empty_cage {
        2
    } else {
        1
    };
    let allowed_counts = if scry {
        (0..=candidates.len()).collect::<Vec<_>>()
    } else if drug_dealer || empty_cage {
        vec![2]
    } else {
        vec![1]
    };
    if grid
        .get("requested_cards")
        .and_then(serde_json::Value::as_u64)
        != Some(requested)
        || grid.get("any_number").and_then(serde_json::Value::as_bool) != Some(scry)
        || grid.get("can_cancel").and_then(serde_json::Value::as_bool) != Some(false)
        || grid
            .get("confirm_supported")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        || grid
            .get("cancel_supported")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        || grid.get("allowed_counts") != Some(&serde_json::json!(allowed_counts))
        || grid
            .get("selected_cards")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|cards| {
                if astrolabe.is_some() {
                    cards.len() >= 3
                } else {
                    !cards.is_empty()
                }
            })
    {
        return Err(
            "Java natural grid selection is outside its supported click contract".to_owned(),
        );
    }

    let (primary, mut resolved) = run_decision_actions(observation)?;
    let [action] = primary.as_slice() else {
        return Err(
            "Java natural grid selection does not expose exactly one submit action".to_owned(),
        );
    };
    if action.kind != "submit_grid_select"
        || action.action_id != format!("submit_grid_select:{grid_id}")
        || action.runtime_handle.is_empty()
        || !action.parameters_required
        || action.event_id.as_deref() != expected_event
        || action.option_index.is_some()
        || action.neow_reward_type.is_some()
        || action.neow_drawback.is_some()
        || action.map_x.is_some()
        || action.map_y.is_some()
        || action.map_room_type.is_some()
        || require_no_reward_identity(action).is_err()
        || require_no_potion_identity(action).is_err()
    {
        return Err("Java natural grid submit action has an invalid typed identity".to_owned());
    }

    let mut uuids = BTreeSet::new();
    let mut ordered_uuids = Vec::with_capacity(candidates.len());
    let mut semantics = BTreeSet::new();
    let mut offered_ids = BTreeSet::new();
    let mut bound_choices = Vec::with_capacity(candidates.len());
    if event_offer && candidates.len() != 20 {
        return Err("Java Library omitted its twenty offers".to_owned());
    }
    let source_cards = if exhume {
        &observation.frame["parity_internal"]["zones"]["exhaust"]["cards"]
    } else if discard {
        &observation.frame["parity_internal"]["zones"]["discard"]["cards"]
    } else if draw {
        &observation.frame["parity_internal"]["zones"]["draw"]["cards"]
    } else {
        &observation.frame["parity_internal"]["master_deck"]
    }
    .as_array()
    .ok_or("Java grid selection omitted its source pile")?;
    if (exhume
        || discard
        || matches!(
            combat_grid_kind(observation),
            Some("omniscience" | "draw_to_hand")
        ))
        && source_cards.len() != candidates.len()
    {
        return Err("Java combat grid candidates omit source-pile cards".to_owned());
    }
    if scry && candidates.len() != source_cards.len().min(requested as usize) {
        return Err("Java Scry omitted revealed top-draw candidates".into());
    }
    for (index, card) in candidates.iter().enumerate() {
        let uuid = required_string(card, "card_uuid")?;
        let card_id = canonical_game_id(required_string(card, "card_id")?);
        let upgrades = u8::try_from(required_u64(card, "upgrades")?)
            .map_err(|_| "Java grid candidate upgrades exceed u8".to_owned())?;
        if card["index"].as_u64() != Some(index as u64) || !uuids.insert(uuid) {
            return Err("Java natural grid candidates have invalid order or identity".to_owned());
        }
        ordered_uuids.push(uuid);
        if scry
            && source_cards[source_cards.len() - index - 1]["instance_uuid"].as_str() != Some(uuid)
        {
            return Err(
                "Java Scry candidates are not the exact top draw-pile objects in order".into(),
            );
        }
        let choice = if event_offer {
            if !offered_ids.insert(card_id.clone())
                || source_cards
                    .iter()
                    .any(|card| card["card_uuid"].as_str() == Some(uuid))
                || runtime_card_bindings.contains_key(uuid)
            {
                return Err(
                    "Java Library offers repeat a card or reuse an acquired UUID".to_owned(),
                );
            }
            sts_parity_lockstep::natural_event_card_offer_v1(
                "thelibrary",
                1,
                index as u32,
                &card_id,
                upgrades,
            )?
        } else {
            let binding = runtime_card_bindings.get(uuid).ok_or_else(|| {
                "Java grid candidate UUID has no source-proven card binding".to_owned()
            })?;
            let uuid_field = if combat_grid {
                "instance_uuid"
            } else {
                "card_uuid"
            };
            let master_card = source_cards
                .iter()
                .find(|master| master[uuid_field].as_str() == Some(uuid))
                .ok_or_else(|| "Java grid candidate is absent from its source pile".to_owned())?;
            if master_card["card_id"]
                .as_str()
                .map(canonical_game_id)
                .as_deref()
                != Some(card_id.as_str())
                || master_card["upgrades"].as_u64() != Some(u64::from(upgrades))
            {
                return Err("Java grid candidate disagrees with its source-pile card".to_owned());
            }
            if exhume && card_id == "exhume" {
                return Err("Java Exhume cannot select another Exhume".to_owned());
            }
            if exhume {
                sts_parity_lockstep::natural_exhaust_card_choice_v1(binding, &card_id, upgrades)?
            } else if discard {
                sts_parity_lockstep::natural_discard_card_choice_v1(binding, &card_id, upgrades)?
            } else if draw {
                sts_parity_lockstep::natural_draw_card_choice_v1(binding, &card_id, upgrades)?
            } else {
                natural_deck_card_choice_v1(binding, &card_id, upgrades)?
            }
        };
        bound_choices.push(choice);
    }
    let selections = if drug_dealer || empty_cage {
        (0..bound_choices.len())
            .flat_map(|a| {
                (0..bound_choices.len())
                    .filter(move |b| *b != a)
                    .map(move |b| vec![a, b])
            })
            .collect()
    } else if scry {
        sts_parity_lockstep::canonical_selection_indices_v1(bound_choices.len(), allowed_counts)?
    } else {
        (0..bound_choices.len()).map(|index| vec![index]).collect()
    };
    for indices in selections {
        let choices = indices
            .iter()
            .map(|index| bound_choices[*index].clone())
            .collect::<Vec<_>>();
        let semantic = if astrolabe.is_some() {
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::GridSelect,
                choice: choices[0].clone(),
            }
        } else {
            SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::GridSelect,
                choices,
            }
        };
        let semantic_key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Java grid action: {error}"))?;
        if !semantics.insert(semantic_key) {
            return Err("Java natural grid selection repeats one semantic card".to_owned());
        }
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::SubmitGridSelect {
                selected_card_uuids: indices
                    .iter()
                    .map(|index| ordered_uuids[*index].to_owned())
                    .collect(),
            },
        });
    }
    let expected_grid_id = format!(
        "grid_select:{}:{source_pile}:{}:{}:{scry}:false:{}:selected:{}",
        expected_event.unwrap_or("none"),
        grid.get("selection_reason")
            .and_then(serde_json::Value::as_str)
            .ok_or("grid reason missing")?,
        requested,
        ordered_uuids.join(","),
        astrolabe
            .as_ref()
            .map(|selected| selected.join(","))
            .unwrap_or_default()
    );
    if grid_id != expected_grid_id {
        return Err("Java natural grid identity disagrees with its typed state".to_owned());
    }
    let raw_actions = observation.frame["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| "Java natural grid frame omitted its raw action space".to_owned())?;
    let matching_raw = raw_actions
        .iter()
        .filter(|raw| {
            raw["action_id"].as_str() == Some(action.action_id.as_str())
                && raw["kind"].as_str() == Some("submit_grid_select")
        })
        .collect::<Vec<_>>();
    let [raw] = matching_raw.as_slice() else {
        return Err("Java natural grid submit action has no unique raw contract".to_owned());
    };
    let expected_uuids = ordered_uuids
        .iter()
        .map(|uuid| serde_json::Value::String((*uuid).to_owned()))
        .collect::<Vec<_>>();
    if raw["grid_select_id"].as_str() != Some(grid_id)
        || raw["source_pile"].as_str() != Some(source_pile)
        || raw["selection_reason"].as_str()
            != grid
                .get("selection_reason")
                .and_then(serde_json::Value::as_str)
        || raw["event_id"].as_str() != expected_event
        || raw["parameter_schema"]["field"].as_str() != Some("selected_card_uuids")
        || raw["parameter_schema"]["item_format"].as_str() != Some("uuid")
        || raw["parameter_schema"]["ordered"].as_bool() != Some(true)
        || raw["parameter_schema"]["unique_items"].as_bool() != Some(true)
        || raw["parameter_schema"]["allowed_card_uuids"].as_array() != Some(&expected_uuids)
        || raw["parameter_schema"]["allowed_counts"].as_array()
            != grid
                .get("allowed_counts")
                .and_then(serde_json::Value::as_array)
    {
        return Err("Java natural grid raw action disagrees with its typed decision".to_owned());
    }
    Ok(resolved)
}

pub(crate) fn empty_cage_grid_v1(observation: &JavaRuntimeObservationV1) -> Result<bool, String> {
    let decision = &observation.frame["public_base"]["decision"];
    if decision["grid_select"]["selection_reason"].as_str() != Some("EmptyCage") {
        return Ok(false);
    }
    if decision["kind"].as_str() != Some("grid_select")
        || !decision["event_id"].is_null()
        || decision["grid_select"]["source_pile"].as_str() != Some("MasterDeck")
    {
        return Err("Java Empty Cage selection has a different owner or source pile".into());
    }
    let relics = observation.frame["parity_internal"]["relics"]
        .as_array()
        .ok_or("Java Empty Cage selection omitted owned relics")?;
    if relics
        .iter()
        .filter(|relic| {
            relic["relic_id"].as_str().map(canonical_game_id).as_deref() == Some("emptycage")
        })
        .count()
        != 1
    {
        return Err("Java Empty Cage selection requires one owned relic".into());
    }
    Ok(true)
}

pub(crate) fn astrolabe_grid_selection_v1(
    observation: &JavaRuntimeObservationV1,
) -> Result<Option<Vec<String>>, String> {
    let decision = &observation.frame["public_base"]["decision"];
    let grid = &decision["grid_select"];
    if grid["selection_reason"].as_str() != Some("Astrolabe") {
        return Ok(None);
    }
    if decision["kind"].as_str() != Some("grid_select")
        || !decision["event_id"].is_null()
        || grid["source_pile"].as_str() != Some("MasterDeck")
        || grid["requested_cards"].as_u64() != Some(3)
    {
        return Err("Java Astrolabe grid has a different owner or source".into());
    }
    let relics = observation.frame["parity_internal"]["relics"]
        .as_array()
        .ok_or("Astrolabe relic evidence missing")?;
    if relics
        .iter()
        .filter(|r| {
            r["relic_id"]
                .as_str()
                .is_some_and(|id| canonical_game_id(id) == "astrolabe")
        })
        .count()
        != 1
    {
        return Err("Java Astrolabe grid requires one owned Astrolabe".into());
    }
    let selected = grid["selected_cards"]
        .as_array()
        .ok_or("Astrolabe selected cards missing")?;
    let candidates = grid["candidate_cards"]
        .as_array()
        .ok_or("Astrolabe candidates missing")?;
    let deck = observation.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or("Astrolabe deck missing")?;
    let mut uuids = Vec::new();
    for card in selected {
        let uuid = required_string(card, "card_uuid")?;
        if uuids.iter().any(|old| old == uuid)
            || candidates
                .iter()
                .any(|c| c["card_uuid"].as_str() == Some(uuid))
            || !deck.iter().any(|c| {
                c["card_uuid"].as_str() == Some(uuid)
                    && c["card_id"] == card["card_id"]
                    && c["upgrades"] == card["upgrades"]
            })
        {
            return Err("Java Astrolabe selected prefix is not a unique subset of the deck".into());
        }
        uuids.push(uuid.to_owned());
    }
    if uuids.len() >= 3 {
        return Err("Java Astrolabe published an unsettled full selection".into());
    }
    Ok(Some(uuids))
}

pub(crate) fn bottle_grid_identity_v1(
    observation: &JavaRuntimeObservationV1,
) -> Result<Option<(&'static str, &'static str)>, String> {
    let decision = &observation.frame["public_base"]["decision"];
    let identity = match decision["grid_select"]["selection_reason"].as_str() {
        Some("BottleFlame") => ("bottledflame", "bottle_flame"),
        Some("BottleLightning") => ("bottledlightning", "bottle_lightning"),
        Some("BottleTornado") => ("bottledtornado", "bottle_tornado"),
        _ => return Ok(None),
    };
    if decision["kind"].as_str() != Some("grid_select")
        || decision["grid_select"]["source_pile"].as_str() != Some("MasterDeck")
        || !decision["event_id"].is_null()
    {
        return Err("Java bottle selection has a different owner or source pile".to_owned());
    }
    let relics = observation.frame["parity_internal"]["relics"]
        .as_array()
        .ok_or("Java bottle selection omitted owned relics")?;
    let owners = relics
        .iter()
        .filter(|relic| {
            relic["relic_id"]
                .as_str()
                .is_some_and(|id| canonical_game_id(id) == identity.0)
        })
        .collect::<Vec<_>>();
    if owners.len() != 1 || !owners[0]["bound_card_uuid"].is_null() {
        return Err("Java bottle selection requires one unbound owned relic".to_owned());
    }
    Ok(Some(identity))
}

pub(crate) fn is_exhume_selection(observation: &JavaRuntimeObservationV1) -> bool {
    combat_grid_kind(observation) == Some("exhume")
}

pub(crate) fn is_supported_combat_grid_select(observation: &JavaRuntimeObservationV1) -> bool {
    combat_grid_kind(observation).is_some()
}

pub(crate) fn combat_grid_kind(observation: &JavaRuntimeObservationV1) -> Option<&'static str> {
    let decision = &observation.frame["public_base"]["decision"];
    if observation.frame["boundary"]["kind"].as_str() != Some("run_decision")
        || decision["kind"].as_str() != Some("grid_select")
    {
        return None;
    }
    natural_combat_grid_kind_v1(
        decision["grid_select"]["source_pile"].as_str(),
        decision["grid_select"]["selection_reason"].as_str(),
    )
}

/// The card identity used by both live action mapping and recorded selection
/// replay. Surface admission and runtime UUID binding remain separate checks.
pub fn grid_selection_card_kind_v1(observation: &JavaRuntimeObservationV1) -> &'static str {
    let grid = &observation.frame["public_base"]["decision"]["grid_select"];
    let source = grid["source_pile"].as_str();
    let reason = grid["selection_reason"].as_str();
    match natural_combat_grid_kind_v1(source, reason) {
        Some("exhume") => "exhaust_card",
        Some(_) if source == Some("Discard") => "discard_card",
        Some(_) => "draw_card",
        None if source == Some("EventOffer") && reason == Some("TheLibrary") => "event_card_offer",
        None => "deck_card",
    }
}

fn hand_select_actions(
    observation: &JavaRuntimeObservationV1,
    runtime_card_bindings: &BTreeMap<String, String>,
    profile: JavaHandSelectProfileV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let decision = &observation.frame["public_base"]["decision"];
    let hand_select = decision["hand_select"]
        .as_object()
        .ok_or_else(|| "Java natural hand selection omitted its typed state".to_owned())?;
    let hand_select_id = hand_select
        .get("hand_select_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Java natural hand selection omitted its identity".to_owned())?;
    let selection_reason = hand_select
        .get("selection_reason")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Java natural hand selection omitted its display reason".to_owned())?;
    let requested_cards = hand_select
        .get("requested_cards")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Java natural hand selection omitted its requested count".to_owned())?;
    let any_number = hand_select
        .get("any_number")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Java natural hand selection omitted any-number state".to_owned())?;
    let can_pick_zero = hand_select
        .get("can_pick_zero")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Java natural hand selection omitted zero-card state".to_owned())?;
    let up_to = hand_select
        .get("up_to")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Java natural hand selection omitted up-to state".to_owned())?;
    let allowed_counts = hand_select
        .get("allowed_counts")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Java natural hand selection omitted allowed counts".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| {
                    "Java natural hand selection has an invalid allowed count".to_owned()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if hand_select
        .get("confirm_supported")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || hand_select
            .get("cancel_supported")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        || hand_select
            .get("selected_cards")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|cards| !cards.is_empty())
    {
        return Err("Java natural hand selection is not an atomic submit surface".to_owned());
    }
    let _ = selection_reason;

    let (primary, mut resolved) = run_decision_actions(observation)?;
    let [action] = primary.as_slice() else {
        return Err(
            "Java natural hand selection does not expose exactly one submit action".to_owned(),
        );
    };
    if action.kind != "submit_hand_select"
        || action.action_id != format!("submit_hand_select:{hand_select_id}")
        || action.runtime_handle.is_empty()
        || !action.parameters_required
        || action.event_id.is_some()
        || action.option_index.is_some()
        || action.neow_reward_type.is_some()
        || action.neow_drawback.is_some()
        || action.map_x.is_some()
        || action.map_y.is_some()
        || action.map_room_type.is_some()
        || require_no_reward_identity(action).is_err()
        || require_no_potion_identity(action).is_err()
    {
        return Err("Java natural hand submit action has an invalid typed identity".to_owned());
    }

    let hand = observation.frame["parity_internal"]["zones"]["hand"]["cards"]
        .as_array()
        .ok_or_else(|| "Java natural hand selection omitted the combat hand".to_owned())?;
    let candidates = hand_select
        .get("candidate_cards")
        .and_then(serde_json::Value::as_array)
        .filter(|cards| !cards.is_empty())
        .ok_or_else(|| "Java natural hand selection has no candidate cards".to_owned())?;
    let expected_counts = match profile {
        JavaHandSelectProfileV1::SingleCopy
            if requested_cards == 1
                && !any_number
                && !can_pick_zero
                && !up_to
                && candidates.len() > 1 =>
        {
            vec![1]
        }
        JavaHandSelectProfileV1::ToolsOfTheTrade { cards }
        | JavaHandSelectProfileV1::CardDiscard { cards }
            if cards > 0
                && requested_cards == cards
                && cards <= candidates.len()
                && candidates.len() == hand.len()
                && !any_number
                && !can_pick_zero
                && !up_to =>
        {
            vec![cards]
        }
        JavaHandSelectProfileV1::SingleExhaust
        | JavaHandSelectProfileV1::SinglePutOnDrawPile
        | JavaHandSelectProfileV1::SingleUpgrade
            if requested_cards == 1 && !any_number && !can_pick_zero && !up_to =>
        {
            vec![1]
        }
        JavaHandSelectProfileV1::Retain { max_cards }
            if max_cards > 0
                && requested_cards == max_cards
                && !any_number
                && can_pick_zero
                && up_to
                && candidates.len() == hand.len() =>
        {
            (0..=candidates.len().min(max_cards)).collect()
        }
        JavaHandSelectProfileV1::GamblingChip | JavaHandSelectProfileV1::PotionExhaust
            if requested_cards == 99 && any_number && can_pick_zero && !up_to =>
        {
            (0..=candidates.len()).collect()
        }
        JavaHandSelectProfileV1::OptionalExhaust { max_cards }
            if matches!(max_cards, 3 | 5)
                && requested_cards == max_cards
                && any_number
                && can_pick_zero
                && !up_to =>
        {
            (0..=candidates.len().min(max_cards)).collect()
        }
        _ => {
            return Err(format!(
                "Java natural hand selection disagrees with its authorized {profile:?} profile"
            ))
        }
    };
    if allowed_counts != expected_counts {
        return Err("Java natural hand selection has unexpected allowed counts".to_owned());
    }
    let mut ordered_uuids = Vec::with_capacity(candidates.len());
    let mut bound_candidates = Vec::with_capacity(candidates.len());
    let mut seen_uuids = BTreeSet::new();
    let mut semantics = BTreeSet::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let uuid = required_string(candidate, "card_uuid")?;
        let card_id = canonical_game_id(required_string(candidate, "card_id")?);
        let upgrades = u8::try_from(required_u64(candidate, "upgrades")?)
            .map_err(|_| "Java hand candidate upgrades exceed u8".to_owned())?;
        if candidate["index"].as_u64() != Some(index as u64) || !seen_uuids.insert(uuid) {
            return Err("Java natural hand candidates have invalid order or identity".to_owned());
        }
        ordered_uuids.push(uuid);
        let binding = runtime_card_bindings.get(uuid).ok_or_else(|| {
            "Java hand candidate UUID has no source-proven combat-card binding".to_owned()
        })?;
        let hand_card = hand
            .iter()
            .find(|card| card["instance_uuid"].as_str() == Some(uuid))
            .ok_or_else(|| "Java hand candidate is absent from the combat hand".to_owned())?;
        if hand_card["card_id"]
            .as_str()
            .map(canonical_game_id)
            .as_deref()
            != Some(card_id.as_str())
            || hand_card["upgrades"].as_u64() != Some(u64::from(upgrades))
            || hand_card["cost_for_turn"].as_i64() != candidate["cost_for_turn"].as_i64()
        {
            return Err("Java hand candidate disagrees with its combat card".to_owned());
        }
        bound_candidates.push(natural_hand_card_choice_v1(binding, &card_id, upgrades)?);
    }
    for indices in canonical_selection_indices_v1(candidates.len(), allowed_counts.clone())? {
        let semantic = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect,
            choices: indices
                .iter()
                .map(|index| bound_candidates[*index].clone())
                .collect(),
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Java hand-selection action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Java natural hand selection repeats one semantic submission".to_owned());
        }
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::SubmitHandSelect {
                selected_card_uuids: indices
                    .iter()
                    .map(|index| ordered_uuids[*index].to_owned())
                    .collect(),
            },
        });
    }

    let expected_id = format!(
        "hand_select:{requested_cards}:{any_number}:{can_pick_zero}:{up_to}:{}:selected:",
        ordered_uuids.join(",")
    );
    if hand_select_id != expected_id {
        return Err("Java natural hand identity disagrees with its typed state".to_owned());
    }
    let raw_actions = observation.frame["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| "Java natural hand frame omitted its raw action space".to_owned())?;
    let matching_raw = raw_actions
        .iter()
        .filter(|raw| {
            raw["action_id"].as_str() == Some(action.action_id.as_str())
                && raw["kind"].as_str() == Some("submit_hand_select")
        })
        .collect::<Vec<_>>();
    let [raw] = matching_raw.as_slice() else {
        return Err("Java natural hand submit action has no unique raw contract".to_owned());
    };
    let expected_uuids = ordered_uuids
        .iter()
        .map(|uuid| serde_json::Value::String((*uuid).to_owned()))
        .collect::<Vec<_>>();
    if raw["hand_select_id"].as_str() != Some(hand_select_id)
        || raw["parameter_schema"]["field"].as_str() != Some("selected_card_uuids")
        || raw["parameter_schema"]["item_format"].as_str() != Some("uuid")
        || raw["parameter_schema"]["ordered"].as_bool() != Some(true)
        || raw["parameter_schema"]["unique_items"].as_bool() != Some(true)
        || raw["parameter_schema"]["allowed_card_uuids"].as_array() != Some(&expected_uuids)
        || raw["parameter_schema"]["allowed_counts"]
            != serde_json::to_value(&allowed_counts)
                .map_err(|error| format!("failed to encode hand allowed counts: {error}"))?
    {
        return Err("Java natural hand raw action disagrees with its typed decision".to_owned());
    }
    Ok(resolved)
}

fn reward_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let boundary_kind = crate::projection::boundary_kind(observation)?;
    let rewards = crate::projection::rewards::project_rewards_v1(observation, boundary_kind)?;
    let (items, surface) = match &rewards {
        NaturalRunRewardsV2::Combat { items, .. } => (items, DecisionSurfaceV1::CombatReward),
        NaturalRunRewardsV2::Treasure { items, .. } => (items, DecisionSurfaceV1::TreasureReward),
        NaturalRunRewardsV2::Inactive => {
            return Err("Java reward boundary omitted its rewards".to_owned())
        }
    };
    let mut occurrences = BTreeMap::<String, u32>::new();
    let mut item_choices = Vec::with_capacity(items.len());
    for item in items {
        let key = serde_json::to_string(item)
            .map_err(|error| format!("failed to encode Java reward identity: {error}"))?;
        let occurrence = *occurrences.entry(key.clone()).or_default();
        *occurrences
            .get_mut(&key)
            .expect("reward occurrence was inserted") += 1;
        item_choices.push(natural_reward_choice_v1(item, occurrence));
    }

    let mut claimed_indices = BTreeSet::new();
    let mut semantics = BTreeSet::new();
    let (actions, mut resolved) = run_decision_actions(observation)?;
    for discard in &resolved {
        semantics.insert(
            serde_json::to_string(&discard.semantic)
                .map_err(|error| format!("failed to encode Java reward action: {error}"))?,
        );
    }
    for action in actions {
        require_parameter_free(action)?;
        require_no_potion_identity(action)?;
        let semantic = match action.kind.as_str() {
            "claim_combat_reward" => {
                let index = action
                    .reward_index
                    .ok_or_else(|| "Java combat reward action omitted its index".to_owned())?;
                if !claimed_indices.insert(index) {
                    return Err("Java combat reward repeats one claim index".to_owned());
                }
                let item = items.get(index).ok_or_else(|| {
                    "Java combat reward action points outside its item list".to_owned()
                })?;
                validate_reward_action_identity(action, item, items, index)?;
                SemanticActionV1::Select {
                    surface,
                    choice: item_choices[index].clone(),
                }
            }
            "close_combat_reward" | "close_shop_rewards" => {
                require_no_reward_identity(action)?;
                let shop = crate::projection::is_shop_reward_overlay(observation)?;
                if shop != (action.kind == "close_shop_rewards") {
                    return Err(
                        "Java reward close action disagrees with its return context".to_owned()
                    );
                }
                if shop {
                    SemanticActionV1::Cancel { surface }
                } else {
                    SemanticActionV1::Proceed { surface }
                }
            }
            other => {
                return Err(format!(
                    "Java combat reward exposes unrelated action {other:?}"
                ))
            }
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Java reward action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Java combat reward repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    if resolved.is_empty() {
        return Err("Java combat reward exposes no semantic actions".to_owned());
    }
    Ok(resolved)
}

fn card_reward_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let boundary = crate::projection::boundary_kind(observation)?;
    let discovery = boundary == "natural_combat_discovery";
    let codex = boundary == "natural_combat_codex";
    let neow = boundary == "natural_neow_card_reward";
    if !discovery && !codex && !neow && boundary != "natural_card_reward" {
        return Err("Java card reward is not an ordinary natural reward".to_owned());
    }
    let reward = observation.frame["public_base"]["decision"]["card_reward"]
        .as_object()
        .ok_or_else(|| "Java card reward omitted its typed decision".to_owned())?;
    if reward
        .get("source_kind")
        .and_then(serde_json::Value::as_str)
        != Some(if discovery {
            "combat_discovery"
        } else if codex {
            "codex"
        } else if neow {
            "neow_reward"
        } else {
            "ordinary_reward"
        })
    {
        return Err(
            "Java natural card reward has an unsupported source or control grant".to_owned(),
        );
    }
    let bowl_available = reward
        .get("singing_bowl_available")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Java card reward omitted Singing Bowl availability".to_owned())?;
    let bowl_supported = reward
        .get("singing_bowl_control_supported")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Java card reward omitted Singing Bowl control support".to_owned())?;
    if bowl_available && (!bowl_supported || discovery || codex) {
        return Err("Java Singing Bowl lacks ordinary reward control".to_owned());
    }
    let skip_available = reward
        .get("skip_available")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Java card reward omitted skip availability".to_owned())?;
    let cards = reward
        .get("cards")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Java card reward omitted its offered cards".to_owned())?;
    if cards.is_empty() {
        return Err("Java ordinary card reward has no offered cards".to_owned());
    }

    let mut occurrences = BTreeMap::<(String, u8), u32>::new();
    let mut choices = Vec::with_capacity(cards.len());
    let mut action_ids = Vec::with_capacity(cards.len());
    for (expected_index, card) in cards.iter().enumerate() {
        let index = usize::try_from(required_u64(card, "reward_index")?)
            .map_err(|_| "Java card reward index exceeds usize".to_owned())?;
        let java_uuid = required_string(card, "card_uuid")?;
        let raw_card_id = required_string(card, "card_id")?;
        let upgrades = u8::try_from(required_u64(card, "upgrades")?)
            .map_err(|_| "Java card reward upgrades exceed u8".to_owned())?;
        if index != expected_index || java_uuid.is_empty() {
            return Err(
                "Java card reward offers are not contiguously and exactly identified".to_owned(),
            );
        }
        let card = NaturalRewardCardV1 {
            card_id: canonical_game_id(raw_card_id),
            upgrades,
        };
        let key = (card.card_id.clone(), card.upgrades);
        let occurrence = *occurrences.entry(key.clone()).or_default();
        *occurrences
            .get_mut(&key)
            .expect("card-reward occurrence was inserted") += 1;
        choices.push(if discovery {
            sts_parity_lockstep::natural_discovery_choice_v1(&card, occurrence)
        } else if codex {
            sts_parity_lockstep::natural_codex_choice_v1(&card, occurrence)
        } else {
            natural_card_reward_choice_v1(&card, occurrence)
        });
        action_ids.push(format!(
            "card_reward:{index}:{java_uuid}:{raw_card_id}:{upgrades}"
        ));
    }

    let (actions, mut resolved) = run_decision_actions(observation)?;
    let mut selected = BTreeSet::new();
    let mut skip_count = 0_usize;
    let mut bowl_count = 0_usize;
    for action in actions {
        require_parameter_free(action)?;
        require_no_potion_identity(action)?;
        let semantic = match action.kind.as_str() {
            "choose_card_reward" => {
                let index = action
                    .reward_index
                    .ok_or_else(|| "Java card reward action omitted its option index".to_owned())?;
                let expected_action_id = action_ids.get(index).ok_or_else(|| {
                    "Java card reward action points outside its offered cards".to_owned()
                })?;
                let choice = choices.get(index).ok_or_else(|| {
                    "Java card reward action points outside its semantic choices".to_owned()
                })?;
                if !selected.insert(index)
                    || action.reward_type.as_deref() != Some("CARD")
                    || action.reward_id.is_some()
                    || action.reward_gold.is_some()
                    || action.action_id != *expected_action_id
                {
                    return Err("Java card reward action disagrees with its typed offer".to_owned());
                }
                SemanticActionV1::Select {
                    surface: if discovery {
                        DecisionSurfaceV1::CombatDiscovery
                    } else if codex {
                        DecisionSurfaceV1::CombatCodex
                    } else {
                        DecisionSurfaceV1::CardReward
                    },
                    choice: choice.clone(),
                }
            }
            "skip_card_reward" => {
                skip_count += 1;
                if action.action_id != "skip_card_reward"
                    || action.reward_type.as_deref() != Some("CARD")
                    || action.reward_index.is_some()
                    || action.reward_id.is_some()
                    || action.reward_gold.is_some()
                {
                    return Err("Java card reward skip is not exactly typed".to_owned());
                }
                SemanticActionV1::Cancel {
                    surface: if discovery {
                        DecisionSurfaceV1::CombatDiscovery
                    } else if codex {
                        DecisionSurfaceV1::CombatCodex
                    } else {
                        DecisionSurfaceV1::CardReward
                    },
                }
            }
            "take_singing_bowl" => {
                bowl_count += 1;
                if !bowl_available
                    || !bowl_supported
                    || action.action_id != "take_singing_bowl"
                    || action.reward_type.as_deref() != Some("CARD")
                    || action.reward_index.is_some()
                    || action.reward_id.is_some()
                    || action.reward_gold.is_some()
                {
                    return Err("Java Singing Bowl action is not exactly typed".into());
                }
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CardReward,
                    choice: sts_parity_lockstep::natural_singing_bowl_choice_v1(),
                }
            }
            other => {
                return Err(format!(
                    "Java card reward exposes unrelated action {other:?}"
                ))
            }
        };
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    if selected.len() != cards.len()
        || skip_count != usize::from(skip_available)
        || bowl_count != usize::from(bowl_available)
    {
        return Err("Java card reward action space is incomplete or repeated".to_owned());
    }
    Ok(resolved)
}

fn run_potion_action(
    observation: &JavaRuntimeObservationV1,
    action: &JavaRuntimeActionV1,
) -> Result<SemanticActionV1, String> {
    let use_potion = match action.kind.as_str() {
        "use_potion" => true,
        "discard_potion" => false,
        _ => return Err("Java run potion has an unsupported action kind".to_owned()),
    };
    require_parameter_free(action)?;
    require_no_reward_identity(action)?;
    if action.event_id.is_some()
        || action.option_index.is_some()
        || action.neow_reward_type.is_some()
        || action.neow_drawback.is_some()
        || action.map_x.is_some()
        || action.map_y.is_some()
        || action.map_room_type.is_some()
    {
        return Err("Java run potion action carries a primary decision identity".to_owned());
    }
    let slot = action
        .potion_slot
        .ok_or_else(|| "Java run potion action omitted its slot".to_owned())?;
    let raw_id = action
        .potion_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Java run potion action omitted its potion ID".to_owned())?;
    if action.action_id != format!("{}:{slot}:{raw_id}", action.kind) {
        return Err("Java run potion action ID disagrees with its typed identity".to_owned());
    }
    let potion = observation.frame["parity_internal"]["potions"]
        .as_array()
        .and_then(|potions| {
            potions
                .iter()
                .find(|potion| potion["slot_index"].as_u64() == Some(slot as u64))
        })
        .ok_or_else(|| "Java run potion action points outside the potion belt".to_owned())?;
    if potion["empty"].as_bool() != Some(false) || potion["potion_id"].as_str() != Some(raw_id) {
        return Err("Java run potion action disagrees with its observed slot".to_owned());
    }
    if use_potion
        && (potion["can_use"].as_bool() != Some(true)
            || potion["target_required"].as_bool() != Some(false)
            || potion["potion_slot"].as_u64() != Some(slot as u64))
    {
        return Err("Java run potion use lacks its usable untargeted slot evidence".to_owned());
    }
    let slot = u8::try_from(slot).map_err(|_| "Java potion slot exceeds u8".to_owned())?;
    let potion_id = canonical_game_id(raw_id);
    Ok(if use_potion {
        SemanticActionV1::UsePotion {
            slot,
            potion_id,
            target: None,
        }
    } else {
        SemanticActionV1::DiscardPotion { slot, potion_id }
    })
}

fn validate_reward_action_identity(
    action: &JavaRuntimeActionV1,
    item: &NaturalRunRewardItemV1,
    items: &[NaturalRunRewardItemV1],
    index: usize,
) -> Result<(), String> {
    let reward_type = action
        .reward_type
        .as_deref()
        .ok_or_else(|| "Java reward action omitted its type".to_owned())?;
    let reward_id = action
        .reward_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Java reward action omitted its runtime identity".to_owned())?;
    let valid = match item {
        NaturalRunRewardItemV1::Gold { amount } => {
            reward_type == "GOLD"
                && action.reward_gold == Some(i64::from(*amount))
                && reward_id == amount.to_string()
        }
        NaturalRunRewardItemV1::StolenGold { amount } => {
            reward_type == "STOLEN_GOLD"
                && action.reward_gold == Some(i64::from(*amount))
                && reward_id == amount.to_string()
        }
        NaturalRunRewardItemV1::Card { cards } => {
            reward_type == "CARD"
                && action.reward_gold.is_none()
                && !cards.is_empty()
                && reward_id.split(',').count() == cards.len()
                && reward_id.split(',').all(|uuid| !uuid.is_empty())
        }
        NaturalRunRewardItemV1::Relic { relic_id } => {
            reward_type == "RELIC"
                && action.reward_gold.is_none()
                && canonical_game_id(reward_id) == *relic_id
        }
        NaturalRunRewardItemV1::Potion { potion_id } => {
            reward_type == "POTION"
                && action.reward_gold.is_none()
                && canonical_game_id(reward_id) == *potion_id
        }
        NaturalRunRewardItemV1::EmeraldKey => {
            reward_type == "EMERALD_KEY"
                && action.reward_gold.is_none()
                && reward_id == "EMERALD_KEY"
        }
        NaturalRunRewardItemV1::SapphireKey => {
            let linked_relic_id = index
                .checked_sub(1)
                .and_then(|linked_index| items.get(linked_index))
                .and_then(|linked| match linked {
                    NaturalRunRewardItemV1::Relic { relic_id } => Some(relic_id),
                    _ => None,
                })
                .ok_or_else(|| {
                    "Java Sapphire Key reward is not linked to an immediately preceding relic"
                        .to_owned()
                })?;
            reward_type == "SAPPHIRE_KEY"
                && action.reward_gold.is_none()
                && canonical_game_id(reward_id) == *linked_relic_id
        }
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Java reward action at index {index} disagrees with its typed item"
        ))
    }
}

fn require_no_reward_identity(action: &JavaRuntimeActionV1) -> Result<(), String> {
    if action.reward_type.is_none()
        && action.reward_index.is_none()
        && action.reward_id.is_none()
        && action.reward_gold.is_none()
    {
        Ok(())
    } else {
        Err("Java reward action carries an unexpected item identity".to_owned())
    }
}

fn require_no_potion_identity(action: &JavaRuntimeActionV1) -> Result<(), String> {
    if action.potion_slot.is_none() && action.potion_id.is_none() {
        Ok(())
    } else {
        Err("Java primary run action carries a potion identity".to_owned())
    }
}

fn combat_actions(
    observation: &JavaRuntimeObservationV1,
    runtime_card_bindings: &BTreeMap<String, String>,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let raw_actions = observation.frame["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| "Java combat frame omitted its typed action space".to_owned())?;
    if raw_actions.len() != observation.actions.len() {
        return Err("Java combat frame and live action handles disagree in length".to_owned());
    }
    let mut semantics = BTreeSet::new();
    let mut lifecycle_actions = BTreeSet::new();
    let mut resolved = Vec::with_capacity(raw_actions.len());
    for raw in raw_actions {
        let action_id = required_string(raw, "action_id")?;
        let kind = required_string(raw, "kind")?;
        let live = observation
            .actions
            .iter()
            .filter(|candidate| candidate.action_id == action_id && candidate.kind == kind)
            .collect::<Vec<_>>();
        let [live] = live.as_slice() else {
            return Err(format!(
                "Java combat action {action_id:?} resolves to {} live handles instead of one",
                live.len()
            ));
        };
        require_parameter_free(live)?;
        if is_oracle_lifecycle_action(kind) {
            validate_oracle_lifecycle_action(live, &mut lifecycle_actions)?;
            continue;
        }
        let semantic = match kind {
            "play_card" => {
                let uuid = required_string(raw, "card_uuid")?;
                let binding = runtime_card_bindings.get(uuid).ok_or_else(|| {
                    "Java playable card UUID has no source-proven binding".to_owned()
                })?;
                let card_id = canonical_game_id(required_string(raw, "card_id")?);
                validate_hand_action(observation, raw, uuid, &card_id)?;
                SemanticActionV1::PlayCard {
                    card_binding: binding.clone(),
                    card_id,
                    target: combat_target(observation, raw)?,
                }
            }
            "use_potion" => SemanticActionV1::UsePotion {
                slot: u8::try_from(required_u64(raw, "potion_slot")?)
                    .map_err(|_| "Java potion slot exceeds u8".to_owned())?,
                potion_id: canonical_game_id(required_string(raw, "potion_id")?),
                target: combat_target(observation, raw)?,
            },
            "discard_potion" => {
                require_no_target(raw)?;
                SemanticActionV1::DiscardPotion {
                    slot: u8::try_from(required_u64(raw, "potion_slot")?)
                        .map_err(|_| "Java potion slot exceeds u8".to_owned())?,
                    potion_id: canonical_game_id(required_string(raw, "potion_id")?),
                }
            }
            "end_turn" => {
                require_no_target(raw)?;
                SemanticActionV1::EndTurn
            }
            other => return Err(format!("Java combat action kind {other:?} is unsupported")),
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Java combat action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Java combat action space repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedJavaActionV1 {
            semantic,
            runtime_handle: live.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    if resolved.is_empty() {
        return Err("Java combat player turn exposes no semantic actions".to_owned());
    }
    Ok(resolved)
}

fn is_oracle_lifecycle_action(kind: &str) -> bool {
    matches!(
        kind,
        "run_reset_to_main_menu_preserving_save" | "run_discard_to_main_menu"
    )
}

pub(crate) fn run_decision_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<(Vec<&JavaRuntimeActionV1>, Vec<ResolvedJavaActionV1>), String> {
    let mut lifecycle_actions = BTreeSet::new();
    let mut seen_potion_actions = BTreeSet::new();
    let mut primary_actions = Vec::with_capacity(observation.actions.len());
    let mut potion_actions = Vec::new();
    for action in &observation.actions {
        if is_oracle_lifecycle_action(&action.kind) {
            validate_oracle_lifecycle_action(action, &mut lifecycle_actions)?;
        } else if matches!(action.kind.as_str(), "use_potion" | "discard_potion") {
            let semantic = run_potion_action(observation, action)?;
            let slot = match &semantic {
                SemanticActionV1::UsePotion { slot, .. }
                | SemanticActionV1::DiscardPotion { slot, .. } => *slot,
                _ => unreachable!("run potion resolver returned another action"),
            };
            if !seen_potion_actions.insert((action.kind.as_str(), slot)) {
                return Err("Java run decision repeats one potion action for a slot".to_owned());
            }
            potion_actions.push(ResolvedJavaActionV1 {
                semantic,
                runtime_handle: action.runtime_handle.clone(),
                execution: JavaActionExecutionV1::Act,
            });
        } else {
            primary_actions.push(action);
        }
    }
    Ok((primary_actions, potion_actions))
}

fn validate_oracle_lifecycle_action<'a>(
    action: &'a JavaRuntimeActionV1,
    seen: &mut BTreeSet<&'a str>,
) -> Result<(), String> {
    require_parameter_free(action)?;
    let has_player_identity = action.event_id.is_some()
        || action.option_index.is_some()
        || action.neow_reward_type.is_some()
        || action.neow_drawback.is_some()
        || action.map_x.is_some()
        || action.map_y.is_some()
        || action.map_room_type.is_some()
        || action.reward_type.is_some()
        || action.reward_index.is_some()
        || action.reward_id.is_some()
        || action.reward_gold.is_some()
        || action.potion_slot.is_some()
        || action.potion_id.is_some();
    if action.action_id != action.kind || has_player_identity || !seen.insert(&action.kind) {
        return Err("Java action space has an invalid oracle lifecycle action".to_owned());
    }
    Ok(())
}

fn validate_hand_action(
    observation: &JavaRuntimeObservationV1,
    raw: &serde_json::Value,
    uuid: &str,
    card_id: &str,
) -> Result<(), String> {
    let index = usize::try_from(required_u64(raw, "hand_index")?)
        .map_err(|_| "Java hand index exceeds usize".to_owned())?;
    let card = observation.frame["public_base"]["combat"]["hand"]
        .as_array()
        .and_then(|hand| hand.get(index))
        .ok_or_else(|| "Java combat action points outside the public hand".to_owned())?;
    if card["instance_uuid"].as_str() != Some(uuid)
        || card["card_id"].as_str().map(canonical_game_id).as_deref() != Some(card_id)
    {
        return Err("Java combat action disagrees with its public hand card".to_owned());
    }
    Ok(())
}

fn combat_target(
    observation: &JavaRuntimeObservationV1,
    raw: &serde_json::Value,
) -> Result<Option<SemanticChoiceV1>, String> {
    let (Some(index), Some(instance_id), Some(raw_id)) = (
        raw["target_monster_index"].as_u64(),
        raw["target_monster_instance_id"].as_u64(),
        raw["target_monster_id"].as_str(),
    ) else {
        require_no_target(raw)?;
        return Ok(None);
    };
    let index = usize::try_from(index).map_err(|_| "Java target index exceeds usize".to_owned())?;
    let monsters = observation.frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or_else(|| "Java combat frame omitted its public monsters".to_owned())?;
    let monster = monsters
        .get(index)
        .ok_or_else(|| "Java combat action targets an unknown monster index".to_owned())?;
    let enemy_id = canonical_game_id(raw_id);
    if monster["index"].as_u64() != Some(index as u64)
        || monster["instance_id"].as_u64() != Some(instance_id)
        || monster["id"].as_str().map(canonical_game_id).as_deref() != Some(enemy_id.as_str())
    {
        return Err("Java combat target disagrees with the observed monster".to_owned());
    }
    let occurrence = monsters[..index]
        .iter()
        .filter(|monster| {
            monster["id"]
                .as_str()
                .is_some_and(|id| canonical_game_id(id) == enemy_id)
        })
        .count();
    Ok(Some(SemanticChoiceV1::new(
        "monster",
        enemy_id,
        u32::try_from(occurrence).map_err(|_| "monster occurrence exceeds u32".to_owned())?,
    )))
}

fn require_no_target(raw: &serde_json::Value) -> Result<(), String> {
    if raw["target_monster_index"].is_null()
        && raw["target_monster_instance_id"].is_null()
        && raw["target_monster_id"].is_null()
    {
        Ok(())
    } else {
        Err("Java combat action carries a partial or unexpected target".to_owned())
    }
}

fn required_string<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Java combat action omitted string {field}"))
}

fn required_u64(value: &serde_json::Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .ok_or_else(|| format!("Java combat action omitted unsigned integer {field}"))
}

fn neow_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let screen = observation.frame["public_base"]["decision"]["event_screen_index"]
        .as_i64()
        .ok_or_else(|| "Java Neow decision omitted its screen index".to_owned())?;
    let (primary_actions, mut potion_actions) = run_decision_actions(observation)?;
    let actions = primary_actions
        .iter()
        .copied()
        .filter(|action| {
            action.kind == "choose_event_option" && action.event_id.as_deref() == Some("Neow")
        })
        .collect::<Vec<_>>();
    let expected_count = match screen {
        0 | 1 | 99 => 1,
        3 if matches!(actions.len(), 2 | 4) => actions.len(),
        3 => {
            return Err(format!(
                "Java Neow reward screen exposes {} actions instead of two or four",
                actions.len()
            ))
        }
        other => return Err(format!("Java Neow screen {other} is unsupported")),
    };
    if actions.len() != expected_count || primary_actions.len() != expected_count {
        return Err("Java Neow boundary exposes unrelated or repeated actions".to_owned());
    }

    let mut option_indices = BTreeSet::new();
    let mut semantic_keys = BTreeSet::new();
    let mut resolved = actions
        .into_iter()
        .map(|action| {
            require_parameter_free(action)?;
            require_no_potion_identity(action)?;
            let option = action
                .option_index
                .ok_or_else(|| "Java Neow action omitted its option index".to_owned())?;
            if !option_indices.insert(option) {
                return Err("Java Neow action space repeats an option index".to_owned());
            }
            let semantic = match screen {
                0 | 1 | 99 => {
                    if option != 0
                        || action.neow_reward_type.is_some()
                        || action.neow_drawback.is_some()
                    {
                        return Err("Java Neow proceed action carries choice identity".to_owned());
                    }
                    SemanticActionV1::Proceed {
                        surface: DecisionSurfaceV1::Neow,
                    }
                }
                3 => {
                    let mut attributes = BTreeMap::new();
                    attributes.insert(
                        "drawback".to_owned(),
                        SemanticScalarV1::Text(
                            neow_drawback_id(action.neow_drawback.as_deref().ok_or_else(
                                || "Java Neow choice omitted its drawback".to_owned(),
                            )?)?
                            .to_owned(),
                        ),
                    );
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::Neow,
                        choice: SemanticChoiceV1 {
                            kind: "neow_reward".to_owned(),
                            id: neow_reward_id(action.neow_reward_type.as_deref().ok_or_else(
                                || "Java Neow choice omitted its reward".to_owned(),
                            )?)?
                            .to_owned(),
                            occurrence: 0,
                            attributes,
                        },
                    }
                }
                _ => unreachable!("screen was checked above"),
            };
            let key = serde_json::to_string(&semantic)
                .map_err(|error| format!("failed to encode Java semantic action: {error}"))?;
            if !semantic_keys.insert(key) {
                return Err("Java Neow action space repeats a semantic action".to_owned());
            }
            Ok(ResolvedJavaActionV1 {
                semantic,
                runtime_handle: action.runtime_handle.clone(),
                execution: JavaActionExecutionV1::Act,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    resolved.append(&mut potion_actions);
    Ok(resolved)
}

fn map_actions(
    observation: &JavaRuntimeObservationV1,
) -> Result<Vec<ResolvedJavaActionV1>, String> {
    let mut coordinates = BTreeSet::new();
    let mut saw_close = false;
    let (actions, mut resolved) = run_decision_actions(observation)?;
    for action in actions {
        require_parameter_free(action)?;
        require_no_potion_identity(action)?;
        if action.kind == "close_map" {
            if saw_close
                || action.action_id != "close_map"
                || action.event_id.is_some()
                || action.option_index.is_some()
                || action.neow_reward_type.is_some()
                || action.neow_drawback.is_some()
                || action.map_x.is_some()
                || action.map_y.is_some()
                || action.map_room_type.is_some()
                || require_no_reward_identity(action).is_err()
            {
                return Err("Java Map close action has invalid identity fields".to_owned());
            }
            saw_close = true;
            resolved.push(ResolvedJavaActionV1 {
                semantic: SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::Map,
                },
                runtime_handle: action.runtime_handle.clone(),
                execution: JavaActionExecutionV1::Act,
            });
            continue;
        }
        if action.kind != "choose_map_node" {
            return Err(format!(
                "Java Map boundary exposes unrelated action {:?}",
                action.kind
            ));
        }
        let x = action
            .map_x
            .ok_or_else(|| "Java Map action omitted x".to_owned())?;
        let y = action
            .map_y
            .ok_or_else(|| "Java Map action omitted y".to_owned())?;
        if !coordinates.insert((x, y)) {
            return Err("Java Map action space repeats one coordinate".to_owned());
        }
        if action.action_id != format!("map_node:{x}:{y}") {
            return Err("Java Map action ID disagrees with its coordinates".to_owned());
        }
        let room = java_map_room_id(
            action
                .map_room_type
                .as_deref()
                .ok_or_else(|| "Java Map action omitted room type".to_owned())?,
        )?;
        let mut attributes = BTreeMap::new();
        attributes.insert("x".to_owned(), SemanticScalarV1::Integer(x));
        attributes.insert("y".to_owned(), SemanticScalarV1::Integer(y));
        attributes.insert("room".to_owned(), SemanticScalarV1::Text(room.to_owned()));
        resolved.push(ResolvedJavaActionV1 {
            semantic: SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Map,
                choice: SemanticChoiceV1 {
                    kind: "map_node".to_owned(),
                    id: natural_map_node_id_v1(x, y)?,
                    occurrence: 0,
                    attributes,
                },
            },
            runtime_handle: action.runtime_handle.clone(),
            execution: JavaActionExecutionV1::Act,
        });
    }
    if resolved.is_empty() {
        return Err("Java Map boundary exposes no actions".to_owned());
    }
    Ok(resolved)
}

fn require_parameter_free(action: &JavaRuntimeActionV1) -> Result<(), String> {
    if action.parameters_required {
        Err(format!(
            "Java action {:?} requires unmodeled parameters",
            action.action_id
        ))
    } else if action.runtime_handle.is_empty() || action.action_id.is_empty() {
        Err("Java action omitted its runtime identity".to_owned())
    } else {
        Ok(())
    }
}

pub(crate) fn java_map_room_id(value: &str) -> Result<&'static str, String> {
    match value.rsplit('.').next().unwrap_or(value) {
        "EventRoom" => Ok("question_mark"),
        "MonsterRoom" => Ok("monster"),
        "MonsterRoomElite" => Ok("elite"),
        "MonsterRoomBoss" => Ok("boss"),
        "RestRoom" => Ok("rest"),
        "ShopRoom" => Ok("shop"),
        "TreasureRoom" => Ok("treasure"),
        "TrueVictoryRoom" => Ok("victory"),
        other => Err(format!("Java map room type {other:?} is unsupported")),
    }
}

fn neow_reward_id(value: &str) -> Result<&'static str, String> {
    match value {
        "THREE_ENEMY_KILL" => Ok("three_enemy_kill"),
        "TEN_PERCENT_HP_BONUS" => Ok("ten_percent_hp_bonus"),
        "THREE_CARDS" => Ok("three_cards"),
        "ONE_RANDOM_RARE_CARD" => Ok("one_random_rare_card"),
        "REMOVE_CARD" => Ok("remove_card"),
        "UPGRADE_CARD" => Ok("upgrade_card"),
        "TRANSFORM_CARD" => Ok("transform_card"),
        "RANDOM_COLORLESS" => Ok("random_colorless"),
        "THREE_SMALL_POTIONS" => Ok("three_small_potions"),
        "RANDOM_COMMON_RELIC" => Ok("random_common_relic"),
        "HUNDRED_GOLD" => Ok("hundred_gold"),
        "RANDOM_COLORLESS_2" => Ok("random_colorless_2"),
        "REMOVE_TWO" => Ok("remove_two"),
        "ONE_RARE_RELIC" => Ok("one_rare_relic"),
        "THREE_RARE_CARDS" => Ok("three_rare_cards"),
        "TWO_FIFTY_GOLD" => Ok("two_fifty_gold"),
        "TRANSFORM_TWO_CARDS" => Ok("transform_two_cards"),
        "TWENTY_PERCENT_HP_BONUS" => Ok("twenty_percent_hp_bonus"),
        "BOSS_RELIC" => Ok("boss_relic"),
        other => Err(format!("Java Neow reward {other:?} is unsupported")),
    }
}

fn neow_drawback_id(value: &str) -> Result<&'static str, String> {
    match value {
        "NONE" => Ok("none"),
        "TEN_PERCENT_HP_LOSS" => Ok("ten_percent_hp_loss"),
        "NO_GOLD" => Ok("no_gold"),
        "CURSE" => Ok("curse"),
        "PERCENT_DAMAGE" => Ok("percent_damage"),
        other => Err(format!("Java Neow drawback {other:?} is unsupported")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cage_grid_preserves_duplicate_card_bindings_and_requires_complete_pairs() {
        use serde_json::json;
        let uuids = [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
            "33333333-3333-3333-3333-333333333333",
        ];
        let grid_id = format!(
            "grid_select:none:MasterDeck:EmptyCage:2:false:false:{}:selected:",
            uuids.join(",")
        );
        let action_id = format!("submit_grid_select:{grid_id}");
        let candidates = uuids.iter().enumerate().map(|(index,uuid)| json!({"index":index,"card_uuid":uuid,"card_id":"Strike_G","upgrades":0})).collect::<Vec<_>>();
        let observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 { implementation:"fixture".into(),version:"1".into(),session_epoch:"empty-cage".into() },
            frame_id:1,
            frame:json!({"boundary":{"kind":"run_decision"},
                "public_base":{"decision":{"kind":"grid_select","event_id":null,"grid_select":{
                    "grid_select_id":grid_id,"source_pile":"MasterDeck","selection_reason":"EmptyCage",
                    "requested_cards":2,"any_number":false,"can_cancel":false,"allowed_counts":[2],
                    "confirm_supported":false,"cancel_supported":false,"selected_cards":[],"candidate_cards":candidates}}},
                "parity_internal":{"master_deck":candidates,"relics":[{"relic_id":"Empty Cage"}]},
                "action_space":{"actions":[{"action_id":action_id,"kind":"submit_grid_select","grid_select_id":grid_id,
                    "source_pile":"MasterDeck","selection_reason":"EmptyCage","event_id":null,
                    "parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,"unique_items":true,"allowed_counts":[2],"allowed_card_uuids":uuids}}]}}),
            actions:vec![serde_json::from_value(json!({"runtime_handle":"f1:a0","action_id":action_id,"kind":"submit_grid_select","parameters_required":true})).unwrap()],
        };
        let bindings = uuids
            .iter()
            .enumerate()
            .map(|(i, u)| (u.to_string(), format!("natural_run/starter/{i}")))
            .collect();
        let actions = resolved_actions_v1(&observation, Some(&bindings), None).unwrap();
        assert_eq!(actions.len(), 6);
        assert!(actions.iter().all(|a|matches!(&a.semantic,SemanticActionV1::SubmitSelection {choices,..} if choices.len()==2 && choices[0].id!=choices[1].id)));
        assert!(resolved_actions_v1(&observation, Some(&BTreeMap::new()), None).is_err());
        for fault in 0..3 {
            let mut invalid = observation.clone();
            match fault {
                0 => invalid.frame["parity_internal"]["relics"] = json!([]),
                1 => {
                    invalid.frame["public_base"]["decision"]["grid_select"]["allowed_counts"] =
                        json!([1])
                }
                _ => invalid.frame["public_base"]["decision"]["event_id"] = json!("Neow"),
            }
            assert!(resolved_actions_v1(&invalid, Some(&bindings), None).is_err());
        }
    }

    #[test]
    fn treasure_requires_distinct_open_and_proceed_actions() {
        let observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "fixture".into(),
                version: "1".into(),
                session_epoch: "treasure".into(),
            },
            frame_id: 1,
            frame: serde_json::json!({
                "boundary": {"kind": "run_decision"},
                "public_base": {"decision": {"kind": "treasure"}},
                "parity_internal": {"potions": [
                    {"slot_index": 0, "empty": false, "potion_id": "LiquidBronze"}
                ]}
            }),
            actions: serde_json::from_value(serde_json::json!([
                {"runtime_handle": "open", "action_id": "open_chest", "kind": "open_chest", "parameters_required": false},
                {"runtime_handle": "leave", "action_id": "proceed_treasure", "kind": "proceed_treasure", "parameters_required": false},
                {"runtime_handle": "discard", "action_id": "discard_potion:0:LiquidBronze", "kind": "discard_potion", "parameters_required": false, "potion_slot": 0, "potion_id": "LiquidBronze"}
            ])).unwrap(),
        };
        let actions = resolved_actions_v1(&observation, None, None).unwrap();
        assert_eq!(actions.len(), 3);
        for (handle, semantic) in [
            (
                "open",
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Treasure,
                    choice: SemanticChoiceV1::new("treasure_chest", "open", 0),
                },
            ),
            (
                "leave",
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Treasure,
                },
            ),
            (
                "discard",
                SemanticActionV1::DiscardPotion {
                    slot: 0,
                    potion_id: "liquidbronze".into(),
                },
            ),
        ] {
            assert!(actions
                .iter()
                .any(|action| action.runtime_handle == handle && action.semantic == semantic));
        }
        for index in 0..2 {
            for case in ["missing", "duplicate", "id", "parameters", "reward"] {
                let mut invalid = observation.clone();
                match case {
                    "missing" => {
                        invalid.actions.remove(index);
                    }
                    "duplicate" => invalid.actions.push(invalid.actions[index].clone()),
                    "id" => invalid.actions[index].action_id = "proceed".into(),
                    "parameters" => invalid.actions[index].parameters_required = true,
                    "reward" => invalid.actions[index].reward_id = Some("Anchor".into()),
                    _ => unreachable!(),
                }
                assert!(
                    resolved_actions_v1(&invalid, None, None).is_err(),
                    "{index} {case}"
                );
            }
        }
    }

    #[test]
    fn run_potion_use_requires_the_observed_usable_untargeted_slot() {
        for potion_id in ["EntropicBrew", "Fruit Juice", "BloodPotion"] {
            let mut observation = JavaRuntimeObservationV1 {
                producer: ProducerIdentityV1 { implementation: "fixture".into(), version:"1".into(), session_epoch:"run-potion".into() },
                frame_id: 1,
                frame: serde_json::json!({"parity_internal":{"potions":[{
                    "slot_index":0, "potion_slot":0, "empty":false, "potion_id":potion_id,
                    "can_use":true, "target_required":false
                }]}}),
                actions: serde_json::from_value(serde_json::json!([
                    {"runtime_handle":"f1:a0", "kind":"close_combat_reward", "action_id":"close_combat_reward", "parameters_required":false},
                    {"runtime_handle":"f1:a1", "kind":"use_potion", "action_id":format!("use_potion:0:{potion_id}"), "parameters_required":false, "potion_slot":0, "potion_id":potion_id},
                    {"runtime_handle":"f1:a2", "kind":"discard_potion", "action_id":format!("discard_potion:0:{potion_id}"), "parameters_required":false, "potion_slot":0, "potion_id":potion_id}
                ])).unwrap(),
            };
            let (primary, actions) = run_decision_actions(&observation).unwrap();
            assert_eq!(primary.len(), 1);
            assert_eq!(actions.len(), 2);
            assert!(actions.iter().any(|action| action.semantic
                == SemanticActionV1::UsePotion {
                    slot: 0,
                    potion_id: canonical_game_id(potion_id),
                    target: None
                }));
            for (field, value) in [
                ("can_use", serde_json::json!(false)),
                ("target_required", serde_json::json!(true)),
                ("potion_slot", serde_json::json!(1)),
                ("potion_id", serde_json::json!("Fire Potion")),
                ("empty", serde_json::json!(true)),
            ] {
                let mut invalid = observation.clone();
                invalid.frame["parity_internal"]["potions"][0][field] = value;
                assert!(run_decision_actions(&invalid).is_err(), "{field}");
            }
            observation.actions.push(observation.actions[1].clone());
            assert!(run_decision_actions(&observation).is_err());
        }
    }

    #[test]
    fn boss_completion_requires_one_unambiguous_proceed_and_preserves_potion_discards() {
        for (kind, action_id, surface) in [
            (
                "double_boss_complete",
                "proceed_double_boss",
                DecisionSurfaceV1::DoubleBoss,
            ),
            (
                "act3_victory_complete",
                "proceed_act3_victory",
                DecisionSurfaceV1::Act3Victory,
            ),
            (
                "true_victory_complete",
                "proceed_true_victory",
                DecisionSurfaceV1::TrueVictory,
            ),
        ] {
            let observation = JavaRuntimeObservationV1 {
                producer: ProducerIdentityV1 {
                    implementation: "fixture".to_owned(),
                    version: "1".to_owned(),
                    session_epoch: "double-boss".to_owned(),
                },
                frame_id: 1,
                frame: serde_json::json!({
                    "boundary": {"kind": "run_decision"},
                    "public_base": {"decision": {"kind": kind}},
                    "parity_internal": {"potions": [
                        {"slot_index": 0, "empty": false, "potion_id": "LiquidBronze"}
                    ]}
                }),
                actions: serde_json::from_value(serde_json::json!([
                    {"runtime_handle": "f1:a0", "action_id": action_id,
                     "kind": action_id, "parameters_required": false},
                    {"runtime_handle": "f1:a1", "action_id": "discard_potion:0:LiquidBronze",
                     "kind": "discard_potion", "parameters_required": false,
                     "potion_slot": 0, "potion_id": "LiquidBronze"}
                ]))
                .unwrap(),
            };
            let actions = resolved_actions_v1(&observation, None, None).unwrap();
            assert_eq!(actions.len(), 2);
            assert!(actions
                .iter()
                .any(|action| action.semantic == SemanticActionV1::Proceed { surface }));
            assert!(actions.iter().any(|action| action.semantic
                == SemanticActionV1::DiscardPotion {
                    slot: 0,
                    potion_id: "liquidbronze".to_owned(),
                }));
            for case in [
                "missing",
                "duplicate",
                "kind",
                "id",
                "parameters",
                "event",
                "reward",
                "potion",
            ] {
                let mut invalid = observation.clone();
                match case {
                    "missing" => {
                        invalid.actions.remove(0);
                    }
                    "duplicate" => invalid.actions.push(invalid.actions[0].clone()),
                    "kind" => invalid.actions[0].kind = "proceed".to_owned(),
                    "id" => invalid.actions[0].action_id = "proceed".to_owned(),
                    "parameters" => invalid.actions[0].parameters_required = true,
                    "event" => invalid.actions[0].event_id = Some("Neow".to_owned()),
                    "reward" => invalid.actions[0].reward_id = Some("Black Blood".to_owned()),
                    "potion" => invalid.actions[0].potion_slot = Some(0),
                    _ => unreachable!(),
                }
                assert!(resolved_actions_v1(&invalid, None, None).is_err(), "{case}");
            }
        }
    }
    use sts_parity_lockstep::ProducerIdentityV1;

    #[test]
    fn boss_relic_actions_require_complete_typed_offers_and_keep_skip_distinct() {
        let actions = ["Pandora's Box", "Black Blood", "SacredBark"].iter().enumerate().map(|(index, id)| {
            serde_json::from_value(serde_json::json!({
                "runtime_handle": format!("f1:a{index}"), "action_id": format!("boss_relic:{index}:{id}"),
                "kind": "choose_boss_relic", "reward_type": "BOSS_RELIC", "reward_index": index,
                "reward_id": id, "parameters_required": false,
            })).unwrap()
        }).chain(std::iter::once(serde_json::from_value(serde_json::json!({
            "runtime_handle": "f1:a3", "action_id": "skip_boss_relic", "kind": "skip_boss_relic",
            "reward_type": "BOSS_RELIC", "parameters_required": false,
        })).unwrap())).collect();
        let observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "test".to_owned(),
                session_epoch: "test".to_owned(),
            },
            frame_id: 1,
            frame: serde_json::json!({"public_base": {"decision": {"kind": "boss_relic"}}}),
            actions,
        };
        assert_eq!(
            boss_relic_selection(&observation).unwrap(),
            sts_parity_lockstep::NaturalBossRelicChoiceV1 {
                relics: vec![
                    "pandorasbox".to_owned(),
                    "blackblood".to_owned(),
                    "sacredbark".to_owned()
                ],
                can_skip: true,
            }
        );
        let resolved = boss_relic_actions(&observation).unwrap();
        assert_eq!(resolved.len(), 4);
        assert_eq!(
            resolved[1].semantic,
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::BossRelic,
                choice: sts_parity_lockstep::natural_boss_relic_choice_v1("blackblood", 0)
            }
        );
        assert_eq!(
            resolved[3].semantic,
            SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::BossRelic
            }
        );
        let mut with_potion = observation.clone();
        with_potion.frame["parity_internal"] = serde_json::json!({
            "potions": [{"slot_index": 1, "empty": false, "potion_id": "LiquidBronze"}]
        });
        with_potion.actions.push(
            serde_json::from_value(serde_json::json!({
                "runtime_handle": "f1:a4", "action_id": "discard_potion:1:LiquidBronze",
                "kind": "discard_potion", "potion_slot": 1, "potion_id": "LiquidBronze",
                "parameters_required": false,
            }))
            .unwrap(),
        );
        assert_eq!(
            boss_relic_selection(&with_potion).unwrap(),
            boss_relic_selection(&observation).unwrap()
        );
        let with_discards = boss_relic_actions(&with_potion).unwrap();
        assert_eq!(with_discards.len(), 5);
        assert!(with_discards.iter().any(|action| action.semantic
            == SemanticActionV1::DiscardPotion {
                slot: 1,
                potion_id: "liquidbronze".to_owned(),
            }));
        with_potion.frame["parity_internal"]["potions"][0]["empty"] = serde_json::json!(true);
        assert!(boss_relic_actions(&with_potion).is_err());
        for field in ["id", "index", "missing"] {
            let mut invalid = observation.clone();
            match field {
                "id" => invalid.actions[0].reward_id = Some("Black Blood".to_owned()),
                "index" => invalid.actions[0].reward_index = Some(1),
                _ => {
                    invalid.actions.remove(0);
                }
            }
            assert!(boss_relic_actions(&invalid).is_err());
        }
    }

    #[test]
    fn library_grid_resolves_typed_offers_without_master_deck_bindings() {
        use serde_json::json;
        let ids = [
            "Shockwave",
            "Thunderclap",
            "Entrench",
            "Carnage",
            "Armaments",
            "Cleave",
            "Battle Trance",
            "Clothesline",
            "Perfected Strike",
            "Heavy Blade",
            "Anger",
            "Inflame",
            "Fire Breathing",
            "Wild Strike",
            "Twin Strike",
            "Flex",
            "Combust",
            "Havoc",
            "Sword Boomerang",
            "True Grit",
        ];
        let uuids = (0..20)
            .map(|i| format!("00000000-0000-0000-0000-{i:012}"))
            .collect::<Vec<_>>();
        let grid_id = format!(
            "grid_select:TheLibrary:EventOffer:TheLibrary:1:false:false:{}:selected:",
            uuids.join(",")
        );
        let action_id = format!("submit_grid_select:{grid_id}");
        let candidates = ids
            .iter()
            .enumerate()
            .map(|(i, id)| json!({"index":i,"card_id":id,"upgrades":0,"card_uuid":uuids[i]}))
            .collect::<Vec<_>>();
        let observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 { implementation:"fixture".into(),version:"1".into(),session_epoch:"library".into() }, frame_id:1,
            frame: json!({
                "boundary":{"kind":"run_decision"},
                "public_base":{"decision":{"kind":"grid_select","event_id":"TheLibrary","event_screen_index":1,
                    "event_identity":{"status":"supported","event_id":"TheLibrary","runtime_class":"com.megacrit.cardcrawl.events.city.TheLibrary"},
                    "grid_select":{"grid_select_id":grid_id,"source_pile":"EventOffer","selection_reason":"TheLibrary","requested_cards":1,"any_number":false,"can_cancel":false,"allowed_counts":[1],"confirm_supported":false,"cancel_supported":false,"selected_cards":[],"candidate_cards":candidates}}},
                "parity_internal":{"master_deck":[{"card_id":"Shockwave","upgrades":0,"card_uuid":"existing"}]},
                "action_space":{"actions":[{"action_id":action_id,"kind":"submit_grid_select","grid_select_id":grid_id,"source_pile":"EventOffer","selection_reason":"TheLibrary","event_id":"TheLibrary","parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,"unique_items":true,"allowed_counts":[1],"allowed_card_uuids":uuids}}]}
            }),
            actions:vec![serde_json::from_value(json!({"runtime_handle":"pick","action_id":action_id,"kind":"submit_grid_select","parameters_required":true,"event_id":"TheLibrary"})).unwrap()],
        };
        let actions = resolved_actions_v1(&observation, Some(&BTreeMap::new()), None).unwrap();
        assert_eq!(actions.len(), 20);
        for (i, action) in actions.iter().enumerate() {
            assert_eq!(
                action.semantic,
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::GridSelect,
                    choices: vec![sts_parity_lockstep::natural_event_card_offer_v1(
                        "thelibrary",
                        1,
                        i as u32,
                        &canonical_game_id(ids[i]),
                        0
                    )
                    .unwrap()]
                }
            );
            assert_eq!(
                action.execution,
                JavaActionExecutionV1::SubmitGridSelect {
                    selected_card_uuids: vec![uuids[i].clone()]
                }
            );
        }
        for fault in 0..7 {
            let mut invalid = observation.clone();
            match fault {
                0 => {
                    invalid.frame["public_base"]["decision"]["grid_select"]["source_pile"] =
                        json!("MasterDeck")
                }
                1 => invalid.frame["public_base"]["decision"]["event_screen_index"] = json!(0),
                2 => {
                    invalid.frame["public_base"]["decision"]["grid_select"]["candidate_cards"][1]
                        ["card_id"] = json!(ids[0])
                }
                3 => {
                    invalid.frame["public_base"]["decision"]["grid_select"]["candidate_cards"][0]
                        ["index"] = json!(1)
                }
                4 => {
                    invalid.frame["parity_internal"]["master_deck"][0]["card_uuid"] =
                        json!(uuids[0])
                }
                5 => {
                    invalid.frame["public_base"]["decision"]["grid_select"]["candidate_cards"]
                        .as_array_mut()
                        .unwrap()
                        .pop();
                }
                _ => {
                    invalid.frame["action_space"]["actions"][0]["parameter_schema"]
                        ["allowed_card_uuids"][0] = json!(uuids[1])
                }
            }
            assert!(
                resolved_actions_v1(&invalid, Some(&BTreeMap::new()), None).is_err(),
                "fault {fault}"
            );
        }
    }

    #[test]
    fn scry_uses_original_peek_order_and_checks_none_all_and_raw_count_contracts() {
        use serde_json::json;
        let uuids = ["scry-top", "scry-second", "scry-third"];
        let grid_id = format!(
            "grid_select:none:Draw:Scry:3:true:false:{}:selected:",
            uuids.join(",")
        );
        let action_id = format!("submit_grid_select:{grid_id}");
        let candidates = uuids.iter().enumerate().map(|(index,uuid)|
            json!({"index":index,"card_uuid":uuid,"card_id":"Strike_P","upgrades":0})).collect::<Vec<_>>();
        let cards = uuids
            .iter()
            .rev()
            .map(|uuid| json!({"instance_uuid":uuid,"card_id":"Strike_P","upgrades":0}))
            .collect::<Vec<_>>();
        let mut frame = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "constructed".into(),
                version: "1".into(),
                session_epoch: "scry-projection".into(),
            },
            frame_id: 1,
            frame: json!({
                "boundary":{"kind":"run_decision"},
                "public_base":{"decision":{"kind":"grid_select","event_id":null,"grid_select":{
                    "grid_select_id":grid_id,"source_pile":"Draw","selection_reason":"Scry",
                    "requested_cards":3,"any_number":true,"can_cancel":false,"allowed_counts":[0,1,2,3],
                    "confirm_supported":false,"cancel_supported":false,"selected_cards":[],"candidate_cards":candidates}}},
                "parity_internal":{"zones":{"draw":{"cards":cards}}},
                "action_space":{"actions":[{"action_id":action_id,"kind":"submit_grid_select",
                    "grid_select_id":grid_id,"source_pile":"Draw","selection_reason":"Scry","event_id":null,
                    "parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,
                        "unique_items":true,"allowed_counts":[0,1,2,3],"allowed_card_uuids":uuids}}]},
            }),
            actions: vec![serde_json::from_value(
                json!({"runtime_handle":"scry:a0","action_id":action_id,
                "kind":"submit_grid_select","parameters_required":true}),
            )
            .unwrap()],
        };
        let bindings = uuids
            .iter()
            .enumerate()
            .map(|(index, uuid)| (uuid.to_string(), format!("natural_run/starter/{index}")))
            .collect::<BTreeMap<_, _>>();
        let actions = resolved_actions_v1(&frame, Some(&bindings), None).unwrap();
        assert_eq!(actions.len(), 8);
        assert_eq!(
            actions[0].execution,
            JavaActionExecutionV1::SubmitGridSelect {
                selected_card_uuids: vec![]
            }
        );
        assert_eq!(
            actions[7].execution,
            JavaActionExecutionV1::SubmitGridSelect {
                selected_card_uuids: uuids.iter().map(|uuid| uuid.to_string()).collect()
            }
        );
        assert!(resolved_actions_v1(&frame, Some(&BTreeMap::new()), None).is_err());
        frame.frame["parity_internal"]["zones"]["draw"]["cards"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert!(resolved_actions_v1(&frame, Some(&bindings), None).is_err());
        frame.frame["parity_internal"]["zones"]["draw"]["cards"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        frame.frame["action_space"]["actions"][0]["parameter_schema"]["allowed_counts"] =
            json!([1]);
        assert!(resolved_actions_v1(&frame, Some(&bindings), None).is_err());
    }

    #[test]
    fn exhume_grid_keeps_duplicate_card_bindings_and_checks_the_raw_submission() {
        use serde_json::json;
        let uuids = [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
        ];
        let grid_id = format!(
            "grid_select:none:Exhaust:Exhume:1:false:false:{}:selected:",
            uuids.join(",")
        );
        let action_id = format!("submit_grid_select:{grid_id}");
        let candidates = uuids
            .iter()
            .enumerate()
            .map(|(index, uuid)| {
                json!({
                    "index":index,"card_uuid":uuid,"card_id":"Impervious","upgrades":0,
                })
            })
            .collect::<Vec<_>>();
        let cards = uuids
            .iter()
            .map(|uuid| json!({"instance_uuid":uuid,"card_id":"Impervious","upgrades":0}))
            .collect::<Vec<_>>();
        let observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "fixture".to_owned(),
                version: "1".to_owned(),
                session_epoch: "exhume".to_owned(),
            },
            frame_id: 1,
            frame: json!({
                "boundary":{"kind":"run_decision"},
                "public_base":{"decision":{"kind":"grid_select", "event_id":null,
                    "grid_select":{"grid_select_id":grid_id,"source_pile":"Exhaust","selection_reason":"Exhume",
                    "requested_cards":1,"any_number":false,"can_cancel":false,"allowed_counts":[1],
                    "confirm_supported":false,"cancel_supported":false,"selected_cards":[],"candidate_cards":candidates}}},
                "parity_internal":{"zones":{"exhaust":{"cards":cards}}},
                "action_space":{"actions":[{"action_id":action_id,"kind":"submit_grid_select",
                    "grid_select_id":grid_id,"source_pile":"Exhaust","selection_reason":"Exhume","event_id":null,
                    "parameter_schema":{"field":"selected_card_uuids","item_format":"uuid","ordered":true,
                        "unique_items":true,"allowed_counts":[1],"allowed_card_uuids":uuids}}]},
            }),
            actions: vec![serde_json::from_value(
                json!({"runtime_handle":"f1:a0","action_id":action_id,
                "kind":"submit_grid_select","parameters_required":true}),
            )
            .unwrap()],
        };
        let bindings = BTreeMap::from([
            (uuids[0].to_owned(), "natural_run/starter/0".to_owned()),
            (uuids[1].to_owned(), "natural_run/starter/1".to_owned()),
        ]);
        let actions = resolved_actions_v1(&observation, Some(&bindings), None).unwrap();
        assert_eq!(actions.len(), 2);
        for (index, action) in actions.iter().enumerate() {
            assert!(
                matches!(&action.semantic, SemanticActionV1::SubmitSelection {
                surface:DecisionSurfaceV1::GridSelect, choices,
            } if choices[0].kind == "exhaust_card" && choices[0].id == bindings[uuids[index]])
            );
            assert_eq!(
                action.execution,
                JavaActionExecutionV1::SubmitGridSelect {
                    selected_card_uuids: vec![uuids[index].to_owned()]
                }
            );
        }
        assert!(resolved_actions_v1(&observation, Some(&BTreeMap::new()), None).is_err());
        for (reason, relic) in [
            ("BottleFlame", "Bottled Flame"),
            ("BottleLightning", "Bottled Lightning"),
            ("BottleTornado", "Bottled Tornado"),
        ] {
            let mut bottle = observation.clone();
            let id = format!(
                "grid_select:none:MasterDeck:{reason}:1:false:false:{}:selected:",
                uuids.join(",")
            );
            let grid = &mut bottle.frame["public_base"]["decision"]["grid_select"];
            grid["grid_select_id"] = json!(id);
            grid["source_pile"] = json!("MasterDeck");
            grid["selection_reason"] = json!(reason);
            let raw = &mut bottle.frame["action_space"]["actions"][0];
            raw["grid_select_id"] = json!(id);
            raw["source_pile"] = json!("MasterDeck");
            raw["selection_reason"] = json!(reason);
            let action_id = format!("submit_grid_select:{id}");
            raw["action_id"] = json!(action_id);
            bottle.actions[0].action_id = action_id;
            bottle.frame["parity_internal"]["master_deck"] =
                bottle.frame["public_base"]["decision"]["grid_select"]["candidate_cards"].clone();
            bottle.frame["parity_internal"]["relics"] =
                json!([{"relic_id":relic,"bound_card_uuid":null}]);
            let actions = resolved_actions_v1(&bottle, Some(&bindings), None).unwrap();
            assert_eq!(actions.len(), 2);
            assert_ne!(actions[0].semantic, actions[1].semantic);
            assert!(actions.iter().all(|a| matches!(&a.semantic, SemanticActionV1::SubmitSelection {choices,..} if choices[0].kind=="deck_card")));
            assert!(resolved_actions_v1(&bottle, Some(&BTreeMap::new()), None).is_err());
            for fault in 0..3 {
                let mut invalid = bottle.clone();
                match fault {
                    0 => invalid.frame["parity_internal"]["relics"] = json!([]),
                    1 => {
                        invalid.frame["parity_internal"]["relics"][0]["bound_card_uuid"] =
                            json!(uuids[0])
                    }
                    _ => invalid.frame["public_base"]["decision"]["event_id"] = json!("unrelated"),
                }
                assert!(resolved_actions_v1(&invalid, Some(&bindings), None).is_err());
            }
        }
        for (source, zone, reason, card_id, choice_kind) in [
            ("Draw", "draw", "DrawPileToHand", "Strike", "draw_card"),
            (
                "Draw",
                "draw",
                "SkillFromDeckToHand",
                "Impervious",
                "draw_card",
            ),
            (
                "Draw",
                "draw",
                "AttackFromDeckToHand",
                "Strike",
                "draw_card",
            ),
            (
                "Discard",
                "discard",
                "DiscardToHand",
                "Strike",
                "discard_card",
            ),
            (
                "Discard",
                "discard",
                "DiscardToHandRetain",
                "Strike",
                "discard_card",
            ),
            (
                "Discard",
                "discard",
                "DiscardToHandNoCostChange",
                "Strike",
                "discard_card",
            ),
            (
                "Discard",
                "discard",
                "MoveToDrawPile",
                "Strike",
                "discard_card",
            ),
        ] {
            let mut draw = observation.clone();
            let id = format!(
                "grid_select:none:{source}:{reason}:1:false:false:{}:selected:",
                uuids.join(",")
            );
            for surface in ["grid", "action"] {
                let raw = if surface == "grid" {
                    &mut draw.frame["public_base"]["decision"]["grid_select"]
                } else {
                    &mut draw.frame["action_space"]["actions"][0]
                };
                raw["grid_select_id"] = json!(id);
                raw["source_pile"] = json!(source);
                raw["selection_reason"] = json!(reason);
            }
            let action_id = format!("submit_grid_select:{id}");
            draw.actions[0].action_id = action_id.clone();
            draw.frame["action_space"]["actions"][0]["action_id"] = json!(action_id);
            draw.frame["parity_internal"]["zones"][zone] =
                draw.frame["parity_internal"]["zones"]["exhaust"].clone();
            for index in 0..2 {
                draw.frame["public_base"]["decision"]["grid_select"]["candidate_cards"][index]
                    ["card_id"] = json!(card_id);
                draw.frame["parity_internal"]["zones"][zone]["cards"][index]["card_id"] =
                    json!(card_id);
            }
            let actions = resolved_actions_v1(&draw, Some(&bindings), None).unwrap();
            assert_eq!(actions.len(), 2);
            assert!(
                actions.iter().all(|action| matches!(&action.semantic,
                SemanticActionV1::SubmitSelection {choices, ..} if choices[0].kind == choice_kind))
            );
            draw.frame["parity_internal"]["zones"][zone]["cards"]
                .as_array_mut()
                .unwrap()
                .remove(0);
            assert!(resolved_actions_v1(&draw, Some(&bindings), None).is_err());
        }
        let mut invalid = observation.clone();
        invalid.frame["action_space"]["actions"][0]["parameter_schema"]["allowed_card_uuids"] =
            json!([uuids[0]]);
        assert!(resolved_actions_v1(&invalid, Some(&bindings), None).is_err());
        let mut invalid = observation;
        invalid.frame["parity_internal"]["zones"]["exhaust"]["cards"][0]["card_id"] =
            json!("Exhume");
        assert!(resolved_actions_v1(&invalid, Some(&bindings), None).is_err());
    }

    #[test]
    fn hand_select_maps_uuid_to_bound_card_and_keeps_raw_submit_transport() {
        let uuid = "11111111-1111-4111-8111-111111111111";
        let hand_select_id = format!("hand_select:1:false:false:false:{uuid}:selected:");
        let raw_action = serde_json::json!({
            "action_id": format!("submit_hand_select:{hand_select_id}"),
            "kind": "submit_hand_select",
            "hand_select_id": hand_select_id,
            "parameter_schema": {
                "field": "selected_card_uuids",
                "item_format": "uuid",
                "ordered": true,
                "unique_items": true,
                "allowed_card_uuids": [uuid],
                "allowed_counts": [1],
            },
        });
        let observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "communicationmod".to_owned(),
                version: "test".to_owned(),
                session_epoch: "test".to_owned(),
            },
            frame_id: 9,
            frame: serde_json::json!({
                "boundary": {"kind": "run_decision", "stable": true},
                "public_base": {
                    "decision": {
                        "kind": "hand_select",
                        "hand_select": {
                            "hand_select_id": hand_select_id,
                            "selection_reason": "localized text is not identity",
                            "requested_cards": 1,
                            "any_number": false,
                            "can_pick_zero": false,
                            "up_to": false,
                            "allowed_counts": [1],
                            "confirm_supported": true,
                            "cancel_supported": false,
                            "candidate_cards": [{
                                "index": 0,
                                "card_uuid": uuid,
                                "card_id": "Defend_R",
                                "name": "Defend",
                                "upgrades": 0,
                                "cost_for_turn": 1,
                            }],
                            "selected_cards": [],
                        },
                    },
                },
                "parity_internal": {
                    "zones": {
                        "hand": {"cards": [{
                            "instance_uuid": uuid,
                            "card_id": "Defend_R",
                            "upgrades": 0,
                            "cost_for_turn": 1,
                        }]},
                    },
                },
                "action_space": {"actions": [raw_action]},
            }),
            actions: vec![JavaRuntimeActionV1 {
                runtime_handle: "f9:a0".to_owned(),
                action_id: format!("submit_hand_select:{hand_select_id}"),
                kind: "submit_hand_select".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: None,
                potion_id: None,
                parameters_required: true,
            }],
        };
        let bindings = BTreeMap::from([(uuid.to_owned(), "natural_run/starter/5".to_owned())]);

        let resolved = resolved_actions_v1(
            &observation,
            Some(&bindings),
            Some(JavaHandSelectProfileV1::SingleExhaust),
        )
        .unwrap();

        assert_eq!(resolved.len(), 1);
        assert!(matches!(
            &resolved[0].semantic,
            SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::HandSelect,
                choices,
            } if choices[0].kind == "hand_card"
                && choices[0].id == "natural_run/starter/5"
        ));
        assert_eq!(
            resolved[0].execution,
            JavaActionExecutionV1::SubmitHandSelect {
                selected_card_uuids: vec![uuid.to_owned()],
            }
        );
        let put_actions = resolved_actions_v1(
            &observation,
            Some(&bindings),
            Some(JavaHandSelectProfileV1::SinglePutOnDrawPile),
        )
        .unwrap();
        assert_eq!(put_actions[0].semantic, resolved[0].semantic);
        assert_eq!(put_actions[0].execution, resolved[0].execution);
        assert!(resolved_actions_v1(&observation, Some(&bindings), None).is_err());
        for (field, value) in [
            ("requested_cards", serde_json::json!(2)),
            ("any_number", serde_json::json!(true)),
        ] {
            let mut invalid = observation.clone();
            invalid.frame["public_base"]["decision"]["hand_select"][field] = value;
            assert!(resolved_actions_v1(
                &invalid,
                Some(&bindings),
                Some(JavaHandSelectProfileV1::SinglePutOnDrawPile)
            )
            .is_err());
        }
    }

    #[test]
    fn optional_potion_hand_select_maps_every_canonical_card_set() {
        let first = "11111111-1111-4111-8111-111111111111";
        let second = "22222222-2222-4222-8222-222222222222";
        let hand_select_id = format!("hand_select:99:true:true:false:{first},{second}:selected:");
        let raw_action = serde_json::json!({
            "action_id": format!("submit_hand_select:{hand_select_id}"),
            "kind": "submit_hand_select",
            "hand_select_id": hand_select_id,
            "parameter_schema": {
                "field": "selected_card_uuids",
                "item_format": "uuid",
                "ordered": true,
                "unique_items": true,
                "allowed_card_uuids": [first, second],
                "allowed_counts": [0, 1, 2],
            },
        });
        let cards = serde_json::json!([{
            "index": 0,
            "card_uuid": first,
            "card_id": "True Grit",
            "name": "True Grit",
            "upgrades": 1,
            "cost_for_turn": 1,
        }, {
            "index": 1,
            "card_uuid": second,
            "card_id": "Defend_R",
            "name": "Defend",
            "upgrades": 0,
            "cost_for_turn": 1,
        }]);
        let observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "communicationmod".to_owned(),
                version: "test".to_owned(),
                session_epoch: "test".to_owned(),
            },
            frame_id: 10,
            frame: serde_json::json!({
                "boundary": {"kind": "run_decision", "stable": true},
                "public_base": {
                    "decision": {
                        "kind": "hand_select",
                        "hand_select": {
                            "hand_select_id": hand_select_id,
                            "selection_reason": "localized discard and redraw text",
                            "requested_cards": 99,
                            "any_number": true,
                            "can_pick_zero": true,
                            "up_to": false,
                            "allowed_counts": [0, 1, 2],
                            "confirm_supported": true,
                            "cancel_supported": false,
                            "candidate_cards": cards,
                            "selected_cards": [],
                        },
                    },
                },
                "parity_internal": {
                    "zones": {
                        "hand": {"cards": [{
                            "instance_uuid": first,
                            "card_id": "True Grit",
                            "upgrades": 1,
                            "cost_for_turn": 1,
                        }, {
                            "instance_uuid": second,
                            "card_id": "Defend_R",
                            "upgrades": 0,
                            "cost_for_turn": 1,
                        }]},
                    },
                },
                "action_space": {"actions": [raw_action]},
            }),
            actions: vec![JavaRuntimeActionV1 {
                runtime_handle: "f10:a0".to_owned(),
                action_id: format!("submit_hand_select:{hand_select_id}"),
                kind: "submit_hand_select".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: None,
                potion_id: None,
                parameters_required: true,
            }],
        };
        let bindings = BTreeMap::from([
            (first.to_owned(), "natural_run/starter/0".to_owned()),
            (second.to_owned(), "natural_run/starter/5".to_owned()),
        ]);

        for profile in [
            JavaHandSelectProfileV1::GamblingChip,
            JavaHandSelectProfileV1::PotionExhaust,
            JavaHandSelectProfileV1::OptionalExhaust { max_cards: 3 },
            JavaHandSelectProfileV1::OptionalExhaust { max_cards: 5 },
        ] {
            let mut observation = observation.clone();
            if let JavaHandSelectProfileV1::OptionalExhaust { max_cards } = profile {
                let id =
                    format!("hand_select:{max_cards}:true:true:false:{first},{second}:selected:");
                observation.frame["public_base"]["decision"]["hand_select"]["requested_cards"] =
                    serde_json::json!(max_cards);
                observation.frame["public_base"]["decision"]["hand_select"]["hand_select_id"] =
                    serde_json::json!(id);
                observation.frame["action_space"]["actions"][0]["hand_select_id"] =
                    serde_json::json!(id);
                let action_id = format!("submit_hand_select:{id}");
                observation.frame["action_space"]["actions"][0]["action_id"] =
                    serde_json::json!(action_id);
                observation.actions[0].action_id = action_id;
            }
            let resolved =
                resolved_actions_v1(&observation, Some(&bindings), Some(profile)).unwrap();

            assert_eq!(resolved.len(), 4);
            assert_eq!(
                resolved
                    .iter()
                    .map(|action| match &action.execution {
                        JavaActionExecutionV1::SubmitHandSelect {
                            selected_card_uuids,
                        } => selected_card_uuids.clone(),
                        other => panic!("unexpected Gambler execution {other:?}"),
                    })
                    .collect::<Vec<_>>(),
                vec![
                    vec![],
                    vec![first.to_owned()],
                    vec![second.to_owned()],
                    vec![first.to_owned(), second.to_owned()],
                ]
            );
            for (field, value) in [
                ("requested_cards", serde_json::json!(1)),
                ("any_number", serde_json::json!(false)),
                ("can_pick_zero", serde_json::json!(false)),
                ("up_to", serde_json::json!(true)),
                ("allowed_counts", serde_json::json!([1, 2])),
            ] {
                let mut invalid = observation.clone();
                invalid.frame["public_base"]["decision"]["hand_select"][field] = value;
                assert!(resolved_actions_v1(&invalid, Some(&bindings), Some(profile)).is_err());
            }
        }
    }
}
