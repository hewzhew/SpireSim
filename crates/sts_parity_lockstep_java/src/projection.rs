use crate::{
    action::{
        java_event_boundary_identity_v1, java_grid_select_boundary_identity_v1, java_map_room_id,
        resolved_actions_v1, JavaHandSelectProfileV1, ResolvedJavaActionV1,
    },
    transport::JavaRuntimeObservationV1,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::{canonical_game_id, RUN_CONTROL_PROFILE};
use sts_parity_lockstep::{
    natural_map_node_id_v1, natural_run_card_binding_v1, natural_run_card_source_key_v1,
    DomainObservationV1, NaturalRunBootstrapV1, NaturalRunCardSourceV1, ParityDomainV1,
    ParitySnapshotV1, SemanticActionSpaceV1, SnapshotContractV1,
    NATURAL_RUN_CARD_BINDINGS_SCHEMA_V3, NATURAL_RUN_CARD_POOLS_SCHEMA_V1,
    NATURAL_RUN_COMBAT_SCHEMA_V4, NATURAL_RUN_MAP_SCHEMA_V2, NATURAL_RUN_MASTER_DECK_SCHEMA_V2,
    NATURAL_RUN_POTIONS_SCHEMA_V1, NATURAL_RUN_RELICS_SCHEMA_V3, NATURAL_RUN_REWARDS_SCHEMA_V2,
    NATURAL_RUN_RNG_SCHEMA_V4, NATURAL_RUN_ROOM_SCHEMA_V3, NATURAL_RUN_SETTLEMENT_SCHEMA_V1,
    NATURAL_RUN_SNAPSHOT_CONTRACT_SCHEMA_V3, NATURAL_RUN_SNAPSHOT_DOMAINS_V3,
    NATURAL_RUN_STATE_SCHEMA_V2,
};

mod card_pools;
mod combat;
mod relics;
pub(crate) mod rewards;
mod rng;
mod run;
pub use run::java_act_exit_v1;

#[cfg(test)]
mod observed_regressions;

pub(crate) struct ProjectedJavaBoundaryV1 {
    pub snapshot: ParitySnapshotV1,
    pub actions: Vec<ResolvedJavaActionV1>,
    pub runtime_card_bindings: BTreeMap<String, String>,
}

pub(crate) fn project_snapshot_v1(
    observation: &JavaRuntimeObservationV1,
    bootstrap: &NaturalRunBootstrapV1,
    master_deck_rebindings: &BTreeMap<String, JavaStableCardBindingV1>,
    combat_birth_bindings: &BTreeMap<String, JavaStableCardBindingV1>,
    hand_select_profile: Option<JavaHandSelectProfileV1>,
) -> Result<ProjectedJavaBoundaryV1, String> {
    validate_frame(observation, bootstrap)?;
    let boundary_kind = boundary_kind(observation)?;
    let bound_deck = bound_deck(
        observation,
        bootstrap,
        master_deck_rebindings,
        combat_birth_bindings,
    )?;
    let runtime_card_bindings = bound_deck.as_ref().map(|bound| &bound.runtime_bindings);
    let resolved = resolved_actions_v1(observation, runtime_card_bindings, hand_select_profile)?;
    let action_space = SemanticActionSpaceV1::new(
        resolved
            .iter()
            .map(|action| action.semantic.clone())
            .collect::<Vec<_>>(),
    )?;
    let (_, action_observation) = action_space.domain_observation()?;
    let contract = SnapshotContractV1::new(
        NATURAL_RUN_SNAPSHOT_CONTRACT_SCHEMA_V3,
        NATURAL_RUN_SNAPSHOT_DOMAINS_V3
            .into_iter()
            .map(|domain| (domain, true)),
    )?;

    let mut domains = BTreeMap::new();
    match run::project_run_v1(&observation.frame)? {
        Some(run) => {
            domains.insert(
                ParityDomainV1::Run,
                DomainObservationV1::observed(NATURAL_RUN_STATE_SCHEMA_V2, run),
            );
        }
        None => {
            domains.insert(
                ParityDomainV1::Run,
                DomainObservationV1::unobserved(
                    "CommunicationMod did not expose the deterministic natural-run state",
                ),
            );
        }
    }
    domains.insert(
        ParityDomainV1::Map,
        DomainObservationV1::observed(NATURAL_RUN_MAP_SCHEMA_V2, public_map(observation)?),
    );
    let mut projected_room = room(observation)?;
    if let Some(selected) = crate::action::astrolabe_grid_selection_v1(observation)? {
        let bindings = runtime_card_bindings.ok_or("Astrolabe selected cards have no bindings")?;
        projected_room["selected"] = json!(selected
            .iter()
            .map(|uuid| bindings
                .get(uuid)
                .cloned()
                .ok_or("Astrolabe selected UUID has no source binding"))
            .collect::<Result<Vec<_>, _>>()?);
    }
    domains.insert(
        ParityDomainV1::Room,
        DomainObservationV1::observed(NATURAL_RUN_ROOM_SCHEMA_V3, projected_room),
    );
    domains.insert(ParityDomainV1::ActionSpace, action_observation);
    let java_card_bindings = match bound_deck {
        Some(bound) => {
            domains.insert(
                ParityDomainV1::MasterDeck,
                DomainObservationV1::observed(NATURAL_RUN_MASTER_DECK_SCHEMA_V2, bound.deck),
            );
            domains.insert(
                ParityDomainV1::CardBindings,
                DomainObservationV1::observed(NATURAL_RUN_CARD_BINDINGS_SCHEMA_V3, bound.relations),
            );
            Some(bound.runtime_bindings)
        }
        None => {
            let reason = "CommunicationMod did not prove every active card's construction source";
            domains.insert(
                ParityDomainV1::MasterDeck,
                DomainObservationV1::unobserved(reason),
            );
            domains.insert(
                ParityDomainV1::CardBindings,
                DomainObservationV1::unobserved(reason),
            );
            None
        }
    };
    let combat = combat::project_combat_v4(
        observation,
        java_card_bindings.as_ref(),
        boundary_kind,
        hand_select_profile,
    )?;
    match relics::project_relics_v3(
        &observation.frame,
        java_card_bindings.as_ref(),
        matches!(
            boundary_kind,
            "natural_combat"
                | "natural_combat_discovery"
                | "natural_combat_codex"
                | "natural_combat_grid_select"
                | "natural_hand_select"
        ),
    )? {
        Some(relics) => {
            domains.insert(
                ParityDomainV1::Relics,
                DomainObservationV1::observed(NATURAL_RUN_RELICS_SCHEMA_V3, relics),
            );
        }
        None => {
            domains.insert(
                ParityDomainV1::Relics,
                DomainObservationV1::unobserved(
                    "CommunicationMod did not expose every required relic state or binding",
                ),
            );
        }
    }
    domains.insert(
        ParityDomainV1::Potions,
        DomainObservationV1::observed(NATURAL_RUN_POTIONS_SCHEMA_V1, potions(observation)?),
    );
    domains.insert(
        ParityDomainV1::Combat,
        DomainObservationV1::observed(
            NATURAL_RUN_COMBAT_SCHEMA_V4,
            serde_json::to_value(combat)
                .map_err(|error| format!("failed to encode Java combat: {error}"))?,
        ),
    );
    domains.insert(
        ParityDomainV1::Rewards,
        DomainObservationV1::observed(
            NATURAL_RUN_REWARDS_SCHEMA_V2,
            serde_json::to_value(rewards::project_rewards_v1(observation, boundary_kind)?)
                .map_err(|error| format!("failed to encode Java rewards: {error}"))?,
        ),
    );
    match rng::project_rng_streams_v1(observation, boundary_kind, bootstrap.neow_eligibility)? {
        Some(streams) => {
            domains.insert(
                ParityDomainV1::RngStreams,
                DomainObservationV1::observed(NATURAL_RUN_RNG_SCHEMA_V4, streams),
            );
        }
        None => {
            domains.insert(
                ParityDomainV1::RngStreams,
                DomainObservationV1::unobserved(
                    "CommunicationMod did not expose the complete active natural-run RNG family",
                ),
            );
        }
    }
    match card_pools::project_card_pools_v1(&observation.frame)? {
        Some(pools) => {
            domains.insert(
                ParityDomainV1::CardPools,
                DomainObservationV1::observed(NATURAL_RUN_CARD_POOLS_SCHEMA_V1, pools),
            );
        }
        None => {
            domains.insert(
                ParityDomainV1::CardPools,
                DomainObservationV1::unobserved(
                    "CommunicationMod did not expose every declared natural-run card pool",
                ),
            );
        }
    }
    domains.insert(
        ParityDomainV1::RuntimeSettlement,
        DomainObservationV1::observed(
            NATURAL_RUN_SETTLEMENT_SCHEMA_V1,
            json!({
                "stable_player_choice": true,
                "automatic_transition_pending": false,
            }),
        ),
    );

    let snapshot = ParitySnapshotV1 {
        observation_id: format!(
            "{}:frame:{}",
            observation.producer.session_epoch, observation.frame_id
        ),
        boundary_kind: boundary_kind.to_owned(),
        producer: observation.producer.clone(),
        contract,
        domains,
    };
    snapshot.validate()?;
    Ok(ProjectedJavaBoundaryV1 {
        snapshot,
        actions: resolved,
        runtime_card_bindings: java_card_bindings.unwrap_or_default(),
    })
}

pub(crate) fn reproject_recorded_boundary_v1(
    observation: &JavaRuntimeObservationV1,
    bootstrap: &NaturalRunBootstrapV1,
) -> Result<ParitySnapshotV1, String> {
    project_snapshot_v1(
        observation,
        bootstrap,
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
    )
    .map(|projected| projected.snapshot)
}

fn validate_frame(
    observation: &JavaRuntimeObservationV1,
    bootstrap: &NaturalRunBootstrapV1,
) -> Result<(), String> {
    bootstrap.validate()?;
    let frame = &observation.frame;
    if frame["kind"].as_str() != Some("state_frame")
        || frame["profile"].as_str() != Some(RUN_CONTROL_PROFILE)
        || frame["frame_id"].as_u64() != Some(observation.frame_id)
        || frame["boundary"]["stable"].as_bool() != Some(true)
        || !frame["scenario"].is_null()
    {
        return Err("Java observation is not one stable natural run decision".to_owned());
    }
    if observation.producer.implementation.is_empty()
        || observation.producer.version.is_empty()
        || observation.producer.session_epoch.is_empty()
    {
        return Err("Java observation has an invalid producer identity".to_owned());
    }

    let run = &frame["public_base"]["run"];
    let seed = canonical_engine_seed_from_java_long(
        frame["parity_internal"]["run_seed"]
            .as_str()
            .ok_or_else(|| "Java frame omitted its run seed".to_owned())?,
    )?;
    if seed != bootstrap.seed
        || run["ascension"].as_u64() != Some(u64::from(bootstrap.ascension))
        || run["character"].as_str() != Some(bootstrap.player_class.as_str())
        || run["final_act_available"].as_bool() != Some(bootstrap.final_act)
        || frame["parity_internal"]["content_profile"].as_str() != Some("all_unlocked")
    {
        return Err("Java run configuration differs from the shared bootstrap".to_owned());
    }
    boundary_kind(observation)?;
    Ok(())
}

pub(crate) fn boundary_kind(
    observation: &JavaRuntimeObservationV1,
) -> Result<&'static str, String> {
    if observation.frame["boundary"]["kind"].as_str() == Some("combat_player_decision") {
        return Ok("natural_combat");
    }
    let decision = &observation.frame["public_base"]["decision"];
    match (
        observation.frame["boundary"]["kind"].as_str(),
        decision["kind"].as_str(),
    ) {
        (Some("run_decision"), Some("event"))
            if decision["event_id"].as_str() == Some("Neow")
                && matches!(
                    decision["event_screen_index"].as_i64(),
                    Some(0 | 1 | 3 | 99)
                ) =>
        {
            Ok("natural_neow")
        }
        (Some("run_decision"), Some("event")) => {
            java_event_boundary_identity_v1(observation)?;
            Ok("natural_event")
        }
        (Some("run_decision"), Some("grid_select")) => {
            if crate::action::empty_cage_grid_v1(observation)? {
                return Ok("natural_grid_select");
            }
            if crate::action::astrolabe_grid_selection_v1(observation)?.is_some() {
                return Ok("natural_grid_select");
            }
            if crate::action::is_supported_combat_grid_select(observation) {
                return Ok("natural_combat_grid_select");
            }
            if crate::action::bottle_grid_identity_v1(observation)?.is_some() {
                return Ok("natural_grid_select");
            }
            let identity = java_grid_select_boundary_identity_v1(observation)?;
            if identity.event.raw_id == "Neow" {
                Ok("natural_neow_grid_select")
            } else {
                Ok("natural_grid_select")
            }
        }
        (Some("run_decision"), Some("hand_select")) => Ok("natural_hand_select"),
        (Some("run_decision"), Some("card_reward"))
            if decision["card_reward"]["source_kind"].as_str() == Some("combat_discovery") =>
        {
            Ok("natural_combat_discovery")
        }
        (Some("run_decision"), Some("card_reward"))
            if decision["card_reward"]["source_kind"].as_str() == Some("codex") =>
        {
            Ok("natural_combat_codex")
        }
        (Some("run_decision"), Some("map")) => Ok("natural_map"),
        (Some("run_decision"), Some("shop")) => Ok("natural_shop"),
        (Some("run_decision"), Some("shop_entry")) => Ok("natural_shop_entry"),
        (Some("run_decision"), Some("shop_purge_card")) => Ok("natural_shop_purge_card"),
        (Some("run_decision"), Some("shop_purge_confirm")) => Ok("natural_shop_purge_confirm"),
        (Some("run_decision"), Some("combat_reward"))
            if current_map_room(observation)? == "treasure" =>
        {
            Ok("natural_treasure_reward")
        }
        (Some("run_decision"), Some("combat_reward")) => Ok("natural_combat_reward"),
        (Some("terminal"), Some("heart_victory")) => {
            let run = &observation.frame["public_base"]["run"];
            if required_u64(run, "act")? != 4
                || required_i64(run, "current_hp")? <= 0
                || active_map_coordinates(observation)? != (3, 4)
                || current_map_room(observation)? != "victory"
            {
                return Err(
                    "Java heart victory lacks a living Act 4 TrueVictoryRoom endpoint".to_owned(),
                );
            }
            Ok("natural_heart_victory")
        }
        (Some("run_decision"), Some("true_victory_complete")) => {
            Ok("natural_true_victory_complete")
        }
        (Some("run_decision"), Some("double_boss_complete")) => Ok("natural_double_boss_complete"),
        (Some("run_decision"), Some("act3_victory_complete")) => {
            Ok("natural_act3_victory_complete")
        }
        (Some("run_decision"), Some("card_reward"))
            if decision["card_reward"]["source_kind"].as_str() == Some("neow_reward") =>
        {
            if active_map_coordinates(observation)? != (-1, -1)
                || observation.frame["public_base"]["run"]["act"].as_u64() != Some(1)
            {
                return Err("Java Neow card reward is outside the natural Neow entrance".to_owned());
            }
            Ok("natural_neow_card_reward")
        }
        (Some("run_decision"), Some("card_reward"))
            if decision["card_reward"]["source_kind"].as_str() == Some("ordinary_reward") =>
        {
            Ok("natural_card_reward")
        }
        (Some("run_decision"), Some("card_reward")) => {
            Err("Java lockstep does not support this card reward source".to_owned())
        }
        (Some("run_decision"), Some("treasure")) => Ok("natural_treasure"),
        (Some("run_decision"), Some("boss_relic")) => Ok("natural_boss_relic"),
        (Some("run_decision"), Some("boss_treasure")) => Ok("natural_boss_treasure"),
        (Some("run_decision"), Some("boss_treasure_complete")) => {
            Ok("natural_boss_treasure_complete")
        }
        (Some("run_decision"), Some("campfire")) => Ok("natural_campfire"),
        (Some("run_decision"), Some("smith_card")) => Ok("natural_campfire_smith"),
        (Some("run_decision"), Some("smith_confirm")) => Ok("natural_campfire_smith_confirm"),
        (Some("run_decision"), Some("campfire_complete")) => Ok("natural_campfire_complete"),
        _ => Err("Java boundary is not a supported natural lockstep boundary".to_owned()),
    }
}

