use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardState, EventChoiceMeta, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum LiveEventRebuildResult {
    Ready {
        event_id: EventId,
        current_screen: usize,
        event_state: EventState,
        options: Vec<EventOption>,
    },
    UnknownEventName {
        raw_event_name: Option<String>,
        current_screen: usize,
    },
    MissingSemanticsState {
        event_id: EventId,
        current_screen: usize,
        missing_keys: Vec<String>,
    },
    StateDecodeFailed {
        event_id: EventId,
        current_screen: usize,
    },
    UnsupportedEvent {
        event_id: EventId,
        current_screen: usize,
    },
    OptionCountMismatch {
        event_id: EventId,
        current_screen: usize,
        expected: usize,
        actual: usize,
    },
    DisabledMismatch {
        event_id: EventId,
        current_screen: usize,
        mismatched_indices: Vec<usize>,
    },
}

impl LiveEventRebuildResult {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn screen_semantics_incomplete(&self) -> bool {
        matches!(
            self,
            Self::MissingSemanticsState { .. }
                | Self::StateDecodeFailed { .. }
                | Self::OptionCountMismatch { .. }
                | Self::DisabledMismatch { .. }
        )
    }
}

pub fn event_id_from_protocol_or_legacy_name(raw: &str) -> Option<EventId> {
    EventId::from_stable_id(raw)
        .or_else(|| crate::state::events::event_id_from_legacy_live_name(raw))
}

fn live_event_raw_name(screen_state: &Value) -> Option<String> {
    screen_state
        .get("event_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            screen_state
                .get("event_name")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        })
        .map(ToString::to_string)
}

