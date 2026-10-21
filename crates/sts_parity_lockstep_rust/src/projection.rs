use crate::{
    action::{
        is_supported_hand_select, map_node_id, map_room_id, natural_event_screen_v1,
        resolved_actions_v1, selection_kind_v1, ResolvedRustActionV1,
    },
    bootstrap::BoundCardSourceV1,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use sts_java_parity_protocol::{canonical_encounter_schedule_id_v1, canonical_game_id};
use sts_oracle_run_control::{
    agent::information::combat::{
        combat_public_intent_facts_v1, HiddenInformationReasonV1, ObservationEvidenceKindV1,
    },
    content::cards::{
        colorless_pool_for_rarity, defect_pool_for_rarity, ironclad_pool_for_rarity,
        random_colorless_in_combat_pool, silent_pool_for_rarity, watcher_pool_for_rarity, CardId,
        CardRarity,
    },
    content::events::neow::{event_eligibility, NeowEligibilityV1},
    content::powers::PowerId,
    content::relics::{
        persistent_state::persistent_relic_bound_card_lineage_v1, RelicId, RelicState,
    },
    eval::run_control::{
        run_map_comparison_topology_v1, RunControlSession, RunMapComparisonTopologyV1,
    },
    runtime::{
        combat::{CardLineageId, CombatCard, PowerPayload},
        rng::StsRng,
    },
    state::{
        core::{EngineState, PendingChoice},
        events::EventId,
        map::node::RoomType,
        rewards::{RewardItem, RewardScreenContext, TreasureChestSize},
        selection::DomainEventSource,
    },
};
use sts_parity_lockstep::{
    canonicalize_exhaust_by_binding_v4, DomainObservationV1, NaturalCombatCardV4,
    NaturalCombatDecisionV4, NaturalCombatIntentEvidenceV4, NaturalCombatIntentV4,
    NaturalCombatMonsterV4, NaturalCombatPlayerV4, NaturalCombatPowerStateV4, NaturalCombatPowerV4,
    NaturalCombatZonesV4, NaturalRelicCombatStateV1, NaturalRelicV3, NaturalRewardCardV1,
    NaturalRunCombatV4, NaturalRunDropStateV2, NaturalRunEncounterScheduleV2,
    NaturalRunEventChanceBitsV2, NaturalRunEventScheduleV2, NaturalRunKeysV2,
    NaturalRunRelicPoolsV2, NaturalRunRewardItemV1, NaturalRunRewardsV2, NaturalRunRngRoomV3,
    NaturalRunRngStreamV2, NaturalRunRngStreamsV2, NaturalRunRoomFlagsV2, NaturalRunStateV2,
    ParityDomainV1, ParitySnapshotV1, ProducerIdentityV1, SemanticActionSpaceV1,
    SnapshotContractV1, NATURAL_RUN_CARD_BINDINGS_SCHEMA_V3, NATURAL_RUN_CARD_POOLS_SCHEMA_V1,
    NATURAL_RUN_COMBAT_SCHEMA_V4, NATURAL_RUN_MAP_SCHEMA_V2, NATURAL_RUN_MASTER_DECK_SCHEMA_V2,
    NATURAL_RUN_POTIONS_SCHEMA_V1, NATURAL_RUN_RELICS_SCHEMA_V3, NATURAL_RUN_REWARDS_SCHEMA_V2,
    NATURAL_RUN_RNG_SCHEMA_V4, NATURAL_RUN_ROOM_SCHEMA_V3, NATURAL_RUN_SETTLEMENT_SCHEMA_V1,
    NATURAL_RUN_SNAPSHOT_CONTRACT_SCHEMA_V3, NATURAL_RUN_SNAPSHOT_DOMAINS_V3,
    NATURAL_RUN_STATE_SCHEMA_V2,
};

pub(crate) fn project_snapshot_v1(
    session: &RunControlSession,
    producer: ProducerIdentityV1,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
) -> Result<(ParitySnapshotV1, Vec<ResolvedRustActionV1>), String> {
    let boundary_kind = match &session.engine_state {
        EngineState::EventRoom
            if session.run_state.event_state.as_ref().map(|event| event.id)
                == Some(EventId::Neow) =>
        {
            "natural_neow"
        }
        EngineState::EventRoom
            if session.run_state.event_state.as_ref().is_some_and(|event| {
                event.id == EventId::TheLibrary && event.current_screen == 1
            }) =>
        {
            "natural_grid_select"
        }
        EngineState::EventRoom => "natural_event",
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => "natural_map",
        EngineState::CombatPlayerTurn => "natural_combat",
        EngineState::PendingChoice(PendingChoice::DiscoverySelect(_)) => "natural_combat_discovery",
        EngineState::PendingChoice(PendingChoice::CardRewardSelect { .. })
            if crate::action::is_supported_codex(session) =>
        {
            "natural_combat_codex"
        }
        EngineState::PendingChoice(
            PendingChoice::GridSelect { .. } | PendingChoice::ScrySelect { .. },
        ) if crate::action::is_supported_combat_grid_select(session) => {
            "natural_combat_grid_select"
        }
        EngineState::PendingChoice(PendingChoice::HandSelect { .. })
            if is_supported_hand_select(session) =>
        {
            "natural_hand_select"
        }
        EngineState::RewardScreen(rewards)
        | EngineState::RewardOverlay {
            reward_state: rewards,
            ..
        } => match rewards.screen_context {
            RewardScreenContext::NeowCardChoice => "natural_neow_card_reward",
            RewardScreenContext::Standard if rewards.pending_card_choice.is_some() => {
                "natural_card_reward"
            }
            RewardScreenContext::Standard => "natural_combat_reward",
            RewardScreenContext::TreasureRoom => "natural_treasure_reward",
            RewardScreenContext::SmokedCombat => {
                validate_smoke_reward(session, rewards)?;
                "natural_combat_reward"
            }
            RewardScreenContext::MuggedCombat => {
                return Err("Rust natural lockstep does not support this reward context".to_owned())
            }
        },
        EngineState::TreasureRoom(_) => "natural_treasure",
        EngineState::BossRelicSelect(_) => "natural_boss_relic",
        EngineState::DoubleBossComplete => "natural_double_boss_complete",
        EngineState::Act3VictoryComplete => "natural_act3_victory_complete",
        EngineState::TrueVictoryComplete => "natural_true_victory_complete",
        EngineState::GameOver(_) if crate::action::is_heart_victory(session) => {
            "natural_heart_victory"
        }
        EngineState::Campfire => "natural_campfire",
        EngineState::Shop(_) => "natural_shop",
        EngineState::RunPendingChoice(choice)
            if choice.source
                == sts_oracle_run_control::state::selection::DomainEventSource::CampfireSmith =>
        {
            "natural_campfire_smith"
        }
        EngineState::RunPendingChoice(choice)
            if choice.source
                == sts_oracle_run_control::state::selection::DomainEventSource::Event(
                    EventId::Neow,
                ) =>
        {
            "natural_neow_grid_select"
        }
        EngineState::RunPendingChoice(_) => "natural_grid_select",
        _ => unreachable!("resolved_actions_v1 accepted only a natural lockstep boundary"),
    };
    let resolved = resolved_actions_v1(session, bindings)?;
    let action_space = SemanticActionSpaceV1::new(
        resolved
            .iter()
            .map(|action| action.semantic.clone())
            .collect::<Vec<_>>(),
    )?;
    let (action_domain, action_observation) = action_space.domain_observation()?;
    debug_assert_eq!(action_domain, ParityDomainV1::ActionSpace);

    let contract = SnapshotContractV1::new(
        NATURAL_RUN_SNAPSHOT_CONTRACT_SCHEMA_V3,
        NATURAL_RUN_SNAPSHOT_DOMAINS_V3
            .into_iter()
            .map(|domain| (domain, true)),
    )?;
    let mut domains = BTreeMap::new();
    insert_observed(
        &mut domains,
        ParityDomainV1::Run,
        NATURAL_RUN_STATE_SCHEMA_V2,
        run(session)?,
    )?;
    insert_observed(
        &mut domains,
        ParityDomainV1::Map,
        NATURAL_RUN_MAP_SCHEMA_V2,
        public_map(session)?,
    )?;
    insert_observed(
        &mut domains,
        ParityDomainV1::Room,
        NATURAL_RUN_ROOM_SCHEMA_V3,
        room_with_bindings(session, bindings)?,
    )?;
    domains.insert(ParityDomainV1::ActionSpace, action_observation);

    match bound_deck(session, bindings) {
        Ok((deck, relation)) => {
            insert_observed(
                &mut domains,
                ParityDomainV1::MasterDeck,
                NATURAL_RUN_MASTER_DECK_SCHEMA_V2,
                deck,
            )?;
            insert_observed(
                &mut domains,
                ParityDomainV1::CardBindings,
                NATURAL_RUN_CARD_BINDINGS_SCHEMA_V3,
                relation,
            )?;
        }
        Err(reason) => {
            domains.insert(
                ParityDomainV1::MasterDeck,
                DomainObservationV1::unobserved(reason.clone()),
            );
            domains.insert(
                ParityDomainV1::CardBindings,
                DomainObservationV1::unobserved(reason),
            );
        }
    }
    match relics(session, bindings)? {
        Some(relics) => insert_observed(
            &mut domains,
            ParityDomainV1::Relics,
            NATURAL_RUN_RELICS_SCHEMA_V3,
            relics,
        )?,
        None => {
            domains.insert(
                ParityDomainV1::Relics,
                DomainObservationV1::unobserved(
                    "Rust did not prove every persistent relic binding",
                ),
            );
        }
    }
    insert_observed(
        &mut domains,
        ParityDomainV1::Potions,
        NATURAL_RUN_POTIONS_SCHEMA_V1,
        potions(session),
    )?;
    let combat = combat(session, bindings)?;
    insert_observed(
        &mut domains,
        ParityDomainV1::Combat,
        NATURAL_RUN_COMBAT_SCHEMA_V4,
        serde_json::to_value(combat)
            .map_err(|error| format!("failed to encode Rust combat: {error}"))?,
    )?;
    insert_observed(
        &mut domains,
        ParityDomainV1::Rewards,
        NATURAL_RUN_REWARDS_SCHEMA_V2,
        serde_json::to_value(rewards(session)?)
            .map_err(|error| format!("failed to encode Rust rewards: {error}"))?,
    )?;
    insert_observed(
        &mut domains,
        ParityDomainV1::RngStreams,
        NATURAL_RUN_RNG_SCHEMA_V4,
        rng_streams(session, boundary_kind)?,
    )?;
    insert_observed(
        &mut domains,
        ParityDomainV1::CardPools,
        NATURAL_RUN_CARD_POOLS_SCHEMA_V1,
        card_pools(session)?,
    )?;
    insert_observed(
        &mut domains,
        ParityDomainV1::RuntimeSettlement,
        NATURAL_RUN_SETTLEMENT_SCHEMA_V1,
        json!({
            "stable_player_choice": true,
            "automatic_transition_pending": false,
        }),
    )?;

    let snapshot = ParitySnapshotV1 {
        observation_id: format!(
            "{}:decision:{}",
            producer.session_epoch, session.decision_step
        ),
        boundary_kind: boundary_kind.to_owned(),
        producer,
        contract,
        domains,
    };
    snapshot.validate()?;
    Ok((snapshot, resolved))
}

fn insert_observed(
    domains: &mut BTreeMap<ParityDomainV1, DomainObservationV1>,
    domain: ParityDomainV1,
    schema: &str,
    value: Value,
) -> Result<(), String> {
    if domains
        .insert(domain, DomainObservationV1::observed(schema, value))
        .is_some()
    {
        return Err(format!("Rust projection repeated domain {domain:?}"));
    }
    Ok(())
}

fn run(session: &RunControlSession) -> Result<Value, String> {
    let run = &session.run_state;
    let (current_hp, max_hp) = session.visible_player_hp();
    let gold = session
        .active_combat
        .as_ref()
        .map(|active| active.combat_state.entities.player.gold)
        .unwrap_or(run.gold);
    let shop_purge_base_cost = run
        .shop_purge_count
        .checked_mul(25)
        .and_then(|cost| cost.checked_add(75))
        .filter(|_| run.shop_purge_count >= 0)
        .ok_or_else(|| "shop purge count cannot produce an original base cost".to_owned())?;
    let events = &run.event_generator;
    let mut state = NaturalRunStateV2 {
        seed: run.seed.to_string(),
        ascension: u64::from(run.ascension_level),
        character: character_id(run.player_class)?.to_owned(),
        act: u64::from(run.act_num),
        floor: i64::from(run.floor_num),
        current_hp: i64::from(current_hp),
        max_hp: i64::from(max_hp),
        gold: i64::from(gold),
        keys: NaturalRunKeysV2 {
            ruby: run.keys[0],
            sapphire: run.keys[1],
            emerald: run.keys[2],
        },
        final_act_available: run.is_final_act_available,
        daily: run.is_daily_run,
        shop_purge_base_cost: i64::from(shop_purge_base_cost),
        drop_state: NaturalRunDropStateV2 {
            potion_chance_modifier: i64::from(run.potion_drop_chance_mod),
            card_rarity_offset: i64::from(run.card_blizz_randomizer),
            card_upgrade_chance_bits: run.card_upgraded_chance.to_bits(),
        },
        room_flags: NaturalRunRoomFlagsV2 {
            mugged: run.room_mugged,
            smoked: run.room_smoked,
        },
        relic_pools: NaturalRunRelicPoolsV2 {
            common: stable_ids(&run.common_relic_pool),
            uncommon: stable_ids(&run.uncommon_relic_pool),
            rare: stable_ids(&run.rare_relic_pool),
            shop: stable_ids(&run.shop_relic_pool),
            boss: stable_ids(&run.boss_relic_pool),
        },
        encounter_schedule: NaturalRunEncounterScheduleV2 {
            normal: encounter_ids(&run.monster_list),
            elite: encounter_ids(&run.elite_monster_list),
            boss_key: run
                .boss_key
                .map(|id| canonical_encounter_schedule_id_v1(id.stable_id())),
            boss_queue: encounter_ids(&run.boss_list),
        },
        event_schedule: NaturalRunEventScheduleV2 {
            events: event_ids(&events.event_pool),
            shrines: event_ids(&events.shrine_pool),
            one_time_events: event_ids(&events.one_time_event_pool),
            chance_bits: NaturalRunEventChanceBitsV2 {
                elite: events.elite_chance.to_bits(),
                monster: events.monster_chance.to_bits(),
                shop: events.shop_chance.to_bits(),
                treasure: events.treasure_chance.to_bits(),
                shrine: events.shrine_chance.to_bits(),
            },
        },
    };
    state.validate()?;
    let exit = match session.engine_state {
        EngineState::BossRelicSelect(_) => {
            Some(sts_parity_lockstep::NaturalActExitV1::BossRelicChoice)
        }
        EngineState::EventRoom
            if session
                .run_state
                .event_state
                .as_ref()
                .is_some_and(|event| event.id == EventId::SpireHeart) =>
        {
            Some(sts_parity_lockstep::NaturalActExitV1::SpireHeart)
        }
        _ => None,
    };
    state.retire_finished_act_shrines(exit);
    serde_json::to_value(state)
        .map_err(|error| format!("failed to encode canonical Rust run state: {error}"))
}

fn event_ids(values: &[EventId]) -> Vec<String> {
    values
        .iter()
        .map(|event| canonical_game_id(event.stable_id()))
        .collect()
}

fn stable_ids<T>(values: &[T]) -> Vec<String>
where
    T: Copy + StableId,
{
    values
        .iter()
        .map(|value| canonical_game_id((*value).stable_id()))
        .collect()
}

fn encounter_ids(
    values: &[sts_oracle_run_control::content::monsters::factory::EncounterId],
) -> Vec<String> {
    values
        .iter()
        .map(|value| canonical_encounter_schedule_id_v1(value.stable_id()))
        .collect()
}

trait StableId {
    fn stable_id(self) -> &'static str;
}

impl StableId for sts_oracle_run_control::content::relics::RelicId {
    fn stable_id(self) -> &'static str {
        self.stable_id()
    }
}

impl StableId for sts_oracle_run_control::content::monsters::factory::EncounterId {
    fn stable_id(self) -> &'static str {
        self.stable_id()
    }
}

