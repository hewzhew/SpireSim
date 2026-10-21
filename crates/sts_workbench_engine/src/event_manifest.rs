use crate::domain_manifest::{
    append_action_availability_v1, append_map_context_v1, manifest_scalar_v1,
    render_manifest_prelude_v1, require_internal_id, require_token, ActionAvailabilityV1,
    ActionLeaseEntryV1, ExactTrailV1, ManifestPreludeV1, PublicMapV1, PublicPotionBeltV1,
    RenderedManifestV1, SharedExactSceneV1,
};
use crate::observation::{
    ObservedEventActionKindV1, ObservedEventFollowupV1, ObservedEventOptionV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicEventV1 {
    pub event_id: String,
    pub screen: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EventActionV1 {
    pub action_id: String,
    pub option_index: usize,
    pub semantics: ObservedEventOptionV1,
    pub availability: ActionAvailabilityV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParityLabEventInputV1 {
    pub shared: SharedExactSceneV1,
    pub event: PublicEventV1,
    pub map: PublicMapV1,
    pub trail: ExactTrailV1,
    pub actions: Vec<EventActionV1>,
}

pub(super) fn render_parity_lab_event_v1(
    input: &ParityLabEventInputV1,
) -> Result<RenderedManifestV1, String> {
    if !matches!(input.shared.potions, PublicPotionBeltV1::Persistent(_)) {
        return Err("Event manifest requires a persistent potion belt".to_owned());
    }
    let event_id = manifest_scalar_v1(&input.event.event_id)?;
    let mut additional_gaps = Vec::new();
    if let ExactTrailV1::CoverageGap { gap_id, reason } = &input.trail {
        require_token(gap_id, "trail gap identity")?;
        require_token(reason, "trail gap reason")?;
        additional_gaps.push(gap_id.clone());
    }
    let mut text = match render_manifest_prelude_v1(&input.shared, "event", &additional_gaps)? {
        ManifestPreludeV1::Exact(text) => text,
        ManifestPreludeV1::Blocked(manifest) => return Ok(manifest),
    };
    writeln!(
        text,
        "\n[event id={event_id} screen={}]",
        input.event.screen
    )
    .unwrap();
    append_map_context_v1(
        &mut text,
        input.shared.run.act,
        input.shared.run.floor,
        &input.map,
        &input.trail,
    )?;

    let ordered_actions = validate_actions_v1(input)?;
    writeln!(text, "\n[actions semantics=simulator_public]").unwrap();
    let mut leases = Vec::with_capacity(ordered_actions.len());
    for (ordinal, action) in ordered_actions.into_iter().enumerate() {
        let alias = format!("A{ordinal}");
        let mut line = format!(
            "{alias} choose option={} kind={} next={}",
            action.option_index,
            event_action_kind_token_v1(action.semantics.action_kind),
            event_followup_token_v1(action.semantics.followup),
        );
        if let Some(summary) = &action.semantics.effect_summary {
            write!(line, " effect={}", manifest_scalar_v1(summary)?).unwrap();
        }
        if action.semantics.repeatable {
            line.push_str(" repeatable=true");
        }
        let available = append_action_availability_v1(&mut line, &action.availability)?;
        writeln!(text, "{line}").unwrap();
        leases.push(ActionLeaseEntryV1 {
            alias,
            action_id: Some(action.action_id.clone()),
            available,
        });
    }
    Ok(RenderedManifestV1 {
        text,
        actions: leases,
        diagnostics: Vec::new(),
    })
}

fn event_action_kind_token_v1(kind: ObservedEventActionKindV1) -> &'static str {
    match kind {
        ObservedEventActionKindV1::Unknown => "unknown",
        ObservedEventActionKindV1::Leave => "leave",
        ObservedEventActionKindV1::Continue => "continue",
        ObservedEventActionKindV1::Accept => "accept",
        ObservedEventActionKindV1::Decline => "decline",
        ObservedEventActionKindV1::Fight => "fight",
        ObservedEventActionKindV1::Trade => "trade",
        ObservedEventActionKindV1::DeckOperation => "deck_operation",
        ObservedEventActionKindV1::Gain => "gain",
        ObservedEventActionKindV1::Special => "special",
    }
}

fn event_followup_token_v1(followup: ObservedEventFollowupV1) -> &'static str {
    match followup {
        ObservedEventFollowupV1::None => "none",
        ObservedEventFollowupV1::EventScreen => "event_screen",
        ObservedEventFollowupV1::EventComplete => "event_complete",
        ObservedEventFollowupV1::RemoveCardSelection => "remove_card_selection",
        ObservedEventFollowupV1::UpgradeCardSelection => "upgrade_card_selection",
        ObservedEventFollowupV1::TransformCardSelection => "transform_card_selection",
        ObservedEventFollowupV1::DuplicateCardSelection => "duplicate_card_selection",
        ObservedEventFollowupV1::OfferCardSelection => "offer_card_selection",
        ObservedEventFollowupV1::UnknownSelection => "unknown_selection",
        ObservedEventFollowupV1::Reward => "reward",
        ObservedEventFollowupV1::Combat => "combat",
    }
}

fn validate_actions_v1(input: &ParityLabEventInputV1) -> Result<Vec<&EventActionV1>, String> {
    if input.actions.is_empty() {
        return Err("Event manifest cannot render an empty action surface".to_owned());
    }
    let mut action_ids = BTreeSet::new();
    let mut options = BTreeMap::new();
    for action in &input.actions {
        require_internal_id(&action.action_id)?;
        if !action_ids.insert(action.action_id.as_str()) {
            return Err("Event actions repeat an internal identity".to_owned());
        }
        if options.insert(action.option_index, action).is_some() {
            return Err("Event actions repeat one option index".to_owned());
        }
    }
    Ok(options.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_manifest::{
        ExactParityV1, ManifestAuthorityV1, MapCoordinateV1, MapRoomTagV1, ParityStateV1,
        PublicMapNodeV1, PublicMasterDeckCardV1, PublicPotionBeltV1, PublicRunV1,
        SharedExactSceneV1, TrailPointV1,
    };
    use sts_paired_root::WorldProvenanceV1;

    fn input() -> ParityLabEventInputV1 {
        ParityLabEventInputV1 {
            shared: SharedExactSceneV1 {
                world: WorldProvenanceV1::Natural,
                authority: ManifestAuthorityV1::LabHead,
                parity: ParityStateV1::Exact(ExactParityV1 {
                    exact_domains: 10,
                    total_domains: 10,
                    coverage_gap_ids: Vec::new(),
                }),
                run: PublicRunV1 {
                    seed: "7".to_owned(),
                    ascension: 20,
                    character: "IRONCLAD".to_owned(),
                    act: 1,
                    floor: 2,
                    current_hp: 70,
                    max_hp: 80,
                    gold: 99,
                    final_act_available: true,
                    keys: [false; 3],
                },
                master_deck: vec![PublicMasterDeckCardV1 {
                    ordinal: 0,
                    card_id: "Strike_R".to_owned(),
                    upgrades: 0,
                }],
                relics: Vec::new(),
                potions: PublicPotionBeltV1::Persistent(Vec::new()),
            },
            event: PublicEventV1 {
                event_id: "Big Fish".to_owned(),
                screen: 0,
            },
            map: PublicMapV1 {
                current: Some(MapCoordinateV1 { x: 2, y: 1 }),
                boss_id: Some("hexaghost".to_owned()),
                boss_anchor: Some(MapCoordinateV1 { x: 3, y: 16 }),
                nodes: vec![
                    PublicMapNodeV1 {
                        coordinate: MapCoordinateV1 { x: 1, y: 0 },
                        room: MapRoomTagV1::Question,
                        burning: false,
                        edges: vec![MapCoordinateV1 { x: 2, y: 1 }],
                    },
                    PublicMapNodeV1 {
                        coordinate: MapCoordinateV1 { x: 2, y: 1 },
                        room: MapRoomTagV1::Question,
                        burning: false,
                        edges: vec![MapCoordinateV1 { x: 3, y: 16 }],
                    },
                ],
            },
            trail: ExactTrailV1::Available(vec![
                TrailPointV1::Start,
                TrailPointV1::Node(MapCoordinateV1 { x: 1, y: 0 }),
                TrailPointV1::Node(MapCoordinateV1 { x: 2, y: 1 }),
            ]),
            actions: vec![
                EventActionV1 {
                    action_id: "event-option-two".to_owned(),
                    option_index: 2,
                    semantics: ObservedEventOptionV1 {
                        action_kind: ObservedEventActionKindV1::Leave,
                        effect_summary: None,
                        followup: ObservedEventFollowupV1::EventComplete,
                        repeatable: false,
                    },
                    availability: ActionAvailabilityV1::Unknown {
                        gap_id: "event-option-two-gap".to_owned(),
                    },
                },
                EventActionV1 {
                    action_id: "event-option-zero".to_owned(),
                    option_index: 0,
                    semantics: ObservedEventOptionV1 {
                        action_kind: ObservedEventActionKindV1::Gain,
                        effect_summary: Some("gain 50 gold".to_owned()),
                        followup: ObservedEventFollowupV1::EventScreen,
                        repeatable: false,
                    },
                    availability: ActionAvailabilityV1::Verified,
                },
            ],
        }
    }

    #[test]
    fn event_manifest_keeps_identity_screen_map_and_actions_explicit() {
        let rendered = render_parity_lab_event_v1(&input()).unwrap();
        assert!(rendered.text.contains("scene=event"));
        assert!(rendered.text.contains("[event id=\"Big Fish\" screen=0]"));
        assert!(rendered.text.contains("F02X02 question -> BOSS:boss"));
        assert!(rendered
            .text
            .contains("A0 choose option=0 kind=gain next=event_screen effect=\"gain 50 gold\""));
        assert!(rendered
            .text
            .contains("A1 choose option=2 kind=leave next=event_complete unavailable=unknown gap=event-option-two-gap"));
        assert_eq!(
            rendered.actions[0].action_id.as_deref(),
            Some("event-option-zero")
        );
        assert!(rendered.actions[0].available);
        assert!(!rendered.actions[1].available);
        assert!(!rendered.text.contains("event-option-zero"));
    }

    #[test]
    fn event_manifest_rejects_duplicate_option_indices() {
        let mut input = input();
        input.actions[1].option_index = 2;
        let error = render_parity_lab_event_v1(&input).unwrap_err();
        assert!(error.contains("repeat one option index"));
    }
}
