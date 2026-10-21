use crate::domain_manifest::{
    append_action_availability_v1, append_map_context_v1, manifest_scalar_v1,
    render_manifest_prelude_v1, require_internal_id, require_token, ActionAvailabilityV1,
    ActionLeaseEntryV1, ExactTrailV1, ManifestPreludeV1, ParityStateV1, PublicMapV1,
    PublicPotionBeltV1, RenderedManifestV1, SharedExactSceneV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub(super) const SINGING_BOWL_GAP_ID: &str = "card-reward-singing-bowl-action-unsupported-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCardRewardChoiceV1 {
    pub card_id: String,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCardRewardV1 {
    pub choices: Vec<PublicCardRewardChoiceV1>,
    pub skip_available: bool,
    pub singing_bowl_available: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum CardRewardActionKindV1 {
    Take { choice_index: usize },
    Skip,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CardRewardActionV1 {
    pub action_id: String,
    pub kind: CardRewardActionKindV1,
    pub availability: ActionAvailabilityV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParityLabCardRewardInputV1 {
    pub shared: SharedExactSceneV1,
    pub reward: PublicCardRewardV1,
    pub map: PublicMapV1,
    pub trail: ExactTrailV1,
    pub actions: Vec<CardRewardActionV1>,
}

pub(super) fn render_parity_lab_card_reward_v1(
    input: &ParityLabCardRewardInputV1,
) -> Result<RenderedManifestV1, String> {
    if !matches!(input.shared.potions, PublicPotionBeltV1::Persistent(_)) {
        return Err("CardReward manifest requires a persistent potion belt".to_owned());
    }
    if input.reward.choices.is_empty() {
        return Err("CardReward manifest cannot render an empty choice list".to_owned());
    }
    for choice in &input.reward.choices {
        manifest_scalar_v1(&choice.card_id)?;
    }
    let mut additional_gaps = Vec::new();
    if let ExactTrailV1::CoverageGap { gap_id, reason } = &input.trail {
        require_token(gap_id, "trail gap identity")?;
        require_token(reason, "trail gap reason")?;
        additional_gaps.push(gap_id.clone());
    }
    let mut text = match render_manifest_prelude_v1(&input.shared, "card_reward", &additional_gaps)?
    {
        ManifestPreludeV1::Exact(text) => text,
        ManifestPreludeV1::Blocked(manifest) => return Ok(manifest),
    };
    let ParityStateV1::Exact(parity) = &input.shared.parity else {
        unreachable!("blocked parity returned above")
    };
    if input.reward.singing_bowl_available
        && !parity
            .coverage_gap_ids
            .iter()
            .any(|gap_id| gap_id == SINGING_BOWL_GAP_ID)
    {
        return Err("available Singing Bowl action has no declared coverage gap".to_owned());
    }

    writeln!(
        text,
        "\n[card_reward source=ordinary_reward skip={} singing_bowl={}]",
        input.reward.skip_available, input.reward.singing_bowl_available,
    )
    .unwrap();
    for (ordinal, choice) in input.reward.choices.iter().enumerate() {
        writeln!(
            text,
            "CARD{ordinal} id={} upgrades={}",
            manifest_scalar_v1(&choice.card_id)?,
            choice.upgrades,
        )
        .unwrap();
    }
    append_map_context_v1(
        &mut text,
        input.shared.run.act,
        input.shared.run.floor,
        &input.map,
        &input.trail,
    )?;

    let ordered_actions = validate_actions_v1(input)?;
    writeln!(text, "\n[actions]").unwrap();
    if input.reward.singing_bowl_available {
        writeln!(
            text,
            "none unavailable=coverage_gap gap={SINGING_BOWL_GAP_ID}"
        )
        .unwrap();
        return Ok(RenderedManifestV1 {
            text,
            actions: Vec::new(),
            diagnostics: Vec::new(),
        });
    }

    let mut leases = Vec::with_capacity(ordered_actions.len());
    for (ordinal, action) in ordered_actions.into_iter().enumerate() {
        let alias = format!("A{ordinal}");
        let mut line = match action.kind {
            CardRewardActionKindV1::Take { choice_index } => {
                format!("{alias} take CARD{choice_index}")
            }
            CardRewardActionKindV1::Skip => format!("{alias} skip"),
        };
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

fn validate_actions_v1(
    input: &ParityLabCardRewardInputV1,
) -> Result<Vec<&CardRewardActionV1>, String> {
    let mut action_ids = BTreeSet::new();
    let mut choices = BTreeMap::new();
    let mut skip = None;
    for action in &input.actions {
        require_internal_id(&action.action_id)?;
        if !action_ids.insert(action.action_id.as_str()) {
            return Err("CardReward actions repeat an internal identity".to_owned());
        }
        match action.kind {
            CardRewardActionKindV1::Take { choice_index } => {
                if choice_index >= input.reward.choices.len()
                    || choices.insert(choice_index, action).is_some()
                {
                    return Err("CardReward actions repeat or exceed a visible choice".to_owned());
                }
            }
            CardRewardActionKindV1::Skip => {
                if skip.replace(action).is_some() {
                    return Err("CardReward actions repeat skip".to_owned());
                }
            }
        }
    }
    if choices.len() != input.reward.choices.len() || input.reward.skip_available != skip.is_some()
    {
        return Err("CardReward actions do not cover the Exact room choices".to_owned());
    }
    let mut ordered = choices.into_values().collect::<Vec<_>>();
    if let Some(skip) = skip {
        ordered.push(skip);
    }
    Ok(ordered)
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

    fn input() -> ParityLabCardRewardInputV1 {
        ParityLabCardRewardInputV1 {
            shared: SharedExactSceneV1 {
                world: WorldProvenanceV1::Natural,
                authority: ManifestAuthorityV1::LabHead,
                parity: ParityStateV1::Exact(ExactParityV1 {
                    exact_domains: 9,
                    total_domains: 9,
                    coverage_gap_ids: vec![SINGING_BOWL_GAP_ID.to_owned()],
                }),
                run: PublicRunV1 {
                    seed: "18446744073709551615".to_owned(),
                    ascension: 20,
                    character: "IRONCLAD".to_owned(),
                    act: 1,
                    floor: 0,
                    current_hp: 80,
                    max_hp: 80,
                    gold: 99,
                    final_act_available: true,
                    keys: [false; 3],
                },
                master_deck: vec![PublicMasterDeckCardV1 {
                    ordinal: 0,
                    card_id: "Perfected Strike".to_owned(),
                    upgrades: 0,
                }],
                relics: Vec::new(),
                potions: PublicPotionBeltV1::Persistent(Vec::new()),
            },
            reward: PublicCardRewardV1 {
                choices: vec![
                    PublicCardRewardChoiceV1 {
                        card_id: "Perfected Strike".to_owned(),
                        upgrades: 0,
                    },
                    PublicCardRewardChoiceV1 {
                        card_id: "Bash".to_owned(),
                        upgrades: 0,
                    },
                ],
                skip_available: true,
                singing_bowl_available: false,
            },
            map: PublicMapV1 {
                current: None,
                boss_id: Some("hexaghost".to_owned()),
                boss_anchor: Some(MapCoordinateV1 { x: 3, y: 16 }),
                nodes: vec![PublicMapNodeV1 {
                    coordinate: MapCoordinateV1 { x: 1, y: 0 },
                    room: MapRoomTagV1::Monster,
                    burning: false,
                    edges: vec![MapCoordinateV1 { x: 3, y: 16 }],
                }],
            },
            trail: ExactTrailV1::Available(vec![TrailPointV1::Start]),
            actions: vec![
                CardRewardActionV1 {
                    action_id: "take-perfect".to_owned(),
                    kind: CardRewardActionKindV1::Take { choice_index: 0 },
                    availability: ActionAvailabilityV1::Verified,
                },
                CardRewardActionV1 {
                    action_id: "take-bash".to_owned(),
                    kind: CardRewardActionKindV1::Take { choice_index: 1 },
                    availability: ActionAvailabilityV1::Verified,
                },
                CardRewardActionV1 {
                    action_id: "skip".to_owned(),
                    kind: CardRewardActionKindV1::Skip,
                    availability: ActionAvailabilityV1::Verified,
                },
            ],
        }
    }

    #[test]
    fn card_reward_uses_readable_quoted_scalars_and_action_aliases() {
        let rendered = render_parity_lab_card_reward_v1(&input()).unwrap();
        assert!(rendered
            .text
            .contains("DECK0 id=\"Perfected Strike\" upgrades=0"));
        assert!(rendered
            .text
            .contains("CARD0 id=\"Perfected Strike\" upgrades=0"));
        assert!(rendered.text.contains("A0 take CARD0"));
        assert!(rendered.text.contains("A2 skip"));
        assert_eq!(rendered.actions.len(), 3);
        assert!(!rendered.text.contains("take-perfect"));
    }

    #[test]
    fn available_singing_bowl_blocks_partial_action_surface() {
        let mut input = input();
        input.reward.singing_bowl_available = true;
        let rendered = render_parity_lab_card_reward_v1(&input).unwrap();
        assert!(rendered.text.contains(&format!(
            "none unavailable=coverage_gap gap={SINGING_BOWL_GAP_ID}"
        )));
        assert!(rendered.actions.is_empty());
    }
}