pub fn analyze_live_event_rebuild(
    run_state: &RunState,
    screen_state: &Value,
) -> LiveEventRebuildResult {
    let current_screen = screen_state
        .get("current_screen_index")
        .or_else(|| screen_state.get("current_screen"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let raw_event_name = live_event_raw_name(screen_state);
    let Some(event_id) = raw_event_name
        .as_deref()
        .and_then(event_id_from_protocol_or_legacy_name)
    else {
        return LiveEventRebuildResult::UnknownEventName {
            raw_event_name,
            current_screen,
        };
    };

    let expected_keys = live_event_semantics_state_keys(event_id, current_screen)
        .map(|keys| keys.to_vec())
        .unwrap_or_default();
    let semantics_state = screen_state
        .get("event_semantics_state")
        .unwrap_or(&Value::Null);
    if live_event_requires_semantics_state(event_id, current_screen) {
        let missing_keys = expected_keys
            .iter()
            .filter(|key| semantics_state.get(**key).is_none())
            .map(|key| (*key).to_string())
            .collect::<Vec<_>>();
        if !missing_keys.is_empty() {
            return LiveEventRebuildResult::MissingSemanticsState {
                event_id,
                current_screen,
                missing_keys,
            };
        }
    }

    let Some(event_state) =
        try_build_event_state_from_screen_state(run_state, event_id, current_screen, screen_state)
    else {
        return LiveEventRebuildResult::StateDecodeFailed {
            event_id,
            current_screen,
        };
    };
    let Some(options) = try_get_structured_event_options_for_state(run_state, &event_state) else {
        return LiveEventRebuildResult::UnsupportedEvent {
            event_id,
            current_screen,
        };
    };

    if let Some(live_options) = screen_state.get("options").and_then(Value::as_array) {
        if options.len() != live_options.len() {
            return LiveEventRebuildResult::OptionCountMismatch {
                event_id,
                current_screen,
                expected: options.len(),
                actual: live_options.len(),
            };
        }

        let mismatched_indices = options
            .iter()
            .zip(live_options.iter())
            .enumerate()
            .filter_map(|(index, (option, live_option))| {
                (option.ui.disabled
                    != live_option
                        .get("disabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        if !mismatched_indices.is_empty() {
            return LiveEventRebuildResult::DisabledMismatch {
                event_id,
                current_screen,
                mismatched_indices,
            };
        }
    }

    LiveEventRebuildResult::Ready {
        event_id,
        current_screen,
        event_state,
        options,
    }
}

pub fn live_event_protocol_audit(run_state: &RunState, screen_state: &Value) -> Value {
    let rebuild = analyze_live_event_rebuild(run_state, screen_state);
    let current_screen = screen_state
        .get("current_screen_index")
        .or_else(|| screen_state.get("current_screen"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let event_id = live_event_raw_name(screen_state)
        .as_deref()
        .and_then(event_id_from_protocol_or_legacy_name);
    let expected_keys = event_id
        .and_then(|event_id| live_event_semantics_state_keys(event_id, current_screen))
        .map(|keys| keys.iter().map(|key| key.to_string()).collect::<Vec<_>>())
        .unwrap_or_default();
    let semantics_state = screen_state
        .get("event_semantics_state")
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or(Value::Null);
    let semantics_present = !semantics_state.is_null();

    let (status, missing_keys, option_count_match, disabled_match, detail) = match &rebuild {
        LiveEventRebuildResult::Ready { .. } => ("ready", Vec::new(), true, true, Value::Null),
        LiveEventRebuildResult::UnknownEventName { raw_event_name, .. } => (
            "unknown_event_name",
            Vec::new(),
            false,
            false,
            json!({ "raw_event_name": raw_event_name }),
        ),
        LiveEventRebuildResult::MissingSemanticsState { missing_keys, .. } => (
            "missing_semantics_state",
            missing_keys.clone(),
            false,
            false,
            Value::Null,
        ),
        LiveEventRebuildResult::StateDecodeFailed { .. } => {
            ("state_decode_failed", Vec::new(), false, false, Value::Null)
        }
        LiveEventRebuildResult::UnsupportedEvent { .. } => {
            ("unsupported_event", Vec::new(), false, false, Value::Null)
        }
        LiveEventRebuildResult::OptionCountMismatch {
            expected, actual, ..
        } => (
            "option_count_mismatch",
            Vec::new(),
            false,
            false,
            json!({ "expected": expected, "actual": actual }),
        ),
        LiveEventRebuildResult::DisabledMismatch {
            mismatched_indices, ..
        } => (
            "disabled_mismatch",
            Vec::new(),
            true,
            false,
            json!({ "mismatched_indices": mismatched_indices }),
        ),
    };

    json!({
        "event_id": event_id.map(|id| id.stable_id()),
        "current_screen": current_screen,
        "rebuild_status": status,
        "rebuild_detail": detail,
        "screen_semantics_incomplete": rebuild.screen_semantics_incomplete(),
        "event_semantics_required": event_id
            .map(|event_id| live_event_requires_semantics_state(event_id, current_screen))
            .unwrap_or(false),
        "event_semantics_present": semantics_present,
        "event_semantics_keys": expected_keys,
        "event_semantics_missing_keys": missing_keys,
        "structured_live_ready": rebuild.is_ready(),
        "option_count_match": option_count_match,
        "disabled_match": disabled_match,
    })
}

pub fn handle_event_choice(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    choice_idx: usize,
) -> Result<(), &'static str> {
    let event_state = match &mut run_state.event_state {
        Some(e) => e,
        None => return Err("Not currently in an Event room."),
    };

    // Fast-path resolution if the event is completed. Usually clicking any valid choice when completed leaves the room.
    if event_state.completed {
        // Opening the map does not leave the original event room. Its
        // WeMeetAgain instance still blocks potion use/discard until travel.
        if event_state.id != EventId::WeMeetAgain {
            run_state.event_state = None;
        }
        // The RunLoop needs to transition us back to the Map.
        *engine_state = crate::state::core::EngineState::MapNavigation;
        return Ok(());
    }

    // We dispatch based on the exact Event ID
    match event_state.id {
        EventId::Cleric => {
            crate::content::events::cleric::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::GoldenShrine => crate::content::events::golden_shrine::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::GoldenIdol => {
            crate::content::events::golden_idol::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::LivingWall => {
            crate::content::events::living_wall::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Vampires => {
            crate::content::events::vampires::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::BigFish => {
            crate::content::events::big_fish::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Ssssserpent => {
            crate::content::events::sssserpent::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::WorldOfGoop => {
            crate::content::events::goop_puddle::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::ShiningLight => crate::content::events::shining_light::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::ScrapOoze => {
            crate::content::events::scrap_ooze::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::GoldenWing => {
            crate::content::events::golden_wing::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Purifier => crate::content::events::purification_shrine::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::UpgradeShrine => crate::content::events::upgrade_shrine::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::Transmorgrifier => crate::content::events::transmogrifier::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::Lab => {
            crate::content::events::lab::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Duplicator => {
            crate::content::events::duplicator::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Mushrooms => {
            crate::content::events::mushrooms::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::MaskedBandits => crate::content::events::masked_bandits::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::MysteriousSphere => crate::content::events::mysterious_sphere::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::Colosseum => {
            crate::content::events::colosseum::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Addict => {
            crate::content::events::addict::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::KnowingSkull => crate::content::events::knowing_skull::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::ForgottenAltar => crate::content::events::forgotten_altar::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::Ghosts => {
            crate::content::events::ghosts::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Nest => {
            crate::content::events::nest::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Mausoleum => {
            crate::content::events::mausoleum::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Falling => {
            crate::content::events::falling::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::MoaiHead => {
            crate::content::events::moai_head::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::TombRedMask => crate::content::events::tomb_red_mask::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::WindingHalls => crate::content::events::winding_halls::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::FaceTrader => {
            crate::content::events::face_trader::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::FountainOfCurseCleansing => {
            crate::content::events::fountain::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::Nloth => {
            crate::content::events::nloth::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::CursedTome => {
            crate::content::events::cursed_tome::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::WomanInBlue => crate::content::events::woman_in_blue::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::BackTotheBasics => crate::content::events::back_to_basics::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::Beggar => {
            crate::content::events::beggar::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::BonfireSpirits => crate::content::events::bonfire_elementals::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::Designer => {
            crate::content::events::designer::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::TheLibrary => {
            crate::content::events::the_library::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::WeMeetAgain => crate::content::events::we_meet_again::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::SensoryStone => crate::content::events::sensory_stone::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::MindBloom => {
            crate::content::events::mind_bloom::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::DeadAdventurer => crate::content::events::dead_adventurer::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::NoteForYourself => crate::content::events::note_for_yourself::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::MatchAndKeep => crate::content::events::match_and_keep::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::AccursedBlacksmith => crate::content::events::accursed_blacksmith::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::BonfireElementals => crate::content::events::bonfire_elementals::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::GremlinWheelGame => crate::content::events::gremlin_wheel::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::DrugDealer => {
            crate::content::events::drug_dealer::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::TheJoust => {
            crate::content::events::the_joust::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::SecretPortal => crate::content::events::secret_portal::handle_choice(
            engine_state,
            run_state,
            choice_idx,
        ),
        EventId::Neow => {
            crate::content::events::neow::handle_choice(engine_state, run_state, choice_idx)
        }
        EventId::SpireHeart => {
            crate::content::events::spire_heart::handle_choice(engine_state, run_state, choice_idx)
        }
    }

    Ok(())
}

pub fn handle_event_post_run_pending_choice(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
) -> Result<bool, &'static str> {
    let Some(event_state) = run_state.event_state.as_ref() else {
        return Ok(false);
    };

    let should_resume = matches!(
        (event_state.id, event_state.current_screen),
        (EventId::BonfireSpirits, 2)
            | (EventId::BonfireElementals, 2)
            | (EventId::Designer, 2)
            | (EventId::Beggar, 1)
    );

    if !should_resume {
        return Ok(false);
    }

    handle_event_choice(engine_state, run_state, 0)?;
    Ok(true)
}

pub fn get_event_options(run_state: &RunState) -> Vec<EventOption> {
    if let Some(event_state) = &run_state.event_state {
        if event_state.completed {
            return vec![EventOption::new(
                EventChoiceMeta::new("Leave."),
                EventOptionSemantics {
                    action: EventActionKind::Leave,
                    transition: EventOptionTransition::Complete,
                    terminal: true,
                    ..EventOptionSemantics::default()
                },
            )];
        }

        return try_get_structured_event_options_for_state(run_state, event_state)
            .expect("all event ids must expose structured event options");
    }

    Vec::new()
}

pub fn try_get_structured_event_options_for_state(
    run_state: &RunState,
    event_state: &EventState,
) -> Option<Vec<EventOption>> {
    Some(match event_state.id {
        EventId::GoldenIdol => {
            crate::content::events::golden_idol::get_options(run_state, event_state)
        }
        EventId::Ghosts => crate::content::events::ghosts::get_options(run_state, event_state),
        EventId::Vampires => crate::content::events::vampires::get_options(run_state, event_state),
        EventId::CursedTome => {
            crate::content::events::cursed_tome::get_options(run_state, event_state)
        }
        EventId::WindingHalls => {
            crate::content::events::winding_halls::get_options(run_state, event_state)
        }
        EventId::ForgottenAltar => {
            crate::content::events::forgotten_altar::get_options(run_state, event_state)
        }
        EventId::GoldenShrine => {
            crate::content::events::golden_shrine::get_options(run_state, event_state)
        }
        EventId::GoldenWing => {
            crate::content::events::golden_wing::get_options(run_state, event_state)
        }
        EventId::WeMeetAgain => {
            crate::content::events::we_meet_again::get_options(run_state, event_state)
        }
        EventId::Falling => crate::content::events::falling::get_options(run_state, event_state),
        EventId::MindBloom => {
            crate::content::events::mind_bloom::get_options(run_state, event_state)
        }
        EventId::Designer => crate::content::events::designer::get_options(run_state, event_state),
        EventId::WomanInBlue => {
            crate::content::events::woman_in_blue::get_options(run_state, event_state)
        }
        EventId::SecretPortal => {
            crate::content::events::secret_portal::get_options(run_state, event_state)
        }
        EventId::Cleric => crate::content::events::cleric::get_options(run_state, event_state),
        EventId::Nloth => crate::content::events::nloth::get_options(run_state, event_state),
        EventId::Neow => crate::content::events::neow::get_options(run_state, event_state),
        EventId::SpireHeart => {
            crate::content::events::spire_heart::get_options(run_state, event_state)
        }
        EventId::Addict => crate::content::events::addict::get_options(run_state, event_state),
        EventId::BigFish => crate::content::events::big_fish::get_options(run_state, event_state),
        EventId::DrugDealer => {
            crate::content::events::drug_dealer::get_options(run_state, event_state)
        }
        EventId::AccursedBlacksmith => {
            crate::content::events::accursed_blacksmith::get_options(run_state, event_state)
        }
        EventId::MoaiHead => crate::content::events::moai_head::get_options(run_state, event_state),
        EventId::ShiningLight => {
            crate::content::events::shining_light::get_options(run_state, event_state)
        }
        EventId::LivingWall => {
            crate::content::events::living_wall::get_options(run_state, event_state)
        }
        EventId::UpgradeShrine => {
            crate::content::events::upgrade_shrine::get_options(run_state, event_state)
        }
        EventId::Purifier => {
            crate::content::events::purification_shrine::get_options(run_state, event_state)
        }
        EventId::Transmorgrifier => {
            crate::content::events::transmogrifier::get_options(run_state, event_state)
        }
        EventId::Duplicator => {
            crate::content::events::duplicator::get_options(run_state, event_state)
        }
        EventId::Beggar => crate::content::events::beggar::get_options(run_state, event_state),
        EventId::Ssssserpent => {
            crate::content::events::sssserpent::get_options(run_state, event_state)
        }
        EventId::TombRedMask => {
            crate::content::events::tomb_red_mask::get_options(run_state, event_state)
        }
        EventId::MaskedBandits => {
            crate::content::events::masked_bandits::get_options(run_state, event_state)
        }
        EventId::FaceTrader => {
            crate::content::events::face_trader::get_options(run_state, event_state)
        }
        EventId::WorldOfGoop => {
            crate::content::events::goop_puddle::get_options(run_state, event_state)
        }
        EventId::FountainOfCurseCleansing => {
            crate::content::events::fountain::get_options(run_state, event_state)
        }
        EventId::Lab => crate::content::events::lab::get_options(run_state, event_state),
        EventId::TheJoust => crate::content::events::the_joust::get_options(run_state, event_state),
        EventId::Mausoleum => {
            crate::content::events::mausoleum::get_options(run_state, event_state)
        }
        EventId::BackTotheBasics => {
            crate::content::events::back_to_basics::get_options(run_state, event_state)
        }
        EventId::SensoryStone => {
            crate::content::events::sensory_stone::get_options(run_state, event_state)
        }
        EventId::Nest => crate::content::events::nest::get_options(run_state, event_state),
        EventId::KnowingSkull => {
            crate::content::events::knowing_skull::get_options(run_state, event_state)
        }
        EventId::TheLibrary => {
            crate::content::events::the_library::get_options(run_state, event_state)
        }
        EventId::BonfireElementals => {
            crate::content::events::bonfire_elementals::get_options(run_state, event_state)
        }
        EventId::BonfireSpirits => {
            crate::content::events::bonfire_elementals::get_options(run_state, event_state)
        }
        EventId::ScrapOoze => {
            crate::content::events::scrap_ooze::get_options(run_state, event_state)
        }
        EventId::GremlinWheelGame => {
            crate::content::events::gremlin_wheel::get_options(run_state, event_state)
        }
        EventId::NoteForYourself => {
            crate::content::events::note_for_yourself::get_options(run_state, event_state)
        }
        EventId::Mushrooms => {
            crate::content::events::mushrooms::get_options(run_state, event_state)
        }
        EventId::MysteriousSphere => {
            crate::content::events::mysterious_sphere::get_options(run_state, event_state)
        }
        EventId::Colosseum => {
            crate::content::events::colosseum::get_options(run_state, event_state)
        }
        EventId::DeadAdventurer => {
            crate::content::events::dead_adventurer::get_options(run_state, event_state)
        }
        EventId::MatchAndKeep => {
            crate::content::events::match_and_keep::get_options(run_state, event_state)
        }
    })
}

pub fn try_build_event_state_from_screen_state(
    run_state: &RunState,
    event_id: EventId,
    current_screen: usize,
    screen_state: &Value,
) -> Option<EventState> {
    if event_id == EventId::WeMeetAgain && current_screen == 0 {
        let (internal_state, extra_data) = decode_we_meet_again_state_fields(
            run_state,
            screen_state.get("event_semantics_state")?,
        )?;
        return Some(EventState {
            id: event_id,
            current_screen,
            internal_state,
            completed: false,
            combat_pending: false,
            extra_data,
            prepared_encounter: None,
            card_payload: Vec::new(),
        });
    }

    if event_id == EventId::TheLibrary && current_screen == 1 {
        let card_payload =
            decode_event_card_payload(screen_state.get("event_semantics_state")?, "cards")?;
        return Some(EventState {
            id: event_id,
            current_screen,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            extra_data: Vec::new(),
            prepared_encounter: None,
            card_payload,
        });
    }

    if event_id == EventId::MatchAndKeep && current_screen <= 3 {
        let extra_data = decode_privileged_i32_vec(
            screen_state.get("event_semantics_state")?,
            "board_privileged",
        )?;
        let card_payload = decode_event_card_payload(
            screen_state.get("event_semantics_state")?,
            "cards_privileged",
        )?;
        return Some(EventState {
            id: event_id,
            current_screen,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            extra_data,
            prepared_encounter: None,
            card_payload,
        });
    }

    let internal_state = match event_id {
        EventId::BigFish if current_screen == 0 => {
            decode_big_fish_internal_state(screen_state.get("event_semantics_state")?)?
        }
        EventId::Designer if current_screen == 1 => {
            decode_designer_internal_state(screen_state.get("event_semantics_state")?)?
        }
        EventId::DeadAdventurer if current_screen == 0 || current_screen == 1 => {
            decode_privileged_i32(
                screen_state.get("event_semantics_state")?,
                "internal_state_privileged",
            )?
        }
        EventId::KnowingSkull if current_screen == 1 => {
            decode_knowing_skull_internal_state(screen_state.get("event_semantics_state")?)?
        }
        EventId::Nloth if current_screen == 0 => {
            decode_nloth_internal_state(run_state, screen_state.get("event_semantics_state")?)?
        }
        EventId::WorldOfGoop if current_screen == 0 => {
            decode_goop_puddle_internal_state(screen_state.get("event_semantics_state")?)?
        }
        EventId::ScrapOoze if current_screen == 0 => {
            decode_scrap_ooze_internal_state(run_state, screen_state.get("event_semantics_state")?)?
        }
        EventId::TheJoust if current_screen == 2 || current_screen == 3 => {
            decode_joust_internal_state(current_screen, screen_state.get("event_semantics_state")?)?
        }
        EventId::Falling if current_screen == 1 => {
            decode_falling_internal_state(run_state, screen_state.get("event_semantics_state")?)?
        }
        _ => 0,
    };
    Some(EventState {
        id: event_id,
        current_screen,
        internal_state,
        completed: false,
        combat_pending: false,
        extra_data: Vec::new(),
        prepared_encounter: None,
        card_payload: Vec::new(),
    })
}

pub fn event_semantics_state(run_state: &RunState, event_state: &EventState) -> Option<Value> {
    match event_state.id {
        EventId::BigFish if event_state.current_screen == 0 => Some(json!({
            "heal_amount": event_state.internal_state,
        })),
        EventId::Designer if event_state.current_screen == 1 => Some(json!({
            "adjust_upgrades_one": event_state.internal_state & 1 != 0,
            "clean_up_removes_cards": event_state.internal_state & 2 != 0,
        })),
        EventId::DeadAdventurer
            if event_state.current_screen == 0 || event_state.current_screen == 1 =>
        {
            Some(json!({
                "internal_state_privileged": event_state.internal_state,
            }))
        }
        EventId::MatchAndKeep if event_state.current_screen <= 3 => Some(json!({
            "board_privileged": event_state.extra_data,
            "cards_privileged": event_state.card_payload,
        })),
        EventId::KnowingSkull if event_state.current_screen == 1 => {
            let potion_count = event_state.internal_state & 0xFF;
            let gold_count = (event_state.internal_state >> 8) & 0xFF;
            let card_count = (event_state.internal_state >> 16) & 0xFF;
            Some(json!({
                "potion_count": potion_count,
                "gold_count": gold_count,
                "card_count": card_count,
                "potion_cost": 6 + potion_count,
                "gold_cost": 6 + gold_count,
                "card_cost": 6 + card_count,
            }))
        }
        EventId::Nloth if event_state.current_screen == 0 => {
            let choice1_relic_index = (event_state.internal_state & 0xFF) as usize;
            let choice2_relic_index = ((event_state.internal_state >> 8) & 0xFF) as usize;
            Some(json!({
                "choice1_relic_index": choice1_relic_index,
                "choice2_relic_index": choice2_relic_index,
                "choice1_relic": run_state
                    .relics
                    .get(choice1_relic_index)
                    .map(|relic| relic.id.stable_id()),
                "choice2_relic": run_state
                    .relics
                    .get(choice2_relic_index)
                    .map(|relic| relic.id.stable_id()),
            }))
        }
        EventId::WorldOfGoop if event_state.current_screen == 0 => Some(json!({
            "gold_loss": event_state.internal_state,
        })),
        EventId::ScrapOoze if event_state.current_screen == 0 => {
            let (relic_chance, damage) = scrap_ooze_reach_in_terms(run_state, event_state);
            Some(json!({
                "relic_chance": relic_chance,
                "damage": damage,
            }))
        }
        EventId::TheLibrary if event_state.current_screen == 1 => {
            Some(json!({ "cards": event_state.card_payload }))
        }
        EventId::TheJoust if event_state.current_screen == 2 => Some(json!({
            "bet_for_owner": event_state.internal_state & 1 != 0,
        })),
        EventId::TheJoust if event_state.current_screen == 3 => Some(json!({
            "bet_for_owner": event_state.internal_state & 1 != 0,
            "owner_wins_privileged": event_state.internal_state & 2 != 0,
        })),
        EventId::WeMeetAgain if event_state.current_screen == 0 => {
            let potion_slot = ((event_state.internal_state >> 8) & 0xFF) as usize;
            let gold_amount = event_state.internal_state & 0xFF;
            Some(json!({
                "potion_slot": if potion_slot == 0xFF { None } else { Some(potion_slot) },
                "gold_amount": gold_amount,
                "card_lineage": event_state
                    .extra_data
                    .first()
                    .copied()
                    .filter(|&lineage_payload| lineage_payload != -1)
                    .and_then(crate::runtime::combat::CardLineageId::from_runtime_i32_payload),
            }))
        }
        EventId::Falling if event_state.current_screen == 1 => {
            let skill_idx = (event_state.internal_state & 0x3FF) as usize;
            let power_idx = ((event_state.internal_state >> 10) & 0x3FF) as usize;
            let attack_idx = ((event_state.internal_state >> 20) & 0x3FF) as usize;
            Some(json!({
                "skill_lineage": if skill_idx == 0x3FF { None } else { run_state.master_deck.get(skill_idx).map(|card| card.lineage) },
                "power_lineage": if power_idx == 0x3FF { None } else { run_state.master_deck.get(power_idx).map(|card| card.lineage) },
                "attack_lineage": if attack_idx == 0x3FF { None } else { run_state.master_deck.get(attack_idx).map(|card| card.lineage) },
            }))
        }
        _ => None,
    }
}

pub fn live_event_requires_semantics_state(event_id: EventId, current_screen: usize) -> bool {
    matches!(
        (event_id, current_screen),
        (EventId::Designer, 1)
            | (EventId::DeadAdventurer, 0)
            | (EventId::DeadAdventurer, 1)
            | (EventId::MatchAndKeep, 0)
            | (EventId::MatchAndKeep, 1)
            | (EventId::MatchAndKeep, 2)
            | (EventId::MatchAndKeep, 3)
            | (EventId::KnowingSkull, 1)
            | (EventId::Nloth, 0)
            | (EventId::WorldOfGoop, 0)
            | (EventId::ScrapOoze, 0)
            | (EventId::TheLibrary, 1)
            | (EventId::TheJoust, 2)
            | (EventId::TheJoust, 3)
            | (EventId::WeMeetAgain, 0)
            | (EventId::Falling, 1)
    )
}

pub fn live_event_semantics_state_keys(
    event_id: EventId,
    current_screen: usize,
) -> Option<&'static [&'static str]> {
    match (event_id, current_screen) {
        (EventId::BigFish, 0) => Some(&["heal_amount"]),
        (EventId::Designer, 1) => Some(&["adjust_upgrades_one", "clean_up_removes_cards"]),
        (EventId::DeadAdventurer, 0 | 1) => Some(&["internal_state_privileged"]),
        (EventId::MatchAndKeep, 0..=3) => Some(&["board_privileged", "cards_privileged"]),
        (EventId::KnowingSkull, 1) => Some(&["potion_count", "gold_count", "card_count"]),
        (EventId::Nloth, 0) => Some(&["choice1_relic_index", "choice2_relic_index"]),
        (EventId::WorldOfGoop, 0) => Some(&["gold_loss"]),
        (EventId::ScrapOoze, 0) => Some(&["relic_chance", "damage"]),
        (EventId::TheLibrary, 1) => Some(&["cards"]),
        (EventId::TheJoust, 2) => Some(&["bet_for_owner"]),
        (EventId::TheJoust, 3) => Some(&["bet_for_owner", "owner_wins_privileged"]),
        (EventId::WeMeetAgain, 0) => Some(&["potion_slot", "gold_amount", "card_lineage"]),
        (EventId::Falling, 1) => Some(&["skill_lineage", "power_lineage", "attack_lineage"]),
        _ => None,
    }
}

fn decode_designer_internal_state(event_semantics_state: &Value) -> Option<i32> {
    let adjust_upgrades_one = event_semantics_state
        .get("adjust_upgrades_one")
        .and_then(Value::as_bool)? as i32;
    let clean_up_removes_cards = event_semantics_state
        .get("clean_up_removes_cards")
        .and_then(Value::as_bool)? as i32;
    Some(adjust_upgrades_one | (clean_up_removes_cards << 1))
}

fn decode_big_fish_internal_state(event_semantics_state: &Value) -> Option<i32> {
    let heal_amount = event_semantics_state
        .get("heal_amount")
        .and_then(Value::as_i64)?;
    i32::try_from(heal_amount)
        .ok()
        .filter(|amount| *amount >= 0)
}

fn decode_privileged_i32(event_semantics_state: &Value, key: &str) -> Option<i32> {
    let value = event_semantics_state.get(key).and_then(Value::as_i64)?;
    i32::try_from(value).ok()
}

fn decode_privileged_i32_vec(event_semantics_state: &Value, key: &str) -> Option<Vec<i32>> {
    let values = event_semantics_state.get(key).and_then(Value::as_array)?;
    values
        .iter()
        .map(|value| {
            let value = value.as_i64()?;
            i32::try_from(value).ok()
        })
        .collect()
}

fn decode_knowing_skull_internal_state(event_semantics_state: &Value) -> Option<i32> {
    let potion_count = decode_small_counter(event_semantics_state, "potion_count")?;
    let gold_count = decode_small_counter(event_semantics_state, "gold_count")?;
    let card_count = decode_small_counter(event_semantics_state, "card_count")?;
    Some(potion_count | (gold_count << 8) | (card_count << 16))
}

fn decode_small_counter(event_semantics_state: &Value, key: &str) -> Option<i32> {
    let value = event_semantics_state.get(key).and_then(Value::as_i64)?;
    let value = i32::try_from(value).ok()?;
    (0..=0xFF).contains(&value).then_some(value)
}

fn decode_nloth_internal_state(run_state: &RunState, event_semantics_state: &Value) -> Option<i32> {
    let choice1_relic_index = event_semantics_state
        .get("choice1_relic_index")
        .and_then(Value::as_u64)? as usize;
    let choice2_relic_index = event_semantics_state
        .get("choice2_relic_index")
        .and_then(Value::as_u64)? as usize;
    if choice1_relic_index >= run_state.relics.len()
        || choice2_relic_index >= run_state.relics.len()
        || choice1_relic_index == choice2_relic_index
        || choice1_relic_index > 0xFF
        || choice2_relic_index > 0xFF
    {
        return None;
    }
    Some((choice1_relic_index as i32) | ((choice2_relic_index as i32) << 8))
}

fn decode_goop_puddle_internal_state(event_semantics_state: &Value) -> Option<i32> {
    let gold_loss = event_semantics_state
        .get("gold_loss")
        .and_then(Value::as_i64)?;
    i32::try_from(gold_loss).ok().filter(|loss| *loss >= 0)
}

fn scrap_ooze_base_damage(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        5
    } else {
        3
    }
}

fn scrap_ooze_reach_in_terms(run_state: &RunState, event_state: &EventState) -> (i32, i32) {
    if event_state.internal_state == 0 {
        (25, scrap_ooze_base_damage(run_state))
    } else {
        (
            event_state.internal_state & 0xFFFF,
            (event_state.internal_state >> 16) & 0xFFFF,
        )
    }
}

fn decode_scrap_ooze_internal_state(
    run_state: &RunState,
    event_semantics_state: &Value,
) -> Option<i32> {
    let relic_chance = event_semantics_state
        .get("relic_chance")
        .and_then(Value::as_i64)?;
    let damage = event_semantics_state
        .get("damage")
        .and_then(Value::as_i64)?;
    let relic_chance = i32::try_from(relic_chance).ok()?;
    let damage = i32::try_from(damage).ok()?;
    if !(0..=100).contains(&relic_chance) || !(0..=255).contains(&damage) {
        return None;
    }
    if relic_chance == 25 && damage == scrap_ooze_base_damage(run_state) {
        Some(0)
    } else {
        Some((damage << 16) | (relic_chance & 0xFFFF))
    }
}

fn decode_event_card_payload(
    event_semantics_state: &Value,
    key: &str,
) -> Option<Vec<EventCardState>> {
    let cards = event_semantics_state.get(key).and_then(Value::as_array)?;
    let mut payload = Vec::with_capacity(cards.len());
    for card in cards {
        let card_id = serde_json::from_value(card.get("card_id")?.clone()).ok()?;
        let upgrades = u8::try_from(card.get("upgrades")?.as_u64()?).ok()?;
        payload.push(EventCardState { card_id, upgrades });
    }
    Some(payload)
}

fn decode_joust_internal_state(
    current_screen: usize,
    event_semantics_state: &Value,
) -> Option<i32> {
    let bet_for_owner = event_semantics_state
        .get("bet_for_owner")
        .and_then(Value::as_bool)? as i32;
    let owner_wins = if current_screen == 3 {
        event_semantics_state
            .get("owner_wins_privileged")
            .and_then(Value::as_bool)? as i32
    } else {
        0
    };
    Some(bet_for_owner | (owner_wins << 1))
}

fn decode_we_meet_again_state_fields(
    run_state: &RunState,
    event_semantics_state: &Value,
) -> Option<(i32, Vec<i32>)> {
    let potion_slot = event_semantics_state
        .get("potion_slot")
        .and_then(|value| {
            if value.is_null() {
                Some(0xFFusize)
            } else {
                value.as_u64().map(|slot| slot as usize)
            }
        })
        .unwrap_or(0xFF);
    let gold_amount = event_semantics_state
        .get("gold_amount")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, 255) as i32;
    let card_lineage = event_semantics_state.get("card_lineage").and_then(|value| {
        (!value.is_null())
            .then(|| serde_json::from_value(value.clone()).ok())
            .flatten()
    });
    let card_lineage = card_lineage
        .filter(|lineage| {
            run_state
                .master_deck
                .iter()
                .any(|card| card.lineage == *lineage)
        })
        .map(crate::runtime::combat::CardLineageId::to_runtime_i32_payload)
        .unwrap_or(-1);
    Some((
        gold_amount | ((potion_slot as i32) << 8),
        vec![card_lineage],
    ))
}

fn decode_falling_internal_state(
    run_state: &RunState,
    event_semantics_state: &Value,
) -> Option<i32> {
    fn idx_for_lineage(run_state: &RunState, value: Option<&Value>) -> i32 {
        let Some(lineage) = value.and_then(|value| {
            (!value.is_null())
                .then(|| serde_json::from_value(value.clone()).ok())
                .flatten()
        }) else {
            return 0x3FF;
        };
        run_state
            .master_deck
            .iter()
            .position(|card| card.lineage == lineage)
            .map(|idx| idx as i32)
            .unwrap_or(0x3FF)
    }

    let skill_idx = idx_for_lineage(run_state, event_semantics_state.get("skill_lineage"));
    let power_idx = idx_for_lineage(run_state, event_semantics_state.get("power_lineage"));
    let attack_idx = idx_for_lineage(run_state, event_semantics_state.get("attack_lineage"));
    Some((skill_idx & 0x3FF) | ((power_idx & 0x3FF) << 10) | ((attack_idx & 0x3FF) << 20))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use serde_json::json;

    #[test]
    fn event_id_from_protocol_or_legacy_name_accepts_legacy_live_aliases() {
        assert_eq!(
            event_id_from_protocol_or_legacy_name("Neow Event"),
            Some(EventId::Neow)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("Liars Game"),
            Some(EventId::Ssssserpent)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("Wing Statue"),
            Some(EventId::GoldenWing)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("Ancient Writing"),
            Some(EventId::BackTotheBasics)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("Old Beggar"),
            Some(EventId::Beggar)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("The Colosseum"),
            Some(EventId::Colosseum)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("The Divine Fountain"),
            Some(EventId::FountainOfCurseCleansing)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("Hypnotizing Colored Mushrooms"),
            Some(EventId::Mushrooms)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("A Note For Yourself"),
            Some(EventId::NoteForYourself)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("We Meet Again!"),
            Some(EventId::WeMeetAgain)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("SecretPortal"),
            Some(EventId::SecretPortal)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("Bonfire Spirits"),
            Some(EventId::BonfireElementals)
        );
    }

    #[test]
    fn event_id_from_protocol_or_legacy_name_accepts_legacy_external_ids() {
        let cases = [
            ("Big Fish", EventId::BigFish),
            ("The Cleric", EventId::Cleric),
            ("Dead Adventurer", EventId::DeadAdventurer),
            ("Golden Idol", EventId::GoldenIdol),
            ("Golden Wing", EventId::GoldenWing),
            ("World of Goop", EventId::WorldOfGoop),
            ("Liars Game", EventId::Ssssserpent),
            ("Living Wall", EventId::LivingWall),
            ("Mushrooms", EventId::Mushrooms),
            ("Scrap Ooze", EventId::ScrapOoze),
            ("Shining Light", EventId::ShiningLight),
            ("Addict", EventId::Addict),
            ("Back to Basics", EventId::BackTotheBasics),
            ("Beggar", EventId::Beggar),
            ("Colosseum", EventId::Colosseum),
            ("Cursed Tome", EventId::CursedTome),
            ("Drug Dealer", EventId::DrugDealer),
            ("Forgotten Altar", EventId::ForgottenAltar),
            ("Ghosts", EventId::Ghosts),
            ("Masked Bandits", EventId::MaskedBandits),
            ("Nest", EventId::Nest),
            ("The Library", EventId::TheLibrary),
            ("The Mausoleum", EventId::Mausoleum),
            ("Vampires", EventId::Vampires),
            ("Falling", EventId::Falling),
            ("MindBloom", EventId::MindBloom),
            ("The Moai Head", EventId::MoaiHead),
            ("Mysterious Sphere", EventId::MysteriousSphere),
            ("SensoryStone", EventId::SensoryStone),
            ("Tomb of Lord Red Mask", EventId::TombRedMask),
            ("Winding Halls", EventId::WindingHalls),
            ("Accursed Blacksmith", EventId::AccursedBlacksmith),
            ("Bonfire Elementals", EventId::BonfireElementals),
            ("Designer", EventId::Designer),
            ("Duplicator", EventId::Duplicator),
            ("FaceTrader", EventId::FaceTrader),
            ("Fountain of Cleansing", EventId::FountainOfCurseCleansing),
            ("Knowing Skull", EventId::KnowingSkull),
            ("Lab", EventId::Lab),
            ("N'loth", EventId::Nloth),
            ("NoteForYourself", EventId::NoteForYourself),
            ("SecretPortal", EventId::SecretPortal),
            ("The Joust", EventId::TheJoust),
            ("WeMeetAgain", EventId::WeMeetAgain),
            ("The Woman in Blue", EventId::WomanInBlue),
            ("Match and Keep!", EventId::MatchAndKeep),
            ("Wheel of Change", EventId::GremlinWheelGame),
            ("Golden Shrine", EventId::GoldenShrine),
            ("Transmorgrifier", EventId::Transmorgrifier),
            ("Purifier", EventId::Purifier),
            ("Upgrade Shrine", EventId::UpgradeShrine),
        ];

        for (java_id, expected) in cases {
            assert_eq!(
                event_id_from_protocol_or_legacy_name(java_id),
                Some(expected),
                "{java_id}"
            );
        }
    }

    #[test]
    fn event_id_from_protocol_or_legacy_name_accepts_current_stable_ids() {
        assert_eq!(
            event_id_from_protocol_or_legacy_name("BigFish"),
            Some(EventId::BigFish)
        );
        assert_eq!(
            event_id_from_protocol_or_legacy_name("WorldOfGoop"),
            Some(EventId::WorldOfGoop)
        );
    }

    #[test]
    fn event_id_from_protocol_or_legacy_name_rejects_java_runtime_classes() {
        assert_eq!(
            event_id_from_protocol_or_legacy_name("com.megacrit.cardcrawl.events.exordium.BigFish"),
            None
        );
    }

    #[test]
    fn big_fish_event_semantics_state_round_trips_frozen_heal_amount() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::BigFish,
            current_screen: 0,
            internal_state: 26,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        let screen_state = json!({
            "event_name": "Big Fish",
            "current_screen": 0,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::BigFish, 0, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, 26);
        assert_eq!(
            live_event_semantics_state_keys(EventId::BigFish, 0),
            Some(&["heal_amount"][..])
        );
    }

    #[test]
    fn designer_event_semantics_state_round_trips_to_internal_state() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::Designer,
            current_screen: 1,
            internal_state: 0b11,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        let screen_state = json!({
            "event_name": "Designer",
            "current_screen": 1,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::Designer, 1, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
    }

    #[test]
    fn we_meet_again_event_semantics_state_round_trips_to_internal_state() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.potions[0] = Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::StrengthPotion,
            90_001,
        ));
        rs.master_deck
            .push(crate::runtime::combat::CombatCard::for_scenario(
                CardId::PommelStrike,
                90_101,
            ));
        let event_state = EventState {
            id: EventId::WeMeetAgain,
            current_screen: 0,
            internal_state: 75,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: vec![crate::test_support::scenario_lineage(90_101).to_runtime_i32_payload()],
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        let screen_state = json!({
            "event_name": "We Meet Again",
            "current_screen": 0,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::WeMeetAgain, 0, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert_eq!(rebuilt.extra_data, event_state.extra_data);
    }

    #[test]
    fn falling_event_semantics_state_round_trips_to_internal_state() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.add_card_to_deck(CardId::ShrugItOff);
        let skill_idx = rs.master_deck.len() - 1;
        rs.add_card_to_deck(CardId::Inflame);
        let power_idx = rs.master_deck.len() - 1;
        let attack_idx = rs
            .master_deck
            .iter()
            .position(|card| card.id == CardId::Strike)
            .unwrap();
        let event_state = EventState {
            id: EventId::Falling,
            current_screen: 1,
            internal_state: (skill_idx as i32 & 0x3FF)
                | (((power_idx as i32) & 0x3FF) << 10)
                | (((attack_idx as i32) & 0x3FF) << 20),
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        let screen_state = json!({
            "event_name": "Falling",
            "current_screen": 1,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::Falling, 1, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
    }

    #[test]
    fn nloth_event_semantics_state_round_trips_to_internal_state() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            RelicState::new(RelicId::Anchor),
            RelicState::new(RelicId::Vajra),
        ];
        let event_state = EventState {
            id: EventId::Nloth,
            current_screen: 0,
            internal_state: 1 | (2 << 8),
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["choice1_relic_index"], 1);
        assert_eq!(semantics["choice2_relic_index"], 2);
        assert_eq!(semantics["choice1_relic"], "Anchor");
        assert_eq!(semantics["choice2_relic"], "Vajra");
        let screen_state = json!({
            "event_name": "N'loth",
            "current_screen": 0,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::Nloth, 0, &screen_state).unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert!(live_event_requires_semantics_state(EventId::Nloth, 0));
    }

    #[test]
    fn goop_puddle_event_semantics_state_round_trips_to_internal_state() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::WorldOfGoop,
            current_screen: 0,
            internal_state: 42,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["gold_loss"], 42);
        let screen_state = json!({
            "event_name": "World of Goop",
            "current_screen": 0,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::WorldOfGoop, 0, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert!(live_event_requires_semantics_state(EventId::WorldOfGoop, 0));
    }

    #[test]
    fn library_event_semantics_state_round_trips_revealed_card_list() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::TheLibrary,
            current_screen: 1,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: vec![
                EventCardState {
                    card_id: CardId::ShrugItOff,
                    upgrades: 1,
                },
                EventCardState {
                    card_id: CardId::PommelStrike,
                    upgrades: 0,
                },
            ],
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["cards"].as_array().unwrap().len(), 2);
        assert_eq!(semantics["cards"][0]["card_id"], "ShrugItOff");
        let screen_state = json!({
            "event_name": "The Library",
            "current_screen": 1,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::TheLibrary, 1, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.card_payload, event_state.card_payload);
        assert!(rebuilt.extra_data.is_empty());
        assert!(live_event_requires_semantics_state(EventId::TheLibrary, 1));
    }

    #[test]
    fn joust_pre_roll_event_semantics_state_round_trips_public_bet() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::TheJoust,
            current_screen: 2,
            internal_state: 1,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["bet_for_owner"], true);
        let screen_state = json!({
            "event_name": "The Joust",
            "current_screen": 2,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::TheJoust, 2, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert!(live_event_requires_semantics_state(EventId::TheJoust, 2));
    }

    #[test]
    fn joust_post_roll_event_semantics_state_round_trips_privileged_result() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::TheJoust,
            current_screen: 3,
            internal_state: 1 | 2,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["bet_for_owner"], true);
        assert_eq!(semantics["owner_wins_privileged"], true);
        let screen_state = json!({
            "event_name": "The Joust",
            "current_screen": 3,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::TheJoust, 3, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert!(live_event_requires_semantics_state(EventId::TheJoust, 3));
    }

    #[test]
    fn scrap_ooze_event_semantics_state_round_trips_current_reach_in_terms() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::ScrapOoze,
            current_screen: 0,
            internal_state: (4 << 16) | 35,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["relic_chance"], 35);
        assert_eq!(semantics["damage"], 4);
        let screen_state = json!({
            "event_name": "Scrap Ooze",
            "current_screen": 0,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::ScrapOoze, 0, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert!(live_event_requires_semantics_state(EventId::ScrapOoze, 0));
    }

    #[test]
    fn dead_adventurer_event_semantics_state_round_trips_privileged_state() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::DeadAdventurer,
            current_screen: 0,
            internal_state: 0x12_345,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["internal_state_privileged"], 0x12_345);
        let screen_state = json!({
            "event_name": "Dead Adventurer",
            "current_screen": 0,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::DeadAdventurer, 0, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert!(live_event_requires_semantics_state(
            EventId::DeadAdventurer,
            0
        ));
    }

    #[test]
    fn match_and_keep_event_semantics_state_round_trips_privileged_board() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::MatchAndKeep,
            current_screen: 1,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: vec![
                EventCardState {
                    card_id: CardId::Strike,
                    upgrades: 0,
                },
                EventCardState {
                    card_id: CardId::Defend,
                    upgrades: 0,
                },
                EventCardState {
                    card_id: CardId::Bash,
                    upgrades: 0,
                },
                EventCardState {
                    card_id: CardId::Cleave,
                    upgrades: 0,
                },
                EventCardState {
                    card_id: CardId::IronWave,
                    upgrades: 0,
                },
                EventCardState {
                    card_id: CardId::TwinStrike,
                    upgrades: 0,
                },
            ],
            extra_data: vec![0, 0, 1, 2, 3, 4, 5, 1, 2, 3, 4, 5, 0, 5, -1, -1, -1, 0],
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(
            semantics["board_privileged"].as_array().unwrap().len(),
            event_state.extra_data.len()
        );
        assert_eq!(semantics["cards_privileged"][0]["card_id"], "Strike");
        let screen_state = json!({
            "event_name": "Match and Keep!",
            "current_screen": 1,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::MatchAndKeep, 1, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.extra_data, event_state.extra_data);
        assert_eq!(rebuilt.card_payload, event_state.card_payload);
        assert!(live_event_requires_semantics_state(
            EventId::MatchAndKeep,
            1
        ));
    }

    #[test]
    fn knowing_skull_event_semantics_state_round_trips_public_cost_counters() {
        let rs = RunState::new(1, 0, true, "Ironclad");
        let event_state = EventState {
            id: EventId::KnowingSkull,
            current_screen: 1,
            internal_state: (2 << 16) | (1 << 8) | 3,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };

        let semantics = event_semantics_state(&rs, &event_state).unwrap();
        assert_eq!(semantics["potion_count"], 3);
        assert_eq!(semantics["gold_count"], 1);
        assert_eq!(semantics["card_count"], 2);
        assert_eq!(semantics["potion_cost"], 9);
        assert_eq!(semantics["gold_cost"], 7);
        assert_eq!(semantics["card_cost"], 8);
        let screen_state = json!({
            "event_name": "Knowing Skull",
            "current_screen": 1,
            "event_semantics_state": semantics,
        });
        let rebuilt =
            try_build_event_state_from_screen_state(&rs, EventId::KnowingSkull, 1, &screen_state)
                .unwrap();

        assert_eq!(rebuilt.internal_state, event_state.internal_state);
        assert!(live_event_requires_semantics_state(
            EventId::KnowingSkull,
            1
        ));
    }
}
