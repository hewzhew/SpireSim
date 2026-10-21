use crate::domain_manifest::{
    append_action_availability_v1, append_map_context_v1, manifest_scalar_v1,
    render_manifest_prelude_v1, require_internal_id, require_token, ActionAvailabilityV1,
    ActionLeaseEntryV1, ExactTrailV1, ManifestPreludeV1, ParityStateV1, PublicMapV1,
    PublicPotionBeltV1, RenderedManifestV1, SharedExactSceneV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub(super) const UNSUPPORTED_REWARD_ACTION_GAP_ID: &str =
    "reward-relic-key-and-full-belt-potion-actions-unsupported-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PublicCombatRewardItemV1 {
    Gold { amount: i32 },
    StolenGold { amount: i32 },
    Card { choice_count: usize },
    Relic { id: String },
    Potion { id: String },
    EmeraldKey,
    SapphireKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCombatRewardV1 {
    pub items: Vec<PublicCombatRewardItemV1>,
    pub close_available: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum CombatRewardActionKindV1 {
    Claim { item_index: usize },
    Close,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CombatRewardActionV1 {
    pub action_id: Option<String>,
    pub kind: CombatRewardActionKindV1,
    pub availability: ActionAvailabilityV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParityLabCombatRewardInputV1 {
    pub shared: SharedExactSceneV1,
    pub reward: PublicCombatRewardV1,
    pub map: PublicMapV1,
    pub trail: ExactTrailV1,
    pub actions: Vec<CombatRewardActionV1>,
}

pub(super) fn render_parity_lab_combat_reward_v1(
    input: &ParityLabCombatRewardInputV1,
) -> Result<RenderedManifestV1, String> {
    let PublicPotionBeltV1::Persistent(potion_slots) = &input.shared.potions else {
        return Err("CombatReward manifest requires a persistent potion belt".to_owned());
    };
    validate_reward_items_v1(&input.reward)?;
    let mut additional_gaps = Vec::new();
    if let ExactTrailV1::CoverageGap { gap_id, reason } = &input.trail {
        require_token(gap_id, "trail gap identity")?;
        require_token(reason, "trail gap reason")?;
        additional_gaps.push(gap_id.clone());
    }
    let mut text =
        match render_manifest_prelude_v1(&input.shared, "combat_reward", &additional_gaps)? {
            ManifestPreludeV1::Exact(text) => text,
            ManifestPreludeV1::Blocked(manifest) => return Ok(manifest),
        };

    writeln!(
        text,
        "\n[combat_reward close={}]",
        input.reward.close_available
    )
    .unwrap();
    if input.reward.items.is_empty() {
        writeln!(text, "none").unwrap();
    }
    for (ordinal, item) in input.reward.items.iter().enumerate() {
        writeln!(text, "{}", render_reward_item_v1(ordinal, item)?).unwrap();
    }
    append_map_context_v1(
        &mut text,
        input.shared.run.act,
        input.shared.run.floor,
        &input.map,
        &input.trail,
    )?;

    let has_empty_potion_slot = potion_slots.iter().any(|slot| slot.id.is_none());
    let ordered_actions = validate_actions_v1(input, has_empty_potion_slot)?;
    writeln!(text, "\n[actions]").unwrap();
    let mut leases = Vec::with_capacity(ordered_actions.len());
    for (ordinal, action) in ordered_actions.iter().enumerate() {
        let alias = format!("A{ordinal}");
        let mut line = match action.kind {
            CombatRewardActionKindV1::Claim { item_index } => {
                format!("{alias} claim REWARD{item_index}")
            }
            CombatRewardActionKindV1::Close => format!("{alias} close"),
        };
        let available = append_action_availability_v1(&mut line, &action.availability)?;
        writeln!(text, "{line}").unwrap();
        leases.push(ActionLeaseEntryV1 {
            alias,
            action_id: action.action_id.clone(),
            available,
        });
    }
    Ok(RenderedManifestV1 {
        text,
        actions: leases,
        diagnostics: Vec::new(),
    })
}

fn validate_reward_items_v1(reward: &PublicCombatRewardV1) -> Result<(), String> {
    if reward.items.is_empty() && !reward.close_available {
        return Err("empty CombatReward page has no close action".to_owned());
    }
    for item in &reward.items {
        match item {
            PublicCombatRewardItemV1::Gold { amount }
            | PublicCombatRewardItemV1::StolenGold { amount }
                if *amount < 0 =>
            {
                return Err("CombatReward gold amount is negative".to_owned());
            }
            PublicCombatRewardItemV1::Card { choice_count } if *choice_count == 0 => {
                return Err("CombatReward card item has no choices".to_owned());
            }
            PublicCombatRewardItemV1::Relic { id } | PublicCombatRewardItemV1::Potion { id } => {
                manifest_scalar_v1(id)?;
            }
            PublicCombatRewardItemV1::Gold { .. }
            | PublicCombatRewardItemV1::StolenGold { .. }
            | PublicCombatRewardItemV1::Card { .. }
            | PublicCombatRewardItemV1::EmeraldKey
            | PublicCombatRewardItemV1::SapphireKey => {}
        }
    }
    Ok(())
}

fn render_reward_item_v1(
    ordinal: usize,
    item: &PublicCombatRewardItemV1,
) -> Result<String, String> {
    Ok(match item {
        PublicCombatRewardItemV1::Gold { amount } => {
            format!("REWARD{ordinal} gold amount={amount}")
        }
        PublicCombatRewardItemV1::StolenGold { amount } => {
            format!("REWARD{ordinal} stolen_gold amount={amount}")
        }
        PublicCombatRewardItemV1::Card { choice_count } => {
            format!("REWARD{ordinal} card choices={choice_count}")
        }
        PublicCombatRewardItemV1::Relic { id } => {
            format!("REWARD{ordinal} relic id={}", manifest_scalar_v1(id)?)
        }
        PublicCombatRewardItemV1::Potion { id } => {
            format!("REWARD{ordinal} potion id={}", manifest_scalar_v1(id)?)
        }
        PublicCombatRewardItemV1::EmeraldKey => format!("REWARD{ordinal} emerald_key"),
        PublicCombatRewardItemV1::SapphireKey => format!("REWARD{ordinal} sapphire_key"),
    })
}

fn validate_actions_v1(
    input: &ParityLabCombatRewardInputV1,
    has_empty_potion_slot: bool,
) -> Result<Vec<CombatRewardActionV1>, String> {
    let mut action_ids = BTreeSet::new();
    let mut claims = BTreeMap::new();
    let mut close = None;
    for action in &input.actions {
        let Some(action_id) = action.action_id.as_deref() else {
            return Err("observed CombatReward action omitted its internal identity".to_owned());
        };
        require_internal_id(action_id)?;
        if !action_ids.insert(action_id) {
            return Err("CombatReward actions repeat an internal identity".to_owned());
        }
        match action.kind {
            CombatRewardActionKindV1::Claim { item_index } => {
                if item_index >= input.reward.items.len()
                    || claims.insert(item_index, action).is_some()
                {
                    return Err("CombatReward actions repeat or exceed a visible item".to_owned());
                }
            }
            CombatRewardActionKindV1::Close => {
                if close.replace(action).is_some() {
                    return Err("CombatReward actions repeat close".to_owned());
                }
            }
        }
    }
    if !input.reward.close_available && close.is_some() {
        return Err("CombatReward action surface invents an unavailable close".to_owned());
    }

    let mut needs_gap = input.reward.close_available && close.is_none();
    for (item_index, item) in input.reward.items.iter().enumerate() {
        let action_expected = match item {
            PublicCombatRewardItemV1::Gold { .. }
            | PublicCombatRewardItemV1::StolenGold { .. }
            | PublicCombatRewardItemV1::Card { .. } => true,
            PublicCombatRewardItemV1::Potion { .. } => has_empty_potion_slot,
            PublicCombatRewardItemV1::Relic { .. }
            | PublicCombatRewardItemV1::EmeraldKey
            | PublicCombatRewardItemV1::SapphireKey => false,
        };
        match (action_expected, claims.contains_key(&item_index)) {
            (true, false) => {
                return Err("supported CombatReward item omitted its typed action".to_owned())
            }
            (false, true) => {
                return Err(
                    "unsupported CombatReward item unexpectedly gained an action".to_owned(),
                )
            }
            (false, false) => needs_gap = true,
            (true, true) => {}
        }
    }
    if claims.len() > input.reward.items.len() {
        return Err("CombatReward action surface exceeds its room items".to_owned());
    }
    if needs_gap {
        let ParityStateV1::Exact(parity) = &input.shared.parity else {
            unreachable!("blocked parity returned above")
        };
        if !parity
            .coverage_gap_ids
            .iter()
            .any(|gap_id| gap_id == UNSUPPORTED_REWARD_ACTION_GAP_ID)
        {
            return Err("partial CombatReward surface has no declared coverage gap".to_owned());
        }
    }
    let unavailable = || CombatRewardActionV1 {
        action_id: None,
        kind: CombatRewardActionKindV1::Close,
        availability: ActionAvailabilityV1::Unknown {
            gap_id: UNSUPPORTED_REWARD_ACTION_GAP_ID.to_owned(),
        },
    };
    let mut ordered = Vec::with_capacity(input.reward.items.len().saturating_add(1));
    for item_index in 0..input.reward.items.len() {
        if let Some(action) = claims.remove(&item_index) {
            ordered.push(action.clone());
        } else {
            let mut action = unavailable();
            action.kind = CombatRewardActionKindV1::Claim { item_index };
            ordered.push(action);
        }
    }
    if input.reward.close_available {
        ordered.push(close.cloned().unwrap_or_else(unavailable));
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_manifest::{
        ExactParityV1, ManifestAuthorityV1, MapCoordinateV1, MapRoomTagV1, ParityStateV1,
        PublicMapNodeV1, PublicPersistentPotionSlotV1, PublicRunV1, SharedExactSceneV1,
        TrailPointV1,
    };
    use sts_paired_root::WorldProvenanceV1;

    fn input() -> ParityLabCombatRewardInputV1 {
        ParityLabCombatRewardInputV1 {
            shared: SharedExactSceneV1 {
                world: WorldProvenanceV1::Natural,
                authority: ManifestAuthorityV1::LabHead,
                parity: ParityStateV1::Exact(ExactParityV1 {
                    exact_domains: 11,
                    total_domains: 11,
                    coverage_gap_ids: vec![UNSUPPORTED_REWARD_ACTION_GAP_ID.to_owned()],
                }),
                run: PublicRunV1 {
                    seed: "1".to_owned(),
                    ascension: 20,
                    character: "IRONCLAD".to_owned(),
                    act: 1,
                    floor: 1,
                    current_hp: 74,
                    max_hp: 75,
                    gold: 99,
                    final_act_available: true,
                    keys: [false; 3],
                },
                master_deck: Vec::new(),
                relics: Vec::new(),
                potions: PublicPotionBeltV1::Persistent(vec![
                    PublicPersistentPotionSlotV1 { slot: 0, id: None },
                    PublicPersistentPotionSlotV1 { slot: 1, id: None },
                ]),
            },
            reward: PublicCombatRewardV1 {
                items: vec![
                    PublicCombatRewardItemV1::Gold { amount: 10 },
                    PublicCombatRewardItemV1::Card { choice_count: 3 },
                ],
                close_available: false,
            },
            map: PublicMapV1 {
                current: Some(MapCoordinateV1 { x: 1, y: 0 }),
                boss_id: Some("hexaghost".to_owned()),
                boss_anchor: Some(MapCoordinateV1 { x: 3, y: 16 }),
                nodes: vec![PublicMapNodeV1 {
                    coordinate: MapCoordinateV1 { x: 1, y: 0 },
                    room: MapRoomTagV1::Monster,
                    burning: false,
                    edges: vec![MapCoordinateV1 { x: 3, y: 16 }],
                }],
            },
            trail: ExactTrailV1::Available(vec![
                TrailPointV1::Start,
                TrailPointV1::Node(MapCoordinateV1 { x: 1, y: 0 }),
            ]),
            actions: vec![
                CombatRewardActionV1 {
                    action_id: Some("gold".to_owned()),
                    kind: CombatRewardActionKindV1::Claim { item_index: 0 },
                    availability: ActionAvailabilityV1::Verified,
                },
                CombatRewardActionV1 {
                    action_id: Some("card".to_owned()),
                    kind: CombatRewardActionKindV1::Claim { item_index: 1 },
                    availability: ActionAvailabilityV1::Verified,
                },
            ],
        }
    }

    #[test]
    fn complete_reward_surface_uses_reward_and_action_aliases() {
        let rendered = render_parity_lab_combat_reward_v1(&input()).unwrap();
        assert!(rendered.text.contains("REWARD0 gold amount=10"));
        assert!(rendered.text.contains("REWARD1 card choices=3"));
        assert!(rendered.text.contains("A0 claim REWARD0"));
        assert!(rendered.text.contains("A1 claim REWARD1"));
        assert_eq!(rendered.actions.len(), 2);
        assert!(!rendered.text.contains("action_id"));
    }

    #[test]
    fn full_belt_potion_remains_visible_without_an_execution_identity() {
        let mut input = input();
        input.shared.potions = PublicPotionBeltV1::Persistent(vec![
            PublicPersistentPotionSlotV1 {
                slot: 0,
                id: Some("firepotion".to_owned()),
            },
            PublicPersistentPotionSlotV1 {
                slot: 1,
                id: Some("weakenpotion".to_owned()),
            },
        ]);
        input.reward.items = vec![PublicCombatRewardItemV1::Potion {
            id: "strengthpotion".to_owned(),
        }];
        input.actions.clear();
        let rendered = render_parity_lab_combat_reward_v1(&input).unwrap();
        assert!(rendered.text.contains(&format!(
            "A0 claim REWARD0 unavailable=unknown gap={UNSUPPORTED_REWARD_ACTION_GAP_ID}"
        )));
        assert_eq!(rendered.actions.len(), 1);
        assert!(rendered.actions[0].action_id.is_none());
    }

    #[test]
    fn visible_nonempty_close_does_not_hide_supported_claims() {
        let mut input = input();
        input.reward.close_available = true;
        let rendered = render_parity_lab_combat_reward_v1(&input).unwrap();
        assert!(rendered.text.contains(&format!(
            "A2 close unavailable=unknown gap={UNSUPPORTED_REWARD_ACTION_GAP_ID}"
        )));
        assert_eq!(rendered.actions.len(), 3);
        assert!(rendered.actions[0].available);
        assert!(rendered.actions[1].available);
        assert!(rendered.actions[2].action_id.is_none());
    }

    #[test]
    fn supported_reward_cannot_silently_omit_its_action() {
        let mut input = input();
        input.actions.pop();
        assert!(render_parity_lab_combat_reward_v1(&input).is_err());
    }

    #[test]
    fn hidden_close_cannot_gain_an_action() {
        let mut input = input();
        input.actions.push(CombatRewardActionV1 {
            action_id: Some("close".to_owned()),
            kind: CombatRewardActionKindV1::Close,
            availability: ActionAvailabilityV1::Verified,
        });
        assert!(render_parity_lab_combat_reward_v1(&input).is_err());
    }
}
