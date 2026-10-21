use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::eval::run_control::RunControlSession;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventEffect, EventId, EventOption, EventOptionConstraint,
    EventOptionTransition, EventRandomOutcomeKind, EventRelicKind, EventSelectionKind,
};

pub const EVENT_BOUNDARY_PACKET_SCHEMA_NAME: &str = "EventBoundaryPacketV2";
pub const EVENT_BOUNDARY_PACKET_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventBoundaryPacketV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub event_id: String,
    pub current_screen: usize,
    pub completed: bool,
    pub boundary_class: String,
    pub candidates: Vec<EventCandidateSnapshotV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventCandidateSnapshotV1 {
    pub candidate_id: String,
    pub command: String,
    pub display_label: String,
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub action_kind: String,
    pub transition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_kind: Option<String>,
    pub terminal: bool,
    pub repeatable: bool,
    pub role: String,
    pub information_tags: Vec<String>,
    pub effects: Vec<EventEffectSnapshotV1>,
    pub constraints: Vec<EventConstraintSnapshotV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventEffectSnapshotV1 {
    pub kind: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventConstraintSnapshotV1 {
    pub kind: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

pub fn event_boundary_packet_from_session_v1(
    session: &RunControlSession,
) -> Option<EventBoundaryPacketV1> {
    if !matches!(session.engine_state, EngineState::EventRoom) {
        return None;
    }
    let event_state = session.run_state.event_state.as_ref()?;
    let options = crate::engine::event_handler::get_event_options(&session.run_state);
    let option_count = options.len();
    let candidates = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            event_candidate_snapshot_v1(
                event_state.id,
                event_state.current_screen,
                index,
                option,
                option_count,
            )
        })
        .collect::<Vec<_>>();
    Some(EventBoundaryPacketV1 {
        schema_name: EVENT_BOUNDARY_PACKET_SCHEMA_NAME.to_string(),
        schema_version: EVENT_BOUNDARY_PACKET_SCHEMA_VERSION,
        event_id: event_state.id.stable_id().to_owned(),
        current_screen: event_state.current_screen,
        completed: event_state.completed,
        boundary_class: event_boundary_class_v1(&candidates),
        candidates,
    })
}

pub fn event_boundary_detail_lines_v1(
    packet: &EventBoundaryPacketV1,
    max_candidates: usize,
) -> Vec<String> {
    let mut lines = vec![format!(
        "event {} screen={} class={}",
        packet.event_id, packet.current_screen, packet.boundary_class
    )];
    lines.extend(
        packet
            .candidates
            .iter()
            .take(max_candidates)
            .map(event_candidate_detail_line_v1),
    );
    let remaining = packet.candidates.len().saturating_sub(max_candidates);
    if remaining > 0 {
        lines.push(format!("event candidates omitted={remaining}"));
    }
    lines
}

fn event_candidate_detail_line_v1(candidate: &EventCandidateSnapshotV1) -> String {
    let effects = event_candidate_effect_summary_v1(candidate);
    let constraints = if candidate.constraints.is_empty() {
        String::new()
    } else {
        format!(
            " constraints=[{}]",
            candidate
                .constraints
                .iter()
                .map(|constraint| constraint.kind.as_str())
                .collect::<Vec<_>>()
                .join("+")
        )
    };
    format!(
        "{} action={} role={} transition={} effects=[{}]{}",
        candidate.command,
        candidate.action_kind,
        candidate.role,
        candidate.transition,
        effects,
        constraints
    )
}

fn event_candidate_effect_summary_v1(candidate: &EventCandidateSnapshotV1) -> String {
    if candidate.effects.is_empty() {
        return "-".to_string();
    }
    candidate
        .effects
        .iter()
        .map(|effect| {
            if effect.params.is_empty() {
                return effect.kind.clone();
            }
            let params = effect
                .params
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("{}({})", effect.kind, params)
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn event_candidate_snapshot_v1(
    event_id: EventId,
    current_screen: usize,
    index: usize,
    option: &EventOption,
    option_count: usize,
) -> EventCandidateSnapshotV1 {
    let (transition, selection_kind) = transition_snapshot_v1(option.semantics.transition);
    let effects = option
        .semantics
        .effects
        .iter()
        .map(effect_snapshot_v1)
        .collect::<Vec<_>>();
    let constraints = option
        .semantics
        .constraints
        .iter()
        .map(constraint_snapshot_v1)
        .collect::<Vec<_>>();
    let information_tags = information_tags_v1(option, &effects);
    EventCandidateSnapshotV1 {
        candidate_id: format!(
            "event:{}:screen{current_screen}:choice{index}",
            event_id.stable_id()
        ),
        command: format!("event {index}"),
        display_label: option.ui.text.clone(),
        disabled: option.ui.disabled,
        disabled_reason: option.ui.disabled_reason.clone(),
        action_kind: action_kind_snapshot_v1(option.semantics.action).to_string(),
        transition,
        selection_kind,
        terminal: option.semantics.terminal,
        repeatable: option.semantics.repeatable,
        role: candidate_role_v1(option, &information_tags, option_count),
        information_tags,
        effects,
        constraints,
    }
}

fn event_boundary_class_v1(candidates: &[EventCandidateSnapshotV1]) -> String {
    if candidates.is_empty() {
        return "no_event_candidates".to_string();
    }
    if candidates.len() == 1 {
        let candidate = &candidates[0];
        if candidate.role == "forced_leave" {
            return "forced_leave".to_string();
        }
        if candidate.role == "trivial_continue" {
            return "trivial_continue".to_string();
        }
        if candidate.role == "combat_branch" {
            return "combat_confirmation".to_string();
        }
        if candidate.role == "target_selection_gate" {
            return "target_selection_gate".to_string();
        }
    }
    if candidates
        .iter()
        .any(|candidate| candidate.role == "stochastic_event")
    {
        return "stochastic_event".to_string();
    }
    if candidates
        .iter()
        .any(|candidate| candidate.role == "target_selection_gate")
    {
        return "target_selection_gate".to_string();
    }
    if candidates
        .iter()
        .any(|candidate| candidate.role == "combat_branch")
    {
        return "strategic_event_with_combat".to_string();
    }
    if candidates
        .iter()
        .any(|candidate| candidate.role == "strategic_choice")
    {
        return "strategic_choice".to_string();
    }
    "event_boundary".to_string()
}

fn candidate_role_v1(
    option: &EventOption,
    information_tags: &[String],
    option_count: usize,
) -> String {
    if option.ui.disabled {
        return "disabled".to_string();
    }
    if information_tags.iter().any(|tag| tag == "random_outcome") {
        return "stochastic_event".to_string();
    }
    if matches!(
        option.semantics.transition,
        EventOptionTransition::OpenSelection(_)
    ) {
        return "target_selection_gate".to_string();
    }
    if matches!(
        option.semantics.transition,
        EventOptionTransition::StartCombat
    ) || option
        .semantics
        .effects
        .iter()
        .any(|effect| matches!(effect, EventEffect::StartCombat))
    {
        return "combat_branch".to_string();
    }
    if option.semantics.action == EventActionKind::Leave
        && option.semantics.terminal
        && option.semantics.effects.is_empty()
    {
        return if option_count == 1 {
            "forced_leave".to_string()
        } else {
            "safe_exit".to_string()
        };
    }
    if matches!(
        option.semantics.action,
        EventActionKind::Continue | EventActionKind::Leave
    ) && option.semantics.effects.is_empty()
        && option_count == 1
    {
        return "trivial_continue".to_string();
    }
    if option.semantics.action == EventActionKind::Unknown {
        return "unknown_semantics".to_string();
    }
    "strategic_choice".to_string()
}

fn information_tags_v1(option: &EventOption, effects: &[EventEffectSnapshotV1]) -> Vec<String> {
    let mut tags = Vec::new();
    if option.semantics.action == EventActionKind::Unknown {
        tags.push("unknown_semantics".to_string());
    } else {
        tags.push("public_structured".to_string());
    }
    if matches!(
        option.semantics.transition,
        EventOptionTransition::OpenSelection(_)
    ) {
        tags.push("opens_selection".to_string());
    }
    if effects.iter().any(|effect| effect.kind == "random_outcome") {
        tags.push("random_outcome".to_string());
    }
    if matches!(
        option.semantics.transition,
        EventOptionTransition::StartCombat
    ) || effects.iter().any(|effect| effect.kind == "start_combat")
    {
        tags.push("starts_combat".to_string());
    }
    tags.sort();
    tags.dedup();
    tags
}

fn transition_snapshot_v1(transition: EventOptionTransition) -> (String, Option<String>) {
    match transition {
        EventOptionTransition::None => ("none".to_string(), None),
        EventOptionTransition::AdvanceScreen => ("advance_screen".to_string(), None),
        EventOptionTransition::Complete => ("complete".to_string(), None),
        EventOptionTransition::OpenReward => ("open_reward".to_string(), None),
        EventOptionTransition::StartCombat => ("start_combat".to_string(), None),
        EventOptionTransition::OpenSelection(kind) => {
            ("open_selection".to_string(), Some(selection_kind_v1(kind)))
        }
    }
}

fn action_kind_snapshot_v1(action: EventActionKind) -> &'static str {
    match action {
        EventActionKind::Unknown => "unknown",
        EventActionKind::Leave => "leave",
        EventActionKind::Continue => "continue",
        EventActionKind::Accept => "accept",
        EventActionKind::Decline => "decline",
        EventActionKind::Fight => "fight",
        EventActionKind::Trade => "trade",
        EventActionKind::DeckOperation => "deck_operation",
        EventActionKind::Gain => "gain",
        EventActionKind::Special => "special",
    }
}

fn selection_kind_v1(kind: EventSelectionKind) -> String {
    match kind {
        EventSelectionKind::None => "none",
        EventSelectionKind::RemoveCard => "remove_card",
        EventSelectionKind::UpgradeCard => "upgrade_card",
        EventSelectionKind::TransformCard => "transform_card",
        EventSelectionKind::DuplicateCard => "duplicate_card",
        EventSelectionKind::OfferCard => "offer_card",
    }
    .to_string()
}

fn effect_snapshot_v1(effect: &EventEffect) -> EventEffectSnapshotV1 {
    let mut params = BTreeMap::new();
    let kind = match effect {
        EventEffect::GainGold(amount) => {
            params.insert("amount".to_string(), amount.to_string());
            "gain_gold"
        }
        EventEffect::GainGoldRange { min, max } => {
            params.insert("min".to_string(), min.to_string());
            params.insert("max".to_string(), max.to_string());
            "gain_gold_range"
        }
        EventEffect::LoseGold(amount) => {
            params.insert("amount".to_string(), amount.to_string());
            "lose_gold"
        }
        EventEffect::LoseHp(amount) => {
            params.insert("amount".to_string(), amount.to_string());
            "lose_hp"
        }
        EventEffect::LoseMaxHp(amount) => {
            params.insert("amount".to_string(), amount.to_string());
            "lose_max_hp"
        }
        EventEffect::Heal(amount) => {
            params.insert("amount".to_string(), amount.to_string());
            "heal"
        }
        EventEffect::GainMaxHp(amount) => {
            params.insert("amount".to_string(), amount.to_string());
            "gain_max_hp"
        }
        EventEffect::ObtainRelic { count, kind } => {
            params.insert("count".to_string(), count.to_string());
            relic_kind_params_v1(*kind, &mut params);
            "obtain_relic"
        }
        EventEffect::ObtainPotion { count } => {
            params.insert("count".to_string(), count.to_string());
            "obtain_potion"
        }
        EventEffect::ObtainCard { count, kind } => {
            params.insert("count".to_string(), count.to_string());
            card_kind_params_v1(*kind, &mut params);
            "obtain_card"
        }
        EventEffect::ObtainColorlessCard { count, kind } => {
            params.insert("count".to_string(), count.to_string());
            card_kind_params_v1(*kind, &mut params);
            "obtain_colorless_card"
        }
        EventEffect::OfferCards { count, kind } => {
            params.insert("count".to_string(), count.to_string());
            card_kind_params_v1(*kind, &mut params);
            "offer_cards"
        }
        EventEffect::ObtainCurse { count, kind } => {
            params.insert("count".to_string(), count.to_string());
            card_kind_params_v1(*kind, &mut params);
            "obtain_curse"
        }
        EventEffect::RemoveCard {
            count,
            target_lineage,
            kind,
        } => {
            params.insert("count".to_string(), count.to_string());
            if let Some(lineage) = target_lineage {
                params.insert("target_lineage".to_string(), lineage.to_string());
            }
            card_kind_params_v1(*kind, &mut params);
            "remove_card"
        }
        EventEffect::UpgradeCard { count } => {
            params.insert("count".to_string(), count.to_string());
            "upgrade_card"
        }
        EventEffect::UpgradeAllCards => "upgrade_all_cards",
        EventEffect::TransformCard { count } => {
            params.insert("count".to_string(), count.to_string());
            "transform_card"
        }
        EventEffect::DuplicateCard { count } => {
            params.insert("count".to_string(), count.to_string());
            "duplicate_card"
        }
        EventEffect::LoseRelic {
            specific,
            starter_only,
        } => {
            params.insert("starter_only".to_string(), starter_only.to_string());
            if let Some(relic) = specific {
                params.insert("relic".to_string(), relic.stable_id().to_string());
            }
            "lose_relic"
        }
        EventEffect::LoseStarterRelic { specific } => {
            if let Some(relic) = specific {
                params.insert("relic".to_string(), relic.stable_id().to_string());
            }
            "lose_starter_relic"
        }
        EventEffect::RandomOutcome {
            kind,
            chance_percent,
        } => {
            params.insert(
                "outcome_kind".to_string(),
                event_random_outcome_kind_v1(*kind).to_string(),
            );
            if let Some(chance_percent) = chance_percent {
                params.insert("chance_percent".to_string(), chance_percent.to_string());
            }
            "random_outcome"
        }
        EventEffect::StartCombat => "start_combat",
    };
    EventEffectSnapshotV1 {
        kind: kind.to_string(),
        params,
    }
}

fn constraint_snapshot_v1(constraint: &EventOptionConstraint) -> EventConstraintSnapshotV1 {
    let mut params = BTreeMap::new();
    let kind = match constraint {
        EventOptionConstraint::RequiresGold(amount) => {
            params.insert("amount".to_string(), amount.to_string());
            "requires_gold"
        }
        EventOptionConstraint::RequiresRelic(relic) => {
            params.insert("relic".to_string(), relic.stable_id().to_string());
            "requires_relic"
        }
        EventOptionConstraint::RequiresRemovableCard => "requires_removable_card",
        EventOptionConstraint::RequiresNonBottledPurgeableCard => {
            "requires_non_bottled_purgeable_card"
        }
        EventOptionConstraint::RequiresUpgradeableCard => "requires_upgradeable_card",
        EventOptionConstraint::RequiresTransformableCard => "requires_transformable_card",
        EventOptionConstraint::RequiresTransformableCards(count) => {
            params.insert("count".to_string(), count.to_string());
            "requires_transformable_cards"
        }
        EventOptionConstraint::RequiresPotion => "requires_potion",
        EventOptionConstraint::RequiresPotionSlotValue => "requires_potion_slot_value",
    };
    EventConstraintSnapshotV1 {
        kind: kind.to_string(),
        params,
    }
}

fn card_kind_params_v1(kind: EventCardKind, params: &mut BTreeMap<String, String>) {
    match kind {
        EventCardKind::Unknown => {
            params.insert("card_kind".to_string(), "unknown".to_string());
        }
        EventCardKind::Specific(card) => {
            params.insert("card_kind".to_string(), "specific".to_string());
            params.insert("card".to_string(), card.stable_id().to_string());
        }
        other => {
            params.insert(
                "card_kind".to_string(),
                event_card_kind_v1(other).to_string(),
            );
        }
    }
}

fn relic_kind_params_v1(kind: EventRelicKind, params: &mut BTreeMap<String, String>) {
    match kind {
        EventRelicKind::Unknown => {
            params.insert("relic_kind".to_string(), "unknown".to_string());
        }
        EventRelicKind::Specific(relic) => {
            params.insert("relic_kind".to_string(), "specific".to_string());
            params.insert("relic".to_string(), relic.stable_id().to_string());
        }
        other => {
            params.insert(
                "relic_kind".to_string(),
                event_relic_kind_v1(other).to_string(),
            );
        }
    }
}

fn event_random_outcome_kind_v1(kind: EventRandomOutcomeKind) -> &'static str {
    match kind {
        EventRandomOutcomeKind::GremlinWheel => "gremlin_wheel",
        EventRandomOutcomeKind::JoustOwnerWin => "joust_owner_win",
        EventRandomOutcomeKind::DeadAdventurerSearch => "dead_adventurer_search",
        EventRandomOutcomeKind::MatchAndKeepFlip => "match_and_keep_flip",
        EventRandomOutcomeKind::ScrapOozeRelicRoll => "scrap_ooze_relic_roll",
    }
}

fn event_card_kind_v1(kind: EventCardKind) -> &'static str {
    match kind {
        EventCardKind::Unknown => "unknown",
        EventCardKind::Specific(_) => "specific",
        EventCardKind::RandomCurse => "random_curse",
        EventCardKind::RandomClassCard => "random_class_card",
        EventCardKind::RandomClassCommonOrUncommon => "random_class_common_or_uncommon",
        EventCardKind::RandomClassRare => "random_class_rare",
        EventCardKind::RandomColorless => "random_colorless",
        EventCardKind::RandomColorlessUncommon => "random_colorless_uncommon",
        EventCardKind::RandomColorlessRare => "random_colorless_rare",
    }
}

fn event_relic_kind_v1(kind: EventRelicKind) -> &'static str {
    match kind {
        EventRelicKind::Unknown => "unknown",
        EventRelicKind::Specific(_) => "specific",
        EventRelicKind::RandomCommonRelic => "random_common_relic",
        EventRelicKind::RandomUncommonRelic => "random_uncommon_relic",
        EventRelicKind::RandomRareRelic => "random_rare_relic",
        EventRelicKind::RandomShopRelic => "random_shop_relic",
        EventRelicKind::RandomBossRelic => "random_boss_relic",
        EventRelicKind::RandomRelic => "random_relic",
        EventRelicKind::RandomBook => "random_book",
        EventRelicKind::RandomFace => "random_face",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::events::{EventChoiceMeta, EventOptionSemantics, EventState};

    #[test]
    fn mushrooms_eat_candidate_keeps_structured_heal_and_curse_effects() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        session.run_state.max_hp = 80;
        session.run_state.current_hp = 60;

        let packet = event_boundary_packet_from_session_v1(&session).unwrap();

        assert_eq!(packet.event_id, "Mushrooms");
        assert_eq!(packet.boundary_class, "strategic_choice");
        let eat = packet
            .candidates
            .iter()
            .find(|candidate| candidate.command == "event 1")
            .unwrap();
        assert_eq!(eat.action_kind, "trade");
        assert_eq!(eat.role, "strategic_choice");
        assert!(eat.effects.iter().any(|effect| effect.kind == "heal"
            && effect.params.get("amount").map(String::as_str) == Some("20")));
        assert!(eat.effects.iter().any(|effect| {
            effect.kind == "obtain_curse"
                && effect.params.get("card_kind").map(String::as_str) == Some("specific")
                && effect.params.get("card").map(String::as_str) == Some("Parasite")
        }));
    }

    #[test]
    fn scrap_ooze_candidate_exposes_probability_without_guaranteeing_relic() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.ascension_level = 20;
        session.run_state.event_state = Some(EventState::new(EventId::ScrapOoze));

        let packet = event_boundary_packet_from_session_v1(&session).unwrap();
        let reach = packet
            .candidates
            .iter()
            .find(|candidate| candidate.command == "event 0")
            .unwrap();

        assert_eq!(reach.role, "stochastic_event");
        assert!(reach.repeatable);
        assert!(reach.effects.iter().any(|effect| {
            effect.kind == "random_outcome"
                && effect.params.get("outcome_kind").map(String::as_str)
                    == Some("scrap_ooze_relic_roll")
                && effect.params.get("chance_percent").map(String::as_str) == Some("25")
        }));
        assert!(!reach
            .effects
            .iter()
            .any(|effect| effect.kind == "obtain_relic"));
    }

    #[test]
    fn single_completed_event_candidate_is_forced_leave() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        let mut event_state = EventState::new(EventId::Mushrooms);
        event_state.completed = true;
        session.run_state.event_state = Some(event_state);

        let packet = event_boundary_packet_from_session_v1(&session).unwrap();

        assert_eq!(packet.boundary_class, "forced_leave");
        assert_eq!(packet.candidates.len(), 1);
        assert_eq!(packet.candidates[0].role, "forced_leave");
    }

    #[test]
    fn open_selection_candidate_records_selection_kind_without_text_parsing() {
        let option = EventOption::new(
            EventChoiceMeta::new("[Remove] Remove a card."),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: None,
                    kind: EventCardKind::Unknown,
                }],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard),
                ..Default::default()
            },
        );

        let candidate = event_candidate_snapshot_v1(EventId::Cleric, 0, 0, &option, 1);

        assert_eq!(candidate.transition, "open_selection");
        assert_eq!(candidate.selection_kind.as_deref(), Some("remove_card"));
        assert_eq!(candidate.role, "target_selection_gate");
        assert!(candidate
            .information_tags
            .iter()
            .any(|tag| tag == "opens_selection"));
        assert!(candidate
            .effects
            .iter()
            .any(|effect| effect.kind == "remove_card"));
    }

    #[test]
    fn detail_lines_are_structured_and_do_not_require_label_parsing() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.event_state = Some(EventState::new(EventId::Mushrooms));
        session.run_state.max_hp = 80;

        let packet = event_boundary_packet_from_session_v1(&session).unwrap();
        let lines = event_boundary_detail_lines_v1(&packet, 2);

        assert_eq!(lines[0], "event Mushrooms screen=0 class=strategic_choice");
        assert!(lines[1].contains("event 0 action=fight role=strategic_choice"));
        assert!(lines[2].contains("event 1 action=trade role=strategic_choice"));
        assert!(lines[2].contains("heal(amount=20)"));
        assert!(lines[2].contains("obtain_curse(card=Parasite,card_kind=specific,count=1)"));
    }

    #[test]
    fn cursed_tome_intro_read_is_strategic_not_trivial_flavor() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.event_state = Some(EventState::new(EventId::CursedTome));

        let packet = event_boundary_packet_from_session_v1(&session).unwrap();

        assert_eq!(packet.boundary_class, "strategic_choice");
        let read = packet
            .candidates
            .iter()
            .find(|candidate| candidate.command == "event 0")
            .unwrap();
        let leave = packet
            .candidates
            .iter()
            .find(|candidate| candidate.command == "event 1")
            .unwrap();
        assert_eq!(read.role, "strategic_choice");
        assert_eq!(leave.role, "safe_exit");
    }
}