fn room(observation: &JavaRuntimeObservationV1) -> Result<Value, String> {
    let mut room = room_base(observation)?;
    if is_shop_reward_overlay(observation)? {
        room["return_to"] = json!({ "kind": "shop", "stock": crate::shop::stock(observation)? });
    }
    if room["event_id"].as_str() == Some("gremlinwheelgame") {
        let raw_screen = observation.frame["public_base"]["decision"]["event_screen_index"]
            .as_i64()
            .ok_or("Java wheel screen missing")?;
        if let Some(result) = crate::action::java_wheel_state_v1(observation, raw_screen)?.1 {
            room["wheel_result"] = json!(result);
        }
    }
    Ok(room)
}

pub(crate) fn is_shop_reward_overlay(
    observation: &JavaRuntimeObservationV1,
) -> Result<bool, String> {
    let decision = &observation.frame["public_base"]["decision"];
    let reward = decision["kind"].as_str() == Some("combat_reward")
        || decision["kind"].as_str() == Some("card_reward")
            && decision["card_reward"]["source_kind"].as_str() == Some("ordinary_reward");
    Ok(reward && current_map_room(observation)? == "shop")
}

fn room_base(observation: &JavaRuntimeObservationV1) -> Result<Value, String> {
    match observation.frame["public_base"]["decision"]["kind"].as_str() {
        Some("boss_relic") => {
            return Ok(
                json!({"kind": "boss_relic", "selection": crate::action::boss_relic_selection(observation)?}),
            )
        }
        Some(kind @ ("boss_treasure" | "boss_treasure_complete")) => {
            return Ok(json!({"kind": kind}))
        }
        _ => (),
    }
    if observation.frame["boundary"]["kind"].as_str() == Some("combat_player_decision") {
        return Ok(json!({
            "kind": "combat",
            "current": current_node(observation)?.ok_or_else(|| "Java combat entry has no current map node".to_owned())?,
        }));
    }
    let decision = &observation.frame["public_base"]["decision"];
    if decision["kind"].as_str() != Some("treasure")
        && !observation.frame["parity_internal"]["treasure_chest"].is_null()
    {
        return Err("Java non-treasure boundary carries a pending chest".to_owned());
    }
    match decision["kind"].as_str() {
        Some(kind @ ("shop" | "shop_entry" | "shop_purge_card" | "shop_purge_confirm")) => {
            Ok(json!({"kind":kind, "current":current_node(observation)?,
                "stock":crate::shop::stock(observation)?}))
        }
        Some("event") if decision["event_id"].as_str() == Some("Neow") => {
            let stage = match decision["event_screen_index"].as_i64() {
                Some(0 | 1) => "introduction",
                Some(3) => "choose_reward",
                Some(99) => "leave",
                _ => return Err("Java Neow screen is unsupported".to_owned()),
            };
            Ok(json!({"kind": "neow", "stage": stage}))
        }
        Some("event") => {
            let identity = java_event_boundary_identity_v1(observation)?;
            let mut room = json!({
                "kind": "event",
                "current": current_node(observation)?.ok_or_else(|| "Java natural event has no current map node".to_owned())?,
                "event_id": identity.canonical_id,
                "screen": identity.screen,
            });
            if identity.canonical_id == "matchandkeep" && identity.screen >= 2 {
                room["match_game"] =
                    serde_json::to_value(crate::match_game::board(observation, identity.screen)?)
                        .map_err(|e| e.to_string())?;
            }
            Ok(room)
        }
        Some("grid_select") => {
            if crate::action::empty_cage_grid_v1(observation)? {
                return Ok(
                    json!({"kind":"relic_selection", "current":current_node(observation)?.ok_or("Empty Cage room missing")?,
                    "relic_id":"emptycage", "selection_kind":"deck_remove", "requested_cards":2}),
                );
            }
            if crate::action::astrolabe_grid_selection_v1(observation)?.is_some() {
                return Ok(
                    json!({"kind":"relic_selection", "current":current_node(observation)?.ok_or("Astrolabe room missing")?,
                    "relic_id":"astrolabe", "selection_kind":"deck_transform_upgraded", "requested_cards":3}),
                );
            }
            if let Some((relic, selection)) = crate::action::bottle_grid_identity_v1(observation)? {
                return Ok(json!({
                    "kind": "relic_selection",
                    "current": current_node(observation)?.ok_or("Java bottle selection has no current room")?,
                    "relic_id": relic,
                    "selection_kind": selection,
                }));
            }
            if crate::action::is_supported_combat_grid_select(observation) {
                return Ok(json!({
                    "kind": "combat_grid_select",
                    "current": current_node(observation)?
                        .ok_or("Java Exhume has no current room")?,
                    "selection_kind": crate::action::combat_grid_kind(observation).ok_or("missing combat grid kind")?,
                }));
            }
            let identity = java_grid_select_boundary_identity_v1(observation)?;
            let (kind, current) = if identity.event.raw_id == "Neow" {
                ("neow_selection", Value::Null)
            } else {
                (
                    "event_selection",
                    json!(current_node(observation)?.ok_or_else(|| {
                        "Java natural grid selection has no current map node".to_owned()
                    })?),
                )
            };
            Ok(json!({
                "kind": kind,
                "current": current,
                "event_id": identity.event.canonical_id,
                "screen": identity.event.screen,
                "selection_kind": identity.selection_kind,
            }))
        }
        Some("hand_select") => Ok(json!({
            "kind": "combat_hand_select",
            "current": current_node(observation)?.ok_or_else(|| "Java hand selection has no current combat room".to_owned())?,
        })),
        Some("map") => {
            let current = current_node(observation)?;
            let kind = if observation
                .actions
                .iter()
                .any(|action| action.kind == "close_map")
            {
                "map_preview"
            } else {
                "map_navigation"
            };
            Ok(json!({"kind": kind, "current": current}))
        }
        Some(
            kind @ ("double_boss_complete"
            | "act3_victory_complete"
            | "true_victory_complete"
            | "heart_victory"),
        ) => Ok(json!({
            "kind": kind,
            "current": current_node(observation)?
                .ok_or("Java double boss completion has no current map node")?,
        })),
        Some("combat_reward") => {
            let room = current_map_room(observation)?;
            let current = current_node(observation)?;
            if current.is_none() && room != "neow" {
                return Err("Java combat reward has no current map node".to_owned());
            }
            Ok(json!({
                "kind": if room == "treasure" {
                    "treasure_reward"
                } else {
                    "combat_reward"
                },
                "current": current,
            }))
        }
        Some("card_reward") => {
            if boundary_kind(observation)? == "natural_neow_card_reward" {
                return Ok(json!({"kind": "neow_card_reward", "current": null}));
            }
            if decision["card_reward"]["source_kind"].as_str() == Some("codex") {
                let reward = &decision["card_reward"];
                let cards = reward["cards"]
                    .as_array()
                    .ok_or("Java Codex omitted offers")?
                    .iter()
                    .map(|card| {
                        Ok(sts_parity_lockstep::NaturalRewardCardV1 {
                            card_id: canonical_game_id(required_str(card, "card_id")?),
                            upgrades: u8::try_from(required_u64(card, "upgrades")?)
                                .map_err(|_| "Codex upgrades exceed u8")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let can_skip = reward["skip_available"]
                    .as_bool()
                    .ok_or("Java Codex omitted skip flag")?;
                if cards.len() != 3 || !can_skip || cards.iter().any(|c| c.upgrades != 0) {
                    return Err("Java Codex has an unsupported choice contract".to_owned());
                }
                return Ok(json!({
                    "kind": "combat_codex",
                    "current": current_node(observation)?.ok_or("Java Codex has no current map node")?,
                    "selection": sts_parity_lockstep::NaturalCombatCodexV1 { cards, can_skip },
                }));
            }
            if decision["card_reward"]["source_kind"].as_str() == Some("combat_discovery") {
                let reward = &decision["card_reward"];
                let context = &reward["discovery_context"];
                let cards = reward["cards"]
                    .as_array()
                    .ok_or("Java Discovery omitted its offers")?
                    .iter()
                    .map(|card| {
                        Ok(sts_parity_lockstep::NaturalRewardCardV1 {
                            card_id: canonical_game_id(
                                card["card_id"]
                                    .as_str()
                                    .ok_or("Java Discovery card ID is absent")?,
                            ),
                            upgrades: u8::try_from(
                                card["upgrades"]
                                    .as_u64()
                                    .ok_or("Java Discovery upgrades are absent")?,
                            )
                            .map_err(|_| "Discovery upgrades exceed u8")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let selection = sts_parity_lockstep::NaturalCombatDiscoveryV1 {
                    cards,
                    amount: u8::try_from(
                        context["amount"]
                            .as_u64()
                            .ok_or("Java Discovery amount is absent")?,
                    )
                    .map_err(|_| "Discovery amount exceeds u8")?,
                    colorless: context["colorless"]
                        .as_bool()
                        .ok_or("Java Discovery colorless flag is absent")?,
                    card_type: if context["card_type"].is_null() {
                        None
                    } else {
                        Some(
                            context["card_type"]
                                .as_str()
                                .ok_or("Java Discovery type is invalid")?
                                .to_lowercase(),
                        )
                    },
                    cost_for_turn: u8::try_from(
                        context["cost_for_turn"]
                            .as_u64()
                            .ok_or("Java Discovery cost is absent")?,
                    )
                    .map_err(|_| "Discovery cost exceeds u8")?,
                    can_skip: reward["skip_available"]
                        .as_bool()
                        .ok_or("Java Discovery skip flag is absent")?,
                };
                if !matches!(selection.amount, 1 | 2)
                    || selection.cards.len() != 3
                    || selection.cost_for_turn != 0
                {
                    return Err("Java Discovery has an unsupported choice contract".to_owned());
                }
                return Ok(
                    json!({"kind": "combat_discovery", "current": current_node(observation)?.ok_or("Java Discovery has no current map node")?, "selection": selection}),
                );
            }
            if decision["card_reward"]["source_kind"].as_str() != Some("ordinary_reward") {
                return Err("Java room cannot project this card reward source".to_owned());
            }
            Ok(json!({
                "kind": "combat_reward",
                "current": current_node(observation)?.ok_or_else(|| "Java card reward has no current map node".to_owned())?,
            }))
        }
        Some("treasure") => Ok(json!({
            "kind": "treasure",
            "current": current_node(observation)?.ok_or_else(|| "Java treasure room has no current map node".to_owned())?,
            "chest": project_treasure_chest(observation)?,
        })),
        Some(kind @ ("campfire" | "smith_card" | "smith_confirm" | "campfire_complete")) => {
            let current = current_node(observation)?
                .ok_or_else(|| "Java campfire boundary has no current map node".to_owned())?;
            let room_kind = match kind {
                "campfire" => "campfire",
                "smith_card" => "campfire_smith",
                "smith_confirm" => "campfire_smith_confirm",
                "campfire_complete" => "campfire_complete",
                _ => unreachable!("campfire decision kind was matched above"),
            };
            Ok(json!({"kind": room_kind, "current": current}))
        }
        _ => Err("Java room is not a supported natural lockstep boundary".to_owned()),
    }
}

fn current_map_room(observation: &JavaRuntimeObservationV1) -> Result<&'static str, String> {
    let map = &observation.frame["public_base"]["map"];
    let (current_x, current_y) = active_map_coordinates(observation)?;
    // Neow can open the ordinary reward screen before the first map choice.
    // Its sentinel is not a missing graph node or a synthetic combat room.
    let run = &observation.frame["public_base"]["run"];
    if (current_x, current_y) == (-1, -1)
        && run["act"].as_u64() == Some(1)
        && run["floor"].as_i64() == Some(0)
        && (observation.frame["public_base"]["decision"]["kind"].as_str() == Some("combat_reward")
            && observation.frame["boundary"]["evidence"]["room_phase"].as_str() == Some("EVENT")
            || observation.frame["public_base"]["decision"]["kind"].as_str() == Some("map")
                && observation.frame["public_base"]["decision"]["current_node"]["room_type"]
                    .as_str()
                    == Some("com.megacrit.cardcrawl.neow.NeowRoom")
                && observation.frame["public_base"]["decision"]["current_node"]["x"].as_i64()
                    == Some(0)
                && observation.frame["public_base"]["decision"]["current_node"]["y"].as_i64()
                    == Some(-1))
    {
        return Ok("neow");
    }
    if (current_x, current_y) == (-1, 15) {
        return Ok("boss");
    }
    let nodes = map["nodes"]
        .as_array()
        .ok_or_else(|| "Java public map omitted its nodes".to_owned())?;
    let matching = nodes
        .iter()
        .filter(|node| {
            node["x"].as_i64() == Some(current_x) && node["y"].as_i64() == Some(current_y)
        })
        .collect::<Vec<_>>();
    let [node] = matching.as_slice() else {
        return Err(format!(
            "Java current map coordinate resolves to {} nodes instead of one",
            matching.len()
        ));
    };
    java_map_room_id(required_str(node, "room_type")?)
}

fn project_treasure_chest(observation: &JavaRuntimeObservationV1) -> Result<Value, String> {
    let chest = observation.frame["parity_internal"]["treasure_chest"]
        .as_object()
        .ok_or_else(|| "Java treasure boundary omitted its pending chest".to_owned())?;
    let size = chest
        .get("size")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java treasure chest omitted its size".to_owned())?;
    let relic_tier = chest
        .get("relic_tier")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java treasure chest omitted its relic tier".to_owned())?;
    let gold_base_amount = match chest.get("gold_base_amount") {
        None => None,
        Some(Value::Null) => {
            return Err(
                "Java treasure chest gold base must be omitted when no gold is present".to_owned(),
            )
        }
        Some(value) => Some(
            value
                .as_i64()
                .ok_or_else(|| "Java treasure chest gold base is not an integer".to_owned())?,
        ),
    };
    let valid = matches!(
        (size, relic_tier, gold_base_amount),
        ("small", "common" | "uncommon", None | Some(25))
            | ("medium", "common" | "uncommon" | "rare", None | Some(50))
            | ("large", "uncommon" | "rare", None | Some(75))
    );
    let expected_fields = if gold_base_amount.is_some() { 3 } else { 2 };
    if !valid || chest.len() != expected_fields {
        return Err("Java treasure chest carries an invalid typed specification".to_owned());
    }
    Ok(json!({
        "size": size,
        "relic_tier": relic_tier,
        "gold_base_amount": gold_base_amount,
    }))
}

fn public_map(observation: &JavaRuntimeObservationV1) -> Result<Value, String> {
    let map = observation.frame["public_base"]["map"]
        .as_object()
        .ok_or_else(|| "Java frame does not expose the complete public map".to_owned())?;
    let (current_x, current_y) = active_map_coordinates(observation)?;
    let raw_nodes = map
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Java public map omitted its nodes".to_owned())?;
    let mut all_coordinates = BTreeSet::new();
    for node in raw_nodes {
        let coordinate = (required_i64(node, "x")?, required_i64(node, "y")?);
        if !all_coordinates.insert(coordinate) {
            return Err("Java public map repeats a node coordinate".to_owned());
        }
    }
    // Native Continue regenerates the map and restores only the taken path.
    // In particular, a past `?` resolved into combat reverts to EventRoom in
    // the reconstructed graph. Rows below the current room are unreachable
    // and have no remaining transition semantics, so they are deliberately
    // outside the active-map parity contract.
    let coordinates = all_coordinates
        .iter()
        .copied()
        .filter(|(_, y)| current_y < 0 || *y >= current_y)
        .collect::<BTreeSet<_>>();
    let publish_synthetic_boss = matches!(current_y, 14 | 15);
    let raw_has_synthetic_boss = coordinates.contains(&(0, 15));

    let mut external_targets = BTreeSet::new();
    let mut nodes = raw_nodes
        .iter()
        .filter(|node| {
            current_y < 0 || node["y"].as_i64().is_some_and(|node_y| node_y >= current_y)
        })
        .map(|node| {
            let x = required_i64(node, "x")?;
            let y = required_i64(node, "y")?;
            let mut links = node["edges"]
                .as_array()
                .ok_or_else(|| "Java public map node omitted its edges".to_owned())?
                .iter()
                .map(|edge| {
                    let target = (
                        required_i64(edge, "destination_x")?,
                        required_i64(edge, "destination_y")?,
                    );
                    if !coordinates.contains(&target) {
                        external_targets.insert(target);
                    }
                    natural_map_node_id_v1(target.0, target.1)
                })
                .collect::<Result<Vec<_>, String>>()?;
            links.sort();
            if links.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err("Java public map repeats an edge".to_owned());
            }
            let room = java_map_room_id(required_str(node, "room_type")?)?;
            Ok(json!({
                "id": natural_map_node_id_v1(x, y)?,
                "floor": y + 1,
                "x": x,
                "y": y,
                "room": room,
                "emerald_key": required_bool(node, "has_emerald_key")?,
                "links": links,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if publish_synthetic_boss && !raw_has_synthetic_boss {
        nodes.push(json!({
            "id": natural_map_node_id_v1(0, 15)?,
            "floor": 16,
            "x": 0,
            "y": 15,
            "room": "boss",
            "emerald_key": false,
            "links": [],
        }));
    }
    nodes.sort_by_key(|node| {
        (
            node["y"].as_i64().unwrap_or_default(),
            node["x"].as_i64().unwrap_or_default(),
        )
    });
    if external_targets.len() > 1 {
        return Err("Java public map has more than one external boss anchor".to_owned());
    }
    let boss = match map.get("boss") {
        Some(Value::String(value)) => {
            Value::String(sts_java_parity_protocol::canonical_encounter_schedule_id_v1(value))
        }
        Some(Value::Null) | None => Value::Null,
        Some(_) => return Err("Java public map boss is not a string or null".to_owned()),
    };
    let boss_anchor = external_targets
        .into_iter()
        .next()
        .map(|(x, y)| natural_map_node_id_v1(x, y))
        .transpose()?;
    let current = coordinate_node_id(current_x, current_y)?;
    Ok(json!({
        "current": current,
        "boss": boss,
        "boss_anchor": boss_anchor,
        "nodes": nodes,
    }))
}

struct BoundDeckProjectionV1 {
    deck: Value,
    relations: Value,
    runtime_bindings: BTreeMap<String, String>,
    stable_master_deck_bindings: BTreeMap<String, JavaStableCardBindingV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JavaStableCardBindingV1 {
    pub binding: String,
    pub source: NaturalRunCardSourceV1,
    // The SpireField tag belongs to the current Java object graph. A native
    // reload erases it; an adjacent acquisition can prove a different durable
    // source without changing or discarding this raw observation.
    pub raw_source: Option<NaturalRunCardSourceV1>,
}

/// Reissues runtime UUID leases after the original game reloads one native
/// autosave generation. The save format preserves ordered card contents but
/// deliberately does not preserve Java UUIDs or SpireField metadata, so the
/// prior Exact boundary supplies the stable source proof while the two raw
/// deck observations prove that the persisted cards did not change.
pub(crate) fn rebind_master_deck_after_native_reload_v1(
    source: &JavaRuntimeObservationV1,
    reloaded: &JavaRuntimeObservationV1,
    bootstrap: &NaturalRunBootstrapV1,
    source_master_deck_rebindings: &BTreeMap<String, JavaStableCardBindingV1>,
    source_combat_birth_bindings: &BTreeMap<String, JavaStableCardBindingV1>,
) -> Result<BTreeMap<String, JavaStableCardBindingV1>, String> {
    let source_bound = bound_deck(
        source,
        bootstrap,
        source_master_deck_rebindings,
        source_combat_birth_bindings,
    )?
    .ok_or_else(|| {
        "native reload source has no complete source-proven master-deck bindings".to_owned()
    })?;
    let source_cards = source.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "native reload source omitted its master deck".to_owned())?;
    let reloaded_cards = reloaded.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "native reload successor omitted its master deck".to_owned())?;
    let projected_cards = source_bound
        .deck
        .as_array()
        .ok_or_else(|| "native reload source produced a malformed canonical deck".to_owned())?;
    if source_cards.len() != reloaded_cards.len() || source_cards.len() != projected_cards.len() {
        return Err("native reload changed the persisted master-deck length".to_owned());
    }

    let mut rebound = BTreeMap::new();
    for (ordinal, ((source_card, reloaded_card), projected_card)) in source_cards
        .iter()
        .zip(reloaded_cards)
        .zip(projected_cards)
        .enumerate()
    {
        if required_u64(source_card, "ordinal")? != ordinal as u64
            || required_u64(reloaded_card, "ordinal")? != ordinal as u64
            || required_u64(projected_card, "ordinal")? != ordinal as u64
        {
            return Err("native reload master-deck ordinals are not contiguous".to_owned());
        }
        let source_uuid = required_str(source_card, "card_uuid")?;
        let stable = source_bound
            .stable_master_deck_bindings
            .get(source_uuid)
            .ok_or_else(|| {
                "native reload source card has no source-proven stable binding".to_owned()
            })?;
        if projected_card["binding"].as_str() != Some(stable.binding.as_str()) {
            return Err("native reload source binding projection is inconsistent".to_owned());
        }

        let reloaded_card_id = canonical_game_id(required_str(reloaded_card, "card_id")?);
        let reloaded_upgrades = required_u64(reloaded_card, "upgrades")?;
        if projected_card["card_id"].as_str() != Some(reloaded_card_id.as_str())
            || projected_card["upgrades"].as_u64() != Some(reloaded_upgrades)
        {
            return Err(format!(
                "native reload changed persisted card identity at master-deck ordinal {ordinal}"
            ));
        }
        if let Some(raw_source) = reloaded_card.get("source").filter(|value| !value.is_null()) {
            let raw_source =
                serde_json::from_value::<NaturalRunCardSourceV1>(raw_source.clone())
                    .map_err(|error| format!("native reload card source is malformed: {error}"))?;
            if raw_source != stable.source {
                return Err(format!(
                    "native reload card source conflicts at master-deck ordinal {ordinal}"
                ));
            }
        }

        let mut stable = stable.clone();
        stable.raw_source = reloaded_card
            .get("source")
            .filter(|value| !value.is_null())
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|error| format!("native reload card source is malformed: {error}"))?;
        let reloaded_uuid = required_str(reloaded_card, "card_uuid")?;
        if reloaded_uuid.is_empty() || rebound.insert(reloaded_uuid.to_owned(), stable).is_some() {
            return Err("native reload repeats or omits a runtime card UUID".to_owned());
        }
    }
    Ok(rebound)
}

fn bound_deck(
    observation: &JavaRuntimeObservationV1,
    bootstrap: &NaturalRunBootstrapV1,
    master_deck_rebindings: &BTreeMap<String, JavaStableCardBindingV1>,
    combat_birth_bindings: &BTreeMap<String, JavaStableCardBindingV1>,
) -> Result<Option<BoundDeckProjectionV1>, String> {
    let cards = observation.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java frame omitted its master deck".to_owned())?;
    let by_source = bootstrap
        .cards
        .iter()
        .map(|card| Ok((natural_run_card_source_key_v1(&card.source)?, card)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    if by_source.len() != bootstrap.cards.len() {
        return Err("shared bootstrap repeats a card source".to_owned());
    }
    let mut seen_sources = BTreeSet::new();
    let mut seen_uuids = BTreeSet::new();
    let mut deck = Vec::with_capacity(cards.len());
    let mut relations = Vec::with_capacity(cards.len());
    let mut runtime_bindings = BTreeMap::new();
    let mut stable_master_deck_bindings = BTreeMap::new();
    for (expected_ordinal, card) in cards.iter().enumerate() {
        if required_u64(card, "ordinal")? != expected_ordinal as u64 {
            return Err("Java master-deck ordinals are not contiguous".to_owned());
        }
        let uuid = required_str(card, "card_uuid")?;
        if !seen_uuids.insert(uuid) {
            return Err("Java master deck repeats one runtime UUID".to_owned());
        }
        let rebound = master_deck_rebindings.get(uuid);
        let raw_source = card.get("source").filter(|source| !source.is_null());
        let source: NaturalRunCardSourceV1 = match (rebound, raw_source) {
            (Some(rebound), Some(raw)) => {
                let raw = serde_json::from_value::<NaturalRunCardSourceV1>(raw.clone())
                    .map_err(|error| format!("Java card source is malformed: {error}"))?;
                if Some(&raw) != rebound.raw_source.as_ref() {
                    return Err(
                        "Java transformed card conflicts with its derived source".to_owned()
                    );
                }
                rebound.source.clone()
            }
            (Some(rebound), None) => rebound.source.clone(),
            (None, Some(raw)) => serde_json::from_value(raw.clone())
                .map_err(|error| format!("Java card source is malformed: {error}"))?,
            (None, None) => return Ok(None),
        };
        let source_key = natural_run_card_source_key_v1(&source)?;
        if !seen_sources.insert(source_key.clone()) {
            return Err("Java master deck repeats one construction source".to_owned());
        }
        let card_id = canonical_game_id(required_str(card, "card_id")?);
        let upgrades = required_u64(card, "upgrades")?;
        let binding = if let Some(rebound) = rebound {
            if natural_run_card_binding_v1(&rebound.source)? != rebound.binding {
                return Err("Java transformed card has an invalid derived binding".to_owned());
            }
            rebound.binding.clone()
        } else if let Some(expected) = by_source.get(&source_key) {
            if card_id != expected.card_id || upgrades < u64::from(expected.upgrades) {
                return Err(
                    "Java card identity or upgrades regressed behind its bootstrap construction source"
                        .to_owned(),
                );
            }
            expected.binding.clone()
        } else {
            natural_run_card_binding_v1(&source)?
        };
        if runtime_bindings
            .values()
            .any(|existing| existing == &binding)
        {
            return Err("Java master deck repeats one stable card binding".to_owned());
        }
        let mut projected = json!({
            "ordinal": expected_ordinal,
            "binding": binding,
            "card_id": card_id,
            "upgrades": upgrades,
        });
        if let Some(misc) = combat::persistent_card_misc(card)? {
            projected["misc"] = json!(misc);
        }
        deck.push(projected);
        relations.push(json!({
            "binding": binding,
            "source": source.clone(),
        }));
        runtime_bindings.insert(uuid.to_owned(), binding.clone());
        let raw_source = raw_source
            .map(|raw| serde_json::from_value(raw.clone()))
            .transpose()
            .map_err(|error| format!("Java card source is malformed: {error}"))?;
        stable_master_deck_bindings.insert(
            uuid.to_owned(),
            JavaStableCardBindingV1 {
                binding,
                source,
                raw_source,
            },
        );
    }

    if observation.frame["boundary"]["kind"].as_str() == Some("combat_player_decision")
        || crate::action::is_supported_combat_grid_select(observation)
        || boundary_kind(observation)? == "natural_combat_discovery"
        || boundary_kind(observation)? == "natural_combat_codex"
        || (observation.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && observation.frame["public_base"]["decision"]["kind"].as_str() == Some("hand_select"))
    {
        let raw_zones = &observation.frame["parity_internal"]["zones"];
        let pending_zones = if crate::action::is_exhume_selection(observation) {
            Some(combat::zones_with_exhume_cards(raw_zones)?)
        } else if !raw_zones["armaments_held"].is_null() {
            Some(combat::zones_with_held_hand_cards(
                raw_zones,
                "armaments_held",
            )?)
        } else if !raw_zones["dual_wield_held"].is_null() {
            Some(combat::zones_with_held_hand_cards(
                raw_zones,
                "dual_wield_held",
            )?)
        } else {
            None
        };
        let zones = pending_zones.as_ref().unwrap_or(raw_zones);
        let mut active_cards = Vec::new();
        for zone_name in ["draw", "hand", "discard", "exhaust", "limbo"] {
            let cards = zones[zone_name]["cards"]
                .as_array()
                .ok_or_else(|| format!("Java combat zone {zone_name:?} omitted its cards"))?;
            let in_use = (zone_name == "limbo")
                .then(|| zones.get("card_in_use"))
                .flatten()
                .filter(|card| !card.is_null());
            active_cards.extend(cards.iter().chain(in_use));
        }
        active_cards.extend(
            combat::stasis_cards(observation)?
                .into_iter()
                .map(|(_, card)| card),
        );
        for card in active_cards {
            let uuid = required_str(card, "instance_uuid")?;
            if runtime_bindings.contains_key(uuid) {
                continue;
            }
            let bound = combat_birth_bindings.get(uuid).ok_or_else(|| {
                "Java combat card UUID has no source-proven combat-birth binding".to_owned()
            })?;
            if runtime_bindings
                .values()
                .any(|existing| existing == &bound.binding)
            {
                return Err("Java combat birth repeats one active stable card binding".to_owned());
            }
            relations.push(json!({
                "binding": bound.binding,
                "source": bound.source,
            }));
            runtime_bindings.insert(uuid.to_owned(), bound.binding.clone());
        }
    }
    relations.sort_by(|left, right| left["binding"].as_str().cmp(&right["binding"].as_str()));
    Ok(Some(BoundDeckProjectionV1 {
        deck: Value::Array(deck),
        relations: Value::Array(relations),
        runtime_bindings,
        stable_master_deck_bindings,
    }))
}

fn potions(observation: &JavaRuntimeObservationV1) -> Result<Value, String> {
    let combat_potions = &observation.frame["public_base"]["combat"]["potions"];
    let potions = if combat_potions.is_array() {
        combat_potions.as_array().expect("checked array")
    } else {
        observation.frame["parity_internal"]["potions"]
            .as_array()
            .ok_or_else(|| "Java frame does not expose persistent potion slots".to_owned())?
    };
    potions
        .iter()
        .enumerate()
        .map(|(expected, potion)| {
            let slot = usize::try_from(required_u64(potion, "slot_index")?)
                .map_err(|_| "Java potion slot exceeds usize".to_owned())?;
            if slot != expected {
                return Err("Java potion slots are not contiguous".to_owned());
            }
            let id = if required_bool(potion, "empty")? {
                Value::Null
            } else {
                Value::String(canonical_game_id(required_str(potion, "potion_id")?))
            };
            Ok(json!({"slot": slot, "id": id}))
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Value::Array)
}

pub(crate) fn current_node(
    observation: &JavaRuntimeObservationV1,
) -> Result<Option<String>, String> {
    let (x, y) = active_map_coordinates(observation)?;
    coordinate_node_id(x, y)
}

fn active_map_coordinates(observation: &JavaRuntimeObservationV1) -> Result<(i64, i64), String> {
    let map = &observation.frame["public_base"]["map"];
    let coordinates = (
        required_i64(map, "current_x")?,
        required_i64(map, "current_y")?,
    );
    if coordinates != (-1, 15) {
        return Ok(coordinates);
    }
    if observation.frame["public_base"]["decision"]["kind"].as_str() == Some("map")
        && is_new_map_entrance(observation)?
    {
        return Ok((-1, -1));
    }
    // DungeonMap.update uses (-1, 15) for every boss, including TheEnding.
    // The Ending graph already contains its one boss, unlike the ordinary
    // acts' external boss anchor. Resolve that typed node for map, room and
    // combat-birth identity together; never invent an additional Act 4 boss.
    if observation.frame["public_base"]["run"]["act"].as_u64() == Some(4) {
        let mut boss = None;
        for node in map["nodes"]
            .as_array()
            .ok_or("Java Ending map omitted nodes")?
        {
            if java_map_room_id(required_str(node, "room_type")?)? == "boss" {
                let coordinates = (required_i64(node, "x")?, required_i64(node, "y")?);
                if coordinates.0 < 0 || coordinates.1 < 0 || boss.replace(coordinates).is_some() {
                    return Err("Java Ending map must contain one nonnegative boss node".to_owned());
                }
            }
        }
        return boss.ok_or_else(|| "Java Ending map omitted its boss node".to_owned());
    }
    Ok(coordinates)
}

fn is_new_map_entrance(observation: &JavaRuntimeObservationV1) -> Result<bool, String> {
    // AbstractDungeon.generateMap resets firstRoomChosen but TheBeyond leaves
    // currMapNode pointing at the previous act's boss treasure. Both the game
    // and CommunicationMod offer row-zero nodes only before the first choice.
    // Validated typed actions therefore prove the new-map entrance even in
    // recorded frames which do not export firstRoomChosen itself.
    let actions = crate::action::resolved_actions_v1(observation, None, None)?;
    let has_first_row = actions.iter().any(|action| matches!(
        &action.semantic,
        sts_parity_lockstep::SemanticActionV1::Select { choice, .. }
            if choice.attributes.get("y") == Some(&sts_parity_lockstep::SemanticScalarV1::Integer(0))
    ));
    if !has_first_row {
        return Ok(false);
    }
    for action in actions {
        match action.semantic {
            sts_parity_lockstep::SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Map,
                choice,
            } if choice.attributes.get("y")
                == Some(&sts_parity_lockstep::SemanticScalarV1::Integer(0)) => {}
            // Run potion actions have already been checked against the real
            // usable, untargeted slot. They do not choose or dismiss a map node.
            sts_parity_lockstep::SemanticActionV1::DiscardPotion { .. }
            | sts_parity_lockstep::SemanticActionV1::UsePotion { target: None, .. } => {}
            _ => {
                return Err(
                    "Java new-map entrance mixes first-row choices with another map action"
                        .to_owned(),
                )
            }
        }
    }
    Ok(true)
}

fn coordinate_node_id(x: i64, y: i64) -> Result<Option<String>, String> {
    match (x, y) {
        (-1, -1) => Ok(None),
        // Original-game DungeonMap creates ordinary Act boss rooms as the
        // explicit sentinel MapRoomNode(-1, 15). The shared room/action
        // coordinate is Rust's equivalent synthetic control point (0, 15);
        // public-map topology separately keeps the external visual boss anchor.
        (-1, 15) => natural_map_node_id_v1(0, 15).map(Some),
        (x, y) if x >= 0 && y >= 0 => natural_map_node_id_v1(x, y).map(Some),
        _ => Err("Java public map has a partial current coordinate".to_owned()),
    }
}

fn canonical_engine_seed_from_java_long(value: &str) -> Result<String, String> {
    if let Ok(signed) = value.parse::<i64>() {
        return Ok((signed as u64).to_string());
    }
    value
        .parse::<u64>()
        .map(|unsigned| unsigned.to_string())
        .map_err(|_| "Java run seed is not an integer".to_owned())
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("Java value omitted string {field}"))
}

fn required_i64(value: &Value, field: &str) -> Result<i64, String> {
    value[field]
        .as_i64()
        .ok_or_else(|| format!("Java value omitted integer {field}"))
}

fn required_u64(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .ok_or_else(|| format!("Java value omitted unsigned integer {field}"))
}

fn required_bool(value: &Value, field: &str) -> Result<bool, String> {
    value[field]
        .as_bool()
        .ok_or_else(|| format!("Java value omitted boolean {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genetic_algorithm_master_deck_requires_each_observed_growth_value() {
        let bootstrap =
            sts_parity_lockstep_rust::natural_run_bootstrap_v1(&Default::default()).unwrap();
        let mut observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "fixture".into(),
                version: "test".into(),
                session_epoch: "test".into(),
            },
            frame_id: 1,
            frame: json!({
                "boundary":{"kind":"run_decision"}, "public_base":{"decision":{"kind":"map"}},
                "parity_internal":{"master_deck":[
                    {"ordinal":0,"card_uuid":"first","card_id":"Genetic Algorithm","upgrades":0,"misc":3,
                     "source":{"kind":"card_reward","room_id":"F01X02","product_slot":0}},
                    {"ordinal":1,"card_uuid":"second","card_id":"Genetic Algorithm","upgrades":0,"misc":17,
                     "source":{"kind":"card_reward","room_id":"F01X02","product_slot":1}}
                ]}
            }),
            actions: vec![],
        };
        let project = |observation: &JavaRuntimeObservationV1| {
            bound_deck(observation, &bootstrap, &BTreeMap::new(), &BTreeMap::new())
        };
        let before = project(&observation).unwrap().unwrap().deck;
        assert_eq!(before[0]["misc"], 3);
        assert_eq!(before[1]["misc"], 17);
        observation.frame["parity_internal"]["master_deck"][0]["misc"] = json!(5);
        let after = project(&observation).unwrap().unwrap().deck;
        assert_eq!(after[0]["misc"], 5);
        assert_eq!(after[1], before[1]);
        observation.frame["parity_internal"]["master_deck"][0]
            .as_object_mut()
            .unwrap()
            .remove("misc");
        assert!(project(&observation).is_err());
    }

    #[test]
    fn original_boss_sentinel_has_one_shared_room_coordinate() {
        assert_eq!(coordinate_node_id(-1, -1).unwrap(), None);
        assert_eq!(
            coordinate_node_id(-1, 15).unwrap().as_deref(),
            Some("F16X00")
        );
        assert!(coordinate_node_id(-1, 14).is_err());
        assert!(coordinate_node_id(0, -1).is_err());
    }

    #[test]
    fn heart_terminal_requires_the_living_ending_victory_room_and_no_gameplay_actions() {
        let observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "fixture".into(),
                version: "test".into(),
                session_epoch: "test".into(),
            },
            frame_id: 1,
            frame: json!({
                "boundary": {"kind": "terminal"},
                "public_base": {
                    "run": {"act": 4, "current_hp": 10},
                    "decision": {"kind": "heart_victory"},
                    "map": {"current_x": 3, "current_y": 4, "boss": "The Heart", "nodes": [
                        {"x": 3, "y": 4, "room_type": "TrueVictoryRoom", "has_emerald_key": false, "edges": []}
                    ]}
                }
            }),
            actions: vec![],
        };
        assert_eq!(
            boundary_kind(&observation).unwrap(),
            "natural_heart_victory"
        );
        assert_eq!(
            room(&observation).unwrap(),
            json!({"kind": "heart_victory", "current": "F05X03"})
        );
        assert!(resolved_actions_v1(&observation, None, None)
            .unwrap()
            .is_empty());
        let mut managed = observation.clone();
        managed.actions.push(
            serde_json::from_value(json!({
                "runtime_handle": "f1:a0", "action_id": "run_discard_to_main_menu",
                "kind": "run_discard_to_main_menu", "parameters_required": false,
            }))
            .unwrap(),
        );
        assert!(resolved_actions_v1(&managed, None, None)
            .unwrap()
            .is_empty());
        for malformed in [
            "duplicate",
            "parameter",
            "identity",
            "reset",
            "potion",
            "alias",
        ] {
            let mut invalid = managed.clone();
            match malformed {
                "duplicate" => invalid.actions.push(invalid.actions[0].clone()),
                "parameter" => invalid.actions[0].parameters_required = true,
                "identity" => invalid.actions[0].potion_slot = Some(0),
                "reset" => {
                    invalid.actions[0].kind = "run_reset_to_main_menu_preserving_save".into();
                    invalid.actions[0].action_id = invalid.actions[0].kind.clone();
                }
                "potion" => invalid.actions[0].kind = "discard_potion".into(),
                "alias" => invalid.actions[0].action_id = "alias".into(),
                _ => unreachable!(),
            }
            assert!(
                resolved_actions_v1(&invalid, None, None).is_err(),
                "{malformed}"
            );
        }
        for field in ["act", "current_hp"] {
            let mut invalid = observation.clone();
            invalid.frame["public_base"]["run"][field] = json!(0);
            assert!(boundary_kind(&invalid).is_err());
        }
        let mut invalid = observation.clone();
        invalid.frame["public_base"]["map"]["nodes"][0]["room_type"] = json!("MonsterRoomBoss");
        assert!(boundary_kind(&invalid).is_err());
        let mut invalid = observation.clone();
        invalid.actions.push(
            serde_json::from_value(json!({
                "runtime_handle": "f1:a0", "action_id": "proceed_true_victory",
                "kind": "proceed_true_victory", "parameters_required": false,
            }))
            .unwrap(),
        );
        assert!(resolved_actions_v1(&invalid, None, None).is_err());
    }

    #[test]
    fn ending_map_preserves_the_disconnected_true_victory_room() {
        // TheEnding.generateSpecialMap: five populated nodes, with only the
        // first three linked forward. The victory room is not a map choice.
        let mut observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "test".into(),
                version: "test".into(),
                session_epoch: "test".into(),
            },
            frame_id: 1,
            frame: json!({
                "boundary": {"kind": "run_decision"},
                "public_base": {
                    "run": {"act": 4},
                    "decision": {"kind": "map"},
                    "map": {"current_x": -1, "current_y": 15, "boss": "The Heart", "nodes": [
                        {"x": 3, "y": 0, "room_type": "RestRoom", "has_emerald_key": false,
                         "edges": [{"destination_x": 3, "destination_y": 1}]},
                        {"x": 3, "y": 1, "room_type": "ShopRoom", "has_emerald_key": false,
                         "edges": [{"destination_x": 3, "destination_y": 2}]},
                        {"x": 3, "y": 2, "room_type": "MonsterRoomElite", "has_emerald_key": false,
                         "edges": [{"destination_x": 3, "destination_y": 3}]},
                        {"x": 3, "y": 3, "room_type": "MonsterRoomBoss", "has_emerald_key": false,
                         "edges": []},
                        {"x": 3, "y": 4, "room_type": "com.megacrit.cardcrawl.rooms.TrueVictoryRoom",
                         "has_emerald_key": false, "edges": []}
                    ]}
                }
            }),
            actions: vec![serde_json::from_value(json!({
                "runtime_handle": "f1:a0", "action_id": "map_node:3:0", "kind": "choose_map_node",
                "map_x": 3, "map_y": 0, "map_room_type": "RestRoom", "parameters_required": false,
            }))
            .unwrap()],
        };
        let map = public_map(&observation).unwrap();
        assert_eq!(map["current"], Value::Null);
        assert_eq!(map["boss_anchor"], Value::Null);
        let nodes = map["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[3]["links"], json!([]));
        assert_eq!(
            nodes[4],
            json!({
                "id": "F05X03", "floor": 5, "x": 3, "y": 4,
                "room": "victory", "emerald_key": false, "links": [],
            })
        );
        // The same (-1, 15) now denotes the Heart, after DungeonMap's boss
        // click. The stable graph node must be shared by every combat surface.
        observation.actions.clear();
        observation.frame["boundary"]["kind"] = json!("combat_player_decision");
        observation.frame["public_base"]["decision"] = Value::Null;
        let combat_map = public_map(&observation).unwrap();
        assert_eq!(combat_map["current"], "F04X03");
        assert_eq!(combat_map["boss_anchor"], Value::Null);
        assert_eq!(combat_map["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(room(&observation).unwrap()["current"], "F04X03");
        assert_eq!(current_map_room(&observation).unwrap(), "boss");
        observation.frame["boundary"]["kind"] = json!("run_decision");
        observation.frame["public_base"]["decision"] = json!({"kind": "hand_select"});
        assert_eq!(room(&observation).unwrap()["current"], "F04X03");
        assert_eq!(observation.frame["public_base"]["map"]["current_x"], -1);
        assert_eq!(observation.frame["public_base"]["map"]["current_y"], 15);

        let mut ambiguous = observation.clone();
        ambiguous.frame["public_base"]["map"]["nodes"][4]["room_type"] = json!("MonsterRoomBoss");
        assert!(current_node(&ambiguous).is_err());
        let mut missing = observation.clone();
        missing.frame["public_base"]["map"]["nodes"][3]["room_type"] = json!("MonsterRoom");
        assert!(current_node(&missing).is_err());
        observation.frame["public_base"]["map"]["nodes"][4]["room_type"] = json!("UnknownRoom");
        assert!(public_map(&observation).is_err());
    }

    #[test]
    fn next_act_map_actions_override_the_retained_previous_boss_node() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "test".into(), version: "test".into(), session_epoch: "test".into(),
            },
            frame_id: 1,
            frame: json!({
                "boundary": {"kind": "run_decision"},
                "public_base": {
                    "decision": {"kind": "map"},
                    "map": {"current_x": -1, "current_y": 15, "boss": "Awakened One", "nodes": [
                        {"x": 0, "y": 0, "room_type": "MonsterRoom", "has_emerald_key": false,
                         "edges": [{"destination_x": 0, "destination_y": 14}]},
                        {"x": 0, "y": 14, "room_type": "RestRoom", "has_emerald_key": false,
                         "edges": [{"destination_x": 3, "destination_y": 16}]}
                    ]}
                }
            }),
            actions: vec![serde_json::from_value(json!({
                "runtime_handle": "f1:a0", "action_id": "map_node:0:0", "kind": "choose_map_node",
                "map_x": 0, "map_y": 0, "map_room_type": "MonsterRoom", "parameters_required": false,
            })).unwrap()],
        };
        let entrance = public_map(&observation).unwrap();
        assert_eq!(entrance["current"], Value::Null);
        assert_eq!(entrance["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(entrance["boss_anchor"], "F17X03");
        assert_eq!(room(&observation).unwrap()["current"], Value::Null);
        assert_eq!(observation.frame["public_base"]["map"]["current_y"], 15);

        // A usable run potion is orthogonal to the first map choice. Keep
        // its normal action-space validation and the unchanged entrance.
        for potion_id in ["BloodPotion", "Fruit Juice", "EntropicBrew"] {
            let mut with_potion = observation.clone();
            with_potion.frame["parity_internal"]["potions"] = json!([{
                "slot_index":0, "potion_slot":0, "empty":false, "potion_id":potion_id,
                "can_use":true, "target_required":false,
            }]);
            with_potion.actions.push(
                serde_json::from_value(json!({
                    "runtime_handle":"f1:a1", "kind":"use_potion",
                    "action_id":format!("use_potion:0:{potion_id}"),
                    "parameters_required":false, "potion_slot":0, "potion_id":potion_id,
                }))
                .unwrap(),
            );
            assert_eq!(public_map(&with_potion).unwrap(), entrance);
            assert_eq!(room(&with_potion).unwrap()["current"], Value::Null);
            assert!(crate::action::resolved_actions_v1(&with_potion, None, None)
                .unwrap()
                .iter()
                .any(|action| matches!(
                    action.semantic,
                    sts_parity_lockstep::SemanticActionV1::UsePotion { target: None, .. }
                )));
            with_potion.frame["parity_internal"]["potions"][0]["can_use"] = json!(false);
            assert!(public_map(&with_potion).is_err());
        }

        let close = serde_json::from_value(json!({
            "runtime_handle": "f1:a1", "action_id": "close_map", "kind": "close_map", "parameters_required": false,
        })).unwrap();
        observation.actions.push(close);
        assert!(
            public_map(&observation).is_err(),
            "a dismissable boss preview cannot be an act entrance"
        );
        observation.actions.remove(0);
        assert_eq!(public_map(&observation).unwrap()["current"], "F16X00");
        assert_eq!(room(&observation).unwrap()["current"], "F16X00");
        observation.frame["public_base"]["decision"]["kind"] = json!("combat_reward");
        assert_eq!(active_map_coordinates(&observation).unwrap(), (-1, 15));
    }
}

/// A deliberately partial mechanism view. It grants no card bindings, live
/// action authority, or full-combat parity. Uses the normal adapter projection.
pub fn project_player_mechanism_v1(
    observation: &JavaRuntimeObservationV1,
) -> Result<
    (
        sts_parity_lockstep::NaturalCombatPlayerV4,
        Vec<sts_parity_lockstep::NaturalCombatPowerV4>,
    ),
    String,
> {
    let creatures = combat::project_monster_identities_v4(observation)?;
    let powers = combat::project_powers_with_sources_v4(
        &observation.frame["parity_internal"]["player_powers"],
        "player",
        &creatures,
    )?;
    let player = combat::project_player_v4(
        observation,
        powers.iter().any(|power| power.power_id == "surrounded"),
    )?;
    Ok((player, powers))
}

/// Monster and AI subset through the normal adapter projection.
pub fn project_monster_ai_mechanism_v1(
    observation: &JavaRuntimeObservationV1,
) -> Result<
    (
        Vec<sts_parity_lockstep::NaturalCombatMonsterV4>,
        sts_parity_lockstep::NaturalRunRngStreamV2,
    ),
    String,
> {
    let creatures = combat::project_monster_identities_v4(observation)?;
    let powers = combat::project_powers_with_sources_v4(
        &observation.frame["parity_internal"]["player_powers"],
        "player",
        &creatures,
    )?;
    let monsters = combat::project_monsters_v4(
        observation,
        powers.iter().any(|power| power.power_id == "surrounded"),
    )?;
    let streams = project_combat_rng_mechanism_v1(observation)?;
    let ai = streams
        .0
        .into_iter()
        .find(|stream| stream.owner == "floor.ai")
        .ok_or("mechanism evidence omitted floor.ai")?;
    Ok((monsters, ai))
}

/// Current RNG projection for a declared combat mechanism scope. No import or
/// authority to execute/recover the captured combat is granted.
pub fn project_combat_rng_mechanism_v1(
    observation: &JavaRuntimeObservationV1,
) -> Result<sts_parity_lockstep::NaturalRunRngStreamsV2, String> {
    let streams = rng::project_rng_streams_v1(
        observation,
        "natural_combat",
        sts_parity_lockstep::NaturalRunNeowEligibilityV1::Rich,
    )?
    .ok_or("mechanism evidence omitted RNG streams")?;
    serde_json::from_value(streams).map_err(|error| error.to_string())
}

/// Normal card/relic projection under caller-proven constructed recipe bindings.
/// Missing private relic state and successor UUIDs without a binding fail closed.
pub fn project_cards_relics_mechanism_v1(
    observation: &JavaRuntimeObservationV1,
    bindings: &BTreeMap<String, String>,
) -> Result<
    (
        sts_parity_lockstep::NaturalCombatZonesV4,
        Vec<sts_parity_lockstep::NaturalRelicV3>,
    ),
    String,
> {
    let zones = combat::project_zones_v4(&observation.frame["parity_internal"]["zones"], bindings)?;
    let relics = relics::project_relics_v3(&observation.frame, Some(bindings), true)?
        .ok_or("mechanism relic state is unobserved")?;
    Ok((
        zones,
        serde_json::from_value(relics).map_err(|e| e.to_string())?,
    ))
}