fn public_map(session: &RunControlSession) -> Result<Value, String> {
    let RunMapComparisonTopologyV1 {
        current_x,
        current_y,
        boss,
        mut nodes,
    } = run_map_comparison_topology_v1(session);
    nodes.sort_by_key(|node| (node.y, node.x));
    // Match the original game's native-load equivalence boundary: past rows
    // are unreachable, and Continue does not retain resolved `?` room classes
    // there. The current row remains strict because its room still owns live
    // transition semantics.
    nodes.retain(|node| current_y < 0 || node.y >= current_y);
    let coordinates = nodes
        .iter()
        .map(|node| (node.x, node.y))
        .collect::<BTreeSet<_>>();
    if coordinates.len() != nodes.len() {
        return Err("public map repeats node coordinates".to_owned());
    }
    let mut external_targets = BTreeSet::new();
    let projected = nodes
        .into_iter()
        .map(|mut node| {
            let id = map_node_id(node.x, node.y)?;
            node.edges
                .sort_by_key(|edge| (edge.destination_y, edge.destination_x));
            if node.edges.windows(2).any(|pair| {
                pair[0].destination_x == pair[1].destination_x
                    && pair[0].destination_y == pair[1].destination_y
            }) {
                return Err(format!("public map node {id} repeats an edge"));
            }
            let links = node
                .edges
                .into_iter()
                .map(|edge| {
                    if !coordinates.contains(&(edge.destination_x, edge.destination_y)) {
                        external_targets.insert((edge.destination_x, edge.destination_y));
                    }
                    map_node_id(edge.destination_x, edge.destination_y)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(json!({
                "id": id,
                "floor": node.y + 1,
                "x": node.x,
                "y": node.y,
                "room": map_room_id(node.room),
                "emerald_key": node.has_emerald_key,
                "links": links,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if external_targets.len() > 1 {
        return Err("public map has more than one external boss anchor".to_owned());
    }
    let boss_anchor = external_targets
        .into_iter()
        .next()
        .map(|(x, y)| map_node_id(x, y))
        .transpose()?;
    let current = if current_y == 15 {
        if !coordinates.contains(&(0, 15)) {
            return Err("Rust boss room has no published shared control point".to_owned());
        }
        Value::String(map_node_id(0, 15)?)
    } else if current_x == -1 && current_y == -1 {
        Value::Null
    } else if current_x >= 0 && current_y >= 0 {
        json!(map_node_id(current_x, current_y)?)
    } else {
        return Err("public map has a partial current coordinate".to_owned());
    };
    Ok(json!({
        "current": current,
        "boss": boss.map(|id| canonical_encounter_schedule_id_v1(id.stable_id())),
        "boss_anchor": boss_anchor,
        "nodes": projected,
    }))
}

fn room_with_bindings(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
) -> Result<Value, String> {
    let mut projected = room(session)?;
    if let EngineState::RunPendingChoice(choice) = &session.engine_state {
        if crate::action::is_astrolabe_choice_v1(choice) {
            projected["selected"] = json!(choice
                .selected
                .iter()
                .map(|lineage| bindings
                    .get(lineage)
                    .map(|bound| bound.binding.clone())
                    .ok_or("Rust Astrolabe selected card has no source binding"))
                .collect::<Result<Vec<_>, _>>()?);
        }
    }
    Ok(projected)
}

fn room(session: &RunControlSession) -> Result<Value, String> {
    let mut room = room_base(session)?;
    if room["event_id"].as_str() == Some("gremlinwheelgame") && room["screen"] != 0 {
        let result = session
            .run_state
            .event_state
            .as_ref()
            .ok_or("Rust wheel state missing")?
            .internal_state;
        if !(0..=5).contains(&result) {
            return Err("Rust wheel result outside 0..=5".to_owned());
        }
        room["wheel_result"] = json!(result);
    }
    Ok(room)
}

fn room_base(session: &RunControlSession) -> Result<Value, String> {
    match &session.engine_state {
        EngineState::RewardOverlay { .. } => {
            reward_state(session)?;
            Ok(json!({
                "kind": "combat_reward",
                "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
                "return_to": { "kind": "shop", "stock": crate::shop::stock(session)? },
            }))
        }
        EngineState::Shop(_) => Ok(json!({
            "kind":"shop", "current":map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
            "stock":crate::shop::stock(session)?,
        })),
        EngineState::BossRelicSelect(state) => Ok(json!({
            "kind": "boss_relic",
            "selection": sts_parity_lockstep::NaturalBossRelicChoiceV1 {
                relics: state.relics.iter().map(|id| canonical_game_id(id.stable_id())).collect(),
                can_skip: true,
            },
        })),
        EngineState::EventRoom => {
            let event = session
                .run_state
                .event_state
                .as_ref()
                .ok_or_else(|| "Rust natural event room omitted event state".to_owned())?;
            if event.id == EventId::Neow {
                let stage = match event.current_screen {
                    0 => "introduction",
                    1 => "choose_reward",
                    2 => "leave",
                    screen => return Err(format!("unsupported Neow screen {screen}")),
                };
                Ok(json!({"kind": "neow", "stage": stage}))
            } else if event.id == EventId::TheLibrary && event.current_screen == 1 {
                Ok(json!({
                    "kind": "event_selection",
                    "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
                    "event_id": "thelibrary",
                    "screen": 1,
                    "selection_kind": "event_card_offer",
                }))
            } else {
                let mut room = json!({
                    "kind": "event",
                    "current": map_node_id(
                        session.run_state.map.current_x,
                        session.run_state.map.current_y,
                    )?,
                    "event_id": canonical_game_id(event.id.stable_id()),
                    "screen": natural_event_screen_v1(session)?,
                });
                if event.id == EventId::MatchAndKeep && event.current_screen >= 2 {
                    room["match_game"] = serde_json::to_value(crate::match_game::board(event)?)
                        .map_err(|e| e.to_string())?;
                }
                Ok(room)
            }
        }
        EngineState::RunPendingChoice(choice)
            if choice.source
                == sts_oracle_run_control::state::selection::DomainEventSource::CampfireSmith =>
        {
            Ok(json!({
                "kind": "campfire_smith",
                "current": map_node_id(
                    session.run_state.map.current_x,
                    session.run_state.map.current_y,
                )?,
            }))
        }
        EngineState::RunPendingChoice(choice) if crate::action::is_empty_cage_choice_v1(choice) => {
            Ok(json!({
                "kind": "relic_selection", "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
                "relic_id": "emptycage", "selection_kind": "deck_remove", "requested_cards": 2,
            }))
        }
        EngineState::RunPendingChoice(choice) if crate::action::is_astrolabe_choice_v1(choice) => {
            Ok(json!({
                "kind": "relic_selection", "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
                "relic_id": "astrolabe", "selection_kind": "deck_transform_upgraded", "requested_cards": 3,
            }))
        }
        EngineState::RunPendingChoice(choice)
            if crate::action::bottle_choice_identity_v1(choice).is_some() =>
        {
            Ok(json!({
                "kind": "relic_selection",
                "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
                "relic_id": crate::action::bottle_choice_identity_v1(choice).unwrap(),
                "selection_kind": selection_kind_v1(choice.reason)?,
            }))
        }
        EngineState::RunPendingChoice(choice) => {
            let event = session
                .run_state
                .event_state
                .as_ref()
                .ok_or_else(|| "Rust natural grid selection omitted event state".to_owned())?;
            let current = if event.id == EventId::Neow {
                Value::Null
            } else {
                json!(map_node_id(
                    session.run_state.map.current_x,
                    session.run_state.map.current_y,
                )?)
            };
            Ok(json!({
                "kind": if event.id == EventId::Neow { "neow_selection" } else { "event_selection" },
                "current": current,
                "event_id": canonical_game_id(event.id.stable_id()),
                "screen": natural_event_screen_v1(session)?,
                "selection_kind": selection_kind_v1(choice.reason)?,
            }))
        }
        EngineState::Campfire => Ok(json!({
            "kind": "campfire",
            "current": map_node_id(
                session.run_state.map.current_x,
                session.run_state.map.current_y,
            )?,
        })),
        EngineState::MapNavigation => Ok(json!({
            "kind": "map_navigation",
            "current": if session.run_state.map.current_x >= 0 {
                Some(map_node_id(
                    session.run_state.map.current_x,
                    session.run_state.map.current_y,
                )?)
            } else {
                None
            },
        })),
        EngineState::MapOverlay { .. } => Ok(json!({
            "kind": "map_preview",
            "current": if session.run_state.map.current_x >= 0 {
                Some(map_node_id(
                    session.run_state.map.current_x,
                    session.run_state.map.current_y,
                )?)
            } else {
                None
            },
        })),
        EngineState::CombatPlayerTurn => Ok(json!({
            "kind": "combat",
            "current": map_node_id(
                session.run_state.map.current_x,
                session.run_state.map.current_y,
            )?,
        })),
        EngineState::PendingChoice(PendingChoice::DiscoverySelect(_)) => Ok(json!({
            "kind": "combat_discovery",
            "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
            "selection": crate::action::discovery_state(session)?,
        })),
        EngineState::PendingChoice(PendingChoice::CardRewardSelect { .. })
            if crate::action::is_supported_codex(session) =>
        {
            Ok(json!({
                "kind": "combat_codex",
                "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
                "selection": crate::action::codex_state(session)?,
            }))
        }
        EngineState::PendingChoice(
            PendingChoice::GridSelect { .. } | PendingChoice::ScrySelect { .. },
        ) if crate::action::is_supported_combat_grid_select(session) => Ok(json!({
            "kind": "combat_grid_select",
            "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
            "selection_kind": crate::action::combat_grid_kind(session).ok_or("missing combat grid kind")?,
        })),
        EngineState::PendingChoice(PendingChoice::HandSelect { .. })
            if is_supported_hand_select(session) =>
        {
            Ok(json!({
                "kind": "combat_hand_select",
                "current": map_node_id(
                    session.run_state.map.current_x,
                    session.run_state.map.current_y,
                )?,
            }))
        }
        EngineState::TreasureRoom(chest) => Ok(json!({
            "kind": "treasure",
            "current": map_node_id(
                session.run_state.map.current_x,
                session.run_state.map.current_y,
            )?,
            "chest": {
                "size": match chest.size {
                    TreasureChestSize::Small => "small",
                    TreasureChestSize::Medium => "medium",
                    TreasureChestSize::Large => "large",
                },
                "relic_tier": match chest.base_relic_tier {
                    sts_oracle_run_control::content::relics::RelicTier::Common => "common",
                    sts_oracle_run_control::content::relics::RelicTier::Uncommon => "uncommon",
                    sts_oracle_run_control::content::relics::RelicTier::Rare => "rare",
                    other => return Err(format!("Rust treasure chest has invalid relic tier {other:?}")),
                },
                "gold_base_amount": chest.gold_reward_base_amount,
            },
        })),
        EngineState::Act3VictoryComplete => Ok(json!({
            "kind": "act3_victory_complete",
            "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
        })),
        EngineState::TrueVictoryComplete => Ok(json!({
            "kind": "true_victory_complete",
            "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
        })),
        EngineState::GameOver(_) if crate::action::is_heart_victory(session) => Ok(json!({
            "kind": "heart_victory",
            "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
        })),
        EngineState::DoubleBossComplete => Ok(json!({
            "kind": "double_boss_complete",
            "current": map_node_id(session.run_state.map.current_x, session.run_state.map.current_y)?,
        })),
        EngineState::RewardScreen(rewards) => Ok(json!({
            "kind": match rewards.screen_context {
                RewardScreenContext::NeowCardChoice => "neow_card_reward",
                RewardScreenContext::Standard | RewardScreenContext::SmokedCombat => "combat_reward",
                RewardScreenContext::TreasureRoom => "treasure_reward",
                RewardScreenContext::MuggedCombat => {
                    return Err("Rust room projection does not support this reward context".to_owned())
                }
            },
            "current": if matches!(rewards.screen_context,
                RewardScreenContext::NeowCardChoice | RewardScreenContext::Standard)
                && session.run_state.act_num == 1
                && session.run_state.floor_num == 0
                && session.run_state.map.current_x == -1
                && session.run_state.map.current_y == -1
                && session.run_state.event_state.as_ref().is_some_and(|event| event.id == EventId::Neow)
            { None } else { Some(map_node_id(
                session.run_state.map.current_x,
                session.run_state.map.current_y,
            )?) },
        })),
        _ => Err("Rust boundary is not a supported natural lockstep boundary".to_owned()),
    }
}

fn bound_deck(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
) -> Result<(Value, Value), String> {
    let mut active_bindings = BTreeSet::new();
    let mut relation = Vec::new();
    let mut deck = Vec::new();
    for (ordinal, card) in session.run_state.master_deck.iter().enumerate() {
        let bound = bindings.get(&card.lineage).ok_or_else(|| {
            format!(
                "master-deck lineage {} has no source-proven lockstep binding",
                card.lineage
            )
        })?;
        if !active_bindings.insert(bound.binding.as_str()) {
            return Err("two active master-deck cards share one binding".to_owned());
        }
        let card_id = canonical_game_id(card.id.stable_id());
        let mut projected = json!({
            "ordinal": ordinal,
            "binding": bound.binding,
            "card_id": card_id,
            "upgrades": card.upgrades,
        });
        if let Some(misc) = persistent_card_misc(card)? {
            projected["misc"] = json!(misc);
        }
        deck.push(projected);
        relation.push(json!({
            "binding": bound.binding,
            "source": bound.source,
        }));
    }

    if let Some(active) = session.active_combat.as_ref() {
        let state = &active.combat_state;
        for card in state
            .zones
            .draw_pile
            .iter()
            .chain(&state.zones.hand)
            .chain(state.zones.discard_pile.iter())
            .chain(state.zones.exhaust_pile.iter())
            .chain(&state.zones.limbo)
        {
            let Some(bound) = bindings.get(&card.lineage) else {
                return Err(format!(
                    "active combat lineage {} has no source-proven lockstep binding",
                    card.lineage
                ));
            };
            if !matches!(
                bound.source,
                sts_parity_lockstep::NaturalRunCardSourceV1::CombatBirth { .. }
            ) || !active_bindings.insert(bound.binding.as_str())
            {
                continue;
            }
            relation.push(json!({
                "binding": bound.binding,
                "source": bound.source,
            }));
        }
    }
    relation.sort_by(|left, right| left["binding"].as_str().cmp(&right["binding"].as_str()));
    Ok((Value::Array(deck), Value::Array(relation)))
}

fn relics(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
) -> Result<Option<Value>, String> {
    let relics = session
        .active_combat
        .as_ref()
        .map(|active| active.combat_state.entities.player.relics.as_slice())
        .unwrap_or(session.run_state.relics.as_slice());
    let mut projected = Vec::with_capacity(relics.len());
    for (ordinal, relic) in relics.iter().enumerate() {
        let bound_card = if let Some(lineage) = persistent_relic_bound_card_lineage_v1(relic)? {
            let Some(bound) = bindings.get(&lineage) else {
                return Ok(None);
            };
            Some(bound.binding.clone())
        } else {
            None
        };
        projected.push(project_relic_v3(
            relic,
            ordinal,
            bound_card,
            session.active_combat.is_some(),
        )?);
    }
    serde_json::to_value(projected)
        .map(Some)
        .map_err(|error| format!("failed to encode Rust relics: {error}"))
}

fn relic_combat_state(
    relic: &RelicState,
    combat_active: bool,
) -> Result<Option<NaturalRelicCombatStateV1>, String> {
    if !combat_active {
        return Ok(None);
    }
    if relic.id == RelicId::RedSkull {
        return Ok(Some(NaturalRelicCombatStateV1::RedSkull {
            active: relic.used_up,
        }));
    }
    if relic.id == RelicId::HoveringKite {
        return Ok(Some(NaturalRelicCombatStateV1::HoveringKite {
            triggered_this_turn: relic.used_up,
        }));
    }
    if relic.id == RelicId::CentennialPuzzle {
        return Ok(Some(NaturalRelicCombatStateV1::CentennialPuzzle {
            used_this_combat: relic.used_up,
        }));
    }
    if relic.id == RelicId::OrangePellets {
        if !(0..=6).contains(&relic.counter) {
            return Err("Rust Orange Pellets has an unknown settled combo encoding".to_owned());
        }
        return Ok(Some(NaturalRelicCombatStateV1::OrangePellets {
            attack_played: relic.counter & 1 != 0,
            skill_played: relic.counter & 2 != 0,
            power_played: relic.counter & 4 != 0,
        }));
    }
    if relic.id == RelicId::Lantern {
        return Ok(Some(NaturalRelicCombatStateV1::Lantern {
            first_turn: !relic.used_up,
        }));
    }
    if relic.id != RelicId::ArtOfWar {
        return Ok(None);
    }
    let (first_turn, gain_energy_next) = match relic.counter {
        -1 => (true, true),
        0 => (false, false),
        1 => (false, true),
        _ => return Err("Rust Art of War has an unknown turn-state encoding".to_owned()),
    };
    Ok(Some(NaturalRelicCombatStateV1::ArtOfWar {
        first_turn,
        gain_energy_next,
    }))
}

fn potions(session: &RunControlSession) -> Value {
    let potions = session
        .active_combat
        .as_ref()
        .map(|active| active.combat_state.entities.potions.as_slice())
        .unwrap_or(session.run_state.potions.as_slice());
    Value::Array(
        potions
            .iter()
            .enumerate()
            .map(|(slot, potion)| {
                json!({
                    "slot": slot,
                    "id": potion
                        .as_ref()
                        .map(|potion| canonical_game_id(potion.id.stable_id())),
                })
            })
            .collect(),
    )
}

fn rng_streams(session: &RunControlSession, boundary_kind: &str) -> Result<Value, String> {
    let run = &session.run_state;
    let pool = session
        .active_combat
        .as_ref()
        .map(|active| &active.combat_state.rng.pool)
        .unwrap_or(&run.rng_pool);
    let mut streams = vec![
        rng("run.monster", "run", &pool.monster_rng, true)?,
        rng("run.event", "run", &pool.event_rng, true)?,
        rng("run.merchant", "run", &pool.merchant_rng, true)?,
        rng("run.card", "run", &pool.card_rng, true)?,
        rng("run.treasure", "run", &pool.treasure_rng, true)?,
        rng("run.relic", "run", &pool.relic_rng, true)?,
        rng("run.potion", "run", &pool.potion_rng, true)?,
        rng("run.map", "run", &run.map_rng, false)?,
        rng("floor.monster_hp", "floor", &pool.monster_hp_rng, true)?,
        rng("floor.ai", "floor", &pool.ai_rng, true)?,
        rng("floor.shuffle", "floor", &pool.shuffle_rng, true)?,
        rng("floor.card_random", "floor", &pool.card_random_rng, true)?,
        rng("floor.misc", "floor", &pool.misc_rng, true)?,
    ];
    let neow_event = match &session.engine_state {
        EngineState::EventRoom => run.event_state.as_ref(),
        EngineState::RunPendingChoice(choice)
            if choice.source == DomainEventSource::Event(EventId::Neow) =>
        {
            run.event_state.as_ref()
        }
        _ => None,
    }
    .filter(|event| event.id == EventId::Neow);
    let neow_eligibility = neow_event.map(event_eligibility).transpose()?;
    if neow_eligibility.is_some_and(|eligibility| eligibility != session.neow_eligibility) {
        return Err("Rust Neow event eligibility disagrees with its run recipe".to_owned());
    }
    let neow_is_active = neow_eligibility == Some(NeowEligibilityV1::Rich)
        && (neow_event.is_some_and(|event| event.current_screen == 1)
            || boundary_kind == "natural_neow_grid_select");
    if neow_eligibility == Some(NeowEligibilityV1::Mini) && run.neow_rng.is_some() {
        return Err("Rust Mini Neow unexpectedly owns a Neow RNG".to_owned());
    }
    if neow_event.is_some_and(|event| event.current_screen == 0) && run.neow_rng.is_some() {
        return Err("Rust Neow introduction initialized its RNG too early".to_owned());
    }
    if neow_is_active {
        streams.push(rng(
            "run.neow",
            "run",
            run.neow_rng
                .as_ref()
                .ok_or_else(|| "Rust Rich Neow reward page omitted its RNG".to_owned())?,
            true,
        )?);
    }
    let room = match run.map.get_current_room_type() {
        Some(RoomType::MonsterRoom) => NaturalRunRngRoomV3::Monster,
        Some(RoomType::MonsterRoomElite) => NaturalRunRngRoomV3::Elite,
        _ => NaturalRunRngRoomV3::Other,
    };
    let streams = NaturalRunRngStreamsV2::for_boundary(
        streams,
        boundary_kind,
        neow_is_active,
        room,
        run.keys[2],
    )?;
    serde_json::to_value(streams)
        .map_err(|error| format!("failed to encode canonical Rust RNG streams: {error}"))
}

fn combat(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
) -> Result<NaturalRunCombatV4, String> {
    if session.engine_state != EngineState::CombatPlayerTurn
        && !crate::action::is_supported_discovery(session)
        && !crate::action::is_supported_codex(session)
        && !crate::action::is_exhaust_hand_selection(session)
        && !crate::action::is_discard_hand_selection(session)
        && !crate::action::is_potion_exhaust_selection(session)
        && !crate::action::is_put_on_draw_pile_selection(session)
        && !crate::action::is_upgrade_hand_selection(session)
        && !crate::action::is_copy_hand_selection(session)
        && !crate::action::is_retain_hand_selection(session)
        && !crate::action::is_supported_combat_grid_select(session)
    {
        return Ok(NaturalRunCombatV4::Inactive);
    }
    validate_combat_decision_scope_v2(session)?;
    let active = session
        .active_combat
        .as_ref()
        .filter(|active| active.engine_state == session.engine_state)
        .ok_or_else(|| "Rust combat entry omitted its active player turn".to_owned())?;
    let state = &active.combat_state;
    // Armaments and Dual Wield temporarily remove non-candidates from hand.
    // Preserve the order within both groups, as returnCards appends held cards.
    let mut hand_cards = state.zones.hand.iter().collect::<Vec<_>>();
    if let EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages,
        reason:
            sts_oracle_run_control::state::core::HandSelectReason::Upgrade
            | sts_oracle_run_control::state::core::HandSelectReason::Copy { .. },
        ..
    }) = &session.engine_state
    {
        hand_cards.sort_by_key(|card| !candidate_lineages.contains(&card.lineage));
    }
    let zones = project_zones_v4(state, hand_cards, |card| combat_card(card, bindings))?;

    let (player, player_powers, monsters) = project_creatures_mechanism_v1(state)?;

    Ok(NaturalRunCombatV4::PlayerDecision {
        state: NaturalCombatDecisionV4 {
            turn: state
                .turn
                .turn_count
                .checked_add(1)
                .ok_or_else(|| "Rust combat turn exceeds u32".to_owned())?,
            player,
            zones,
            player_powers,
            monsters,
        },
    })
}

/// Creature-only projection for bounded constructed experiments. Uses the same
/// fields and power/intent semantics as normal combat; grants no card bindings.
pub fn project_creatures_mechanism_v1(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
) -> Result<
    (
        NaturalCombatPlayerV4,
        Vec<NaturalCombatPowerV4>,
        Vec<NaturalCombatMonsterV4>,
    ),
    String,
> {
    let mut enemy_counts = BTreeMap::<String, u32>::new();
    let monsters = state
        .entities
        .monsters
        .iter()
        .map(|monster| {
            let enemy_id = canonical_game_id(monster.enemy_id.stable_id());
            let occurrence = *enemy_counts.entry(enemy_id.clone()).or_default();
            *enemy_counts
                .get_mut(&enemy_id)
                .expect("enemy occurrence was inserted") += 1;
            let facts = combat_public_intent_facts_v1(state, monster.id);
            let (evidence, kind, damage_per_hit, hits, hidden_reason) = match facts.evidence {
                ObservationEvidenceKindV1::VisibleExact => {
                    let intent = facts.intent.ok_or_else(|| {
                        "visible Rust monster intent omitted its semantic kind".to_owned()
                    })?;
                    let hits = intent.is_java_attack_intent().then(|| {
                        u8::try_from(intent.hits()).expect("combat intent hit count fits u8")
                    });
                    (
                        NaturalCombatIntentEvidenceV4::VisibleExact,
                        Some(intent.kind_v1().to_owned()),
                        facts.preview_damage_per_hit,
                        hits,
                        None,
                    )
                }
                ObservationEvidenceKindV1::Hidden => (
                    NaturalCombatIntentEvidenceV4::Hidden,
                    None,
                    None,
                    None,
                    Some(hidden_reason_token(facts.hidden_reason.ok_or_else(
                        || "hidden Rust monster intent omitted its reason".to_owned(),
                    )?)),
                ),
                other => {
                    return Err(format!(
                        "natural combat entry exposed unsupported intent evidence {other:?}"
                    ))
                }
            };
            let powers =
                project_monster_powers_v4(state, monster.id, monster.is_dead_or_escaped())?;
            let public_intent = if monster.is_dead_or_escaped() {
                NaturalCombatIntentV4::terminal_monster()
            } else {
                NaturalCombatIntentV4 {
                    evidence,
                    kind,
                    damage_per_hit,
                    hits,
                    hidden_reason,
                }
            };
            Ok(NaturalCombatMonsterV4 {
                enemy_id,
                occurrence,
                current_hp: monster.current_hp,
                max_hp: monster.max_hp,
                block: monster.block,
                dead_or_escaped: monster.is_dead_or_escaped(),
                left_of_player: surrounded_left_of_player(state, monster.id),
                next_move_id: monster.planned_move_id(),
                move_history: monster.move_history().iter().copied().collect(),
                public_intent,
                powers,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if monsters.is_empty() {
        return Err("natural combat entry has no monsters".to_owned());
    }

    let player = NaturalCombatPlayerV4 {
        current_hp: state.entities.player.current_hp,
        max_hp: state.entities.player.max_hp,
        block: state.entities.player.block,
        energy: state.turn.energy,
        character_state: matches!(state.meta.player_class.as_str(), "Defect" | "Watcher").then(
            || sts_parity_lockstep::NaturalCombatCharacterStateV4 {
                stance: canonical_game_id(state.entities.player.stance.as_str()),
                max_orbs: state.entities.player.max_orbs,
                orbs: state
                    .entities
                    .player
                    .orbs
                    .iter()
                    .map(|orb| sts_parity_lockstep::NaturalCombatOrbV4 {
                        orb_id: canonical_game_id(orb.id.stable_id()),
                        base_passive_amount: orb.base_passive_amount,
                        base_evoke_amount: orb.base_evoke_amount,
                        passive_amount: orb.passive_amount,
                        evoke_amount: orb.evoke_amount,
                    })
                    .collect(),
            },
        ),
        cards_discarded_this_turn: (state.meta.player_class != "Ironclad")
            .then_some(u32::from(state.turn.counters.cards_discarded_this_turn)),
        mantra_gained_this_combat: (state.meta.player_class == "Watcher")
            .then_some(state.turn.counters.mantra_gained_this_combat),
        facing_left: sts_oracle_run_control::content::powers::store::has_power(
            state,
            0,
            PowerId::Surrounded,
        )
        .then_some(state.entities.player.facing_left),
    };
    Ok((player, project_powers_v4(state, 0)?, monsters))
}

/// Only the two streams declared by the bounded EndTurn mechanism experiments.
pub fn project_ai_shuffle_mechanism_v1(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
) -> Result<Vec<NaturalRunRngStreamV2>, String> {
    Ok(vec![
        rng("floor.ai", "floor", &state.rng.ai_rng, true)?,
        rng("floor.shuffle", "floor", &state.rng.shuffle_rng, true)?,
    ])
}

fn surrounded_left_of_player(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
    monster_id: usize,
) -> Option<bool> {
    use sts_oracle_run_control::content::powers::{core::surrounded, store};
    if store::has_power(state, 0, PowerId::Surrounded) {
        surrounded::monster_is_left_of_player(state, monster_id)
    } else {
        None
    }
}

pub(crate) fn validate_combat_decision_scope_v2(session: &RunControlSession) -> Result<(), String> {
    if session.engine_state != EngineState::CombatPlayerTurn
        && !crate::action::is_supported_discovery(session)
        && !crate::action::is_supported_codex(session)
        && !crate::action::is_exhaust_hand_selection(session)
        && !crate::action::is_discard_hand_selection(session)
        && !crate::action::is_potion_exhaust_selection(session)
        && !crate::action::is_put_on_draw_pile_selection(session)
        && !crate::action::is_upgrade_hand_selection(session)
        && !crate::action::is_copy_hand_selection(session)
        && !crate::action::is_retain_hand_selection(session)
        && !crate::action::is_supported_combat_grid_select(session)
    {
        return Ok(());
    }
    let active = session
        .active_combat
        .as_ref()
        .filter(|active| active.engine_state == session.engine_state)
        .ok_or_else(|| "Rust combat entry omitted its active player turn".to_owned())?;
    let state = &active.combat_state;
    if !state.zones.queued_cards.is_empty() || !state.meta.meta_changes.is_empty() {
        return Err("stable Rust combat decision retains unresolved internal work".to_owned());
    }
    if !matches!(
        state.meta.player_class.as_str(),
        "Ironclad" | "Silent" | "Defect" | "Watcher"
    ) {
        return Err("natural combat schema v4 requires a supported base-game class".to_owned());
    }
    if matches!(state.meta.player_class.as_str(), "Ironclad" | "Silent")
        && (!state.entities.player.orbs.is_empty()
            || state.entities.player.max_orbs != 0
            || state.entities.player.stance
                != sts_oracle_run_control::runtime::combat::StanceId::Neutral)
    {
        return Err(
            "Ironclad and Silent natural records require their declared orb/stance-free scope"
                .to_owned(),
        );
    }
    if state.entities.player.orbs.len() != usize::from(state.entities.player.max_orbs) {
        return Err("Rust orb slot count differs from max_orbs".to_owned());
    }
    for card in state
        .zones
        .draw_pile
        .iter()
        .chain(&state.zones.hand)
        .chain(state.zones.discard_pile.iter())
        .chain(state.zones.exhaust_pile.iter())
        .chain(&state.zones.limbo)
    {
        // These non-X cards have already paid their cost before the choice.
        // Auto-play's free-once/energy-on-use markers belong to the suspended
        // non-X play and are cleared on disposal; neither affects this selection
        // or a later play.
        let paid_selection_card = ((crate::action::is_discard_hand_selection(session)
            && card.id == sts_oracle_run_control::content::cards::CardId::Survivor)
            || (crate::action::is_upgrade_hand_selection(session)
                && card.id == sts_oracle_run_control::content::cards::CardId::Armaments)
            || (crate::action::is_copy_hand_selection(session)
                && card.id == sts_oracle_run_control::content::cards::CardId::DualWield)
            || (crate::action::is_put_on_draw_pile_selection(session)
                && matches!(
                    card.id,
                    sts_oracle_run_control::content::cards::CardId::Warcry
                        | sts_oracle_run_control::content::cards::CardId::Setup
                ))
            || (crate::action::combat_grid_kind(session) == Some("omniscience")
                && card.id == sts_oracle_run_control::content::cards::CardId::Omniscience)
            || (crate::action::is_exhaust_hand_selection(session)
                && (card.id == sts_oracle_run_control::content::cards::CardId::BurningPact
                    || card.id == sts_oracle_run_control::content::cards::CardId::Purity
                    || (card.id == sts_oracle_run_control::content::cards::CardId::TrueGrit
                        && card.upgrades > 0))))
            && state
                .zones
                .limbo
                .iter()
                .any(|in_use| in_use.lineage == card.lineage);
        // Java Corruption marks UseCardAction.exhaustCard; Rust carries the
        // same disposal flag on the suspended card until UseCardDone clears it.
        let corruption_exhaust = paid_selection_card
            && sts_oracle_run_control::content::powers::store::has_power(
                state,
                0,
                PowerId::Corruption,
            );
        require_stable_card_state_v4(card, paid_selection_card, corruption_exhaust)?;
    }
    project_powers_v4(state, 0)?;
    for monster in &state.entities.monsters {
        project_monster_powers_v4(state, monster.id, monster.is_dead_or_escaped())?;
    }
    Ok(())
}

fn combat_card(
    card: &CombatCard,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
) -> Result<NaturalCombatCardV4, String> {
    let binding = bindings
        .get(&card.lineage)
        .ok_or_else(|| {
            format!(
                "combat card lineage {} has no source-proven lockstep binding",
                card.lineage
            )
        })?
        .binding
        .clone();
    project_combat_card_v4(card, binding)
}

fn project_combat_card_v4(
    card: &CombatCard,
    binding: String,
) -> Result<NaturalCombatCardV4, String> {
    Ok(NaturalCombatCardV4 {
        binding,
        card_id: canonical_game_id(card.id.stable_id()),
        upgrades: card.upgrades,
        combat_cost: card.combat_cost_without_turn_override_java(),
        cost_for_turn: card.cost_for_turn_java(),
        free_to_play_once: card.free_to_play_once,
        base_damage: persistent_card_damage(card)?,
        misc: persistent_card_misc(card)?,
    })
}

fn persistent_card_misc(card: &CombatCard) -> Result<Option<i32>, String> {
    if !matches!(
        card.id,
        sts_oracle_run_control::content::cards::CardId::GeneticAlgorithm
            | sts_oracle_run_control::content::cards::CardId::RitualDagger
    ) {
        return Ok(None);
    }
    if card.misc_value < 1 {
        return Err(format!(
            "Rust {:?} omitted its positive growth value",
            card.id
        ));
    }
    Ok(Some(card.misc_value))
}

fn persistent_card_damage(card: &CombatCard) -> Result<Option<i32>, String> {
    use sts_oracle_run_control::content::cards::{get_card_definition, CardId};
    if !matches!(card.id, CardId::Rampage | CardId::GlassKnife) {
        return Ok(None);
    }
    let amount = card.base_damage_override.unwrap_or_else(|| {
        let definition = get_card_definition(card.id);
        definition.base_damage + definition.upgrade_damage * i32::from(card.upgrades > 0)
    });
    if amount < 0 {
        return Err(format!(
            "Rust {:?} has negative persistent base damage",
            card.id
        ));
    }
    Ok(Some(amount))
}

fn require_stable_card_state_v4(
    card: &CombatCard,
    paid_selection_card: bool,
    corruption_exhaust: bool,
) -> Result<(), String> {
    if (card.misc_value != 0
        && !matches!(
            card.id,
            sts_oracle_run_control::content::cards::CardId::GeneticAlgorithm
                | sts_oracle_run_control::content::cards::CardId::RitualDagger
        ))
        || (card.base_damage_override.is_some()
            && !matches!(
                card.id,
                sts_oracle_run_control::content::cards::CardId::Rampage
                    | sts_oracle_run_control::content::cards::CardId::GlassKnife
            ))
        || card.base_block_override.is_some()
        || card.exhaust_override.is_some_and(|exhaust| {
            exhaust != sts_oracle_run_control::content::cards::get_card_definition(card.id).exhaust
                && !(paid_selection_card && corruption_exhaust && exhaust)
        })
        || card.retain_override.is_some()
        || (card.energy_on_use != 0 && !paid_selection_card)
    {
        return Err(format!(
            "natural combat schema v4 does not yet cover {:?} mutation: misc={}, damage={:?}, block={:?}, exhaust={:?}, retain={:?}, free_once={}, energy_on_use={}",
            card.id, card.misc_value, card.base_damage_override, card.base_block_override,
            card.exhaust_override, card.retain_override, card.free_to_play_once, card.energy_on_use
        ));
    }
    persistent_card_damage(card)?;
    persistent_card_misc(card)?;
    Ok(())
}

fn project_creature_v4(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
    source: usize,
) -> Result<sts_parity_lockstep::NaturalCombatCreatureV4, String> {
    use sts_parity_lockstep::NaturalCombatCreatureV4;
    if source == 0 {
        return Ok(NaturalCombatCreatureV4::Player);
    }
    let mut counts = BTreeMap::<String, u32>::new();
    let mut found = None;
    for monster in &state.entities.monsters {
        let enemy_id = canonical_game_id(monster.enemy_id.stable_id());
        let occurrence = counts.entry(enemy_id.clone()).or_default();
        if monster.id == source {
            if found.is_some() {
                return Err("Rust power source matches duplicate creature identities".to_owned());
            }
            found = Some(NaturalCombatCreatureV4::Monster {
                enemy_id,
                occurrence: *occurrence,
            });
        }
        *occurrence += 1;
    }
    found.ok_or_else(|| "Rust power source is absent from the current creature boundary".to_owned())
}

fn project_powers_v4(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
    owner: usize,
) -> Result<Vec<NaturalCombatPowerV4>, String> {
    let powers = sts_oracle_run_control::content::powers::store::powers_for(state, owner)
        .unwrap_or_default();
    let mut occurrences = BTreeMap::<String, u32>::new();
    powers
        .iter()
        .map(|power| {
            if power.instance_id.is_some()
                || (!matches!(power.payload, PowerPayload::None)
                    && !matches!(power.power_type, PowerId::Constricted | PowerId::Poison))
            {
                return Err(format!(
                    "natural combat power {:?} carries unsupported instance or payload state",
                    power.power_type
                ));
            }
            let power_id = canonical_game_id(match power.power_type {
                PowerId::Regen => "Regenerate",
                PowerId::BattleHymnPower => "BattleHymn",
                PowerId::RushdownPower => "Rushdown",
                PowerId::NirvanaPower => "Nirvana",
                PowerId::WraithForm => "Wraith Form v2",
                PowerId::DrawCardNextTurn => "Draw Card",
                PowerId::PenNibPower => "Pen Nib",
                other => other.stable_id(),
            });
            let occurrence = *occurrences.entry(power_id.clone()).or_default();
            *occurrences
                .get_mut(&power_id)
                .expect("power occurrence was inserted") += 1;
            let state = match power.power_type {
                PowerId::EchoForm => {
                    if owner != 0
                        || power.amount <= 0
                        || !(0..=power.amount).contains(&power.extra_data)
                        || power.just_applied
                    {
                        return Err(
                            "Rust Echo Form carries invalid owner, stacks or repeat count".into(),
                        );
                    }
                    NaturalCombatPowerStateV4::EchoForm {
                        cards_doubled_this_turn: power.extra_data,
                    }
                }
                PowerId::PenNibPower => {
                    if owner != 0
                        || power.amount != 1
                        || power.extra_data != 0
                        || power.just_applied
                    {
                        return Err("Rust Pen Nib is not the original player marker".into());
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::EndTurnDeathPower => {
                    if owner != 0
                        || power.amount != -1
                        || power.extra_data != 0
                        || power.just_applied
                    {
                        return Err(
                            "Rust Blasphemy death power has invalid owner or sentinel state".into(),
                        );
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::Poison => {
                    if power.amount <= 0 || power.extra_data != 0 || power.just_applied {
                        return Err("Rust Poison carries invalid amount or private state".into());
                    }
                    let PowerPayload::SourceEntity(source) = power.payload else {
                        return Err("Rust Poison omitted its original creature source".into());
                    };
                    NaturalCombatPowerStateV4::Poison {
                        source: project_creature_v4(state, source)?,
                    }
                }
                PowerId::Stasis => {
                    stasis_held_card(state, owner, power)?;
                    NaturalCombatPowerStateV4::Stasis
                }
                PowerId::Constricted => {
                    if power.amount <= 0 || power.extra_data != 0 || power.just_applied {
                        return Err(
                            "Rust Constricted carries invalid amount or private state".to_owned()
                        );
                    }
                    let PowerPayload::SourceEntity(source) = power.payload else {
                        return Err(
                            "Rust Constricted omitted its original creature source".to_owned()
                        );
                    };
                    NaturalCombatPowerStateV4::Constricted {
                        source: project_creature_v4(state, source)?,
                    }
                }
                PowerId::Strength | PowerId::Dexterity | PowerId::Focus | PowerId::Draw => {
                    if power.extra_data != 0 || power.just_applied {
                        return Err(format!(
                            "Rust {:?} carries unsupported private state",
                            power.power_type
                        ));
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::Vulnerable | PowerId::Weak | PowerId::Frail => {
                    if power.extra_data != 0 {
                        return Err(format!(
                            "Rust {:?} carries unsupported extra data",
                            power.power_type
                        ));
                    }
                    NaturalCombatPowerStateV4::RoundDuration {
                        just_applied: power.just_applied,
                    }
                }
                PowerId::Intangible => {
                    if power.amount <= 0 || power.extra_data != 0 {
                        return Err(
                            "Rust Intangible carries invalid amount or extra data".to_owned()
                        );
                    }
                    NaturalCombatPowerStateV4::TurnDuration {
                        just_applied: power.just_applied,
                    }
                }
                PowerId::DrawReduction => {
                    if power.amount <= 0 || power.extra_data != 0 {
                        return Err(
                            "Rust Draw Reduction carries invalid duration or extra data".to_owned()
                        );
                    }
                    NaturalCombatPowerStateV4::RoundDuration {
                        just_applied: power.just_applied,
                    }
                }
                PowerId::CurlUp => {
                    if power.amount <= 0 || !matches!(power.extra_data, 0 | 1) || power.just_applied
                    {
                        return Err(
                            "Rust Curl Up carries an invalid amount or triggered state".to_owned()
                        );
                    }
                    if power.extra_data == 1 {
                        NaturalCombatPowerStateV4::CurlUpTriggered
                    } else {
                        NaturalCombatPowerStateV4::Stateless
                    }
                }
                PowerId::Flight => {
                    if power.amount <= 0 || power.extra_data <= 0 || power.just_applied {
                        return Err(
                            "Rust Flight carries invalid current or stored amount".to_owned()
                        );
                    }
                    NaturalCombatPowerStateV4::Flight {
                        stored_amount: power.extra_data,
                    }
                }
                PowerId::Combust => {
                    if power.amount <= 0 || power.extra_data <= 0 || power.just_applied {
                        return Err("Rust Combust carries invalid damage or HP loss".to_owned());
                    }
                    NaturalCombatPowerStateV4::Combust {
                        hp_loss: power.extra_data,
                    }
                }
                PowerId::Anger
                | PowerId::Angry
                | PowerId::Metallicize
                | PowerId::Buffer
                | PowerId::PlatedArmor
                | PowerId::Regen
                | PowerId::Regeneration
                | PowerId::Thorns
                | PowerId::FlameBarrier
                | PowerId::Rage
                | PowerId::FireBreathing
                | PowerId::IntangiblePlayer
                | PowerId::Hex
                | PowerId::GenericStrengthUp
                | PowerId::BeatOfDeath
                | PowerId::Explosive
                | PowerId::Fading
                | PowerId::SporeCloud
                | PowerId::Artifact
                | PowerId::ModeShift
                | PowerId::SharpHide
                | PowerId::Thievery
                | PowerId::FeelNoPain
                | PowerId::DarkEmbrace
                | PowerId::Rupture
                | PowerId::Juggernaut
                | PowerId::Brutality
                | PowerId::Berserk
                | PowerId::DemonForm
                | PowerId::Evolve
                | PowerId::Curiosity
                | PowerId::Vigor
                | PowerId::LoseStrength
                | PowerId::DexterityDown
                | PowerId::DoubleTap
                | PowerId::DuplicationPower
                | PowerId::ForesightPower
                | PowerId::MentalFortressPower
                | PowerId::BlockReturnPower
                | PowerId::Mantra
                | PowerId::BattleHymnPower
                | PowerId::RushdownPower
                | PowerId::NirvanaPower
                | PowerId::WaveOfTheHandPower
                | PowerId::NoxiousFumes
                | PowerId::Envenom
                | PowerId::CorpseExplosion
                | PowerId::ToolsOfTheTrade
                | PowerId::RetainCards
                | PowerId::Entangle
                | PowerId::Repair
                | PowerId::Energized
                | PowerId::EnergizedBlue
                | PowerId::DrawCardNextTurn
                | PowerId::Choked
                | PowerId::Bias
                | PowerId::LockOn
                | PowerId::Loop
                | PowerId::Equilibrium
                | PowerId::Heatsink
                | PowerId::Hello
                | PowerId::CreativeAI
                | PowerId::Storm
                | PowerId::StaticDischarge
                | PowerId::Amplify
                | PowerId::Blur
                | PowerId::AfterImage => {
                    if power.amount <= 0 || power.extra_data != 0 || power.just_applied {
                        return Err(format!(
                            "Rust {:?} is not a positive stateless stable-boundary power",
                            power.power_type
                        ));
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::WraithForm => {
                    if power.amount == 0 || power.extra_data != 0 || power.just_applied {
                        return Err(
                            "Rust Wraith Form carries invalid signed rate or private state".into(),
                        );
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::Slow => {
                    if power.amount < 0 || power.extra_data != 0 || power.just_applied {
                        return Err("Rust Slow carries invalid amount or private state".to_owned());
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::TimeWarp => {
                    if !(0..12).contains(&power.amount)
                        || power.extra_data != 0
                        || power.just_applied
                    {
                        return Err(
                            "Rust Time Warp carries invalid count or private state".to_owned()
                        );
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::Invincible => {
                    if power.extra_data <= 0
                        || !(0..=power.extra_data).contains(&power.amount)
                        || power.just_applied
                    {
                        return Err(
                            "Rust Invincible carries invalid remaining or maximum amount"
                                .to_owned(),
                        );
                    }
                    NaturalCombatPowerStateV4::Invincible {
                        max_amount: power.extra_data,
                    }
                }
                PowerId::Malleable => {
                    if power.amount <= 0 || power.extra_data <= 0 || power.just_applied {
                        return Err(
                            "Rust Malleable carries invalid current or base amount".to_owned()
                        );
                    }
                    NaturalCombatPowerStateV4::Malleable {
                        base_power: power.extra_data,
                    }
                }
                PowerId::Surrounded | PowerId::BackAttack => {
                    // These sentinels have no expiration hook. Rust's generic
                    // just_applied bookkeeping has no game semantics here.
                    if power.amount != -1 || power.extra_data != 0 {
                        return Err(format!(
                            "Rust {:?} is not the original-game sentinel",
                            power.power_type
                        ));
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::Corruption
                | PowerId::Confusion
                | PowerId::Reactive
                | PowerId::Split
                | PowerId::Minion
                | PowerId::Regrow
                | PowerId::Shifting
                | PowerId::NoDraw
                | PowerId::Barricade
                | PowerId::PainfulStabs
                | PowerId::Electro
                | PowerId::Unawakened => {
                    if power.amount != -1 || power.extra_data != 0 || power.just_applied {
                        return Err(format!(
                            "Rust {:?} is not the stateless original-game sentinel",
                            power.power_type
                        ));
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::Shackled => {
                    if power.amount == 0
                        || !(-999..=999).contains(&power.amount)
                        || power.extra_data != 0
                        || power.just_applied
                    {
                        return Err(
                            "Rust Shackled carries invalid amount or private state".to_owned()
                        );
                    }
                    NaturalCombatPowerStateV4::Stateless
                }
                PowerId::Ritual => {
                    if power.just_applied {
                        return Err("Rust Ritual carries an unexpected generic flag".to_owned());
                    }
                    let player_flag =
                        sts_oracle_run_control::content::powers::core::ritual::extra_data(
                            true, false,
                        );
                    let skip_flag =
                        sts_oracle_run_control::content::powers::core::ritual::extra_data(
                            false, true,
                        );
                    if power.extra_data & !(player_flag | skip_flag) != 0 {
                        return Err("Rust Ritual carries unknown private flag bits".to_owned());
                    }
                    NaturalCombatPowerStateV4::Ritual {
                        skip_first: power.extra_data & skip_flag != 0,
                        player_controlled: power.extra_data & player_flag != 0,
                    }
                }
                other => {
                    return Err(format!(
                        "natural combat schema v4 does not model power {:?}",
                        other
                    ))
                }
            };
            Ok(NaturalCombatPowerV4 {
                power_id,
                occurrence,
                amount: power.amount,
                state,
            })
        })
        .collect()
}

fn project_monster_powers_v4(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
    owner: usize,
    dead_or_escaped: bool,
) -> Result<Vec<NaturalCombatPowerV4>, String> {
    // Rust retains a dead monster's powers long enough for Java-ordered death
    // hooks such as The Specimen.  Java later clears the same list from a
    // wall-clock animation callback.  Neither representation is gameplay
    // state once a stable decision reports the monster as terminal.
    if dead_or_escaped {
        Ok(Vec::new())
    } else {
        project_powers_v4(state, owner)
    }
}

fn validate_smoke_reward(
    session: &RunControlSession,
    rewards: &sts_oracle_run_control::state::rewards::RewardState,
) -> Result<(), String> {
    if !session.run_state.room_smoked
        || session.active_combat.is_some()
        || session.run_state.current_hp <= 0
        || !rewards.items.is_empty()
        || rewards.pending_card_choice.is_some()
        || rewards.pending_card_reward_index.is_some()
        || !rewards.skippable
    {
        return Err("Smoke Bomb reward lacks its closed, living, empty escape boundary".into());
    }
    Ok(())
}

/// The same reward objects survive opening a card choice and returning to the
/// shop. Keep their projection and typed acquisition path shared.
pub(crate) fn reward_state(
    session: &RunControlSession,
) -> Result<Option<&sts_oracle_run_control::state::rewards::RewardState>, String> {
    match &session.engine_state {
        EngineState::RewardScreen(rewards) => Ok(Some(rewards)),
        EngineState::RewardOverlay { reward_state, .. } => {
            crate::shop::state(session)?;
            if reward_state.screen_context != RewardScreenContext::Standard
                || !reward_state.skippable
            {
                return Err(
                    "Rust shop reward overlay has an unsupported opening context".to_owned(),
                );
            }
            Ok(Some(reward_state))
        }
        EngineState::Shop(shop) => {
            if shop.pending_reward_overlay.as_ref().is_some_and(|r| {
                r.screen_context != RewardScreenContext::Standard
                    || !r.skippable
                    || r.pending_card_choice.is_some()
                    || r.pending_card_reward_index.is_some()
                    || r.items.is_empty()
            }) {
                return Err(
                    "Rust closed shop rewards contain an invalid pending surface".to_owned(),
                );
            }
            Ok(shop.pending_reward_overlay.as_ref())
        }
        EngineState::MapOverlay { return_state } => match return_state.as_ref() {
            EngineState::RewardScreen(rewards) => Ok(Some(rewards)),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

pub(crate) fn rewards(session: &RunControlSession) -> Result<NaturalRunRewardsV2, String> {
    let Some(rewards) = reward_state(session)? else {
        return Ok(NaturalRunRewardsV2::Inactive);
    };
    if rewards.screen_context == RewardScreenContext::SmokedCombat {
        validate_smoke_reward(session, rewards)?;
    }
    if rewards.pending_card_choice.is_some() != rewards.pending_card_reward_index.is_some() {
        return Err("Rust reward carries a partial open card choice".to_owned());
    }
    let items = rewards
        .items
        .iter()
        .map(|item| match item {
            RewardItem::Gold { amount } => Ok(NaturalRunRewardItemV1::Gold { amount: *amount }),
            RewardItem::StolenGold { amount } => {
                Ok(NaturalRunRewardItemV1::StolenGold { amount: *amount })
            }
            RewardItem::Card { cards } => Ok(NaturalRunRewardItemV1::Card {
                cards: cards
                    .iter()
                    .map(|card| NaturalRewardCardV1 {
                        card_id: canonical_game_id(card.id.stable_id()),
                        upgrades: card.upgrades,
                    })
                    .collect(),
            }),
            RewardItem::Relic { relic_id } => Ok(NaturalRunRewardItemV1::Relic {
                relic_id: canonical_game_id(relic_id.stable_id()),
            }),
            RewardItem::Potion { potion_id } => Ok(NaturalRunRewardItemV1::Potion {
                potion_id: canonical_game_id(potion_id.stable_id()),
            }),
            RewardItem::EmeraldKey => Ok(NaturalRunRewardItemV1::EmeraldKey),
            RewardItem::SapphireKey => Ok(NaturalRunRewardItemV1::SapphireKey),
        })
        .collect::<Result<Vec<_>, String>>()?;
    match rewards.screen_context {
        RewardScreenContext::NeowCardChoice => Ok(NaturalRunRewardsV2::Inactive),
        RewardScreenContext::Standard | RewardScreenContext::SmokedCombat => {
            Ok(NaturalRunRewardsV2::Combat {
                items,
                skippable: rewards.pending_card_choice.is_none()
                    && rewards.skippable
                    && !matches!(session.engine_state, EngineState::Shop(_)),
            })
        }
        RewardScreenContext::TreasureRoom => Ok(NaturalRunRewardsV2::Treasure {
            items,
            skippable: rewards.skippable,
        }),
        RewardScreenContext::MuggedCombat => {
            Err("natural reward schema v2 does not support this reward context".to_owned())
        }
    }
}

fn hidden_reason_token(reason: HiddenInformationReasonV1) -> String {
    match reason {
        HiddenInformationReasonV1::RunicDome => "runic_dome",
        HiddenInformationReasonV1::IntentNotVisible => "intent_not_visible",
        HiddenInformationReasonV1::DrawPileOrderHidden => "draw_pile_order_hidden",
    }
    .to_owned()
}

fn rng(
    owner: &str,
    scope: &str,
    rng: &StsRng,
    comparable_counter: bool,
) -> Result<NaturalRunRngStreamV2, String> {
    NaturalRunRngStreamV2::new(
        owner,
        scope,
        [format!("{:016X}", rng.seed0), format!("{:016X}", rng.seed1)],
        comparable_counter.then_some(rng.counter),
    )
}

fn card_pools(session: &RunControlSession) -> Result<Value, String> {
    let pool = |rarity| -> Result<&'static [CardId], String> {
        match session.run_state.player_class {
            "Ironclad" => Ok(ironclad_pool_for_rarity(rarity)),
            "Silent" => Ok(silent_pool_for_rarity(rarity)),
            "Defect" => Ok(defect_pool_for_rarity(rarity)),
            "Watcher" => Ok(watcher_pool_for_rarity(rarity)),
            other => Err(format!("unsupported player class {other:?}")),
        }
    };
    let ids = |cards: &[CardId]| {
        cards
            .iter()
            .map(|card| canonical_game_id(card.stable_id()))
            .collect::<Vec<_>>()
    };
    Ok(json!({
        "common": ids(pool(CardRarity::Common)?),
        "uncommon": ids(pool(CardRarity::Uncommon)?),
        "rare": ids(pool(CardRarity::Rare)?),
        "colorless_uncommon": ids(colorless_pool_for_rarity(CardRarity::Uncommon)),
        "colorless_rare": ids(colorless_pool_for_rarity(CardRarity::Rare)),
        "colorless_combat": ids(&random_colorless_in_combat_pool()),
    }))
}

fn character_id(player_class: &str) -> Result<&'static str, String> {
    match player_class {
        "Ironclad" => Ok("IRONCLAD"),
        "Silent" => Ok("THE_SILENT"),
        "Defect" => Ok("DEFECT"),
        "Watcher" => Ok("WATCHER"),
        other => Err(format!("unsupported player class {other:?}")),
    }
}

fn project_relic_v3(
    relic: &RelicState,
    ordinal: usize,
    bound_card: Option<String>,
    combat_active: bool,
) -> Result<NaturalRelicV3, String> {
    let combat_state = relic_combat_state(relic, combat_active)?;
    Ok(NaturalRelicV3 {
        ordinal: ordinal as u64,
        id: canonical_game_id(relic.id.stable_id()),
        // These Rust counters encode private turn flags, not the original
        // AbstractRelic.counter. Preserve the flags separately below.
        counter: if matches!(relic.id, RelicId::ArtOfWar | RelicId::OrangePellets) {
            -1
        } else {
            relic.counter.into()
        },
        // These compact flags encode private per-combat state, not
        // AbstractRelic.usedUp. Keep them separately in combat_state.
        used_up: !matches!(
            relic.id,
            RelicId::Lantern
                | RelicId::CentennialPuzzle
                | RelicId::RedSkull
                | RelicId::HoveringKite
        ) && relic.used_up,
        bound_card,
        combat_state,
    })
}

fn project_zones_v4(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
    hand: Vec<&CombatCard>,
    project: impl Fn(&CombatCard) -> Result<NaturalCombatCardV4, String>,
) -> Result<NaturalCombatZonesV4, String> {
    let mut held = BTreeSet::new();
    let mut stasis = Vec::new();
    for monster in state
        .entities
        .monsters
        .iter()
        .filter(|m| !m.is_dead_or_escaped())
    {
        for power in sts_oracle_run_control::content::powers::store::powers_for(state, monster.id)
            .unwrap_or_default()
            .iter()
            .filter(|p| p.power_type == PowerId::Stasis)
        {
            let card = stasis_held_card(state, monster.id, power)?;
            if !held.insert(card.lineage) {
                return Err("Rust Stasis owners hold the same card".into());
            }
            stasis.push(sts_parity_lockstep::NaturalCombatStasisCardV4 {
                owner: project_creature_v4(state, monster.id)?,
                card: project(card)?,
            });
        }
    }
    let mut exhaust_by_binding = state
        .zones
        .exhaust_pile
        .iter()
        .map(&project)
        .collect::<Result<Vec<_>, _>>()?;
    canonicalize_exhaust_by_binding_v4(&mut exhaust_by_binding)?;
    Ok(NaturalCombatZonesV4 {
        draw_next_first: state
            .zones
            .draw_pile
            .iter()
            .map(&project)
            .collect::<Result<_, _>>()?,
        hand: hand.into_iter().map(&project).collect::<Result<_, _>>()?,
        discard_bottom_first: state
            .zones
            .discard_pile
            .iter()
            .map(&project)
            .collect::<Result<_, _>>()?,
        exhaust_by_binding,
        limbo: state
            .zones
            .limbo
            .iter()
            .filter(|card| !held.contains(&card.lineage))
            .map(|card| {
                let mut projected = project(card)?;
                // Cost is already paid; UseCardDone clears this waiver before disposal.
                projected.free_to_play_once = false;
                Ok::<_, String>(projected)
            })
            .collect::<Result<_, _>>()?,
        stasis,
    })
}

fn stasis_held_card<'a>(
    state: &'a sts_oracle_run_control::runtime::combat::CombatState,
    owner: usize,
    power: &sts_oracle_run_control::runtime::combat::Power,
) -> Result<&'a CombatCard, String> {
    if owner == 0
        || power.amount != -1
        || power.just_applied
        || power.instance_id.is_some()
        || !matches!(power.payload, PowerPayload::None)
    {
        return Err("Rust Stasis has invalid owner or private state".into());
    }
    let lineage =
        sts_oracle_run_control::content::powers::core::stasis::held_card_lineage(power.extra_data)
            .ok_or("Rust Stasis has no held card lineage")?;
    let mut cards = state
        .zones
        .limbo
        .iter()
        .filter(|card| card.lineage == lineage);
    let card = cards
        .next()
        .ok_or("Rust Stasis held card is absent from custody")?;
    if cards.next().is_some()
        || state
            .zones
            .hand
            .iter()
            .chain(state.zones.draw_pile.iter())
            .chain(state.zones.discard_pile.iter())
            .chain(state.zones.exhaust_pile.iter())
            .any(|card| card.lineage == lineage)
    {
        return Err("Rust Stasis held card also occurs in another card location".into());
    }
    Ok(card)
}

/// Projects an already constructed recipe's cards and unbound relics with normal
/// field semantics. The caller must prove scenario-local lineage bindings;
/// this function neither imports state nor establishes any binding.
pub fn project_cards_relics_mechanism_v1(
    state: &sts_oracle_run_control::runtime::combat::CombatState,
    bindings: &BTreeMap<CardLineageId, String>,
) -> Result<(NaturalCombatZonesV4, Vec<NaturalRelicV3>), String> {
    let zones = project_zones_v4(state, state.zones.hand.iter().collect(), |card| {
        require_stable_card_state_v4(card, false, false)?;
        let binding = bindings
            .get(&card.lineage)
            .ok_or("constructed card has no recipe source")?;
        project_combat_card_v4(card, binding.clone())
    })?;
    let relics = state
        .entities
        .player
        .relics
        .iter()
        .enumerate()
        .map(|(ordinal, relic)| {
            if persistent_relic_bound_card_lineage_v1(relic)?.is_some() {
                return Err("mechanism does not cover card-bound relics".into());
            }
            project_relic_v3(relic, ordinal, None, true)
        })
        .collect::<Result<_, String>>()?;
    Ok((zones, relics))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_neow_potion_rewards_have_no_map_room() {
        use sts_oracle_run_control::content::events::neow;
        let mut session = RunControlSession::new(
            sts_oracle_run_control::eval::run_control::RunControlConfig {
                seed: 2026101821,
                player_class: "Silent",
                ascension_level: 20,
                final_act: true,
                ..Default::default()
            },
        );
        neow::handle_choice(&mut session.engine_state, &mut session.run_state, 0);
        let event = session.run_state.event_state.as_ref().unwrap();
        assert_eq!(
            neow::get_choices(&session.run_state, event)[1].text,
            "Obtain 3 random potions."
        );
        neow::handle_choice(&mut session.engine_state, &mut session.run_state, 1);
        let projected = room(&session).unwrap();
        assert_eq!(projected, json!({"kind": "combat_reward", "current": null}));
        let NaturalRunRewardsV2::Combat { items, .. } = rewards(&session).unwrap() else {
            panic!("expected the Neow potion reward screen")
        };
        assert_eq!(items.len(), 3);
        assert!(items
            .iter()
            .all(|item| matches!(item, NaturalRunRewardItemV1::Potion { .. })));

        // Ordinary rewards keep their real room identity. An absent room is
        // valid only for Neow, not an excuse to accept other negative nodes.
        session.run_state.map.current_x = 3;
        session.run_state.map.current_y = 0;
        assert_eq!(room(&session).unwrap()["current"], "F01X03");
        session.run_state.map.current_x = -1;
        session.run_state.map.current_y = -1;
        session.run_state.event_state.as_mut().unwrap().id = EventId::Cleric;
        assert!(room(&session).is_err());
    }

    #[test]
    fn watcher_mantra_total_survives_spending_current_power_and_preserves_overflow() {
        use sts_oracle_run_control::{
            engine::action_handlers::powers::handle_apply_power,
            test_support::{blank_test_combat, test_monster},
        };
        let mut state = blank_test_combat();
        state.meta.player_class = "Watcher".into();
        state.entities.monsters = vec![test_monster(EnemyId::Cultist)];
        handle_apply_power(0, 0, PowerId::Mantra, 5, &mut state);
        handle_apply_power(0, 0, PowerId::Mantra, 5, &mut state);
        assert!(!store::has_power(&state, 0, PowerId::Mantra));
        assert_eq!(
            project_creatures_mechanism_v1(&state)
                .unwrap()
                .0
                .mantra_gained_this_combat,
            Some(10)
        );
        handle_apply_power(0, 0, PowerId::Mantra, 2, &mut state);
        assert_eq!(store::power_amount(&state, 0, PowerId::Mantra), 2);
        assert_eq!(
            project_creatures_mechanism_v1(&state)
                .unwrap()
                .0
                .mantra_gained_this_combat,
            Some(12)
        );
        state.meta.player_class = "Ironclad".into();
        assert_eq!(
            project_creatures_mechanism_v1(&state)
                .unwrap()
                .0
                .mantra_gained_this_combat,
            None
        );
    }

    #[test]
    fn silent_player_projection_keeps_the_runtime_discard_counter() {
        use sts_oracle_run_control::test_support::{blank_test_combat, test_monster};
        let mut state = blank_test_combat();
        state.meta.player_class = "Silent".into();
        state.entities.monsters = vec![test_monster(
            sts_oracle_run_control::content::monsters::EnemyId::Cultist,
        )];
        for count in [0, 1, 4] {
            state.turn.counters.cards_discarded_this_turn = count;
            let (player, _, _) = project_creatures_mechanism_v1(&state).unwrap();
            assert_eq!(player.cards_discarded_this_turn, Some(u32::from(count)));
        }
    }

    #[test]
    fn shrine_pool_retires_only_for_the_actual_spire_heart_exit() {
        use sts_oracle_run_control::state::events::EventState;
        let mut session = RunControlSession::new(Default::default());
        let baseline = run(&session).unwrap();
        assert!(!baseline["event_schedule"]["shrines"]
            .as_array()
            .unwrap()
            .is_empty());
        for act in 1..=4 {
            session.run_state.act_num = act;
            for daily in [false, true] {
                session.run_state.is_daily_run = daily;
                for event in [EventId::SpireHeart, EventId::GoldenShrine] {
                    session.run_state.event_state = Some(EventState::new(event));
                    session.engine_state = EngineState::EventRoom;
                    let projected = run(&session).unwrap();
                    assert_eq!(
                        projected["event_schedule"]["shrines"]
                            .as_array()
                            .unwrap()
                            .is_empty(),
                        act == 3 && !daily && event == EventId::SpireHeart
                    );
                    for field in ["events", "one_time_events", "chance_bits"] {
                        assert_eq!(
                            projected["event_schedule"][field],
                            baseline["event_schedule"][field]
                        );
                    }
                }
            }
        }
        session.run_state.act_num = 3;
        session.run_state.is_daily_run = false;
        session.run_state.event_state = Some(EventState::new(EventId::SpireHeart));
        // A stale event object at a different engine boundary is not an exit.
        session.engine_state = EngineState::MapNavigation;
        assert_eq!(
            run(&session).unwrap()["event_schedule"],
            baseline["event_schedule"]
        );
        session.engine_state = EngineState::EventRoom;
        session
            .run_state
            .event_generator
            .shrine_pool
            .push(session.run_state.event_generator.shrine_pool[0]);
        assert!(run(&session).is_err());
    }

    #[test]
    fn parasite_life_suck_public_intent_matches_canonical_intent() {
        use sts_oracle_run_control::{
            content::monsters::EnemyId,
            runtime::{
                combat::Intent,
                monster_move::{AttackSpec, DamageKind, MonsterMoveSpec},
            },
            test_support::{blank_test_combat, test_monster},
        };
        let mut combat = blank_test_combat();
        combat.meta.ascension_level = 20;
        let mut monster = test_monster(EnemyId::ShelledParasite);
        monster.id = 7;
        let spec = MonsterMoveSpec::AttackBuffIntent(AttackSpec {
            base_damage: 12,
            hits: 1,
            damage_kind: DamageKind::Normal,
        });
        monster.set_planned_move_id(3);
        monster.set_planned_visible_spec(Some(spec));
        combat.entities.monsters.push(monster);
        let facts = combat_public_intent_facts_v1(&combat, 7);
        let (_, _, monsters) = project_creatures_mechanism_v1(&combat).unwrap();
        assert!(matches!(
            facts.intent,
            Some(Intent::AttackBuff {
                damage: 12,
                hits: 1
            })
        ));
        assert_eq!(
            monsters[0].public_intent.kind.as_deref(),
            Some("attack_buff")
        );
        assert_eq!(monsters[0].public_intent.damage_per_hit, Some(12));
        assert_eq!(monsters[0].public_intent.hits, Some(1));
    }

    #[test]
    fn hovering_kite_projects_trigger_and_turn_reset_without_consuming_relic() {
        use sts_oracle_run_control::content::relics::hovering_kite;
        let state = sts_oracle_run_control::test_support::blank_test_combat();
        let mut relic = RelicState::new(RelicId::HoveringKite);
        for triggered in [false, true, false] {
            if triggered {
                assert_eq!(hovering_kite::on_discard(&state, &mut relic).len(), 1);
                assert!(hovering_kite::on_discard(&state, &mut relic).is_empty());
            } else {
                hovering_kite::at_turn_start(&mut relic);
            }
            for active in [false, true] {
                let projected = project_relic_v3(&relic, 0, None, active).unwrap();
                assert!(!projected.used_up);
                assert_eq!(
                    projected.combat_state,
                    active.then_some(NaturalRelicCombatStateV1::HoveringKite {
                        triggered_this_turn: triggered
                    })
                );
            }
        }
        assert_eq!(hovering_kite::on_discard(&state, &mut relic).len(), 1);
    }

    #[test]
    fn red_skull_projects_private_activation_without_consuming_the_relic() {
        use sts_oracle_run_control::content::relics::red_skull;
        let mut relic = RelicState::new(RelicId::RedSkull);
        for active in [false, true] {
            relic.used_up = active;
            for combat_active in [false, true] {
                let projected = project_relic_v3(&relic, 0, None, combat_active).unwrap();
                assert!(!projected.used_up);
                assert_eq!(
                    projected.combat_state,
                    combat_active.then_some(NaturalRelicCombatStateV1::RedSkull { active })
                );
            }
        }
        red_skull::on_victory(&mut relic);
        assert_eq!(
            project_relic_v3(&relic, 0, None, true)
                .unwrap()
                .combat_state,
            Some(NaturalRelicCombatStateV1::RedSkull { active: false })
        );
    }

    #[test]
    fn centennial_puzzle_preserves_trigger_state_without_claiming_consumed_relic() {
        use sts_oracle_run_control::content::relics::centennial_puzzle::CentennialPuzzle;
        let mut relic = RelicState::new(RelicId::CentennialPuzzle);
        assert!(CentennialPuzzle::on_lose_hp(&mut relic, 0).is_empty());
        assert!(!relic.used_up);
        assert_eq!(CentennialPuzzle::on_lose_hp(&mut relic, 3).len(), 1);
        assert!(CentennialPuzzle::on_lose_hp(&mut relic, 1).is_empty());
        let projected = project_relic_v3(&relic, 0, None, true).unwrap();
        assert!(!projected.used_up);
        assert_eq!(
            projected.combat_state,
            Some(NaturalRelicCombatStateV1::CentennialPuzzle {
                used_this_combat: true
            })
        );
        assert_eq!(
            project_relic_v3(&relic, 0, None, false)
                .unwrap()
                .combat_state,
            None
        );
        CentennialPuzzle::at_pre_battle(&mut relic);
        assert_eq!(
            project_relic_v3(&relic, 0, None, true)
                .unwrap()
                .combat_state,
            Some(NaturalRelicCombatStateV1::CentennialPuzzle {
                used_this_combat: false
            })
        );
        assert_eq!(CentennialPuzzle::on_lose_hp(&mut relic, 1).len(), 1);
    }

    #[test]
    fn orange_pellets_projects_partial_combos_and_resets_without_a_display_counter() {
        use sts_oracle_run_control::content::relics::orange_pellets;
        let mut session = RunControlSession::new(Default::default());
        session.run_state.relics = vec![RelicState::new(RelicId::OrangePellets)];
        for mask in 0..7 {
            let relic = &mut session.run_state.relics[0];
            relic.counter = mask;
            assert_eq!(
                relic_combat_state(relic, true).unwrap(),
                Some(NaturalRelicCombatStateV1::OrangePellets {
                    attack_played: mask & 1 != 0,
                    skill_played: mask & 2 != 0,
                    power_played: mask & 4 != 0,
                })
            );
            assert_eq!(relic_combat_state(relic, false).unwrap(), None);
            assert_eq!(
                relics(&session, &BTreeMap::new()).unwrap().unwrap()[0]["counter"],
                -1
            );
        }
        let relic = &mut session.run_state.relics[0];
        for invalid in [-1, 7, 8] {
            relic.counter = invalid;
            assert!(relic_combat_state(relic, true).is_err());
        }
        orange_pellets::at_turn_start(relic);
        let clear = relic_combat_state(relic, true).unwrap();
        assert!(orange_pellets::on_use_card(CardId::Strike, relic).is_empty());
        assert!(orange_pellets::on_use_card(CardId::Defend, relic).is_empty());
        assert_ne!(relic_combat_state(relic, true).unwrap(), clear);
        assert_eq!(orange_pellets::on_use_card(CardId::Inflame, relic).len(), 1);
        assert_eq!(relic_combat_state(relic, true).unwrap(), clear);
        orange_pellets::on_use_card(CardId::Strike, relic);
        orange_pellets::at_turn_start(relic);
        assert_eq!(relic_combat_state(relic, true).unwrap(), clear);
    }

    #[test]
    fn lantern_projection_keeps_first_turn_local_without_permanent_exhaustion() {
        use sts_oracle_run_control::content::relics::lantern;
        let mut session = RunControlSession::new(Default::default());
        session.run_state.relics = vec![
            RelicState::new(RelicId::Lantern),
            RelicState::new(RelicId::LizardTail),
        ];
        session.run_state.relics[1].used_up = true;
        let state = |first_turn| Some(NaturalRelicCombatStateV1::Lantern { first_turn });
        for _ in 0..2 {
            let relic = &mut session.run_state.relics[0];
            lantern::at_pre_battle(relic);
            assert_eq!(relic_combat_state(relic, true).unwrap(), state(true));
            assert_eq!(lantern::at_turn_start(relic).len(), 1);
            assert_eq!(relic_combat_state(relic, true).unwrap(), state(false));
            assert!(lantern::at_turn_start(relic).is_empty());
            assert_eq!(relic_combat_state(relic, true).unwrap(), state(false));
            assert_eq!(relic_combat_state(relic, false).unwrap(), None);
            let projected = relics(&session, &BTreeMap::new()).unwrap().unwrap();
            assert_eq!(projected[0]["used_up"], false);
            assert_eq!(projected[1]["used_up"], true);
        }
    }

    #[test]
    fn art_of_war_projection_preserves_the_energy_schedule_not_a_display_counter() {
        use sts_oracle_run_control::content::relics::art_of_war;
        let combat = sts_oracle_run_control::test_support::blank_test_combat();
        let mut relic = RelicState::new(RelicId::ArtOfWar);
        let state = |first_turn, gain_energy_next| {
            Some(NaturalRelicCombatStateV1::ArtOfWar {
                first_turn,
                gain_energy_next,
            })
        };
        art_of_war::at_pre_battle(&mut relic);
        assert_eq!(relic_combat_state(&relic, true).unwrap(), state(true, true));
        assert!(art_of_war::at_turn_start(&combat, &mut relic).is_empty());
        assert_eq!(
            relic_combat_state(&relic, true).unwrap(),
            state(false, true)
        );
        art_of_war::on_use_card(&combat, &mut relic, CardId::Defend);
        assert_eq!(
            relic_combat_state(&relic, true).unwrap(),
            state(false, true)
        );
        art_of_war::on_use_card(&combat, &mut relic, CardId::Strike);
        assert_eq!(
            relic_combat_state(&relic, true).unwrap(),
            state(false, false)
        );
        assert!(art_of_war::at_turn_start(&combat, &mut relic).is_empty());
        assert_eq!(
            relic_combat_state(&relic, true).unwrap(),
            state(false, true)
        );
        assert_eq!(art_of_war::at_turn_start(&combat, &mut relic).len(), 1);
        assert_eq!(relic_combat_state(&relic, false).unwrap(), None);
        relic.counter = 2;
        assert!(relic_combat_state(&relic, true).is_err());
        art_of_war::at_pre_battle(&mut relic);
        assert_eq!(relic_combat_state(&relic, true).unwrap(), state(true, true));
    }

    use sts_oracle_run_control::{
        content::{
            monsters::EnemyId,
            powers::{store, PowerId},
        },
        runtime::combat::{Power, PowerPayload},
    };

    #[test]
    fn stasis_projects_card_custody_without_treating_it_as_a_pending_play() {
        use sts_oracle_run_control::{
            content::powers::core::stasis::power_holding_card,
            test_support::{blank_test_combat, test_monster},
        };
        use sts_parity_lockstep::NaturalCombatCreatureV4;
        let mut state = blank_test_combat();
        let mut orb = test_monster(EnemyId::BronzeOrb);
        orb.id = 72;
        state.entities.monsters = vec![orb];
        let mut held = CombatCard::for_scenario(CardId::Defend, 1);
        held.upgrades = 1;
        let resolving = CombatCard::for_scenario(CardId::BurningPact, 2);
        state.zones.limbo = vec![held.clone(), resolving.clone()];
        store::set_powers_for(&mut state, 72, vec![power_holding_card(held.lineage)]);
        let project = |card: &CombatCard| {
            project_combat_card_v4(
                card,
                if card.lineage == held.lineage {
                    "held"
                } else {
                    "resolving"
                }
                .into(),
            )
        };
        let zones = project_zones_v4(&state, vec![], project).unwrap();
        assert_eq!(zones.limbo.len(), 1);
        assert_eq!(zones.limbo[0].binding, "resolving");
        assert_eq!(zones.stasis.len(), 1);
        assert_eq!(zones.stasis[0].card.binding, "held");
        assert_eq!(zones.stasis[0].card.upgrades, 1);
        assert_eq!(
            zones.stasis[0].owner,
            NaturalCombatCreatureV4::Monster {
                enemy_id: "bronzeorb".into(),
                occurrence: 0,
            }
        );
        assert_eq!(
            project_powers_v4(&state, 72).unwrap()[0].state,
            NaturalCombatPowerStateV4::Stasis
        );
        let mut invalid = state.clone();
        invalid.zones.hand.push(held.clone());
        assert!(project_zones_v4(&invalid, vec![], project).is_err());
        invalid = state.clone();
        invalid.zones.limbo.remove(0);
        assert!(project_powers_v4(&invalid, 72).is_err());
        invalid = state.clone();
        let mut second = invalid.entities.monsters[0].clone();
        second.id = 8;
        invalid.entities.monsters.push(second);
        store::set_powers_for(&mut invalid, 8, vec![power_holding_card(held.lineage)]);
        assert!(project_zones_v4(&invalid, vec![], project).is_err());
        // Once the holder dies and the card returns, only the ordinary pile owns it.
        state.entities.monsters[0].current_hp = 0;
        state.entities.monsters[0].is_dying = true;
        state.zones.limbo.remove(0);
        state.zones.hand.push(held);
        let zones = project_zones_v4(&state, state.zones.hand.iter().collect(), |card| {
            project_combat_card_v4(card, "returned".into())
        })
        .unwrap();
        assert!(zones.stasis.is_empty());
        assert_eq!(zones.hand[0].binding, "returned");
    }

    #[test]
    fn constricted_projects_source_identity_and_rejects_missing_or_unbound_sources() {
        assert_creature_power_source_projection(PowerId::Constricted);
    }

    #[test]
    fn blasphemy_death_power_preserves_native_sentinel_without_private_state() {
        use sts_oracle_run_control::{
            engine::action_handlers::powers::handle_apply_power, test_support::blank_test_combat,
        };
        let mut state = blank_test_combat();
        handle_apply_power(0, 0, PowerId::EndTurnDeathPower, -1, &mut state);
        let projected = project_powers_v4(&state, 0).unwrap();
        assert_eq!(projected[0].power_id, "endturndeathpower");
        assert_eq!(projected[0].amount, -1);
        assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        for (amount, extra_data, just_applied) in
            [(0, 0, false), (1, 0, false), (-1, 1, false), (-1, 0, true)]
        {
            let mut invalid = state.clone();
            let power = &mut invalid.entities.power_db.get_mut(&0).unwrap()[0];
            power.amount = amount;
            power.extra_data = extra_data;
            power.just_applied = just_applied;
            assert!(project_powers_v4(&invalid, 0).is_err());
        }
        handle_apply_power(0, 7, PowerId::EndTurnDeathPower, -1, &mut state);
        assert!(project_powers_v4(&state, 7).is_err());
    }

    #[test]
    fn poison_projects_source_identity_and_rejects_missing_or_unbound_sources() {
        assert_creature_power_source_projection(PowerId::Poison);
    }

    fn assert_creature_power_source_projection(power_id: PowerId) {
        use sts_oracle_run_control::{
            engine::action_handlers::powers::handle_apply_power,
            test_support::{blank_test_combat, test_monster},
        };
        use sts_parity_lockstep::NaturalCombatCreatureV4;
        let with_source = |source| match power_id {
            PowerId::Poison => NaturalCombatPowerStateV4::Poison { source },
            _ => NaturalCombatPowerStateV4::Constricted { source },
        };
        let mut state = blank_test_combat();
        let mut first = test_monster(EnemyId::SpireGrowth);
        first.id = 72;
        let mut second = first.clone();
        second.id = 8;
        state.entities.monsters = vec![first, second];
        handle_apply_power(8, 0, power_id, 10, &mut state);
        handle_apply_power(72, 0, power_id, 4, &mut state);
        let powers = project_powers_v4(&state, 0).unwrap();
        assert_eq!(powers[0].amount, 14);
        assert_eq!(
            powers[0].state,
            with_source(NaturalCombatCreatureV4::Monster {
                enemy_id: "spiregrowth".to_owned(),
                occurrence: 1
            })
        );
        for invalid_payload in [
            PowerPayload::None,
            PowerPayload::SourceEntity(1),
            PowerPayload::Card(CombatCard::for_scenario(CardId::Strike, 1)),
        ] {
            let mut invalid = state.clone();
            invalid.entities.power_db.get_mut(&0).unwrap()[0].payload = invalid_payload;
            assert!(project_powers_v4(&invalid, 0).is_err());
        }
        for (amount, extra_data, just_applied) in
            [(0, 0, false), (-1, 0, false), (14, 1, false), (14, 0, true)]
        {
            let mut invalid = state.clone();
            let power = &mut invalid.entities.power_db.get_mut(&0).unwrap()[0];
            power.amount = amount;
            power.extra_data = extra_data;
            power.just_applied = just_applied;
            assert!(project_powers_v4(&invalid, 0).is_err());
        }
        let mut duplicate = state.clone();
        duplicate.entities.monsters[0].id = 8;
        assert!(project_powers_v4(&duplicate, 0).is_err());
        let mut player_source = state.clone();
        player_source.entities.power_db.get_mut(&0).unwrap()[0].payload =
            PowerPayload::SourceEntity(0);
        assert_eq!(
            project_powers_v4(&player_source, 0).unwrap()[0].state,
            with_source(NaturalCombatCreatureV4::Player)
        );
        player_source.entities.power_db.get_mut(&0).unwrap()[0].power_type = PowerId::Strength;
        assert!(project_powers_v4(&player_source, 0).is_err());
    }

    #[test]
    fn reward_rng_projection_keeps_event_and_boss_continuations_live() {
        use sts_oracle_run_control::state::map::node::MapRoomNode;
        let mut session = RunControlSession::new(Default::default());
        let mut node = MapRoomNode::new(0, 0);
        node.class = Some(RoomType::MonsterRoom);
        session.run_state.map.graph = vec![vec![node]];
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 0;
        for boundary in ["natural_combat_reward", "natural_card_reward"] {
            for (room, count) in [
                (RoomType::MonsterRoom, 9),
                (RoomType::MonsterRoomElite, 9),
                (RoomType::EventRoom, 13),
                (RoomType::MonsterRoomBoss, 13),
            ] {
                session.run_state.map.graph[0][0].class = Some(room);
                assert_eq!(
                    rng_streams(&session, boundary)
                        .unwrap()
                        .as_array()
                        .unwrap()
                        .len(),
                    count
                );
            }
        }
    }

    #[test]
    fn genetic_algorithm_master_deck_compares_growth_without_merging_copies() {
        let mut session = RunControlSession::new(Default::default());
        let mut first = CombatCard::for_scenario(CardId::GeneticAlgorithm, 811);
        first.misc_value = 3;
        let mut second = CombatCard::for_scenario(CardId::GeneticAlgorithm, 812);
        second.misc_value = 17;
        session.run_state.master_deck = vec![first, second];
        let bindings = session
            .run_state
            .master_deck
            .iter()
            .enumerate()
            .map(|(slot, card)| {
                let source = sts_parity_lockstep::NaturalRunCardSourceV1::CardReward {
                    room_id: "F01X02".into(),
                    product_slot: slot as u32,
                };
                (
                    card.lineage,
                    BoundCardSourceV1 {
                        binding: sts_parity_lockstep::natural_run_card_binding_v1(&source).unwrap(),
                        source,
                        expected_card_id: "geneticalgorithm".into(),
                        expected_upgrades: 0,
                    },
                )
            })
            .collect();
        let (before, _) = bound_deck(&session, &bindings).unwrap();
        assert_eq!(before[0]["misc"], 3);
        assert_eq!(before[1]["misc"], 17);
        session.run_state.master_deck[0].misc_value += 2;
        let (after, _) = bound_deck(&session, &bindings).unwrap();
        assert_eq!(after[0]["misc"], 5);
        assert_eq!(after[1], before[1]);
        session.run_state.master_deck[0].misc_value = 0;
        assert!(bound_deck(&session, &bindings).is_err());
    }

    #[test]
    fn persistent_misc_growth_is_compared_without_using_calculated_damage_or_block() {
        for (id, initial, upgrades, growth) in [
            (CardId::GeneticAlgorithm, 1, 0, 2),
            (CardId::GeneticAlgorithm, 1, 1, 3),
            (CardId::RitualDagger, 15, 0, 3),
            (CardId::RitualDagger, 15, 1, 5),
        ] {
            let mut card = CombatCard::for_scenario(id, 811);
            card.upgrades = upgrades;
            card.base_block_mut = 999;
            card.base_damage_mut = 999;
            for value in [initial, initial + growth, initial + 2 * growth] {
                card.misc_value = value;
                require_stable_card_state_v4(&card, false, false).unwrap();
                let projected = project_combat_card_v4(&card, "genetic-instance".into()).unwrap();
                assert_eq!(serde_json::to_value(projected).unwrap()["misc"], value);
            }
            card.misc_value = -1;
            assert!(require_stable_card_state_v4(&card, false, false).is_err());
            card.misc_value = 0;
            assert!(require_stable_card_state_v4(&card, false, false).is_err());
            card.misc_value = 1;
            card.base_block_override = Some(42);
            assert!(require_stable_card_state_v4(&card, false, false).is_err());
            card.base_block_override = None;
            card.id = CardId::Defend;
            assert!(require_stable_card_state_v4(&card, false, false).is_err());
        }
    }

    #[test]
    fn rampage_damage_accumulates_on_its_own_card_without_using_calculated_damage() {
        use sts_oracle_run_control::engine::action_handlers::cards::handle_modify_card_damage;
        for (upgrades, increment) in [(0, 5), (1, 8)] {
            let mut state = sts_oracle_run_control::test_support::blank_test_combat();
            let mut card = CombatCard::for_scenario(CardId::Rampage, 81);
            card.upgrades = upgrades;
            card.base_damage_mut = 999;
            let lineage = card.lineage;
            state.zones.hand.push(card);
            state
                .zones
                .hand
                .push(CombatCard::for_scenario(CardId::Rampage, 82));
            for count in 0..=2 {
                let card = &state.zones.hand[0];
                require_stable_card_state_v4(card, false, false).unwrap();
                assert_eq!(
                    persistent_card_damage(card).unwrap(),
                    Some(8 + count * increment)
                );
                assert_eq!(
                    persistent_card_damage(&state.zones.hand[1]).unwrap(),
                    Some(8)
                );
                handle_modify_card_damage(lineage, increment, &mut state);
            }
            state.zones.hand[0].base_damage_override = Some(-1);
            assert!(require_stable_card_state_v4(&state.zones.hand[0], false, false).is_err());
            state.zones.hand[0].id = CardId::Strike;
            state.zones.hand[0].base_damage_override = Some(13);
            assert!(require_stable_card_state_v4(&state.zones.hand[0], false, false).is_err());
        }
    }

    #[test]
    fn glass_knife_damage_decays_on_its_own_instance_and_stops_at_zero() {
        use sts_oracle_run_control::engine::action_handlers::cards::handle_modify_card_damage;
        for (upgrades, initial) in [(0, 8), (1, 12)] {
            let mut state = sts_oracle_run_control::test_support::blank_test_combat();
            let mut card = CombatCard::for_scenario(CardId::GlassKnife, 91);
            card.upgrades = upgrades;
            card.base_damage_mut = 999;
            let lineage = card.lineage;
            state.zones.hand.push(card);
            state
                .zones
                .hand
                .push(CombatCard::for_scenario(CardId::GlassKnife, 92));
            for count in 0..=8 {
                let card = &state.zones.hand[0];
                require_stable_card_state_v4(card, false, false).unwrap();
                assert_eq!(
                    persistent_card_damage(card).unwrap(),
                    Some((initial - count * 2).max(0))
                );
                assert_eq!(
                    persistent_card_damage(&state.zones.hand[1]).unwrap(),
                    Some(8)
                );
                handle_modify_card_damage(lineage, -2, &mut state);
            }
            state.zones.hand[0].base_damage_override = Some(-1);
            assert!(require_stable_card_state_v4(&state.zones.hand[0], false, false).is_err());
        }
    }

    fn project_power(
        power_type: PowerId,
        amount: i32,
        extra_data: i32,
        just_applied: bool,
    ) -> Result<Vec<NaturalCombatPowerV4>, String> {
        let mut combat = sts_oracle_run_control::test_support::blank_test_combat();
        let louse = sts_oracle_run_control::test_support::test_monster(EnemyId::LouseNormal);
        let owner = louse.id;
        combat.entities.monsters = vec![louse];
        store::set_powers_for(
            &mut combat,
            owner,
            vec![Power {
                power_type,
                instance_id: None,
                amount,
                extra_data,
                payload: PowerPayload::None,
                just_applied,
            }],
        );
        project_powers_v4(&combat, owner)
    }

    #[test]
    fn amplify_preserves_the_remaining_power_repeat_budget() {
        let powers = project_power(PowerId::Amplify, 1, 0, false).unwrap();
        assert_eq!(powers[0].power_id, "amplify");
        assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        assert!(project_power(PowerId::Amplify, 0, 0, false).is_err());
        assert!(project_power(PowerId::Amplify, 1, 1, false).is_err());
    }

    #[test]
    fn echo_form_preserves_the_independent_turn_repeat_count() {
        let mut combat = sts_oracle_run_control::test_support::blank_test_combat();
        store::set_powers_for(
            &mut combat,
            0,
            vec![Power {
                power_type: PowerId::EchoForm,
                amount: 2,
                extra_data: 0,
                payload: PowerPayload::None,
                instance_id: None,
                just_applied: false,
            }],
        );
        for count in 0..=2 {
            combat.entities.power_db.get_mut(&0).unwrap()[0].extra_data = count;
            let powers = project_powers_v4(&combat, 0).unwrap();
            assert_eq!(powers[0].power_id, "echoform");
            assert_eq!(powers[0].amount, 2);
            assert_eq!(
                powers[0].state,
                NaturalCombatPowerStateV4::EchoForm {
                    cards_doubled_this_turn: count,
                }
            );
        }
        for (amount, count, applied) in [(0, 0, false), (1, -1, false), (1, 2, false), (1, 0, true)]
        {
            let power = &mut combat.entities.power_db.get_mut(&0).unwrap()[0];
            power.amount = amount;
            power.extra_data = count;
            power.just_applied = applied;
            assert!(project_powers_v4(&combat, 0).is_err());
        }
        assert!(project_power(PowerId::EchoForm, 1, 0, false).is_err());
    }

    #[test]
    fn defect_positive_trigger_counts_reject_private_state() {
        for (kind, id) in [
            (PowerId::Heatsink, "heatsink"),
            (PowerId::Hello, "hello"),
            (PowerId::CreativeAI, "creativeai"),
            (PowerId::Storm, "storm"),
            (PowerId::StaticDischarge, "staticdischarge"),
        ] {
            for amount in [1, 2, 3] {
                let powers = project_power(kind, amount, 0, false).unwrap();
                assert_eq!(powers[0].power_id, id);
                assert_eq!(powers[0].amount, amount);
                assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
            }
            for (amount, extra, applied) in
                [(0, 0, false), (-1, 0, false), (1, 1, false), (1, 0, true)]
            {
                assert!(project_power(kind, amount, extra, applied).is_err());
            }
        }
    }

    #[test]
    fn draw_preserves_signed_hand_size_adjustments_without_private_state() {
        for amount in [-2, 0, 1, 3] {
            let powers = project_power(PowerId::Draw, amount, 0, false).unwrap();
            assert_eq!(powers[0].power_id, "draw");
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        assert!(project_power(PowerId::Draw, 1, 1, false).is_err());
        assert!(project_power(PowerId::Draw, 1, 0, true).is_err());
    }

    #[test]
    fn electro_requires_the_original_nonstacking_sentinel() {
        let powers = project_power(PowerId::Electro, -1, 0, false).unwrap();
        assert_eq!(powers[0].power_id, "electro");
        assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        for (amount, extra, applied) in
            [(0, 0, false), (1, 0, false), (-1, 1, false), (-1, 0, true)]
        {
            assert!(project_power(PowerId::Electro, amount, extra, applied).is_err());
        }
    }

    #[test]
    fn retain_cards_preserves_positive_stacks_without_private_state() {
        for amount in [1, 2, 7] {
            let powers = project_power(PowerId::RetainCards, amount, 0, false).unwrap();
            assert_eq!(powers[0].power_id, "retaincards");
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (amount, extra, applied) in [(0, 0, false), (-1, 0, false), (1, 1, false), (1, 0, true)]
        {
            assert!(project_power(PowerId::RetainCards, amount, extra, applied).is_err());
        }
    }

    #[test]
    fn equilibrium_preserves_its_round_count_without_a_just_applied_flag() {
        for amount in [1, 2] {
            let powers = project_power(PowerId::Equilibrium, amount, 0, false).unwrap();
            assert_eq!(powers[0].power_id, "equilibrium");
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (amount, extra, applied) in [(0, 0, false), (-1, 0, false), (1, 1, false), (1, 0, true)]
        {
            assert!(project_power(PowerId::Equilibrium, amount, extra, applied).is_err());
        }
    }

    #[test]
    fn loop_preserves_its_positive_stack_without_private_state() {
        for amount in [1, 2, 7] {
            let powers = project_power(PowerId::Loop, amount, 0, false).unwrap();
            assert_eq!(powers[0].power_id, "loop");
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (amount, extra, applied) in [(0, 0, false), (-1, 0, false), (1, 1, false), (1, 0, true)]
        {
            assert!(project_power(PowerId::Loop, amount, extra, applied).is_err());
        }
    }

    #[test]
    fn lock_on_projects_a_positive_round_counter_without_a_just_applied_flag() {
        let powers = project_power(PowerId::LockOn, 2, 0, false).unwrap();
        assert_eq!(powers[0].power_id, "lockon");
        assert_eq!(powers[0].amount, 2);
        assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        for (amount, extra, applied) in [(0, 0, false), (-1, 0, false), (2, 1, false), (2, 0, true)]
        {
            assert!(project_power(PowerId::LockOn, amount, extra, applied).is_err());
        }
    }

    #[test]
    fn pen_nib_projects_the_original_player_marker_and_rejects_private_state() {
        let mut combat = sts_oracle_run_control::test_support::blank_test_combat();
        store::set_powers_for(
            &mut combat,
            0,
            vec![Power {
                power_type: PowerId::PenNibPower,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                instance_id: None,
                just_applied: false,
            }],
        );
        let powers = project_powers_v4(&combat, 0).unwrap();
        assert_eq!(powers[0].power_id, "pennib");
        assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        for (amount, extra, applied) in [(0, 0, false), (2, 0, false), (1, 1, false), (1, 0, true)]
        {
            let power = &mut combat.entities.power_db.get_mut(&0).unwrap()[0];
            power.amount = amount;
            power.extra_data = extra;
            power.just_applied = applied;
            assert!(project_powers_v4(&combat, 0).is_err());
        }
    }

    #[test]
    fn invincible_preserves_an_exhausted_budget_and_its_independent_reset_amount() {
        for (amount, max_amount) in [(0, 200), (17, 300), (200, 200)] {
            let projected = project_power(PowerId::Invincible, amount, max_amount, false).unwrap();
            assert_eq!(projected[0].amount, amount);
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::Invincible { max_amount }
            );
        }
        for (amount, maximum, just_applied) in [
            (-1, 200, false),
            (201, 200, false),
            (0, 0, false),
            (20, 200, true),
        ] {
            assert!(project_power(PowerId::Invincible, amount, maximum, just_applied).is_err());
        }
    }

    #[test]
    fn flight_projects_current_and_stored_amount_independently() {
        let powers = project_power(PowerId::Flight, 2, 4, false).unwrap();
        assert_eq!(powers[0].amount, 2);
        assert_eq!(
            powers[0].state,
            NaturalCombatPowerStateV4::Flight { stored_amount: 4 }
        );
        for (amount, stored, just_applied) in [(0, 4, false), (2, 0, false), (2, 4, true)] {
            assert!(project_power(PowerId::Flight, amount, stored, just_applied).is_err());
        }
    }

    #[test]
    fn combust_projects_stored_hp_loss_independently_of_damage() {
        for (amount, hp_loss) in [(5, 1), (7, 1), (12, 2), (12, 3)] {
            let projected = project_power(PowerId::Combust, amount, hp_loss, false).unwrap();
            assert_eq!(projected[0].amount, amount);
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::Combust { hp_loss }
            );
        }
        for (amount, hp_loss, just_applied) in
            [(0, 1, false), (5, 0, false), (5, -1, false), (5, 1, true)]
        {
            assert!(project_power(PowerId::Combust, amount, hp_loss, just_applied).is_err());
        }
    }

    #[test]
    fn malleable_projects_current_and_base_amount_independently() {
        let powers = project_power(PowerId::Malleable, 7, 3, false).unwrap();
        assert_eq!(powers[0].amount, 7);
        assert_eq!(
            powers[0].state,
            NaturalCombatPowerStateV4::Malleable { base_power: 3 }
        );
        for (amount, base, just_applied) in [(0, 3, false), (7, 0, false), (7, 3, true)] {
            assert!(project_power(PowerId::Malleable, amount, base, just_applied).is_err());
        }
    }

    #[test]
    fn minion_requires_original_sentinel_without_private_state() {
        let powers = project_power(PowerId::Minion, -1, 0, false).unwrap();
        assert_eq!(powers[0].power_id, "minion");
        assert_eq!(powers[0].amount, -1);
        assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        for (amount, extra, just_applied) in
            [(0, 0, false), (1, 0, false), (-1, 1, false), (-1, 0, true)]
        {
            assert!(project_power(PowerId::Minion, amount, extra, just_applied).is_err());
        }
    }

    #[test]
    fn regrow_requires_the_original_life_link_sentinel() {
        let powers = project_power(PowerId::Regrow, -1, 0, false).unwrap();
        assert_eq!(powers[0].power_id, "regrow");
        assert_eq!(powers[0].amount, -1);
        assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        for (amount, extra, just_applied) in [(1, 0, false), (-1, 1, false), (-1, 0, true)] {
            assert!(project_power(PowerId::Regrow, amount, extra, just_applied).is_err());
        }
    }

    #[test]
    fn surrounded_sentinels_do_not_acquire_an_expiration_from_generic_bookkeeping() {
        for power in [PowerId::Surrounded, PowerId::BackAttack] {
            for just_applied in [true, false] {
                let projected = project_power(power, -1, 0, just_applied).unwrap();
                assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
                assert!(project_power(power, 1, 0, just_applied).is_err());
                assert!(project_power(power, -1, 1, just_applied).is_err());
            }
        }
    }

    #[test]
    fn stateless_powers_preserve_their_distinct_amount_contracts() {
        for (power, amount, id) in [
            (PowerId::Buffer, 1, "buffer"),
            (PowerId::Buffer, 2, "buffer"),
            (PowerId::Barricade, -1, "barricade"),
            (PowerId::DarkEmbrace, 2, "darkembrace"),
            (PowerId::FireBreathing, 6, "firebreathing"),
            (PowerId::IntangiblePlayer, 3, "intangibleplayer"),
            (PowerId::Hex, 2, "hex"),
            (PowerId::GenericStrengthUp, 5, "genericstrengthup"),
            (PowerId::Rupture, 2, "rupture"),
            (PowerId::Juggernaut, 5, "juggernaut"),
            (PowerId::Juggernaut, 7, "juggernaut"),
            (PowerId::Juggernaut, 12, "juggernaut"),
            (PowerId::Berserk, 1, "berserk"),
            (PowerId::Berserk, 2, "berserk"),
            (PowerId::DemonForm, 2, "demonform"),
            (PowerId::DemonForm, 3, "demonform"),
            (PowerId::DemonForm, 5, "demonform"),
            (PowerId::PainfulStabs, -1, "painfulstabs"),
            (PowerId::Thorns, 7, "thorns"),
            (PowerId::FlameBarrier, 4, "flamebarrier"),
            (PowerId::Rage, 3, "rage"),
            (PowerId::Rage, 8, "rage"),
            (PowerId::BeatOfDeath, 2, "beatofdeath"),
            (PowerId::Explosive, 2, "explosive"),
            (PowerId::Fading, 6, "fading"),
            (PowerId::Shifting, -1, "shifting"),
            (PowerId::NoDraw, -1, "nodraw"),
            (PowerId::Evolve, 2, "evolve"),
            (PowerId::Curiosity, 2, "curiosity"),
            (PowerId::Unawakened, -1, "unawakened"),
            (PowerId::Reactive, -1, "reactive"),
            (PowerId::Shackled, 53, "shackled"),
            (PowerId::Shackled, -53, "shackled"),
        ] {
            let projected = project_power(power, amount, 0, false).unwrap();
            assert_eq!(projected[0].power_id, id);
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
            assert!(project_power(power, 0, 0, false).is_err());
            assert!(project_power(power, amount, 1, false).is_err());
            assert!(project_power(power, amount, 0, true).is_err());
        }
        assert!(project_power(PowerId::Barricade, 1, 0, false).is_err());
        assert!(project_power(PowerId::Buffer, -1, 0, false).is_err());
        assert!(project_power(PowerId::DarkEmbrace, -1, 0, false).is_err());
        assert!(project_power(PowerId::Hex, -1, 0, false).is_err());
        assert!(project_power(PowerId::GenericStrengthUp, -1, 0, false).is_err());
        for (amount, extra, just_applied) in [(0, 0, false), (6, 1, false), (6, 0, true)] {
            assert!(project_power(PowerId::FireBreathing, amount, extra, just_applied).is_err());
        }
        assert!(project_power(PowerId::Rupture, -1, 0, false).is_err());
        assert!(project_power(PowerId::Juggernaut, -1, 0, false).is_err());
        assert!(project_power(PowerId::Berserk, -1, 0, false).is_err());
        assert!(project_power(PowerId::DemonForm, -1, 0, false).is_err());
        assert!(project_power(PowerId::PainfulStabs, 1, 0, false).is_err());
        assert!(project_power(PowerId::Thorns, -1, 0, false).is_err());
        assert!(project_power(PowerId::Rage, -1, 0, false).is_err());
        assert!(project_power(PowerId::Explosive, -1, 0, false).is_err());
        assert!(project_power(PowerId::Fading, -1, 0, false).is_err());
        assert!(project_power(PowerId::Shifting, 1, 0, false).is_err());
        assert!(project_power(PowerId::NoDraw, 1, 0, false).is_err());
        assert!(project_power(PowerId::Curiosity, -1, 0, false).is_err());
        assert!(project_power(PowerId::Unawakened, 1, 0, false).is_err());
        assert!(project_power(PowerId::Reactive, 1, 0, false).is_err());
        for amount in [-1000, 1000] {
            assert!(project_power(PowerId::Shackled, amount, 0, false).is_err());
        }
    }

    #[test]
    fn monster_intangible_preserves_its_turn_expiration_flag() {
        for just_applied in [false, true] {
            let projected = project_power(PowerId::Intangible, 1, 0, just_applied).unwrap();
            assert_eq!(projected[0].power_id, "intangible");
            assert_eq!(projected[0].amount, 1);
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::TurnDuration { just_applied }
            );
        }
        assert!(project_power(PowerId::Intangible, 0, 0, false).is_err());
        assert!(project_power(PowerId::Intangible, 1, 1, false).is_err());
    }

    #[test]
    fn slow_keeps_zero_and_positive_counts_without_private_state() {
        for amount in [0, 1, 12] {
            let projected = project_power(PowerId::Slow, amount, 0, false).unwrap();
            assert_eq!(projected[0].power_id, "slow");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (amount, extra, just_applied) in [(-1, 0, false), (0, 1, false), (1, 0, true)] {
            assert!(project_power(PowerId::Slow, amount, extra, just_applied).is_err());
        }
    }

    #[test]
    fn confusion_preserves_the_original_stateless_sentinel() {
        let powers = project_power(PowerId::Confusion, -1, 0, false).unwrap();
        assert_eq!(powers[0].power_id, "confusion");
        assert_eq!(powers[0].amount, -1);
        assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        for (amount, extra, applied) in
            [(0, 0, false), (1, 0, false), (-1, 1, false), (-1, 0, true)]
        {
            assert!(project_power(PowerId::Confusion, amount, extra, applied).is_err());
        }
    }

    #[test]
    fn elixir_keeps_active_combat_and_validates_the_unplayed_hand() {
        use sts_oracle_run_control::state::core::{
            ActiveCombat, CombatContext, HandSelectReason, RoomCombatContext,
        };
        let mut state = sts_oracle_run_control::test_support::blank_test_combat();
        state.entities.monsters = vec![sts_oracle_run_control::test_support::test_monster(
            sts_oracle_run_control::content::monsters::EnemyId::JawWorm,
        )];
        let card = CombatCard::for_scenario(CardId::Strike, 7);
        let lineage = card.lineage;
        state.zones.hand = vec![card];
        let pending = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: vec![lineage],
            min_cards: 0,
            max_cards: 1,
            can_cancel: true,
            reason: HandSelectReason::PotionExhaust {
                has_pending_use_effect: false,
            },
        });
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = pending.clone();
        session.active_combat = Some(ActiveCombat::new(
            pending,
            state,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let bindings = BTreeMap::from([(
            lineage,
            BoundCardSourceV1 {
                binding: "natural_run/starter/0".into(),
                source: sts_parity_lockstep::NaturalRunCardSourceV1::StarterDeck {
                    player_class: "IRONCLAD".into(),
                    source_slot: 0,
                },
                expected_card_id: "strike".into(),
                expected_upgrades: 0,
            },
        )]);
        let NaturalRunCombatV4::PlayerDecision { state } = combat(&session, &bindings).unwrap()
        else {
            panic!("Elixir must retain its combat projection")
        };
        assert_eq!(state.zones.hand.len(), 1);
        assert!(state.zones.limbo.is_empty());
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .zones
            .hand[0]
            .free_to_play_once = true;
        let NaturalRunCombatV4::PlayerDecision { state } = combat(&session, &bindings).unwrap()
        else {
            panic!("free-once cards retain the active combat projection")
        };
        assert!(state.zones.hand[0].free_to_play_once);
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .zones
            .hand[0]
            .energy_on_use = 1;
        assert!(combat(&session, &bindings).is_err());
    }

    #[test]
    fn exhaust_selection_allows_paid_markers_only_on_the_suspended_skill() {
        use sts_oracle_run_control::{
            state::core::{ActiveCombat, CombatContext, HandSelectReason, RoomCombatContext},
            test_support::blank_test_combat,
        };
        for (id, upgrades) in [
            (CardId::BurningPact, 0),
            (CardId::BurningPact, 1),
            (CardId::TrueGrit, 1),
            (CardId::Purity, 0),
            (CardId::Purity, 1),
        ] {
            let mut state = blank_test_combat();
            state.zones.hand = vec![
                CombatCard::for_scenario(CardId::Strike, 1),
                CombatCard::for_scenario(CardId::Strike, 2),
            ];
            let mut played = CombatCard::for_scenario(id, 3);
            played.upgrades = upgrades;
            played.free_to_play_once = true;
            played.energy_on_use = 3;
            state.zones.limbo = vec![played];
            let pending = EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages: state.zones.hand.iter().map(|card| card.lineage).collect(),
                min_cards: u8::from(id != CardId::Purity),
                max_cards: if id == CardId::Purity { 2 } else { 1 },
                can_cancel: id == CardId::Purity,
                reason: HandSelectReason::Exhaust,
            });
            let mut session = RunControlSession::new(Default::default());
            session.engine_state = pending.clone();
            session.active_combat = Some(ActiveCombat::new(
                pending,
                state,
                CombatContext::Room(RoomCombatContext {
                    room_type: RoomType::MonsterRoom,
                }),
            ));
            validate_combat_decision_scope_v2(&session).unwrap();
            session
                .active_combat
                .as_mut()
                .unwrap()
                .combat_state
                .zones
                .hand[0]
                .free_to_play_once = true;
            validate_combat_decision_scope_v2(&session).unwrap();
            session
                .active_combat
                .as_mut()
                .unwrap()
                .combat_state
                .zones
                .hand[0]
                .free_to_play_once = false;
            session
                .active_combat
                .as_mut()
                .unwrap()
                .combat_state
                .zones
                .limbo[0]
                .id = CardId::Defend;
            assert!(validate_combat_decision_scope_v2(&session).is_err());
        }
    }

    #[test]
    fn corruption_disposal_flag_is_only_allowed_on_its_suspended_selection_card() {
        let mut card = CombatCard::for_scenario(CardId::DualWield, 17);
        card.exhaust_override = Some(true);
        require_stable_card_state_v4(&card, true, true).unwrap();
        for (paid, corruption) in [(false, false), (false, true), (true, false)] {
            assert!(require_stable_card_state_v4(&card, paid, corruption).is_err());
        }
        card.retain_override = Some(true);
        assert!(require_stable_card_state_v4(&card, true, true).is_err());
    }

    #[test]
    fn redundant_native_exhaust_flag_keeps_pending_warcry_representable() {
        let mut card = CombatCard::for_scenario(CardId::Warcry, 17);
        card.exhaust_override = Some(true);
        require_stable_card_state_v4(&card, false, false).unwrap();
        card.exhaust_override = Some(false);
        assert!(require_stable_card_state_v4(&card, false, false).is_err());
        card.id = CardId::Defend;
        card.exhaust_override = Some(true);
        assert!(require_stable_card_state_v4(&card, false, false).is_err());
    }

    #[test]
    fn time_warp_keeps_zero_and_only_stable_counts_without_private_state() {
        for amount in [0, 1, 11] {
            let projected = project_power(PowerId::TimeWarp, amount, 0, false).unwrap();
            assert_eq!(projected[0].power_id, "timewarp");
            assert_eq!(projected[0].amount, amount);
            assert_eq!(projected[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (amount, extra, just_applied) in
            [(-1, 0, false), (12, 0, false), (0, 1, false), (1, 0, true)]
        {
            assert!(project_power(PowerId::TimeWarp, amount, extra, just_applied).is_err());
        }
    }

    #[test]
    fn draw_reduction_preserves_the_round_expiration_flag() {
        for just_applied in [false, true] {
            let projected = project_power(PowerId::DrawReduction, 1, 0, just_applied).unwrap();
            assert_eq!(projected[0].power_id, "drawreduction");
            assert_eq!(
                projected[0].state,
                NaturalCombatPowerStateV4::RoundDuration { just_applied }
            );
        }
        assert!(project_power(PowerId::DrawReduction, 0, 0, false).is_err());
        assert!(project_power(PowerId::DrawReduction, 1, 1, false).is_err());
    }

    #[test]
    fn dexterity_loss_preserves_pending_amount_and_rejects_private_state() {
        for amount in [1, 5, 10, 15] {
            let powers = project_power(PowerId::DexterityDown, amount, 0, false).unwrap();
            assert_eq!(powers[0].power_id, "dexteritydown");
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (amount, extra_data, just_applied) in
            [(0, 0, false), (-1, 0, false), (5, 1, false), (5, 0, true)]
        {
            assert!(
                project_power(PowerId::DexterityDown, amount, extra_data, just_applied).is_err()
            );
        }
    }

    #[test]
    fn focus_preserves_signed_amounts_and_rejects_extra_state() {
        for amount in [-3, -1, 0, 1, 2, 999] {
            let powers = project_power(PowerId::Focus, amount, 0, false).unwrap();
            assert_eq!(powers[0].power_id, "focus");
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        assert!(project_power(PowerId::Focus, 1, 1, false).is_err());
        assert!(project_power(PowerId::Focus, 1, 0, true).is_err());
    }

    #[test]
    fn lose_strength_preserves_the_pending_loss_and_rejects_private_state() {
        for amount in [2, 3, 4, 5, 8] {
            let powers = project_power(PowerId::LoseStrength, amount, 0, false).unwrap();
            assert_eq!(powers.len(), 1);
            assert_eq!(powers[0].power_id, "losestrength");
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (amount, extra_data, just_applied) in
            [(0, 0, false), (-1, 0, false), (4, 1, false), (4, 0, true)]
        {
            assert!(
                project_power(PowerId::LoseStrength, amount, extra_data, just_applied).is_err()
            );
        }
    }

    #[test]
    fn act1_stateless_powers_keep_stable_ids_and_reject_private_state() {
        for (power_type, amount, shared_id) in [
            (PowerId::CurlUp, 9, "curlup"),
            (PowerId::Anger, 3, "anger"),
            (PowerId::Angry, 2, "angry"),
            (PowerId::Metallicize, 8, "metallicize"),
            (PowerId::PlatedArmor, 14, "platedarmor"),
            (PowerId::Regen, 6, "regenerate"),
            (PowerId::Regeneration, 5, "regeneration"),
            (PowerId::SporeCloud, 2, "sporecloud"),
            (PowerId::Split, -1, "split"),
            (PowerId::Artifact, 1, "artifact"),
            (PowerId::ModeShift, 30, "modeshift"),
            (PowerId::SharpHide, 4, "sharphide"),
            (PowerId::Thievery, 15, "thievery"),
            (PowerId::FeelNoPain, 3, "feelnopain"),
            (PowerId::Brutality, 1, "brutality"),
            (PowerId::Repair, 7, "repair"),
            (PowerId::Repair, 10, "repair"),
            (PowerId::Energized, 2, "energized"),
            (PowerId::EnergizedBlue, 1, "energizedblue"),
            (PowerId::DrawCardNextTurn, 3, "drawcard"),
            (PowerId::Choked, 3, "choked"),
            (PowerId::Bias, 1, "bias"),
            (PowerId::AfterImage, 1, "afterimage"),
            (PowerId::Blur, 1, "blur"),
            (PowerId::Blur, 2, "blur"),
            (PowerId::MentalFortressPower, 4, "mentalfortresspower"),
            (PowerId::Mantra, 2, "mantra"),
            (PowerId::BattleHymnPower, 1, "battlehymn"),
            (PowerId::RushdownPower, 2, "rushdown"),
            (PowerId::NirvanaPower, 3, "nirvana"),
            (PowerId::NirvanaPower, 4, "nirvana"),
            (PowerId::WaveOfTheHandPower, 1, "waveofthehandpower"),
            (PowerId::WaveOfTheHandPower, 2, "waveofthehandpower"),
            (PowerId::NoxiousFumes, 2, "noxiousfumes"),
            (PowerId::NoxiousFumes, 3, "noxiousfumes"),
            (PowerId::Envenom, 1, "envenom"),
            (PowerId::Envenom, 2, "envenom"),
            (PowerId::CorpseExplosion, 1, "corpseexplosion"),
            (PowerId::CorpseExplosion, 2, "corpseexplosion"),
            (PowerId::ToolsOfTheTrade, 1, "toolsofthetrade"),
            (PowerId::ToolsOfTheTrade, 2, "toolsofthetrade"),
            (PowerId::BlockReturnPower, 2, "blockreturnpower"),
            (PowerId::Vigor, 8, "vigor"),
            (PowerId::DoubleTap, 1, "doubletap"),
            (PowerId::DoubleTap, 2, "doubletap"),
            (PowerId::DuplicationPower, 2, "duplicationpower"),
            (PowerId::Corruption, -1, "corruption"),
            (PowerId::Dexterity, -2, "dexterity"),
            (PowerId::Entangle, 1, "entangle"),
            (PowerId::Entangle, 2, "entangle"),
        ] {
            let powers = project_power(power_type, amount, 0, false).unwrap();
            assert_eq!(powers.len(), 1);
            assert_eq!(powers[0].power_id, shared_id);
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }

        for invalid in [(0, 0, false), (9, 1, false), (9, 0, true)] {
            assert!(project_power(PowerId::FeelNoPain, invalid.0, invalid.1, invalid.2).is_err());
            assert!(project_power(PowerId::DoubleTap, invalid.0, invalid.1, invalid.2).is_err());
            assert!(project_power(PowerId::Repair, invalid.0, invalid.1, invalid.2).is_err());
            for power in [
                PowerId::Energized,
                PowerId::EnergizedBlue,
                PowerId::Bias,
                PowerId::AfterImage,
                PowerId::Blur,
                PowerId::MentalFortressPower,
                PowerId::Mantra,
                PowerId::BattleHymnPower,
                PowerId::RushdownPower,
                PowerId::NirvanaPower,
                PowerId::WaveOfTheHandPower,
                PowerId::NoxiousFumes,
                PowerId::Envenom,
                PowerId::CorpseExplosion,
                PowerId::ToolsOfTheTrade,
                PowerId::Entangle,
                PowerId::BlockReturnPower,
            ] {
                assert!(project_power(power, invalid.0, invalid.1, invalid.2).is_err());
                assert!(project_power(power, -1, 0, false).is_err());
            }
        }
        assert!(project_power(PowerId::Repair, -1, 0, false).is_err());
        assert!(project_power(PowerId::Brutality, 0, 0, false).is_err());
        assert!(project_power(PowerId::Vigor, 0, 0, false).is_err());
        assert!(project_power(PowerId::DuplicationPower, 0, 0, false).is_err());
        assert!(project_power(PowerId::Entangle, 0, 0, false).is_err());
        assert!(project_power(PowerId::Corruption, 0, 0, false).is_err());
        for (amount, extra, just_applied) in
            [(0, 0, false), (35, 0, false), (-1, 1, false), (-1, 0, true)]
        {
            assert!(project_power(PowerId::Split, amount, extra, just_applied).is_err());
        }
        for power in [PowerId::Regen, PowerId::Regeneration] {
            for (amount, extra, just_applied) in [(0, 0, false), (5, 1, false), (5, 0, true)] {
                assert!(project_power(power, amount, extra, just_applied).is_err());
            }
        }
    }

    #[test]
    fn wraith_form_preserves_original_v2_identity_and_signed_dexterity_rate() {
        for rate in [-2, -1, 1] {
            let powers = project_power(PowerId::WraithForm, rate, 0, false).unwrap();
            assert_eq!(powers[0].power_id, "wraithform");
            assert_eq!(powers[0].amount, rate);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::Stateless);
        }
        for (rate, extra, just_applied) in [(0, 0, false), (-1, 1, false), (-1, 0, true)] {
            assert!(project_power(PowerId::WraithForm, rate, extra, just_applied).is_err());
        }
    }

    #[test]
    fn curl_up_preserves_its_pending_trigger_without_losing_block_amount() {
        for amount in [9, 11, 12] {
            let powers = project_power(PowerId::CurlUp, amount, 1, false).unwrap();
            assert_eq!(powers[0].amount, amount);
            assert_eq!(powers[0].state, NaturalCombatPowerStateV4::CurlUpTriggered);
        }
        for (amount, extra, applied) in [
            (0, 1, false),
            (11, 2, false),
            (11, -1, false),
            (11, 1, true),
        ] {
            assert!(project_power(PowerId::CurlUp, amount, extra, applied).is_err());
        }
    }

    #[test]
    fn dead_monster_powers_are_excluded_from_stable_projection() {
        assert_eq!(
            project_power(PowerId::CurlUp, 12, 1, false).unwrap()[0].state,
            NaturalCombatPowerStateV4::CurlUpTriggered
        );

        let mut combat = sts_oracle_run_control::test_support::blank_test_combat();
        let mut louse = sts_oracle_run_control::test_support::test_monster(EnemyId::LouseNormal);
        louse.current_hp = 0;
        louse.is_dying = true;
        let owner = louse.id;
        combat.entities.monsters = vec![louse];
        store::set_powers_for(
            &mut combat,
            owner,
            vec![Power {
                power_type: PowerId::CurlUp,
                instance_id: None,
                amount: 12,
                extra_data: 1,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        let projected = project_monster_powers_v4(&combat, owner, true).unwrap();
        assert!(projected.is_empty());
    }

    #[test]
    fn rust_map_projection_excludes_expired_rows_but_keeps_the_current_room() {
        use sts_oracle_run_control::state::map::node::{MapRoomNode, RoomType};

        let mut past = MapRoomNode::new(0, 0);
        past.class = Some(RoomType::MonsterRoom);
        let mut current = MapRoomNode::new(0, 1);
        current.class = Some(RoomType::RestRoom);
        let mut future = MapRoomNode::new(0, 2);
        future.class = Some(RoomType::ShopRoom);
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        session.run_state.map.graph = vec![vec![past], vec![current], vec![future]];
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 1;

        let projected = public_map(&session).unwrap();
        let nodes = projected["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["id"], "F02X00");
        assert_eq!(nodes[0]["room"], "rest");
        assert_eq!(nodes[1]["id"], "F03X00");
        // Actor visibility may omit an unused cell; parity must still inspect
        // its assigned source room instead of borrowing the actor projection.
        let actor = sts_oracle_run_control::eval::run_control::planner_public_map_v1(&session);
        assert!(!actor.nodes.iter().any(|node| node.y == 2));
    }

    #[test]
    fn comparison_map_does_not_normalize_the_original_boss_drawing_anchor() {
        use sts_oracle_run_control::state::map::{MapEdge, MapRoomNode, MapState, RoomType};
        let mut session = RunControlSession::new(Default::default());
        let mut graph: Vec<Vec<MapRoomNode>> = (0..15)
            .map(|y| (0..7).map(|x| MapRoomNode::new(x, y)).collect())
            .collect();
        graph[14][2].class = Some(RoomType::RestRoom);
        graph[14][2].edges.insert(MapEdge::new(2, 14, 3, 16));
        graph[14][6].class = Some(RoomType::RestRoom);
        session.run_state.map = MapState::new(graph);
        let actor = sts_oracle_run_control::eval::run_control::planner_public_map_v1(&session);
        assert!(actor.nodes.iter().any(|node| (node.x, node.y) == (0, 15)));
        assert!(!actor.nodes.iter().any(|node| node.x == 6));
        let comparison = public_map(&session).unwrap();
        assert_eq!(comparison["boss_anchor"], "F17X03");
        assert_eq!(comparison["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(comparison["nodes"][0]["links"], json!(["F17X03"]));
        assert_eq!(comparison["nodes"][1]["id"], "F15X06");
    }

    #[test]
    fn rust_combat_projection_keeps_persistent_and_turn_costs_distinct() {
        let mut defend = CombatCard::for_scenario(CardId::Defend, 41);
        defend.modify_cost_for_combat_java(-9);
        defend.set_cost_for_turn_java(2);
        let bindings = BTreeMap::from([(
            defend.lineage,
            BoundCardSourceV1 {
                binding: "natural_run/starter/5".to_owned(),
                source: sts_parity_lockstep::NaturalRunCardSourceV1::StarterDeck {
                    player_class: "IRONCLAD".to_owned(),
                    source_slot: 5,
                },
                expected_card_id: "defend_r".to_owned(),
                expected_upgrades: 0,
            },
        )]);

        require_stable_card_state_v4(&defend, false, false).unwrap();
        let projected = combat_card(&defend, &bindings).unwrap();

        assert_eq!(projected.combat_cost, 0);
        assert_eq!(projected.cost_for_turn, 2);
    }
}
