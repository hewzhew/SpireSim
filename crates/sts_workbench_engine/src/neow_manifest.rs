use crate::domain_manifest::{
    append_action_availability_v1, append_map_context_v1, render_manifest_prelude_v1,
    require_internal_id, require_token, ActionAvailabilityV1, ActionLeaseEntryV1, ExactTrailV1,
    ManifestPreludeV1, PublicMapV1, PublicPotionBeltV1, RenderedManifestV1, SharedExactSceneV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use sts_paired_root::{NeowDrawbackIdentityV1, NeowRewardIdentityV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NeowStageV1 {
    ChooseReward,
    Leave,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum NeowActionKindV1 {
    Choose {
        reward: NeowRewardIdentityV1,
        drawback: NeowDrawbackIdentityV1,
    },
    Leave,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NeowActionV1 {
    pub action_id: String,
    pub kind: NeowActionKindV1,
    pub availability: ActionAvailabilityV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParityLabNeowInputV1 {
    pub shared: SharedExactSceneV1,
    pub stage: NeowStageV1,
    pub map: PublicMapV1,
    pub trail: ExactTrailV1,
    pub actions: Vec<NeowActionV1>,
}

pub(super) fn render_parity_lab_neow_v1(
    input: &ParityLabNeowInputV1,
) -> Result<RenderedManifestV1, String> {
    if !matches!(input.shared.potions, PublicPotionBeltV1::Persistent(_)) {
        return Err("Neow manifest requires a persistent potion belt".to_owned());
    }
    let mut additional_gaps = Vec::new();
    if let ExactTrailV1::CoverageGap { gap_id, reason } = &input.trail {
        require_token(gap_id, "trail gap identity")?;
        require_token(reason, "trail gap reason")?;
        additional_gaps.push(gap_id.clone());
    }
    let mut text = match render_manifest_prelude_v1(&input.shared, "neow", &additional_gaps)? {
        ManifestPreludeV1::Exact(text) => text,
        ManifestPreludeV1::Blocked(manifest) => return Ok(manifest),
    };
    writeln!(text, "\n[neow stage={}]", stage_token_v1(input.stage)).unwrap();
    append_map_context_v1(
        &mut text,
        input.shared.run.act,
        input.shared.run.floor,
        &input.map,
        &input.trail,
    )?;

    let ordered_actions = validate_actions_v1(input)?;
    writeln!(text, "\n[actions]").unwrap();
    let mut leases = Vec::with_capacity(ordered_actions.len());
    for (ordinal, action) in ordered_actions.into_iter().enumerate() {
        let alias = format!("A{ordinal}");
        let mut line = match action.kind {
            NeowActionKindV1::Choose { reward, drawback } => format!(
                "{alias} choose reward={} drawback={}",
                reward_token_v1(reward),
                drawback_token_v1(drawback),
            ),
            NeowActionKindV1::Leave => format!("{alias} leave"),
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

fn validate_actions_v1(input: &ParityLabNeowInputV1) -> Result<Vec<&NeowActionV1>, String> {
    let mut action_ids = BTreeSet::new();
    let mut choices = BTreeMap::new();
    let mut leave = None;
    for action in &input.actions {
        require_internal_id(&action.action_id)?;
        if !action_ids.insert(action.action_id.as_str()) {
            return Err("Neow actions repeat an internal identity".to_owned());
        }
        match action.kind {
            NeowActionKindV1::Choose { reward, drawback } => {
                if choices.insert((reward, drawback), action).is_some() {
                    return Err("Neow actions repeat a reward and drawback identity".to_owned());
                }
            }
            NeowActionKindV1::Leave => {
                if leave.replace(action).is_some() {
                    return Err("Neow actions repeat leave".to_owned());
                }
            }
        }
    }
    match input.stage {
        NeowStageV1::ChooseReward => {
            if !matches!(choices.len(), 2 | 4) || leave.is_some() {
                return Err("Neow reward stage requires exactly two or four choices".to_owned());
            }
            Ok(choices.into_values().collect())
        }
        NeowStageV1::Leave => {
            if !choices.is_empty() || leave.is_none() {
                return Err("completed Neow stage requires exactly one leave action".to_owned());
            }
            Ok(vec![leave.expect("leave presence checked above")])
        }
    }
}

fn stage_token_v1(stage: NeowStageV1) -> &'static str {
    match stage {
        NeowStageV1::ChooseReward => "choose_reward",
        NeowStageV1::Leave => "leave",
    }
}

fn reward_token_v1(reward: NeowRewardIdentityV1) -> &'static str {
    match reward {
        NeowRewardIdentityV1::ThreeEnemyKill => "three_enemy_kill",
        NeowRewardIdentityV1::TenPercentHpBonus => "ten_percent_hp_bonus",
        NeowRewardIdentityV1::ThreeCards => "three_cards",
        NeowRewardIdentityV1::OneRandomRareCard => "one_random_rare_card",
        NeowRewardIdentityV1::RemoveCard => "remove_card",
        NeowRewardIdentityV1::UpgradeCard => "upgrade_card",
        NeowRewardIdentityV1::TransformCard => "transform_card",
        NeowRewardIdentityV1::RandomColorless => "random_colorless",
        NeowRewardIdentityV1::ThreeSmallPotions => "three_small_potions",
        NeowRewardIdentityV1::RandomCommonRelic => "random_common_relic",
        NeowRewardIdentityV1::HundredGold => "hundred_gold",
        NeowRewardIdentityV1::RandomColorless2 => "random_colorless_2",
        NeowRewardIdentityV1::RemoveTwo => "remove_two",
        NeowRewardIdentityV1::OneRareRelic => "one_rare_relic",
        NeowRewardIdentityV1::ThreeRareCards => "three_rare_cards",
        NeowRewardIdentityV1::TwoFiftyGold => "two_fifty_gold",
        NeowRewardIdentityV1::TransformTwoCards => "transform_two_cards",
        NeowRewardIdentityV1::TwentyPercentHpBonus => "twenty_percent_hp_bonus",
        NeowRewardIdentityV1::BossRelic => "boss_relic",
    }
}

fn drawback_token_v1(drawback: NeowDrawbackIdentityV1) -> &'static str {
    match drawback {
        NeowDrawbackIdentityV1::None => "none",
        NeowDrawbackIdentityV1::TenPercentHpLoss => "ten_percent_hp_loss",
        NeowDrawbackIdentityV1::NoGold => "no_gold",
        NeowDrawbackIdentityV1::Curse => "curse",
        NeowDrawbackIdentityV1::PercentDamage => "percent_damage",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_manifest::{
        ExactParityV1, ManifestAuthorityV1, MapCoordinateV1, MapRoomTagV1, ParityStateV1,
        PublicMapNodeV1, PublicRunV1,
    };
    use sts_paired_root::WorldProvenanceV1;

    fn input() -> ParityLabNeowInputV1 {
        ParityLabNeowInputV1 {
            shared: SharedExactSceneV1 {
                world: WorldProvenanceV1::Natural,
                authority: ManifestAuthorityV1::LabHead,
                parity: ParityStateV1::Exact(ExactParityV1 {
                    exact_domains: 11,
                    total_domains: 11,
                    coverage_gap_ids: vec!["neow-closed-transition-unverified-v1".to_owned()],
                }),
                run: PublicRunV1 {
                    seed: "1".to_owned(),
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
                master_deck: Vec::new(),
                relics: Vec::new(),
                potions: PublicPotionBeltV1::Persistent(Vec::new()),
            },
            stage: NeowStageV1::ChooseReward,
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
            trail: ExactTrailV1::Available(vec![crate::domain_manifest::TrailPointV1::Start]),
            actions: vec![
                NeowActionV1 {
                    action_id: "lament".to_owned(),
                    kind: NeowActionKindV1::Choose {
                        reward: NeowRewardIdentityV1::ThreeEnemyKill,
                        drawback: NeowDrawbackIdentityV1::None,
                    },
                    availability: ActionAvailabilityV1::Verified,
                },
                NeowActionV1 {
                    action_id: "rare-relic".to_owned(),
                    kind: NeowActionKindV1::Choose {
                        reward: NeowRewardIdentityV1::OneRareRelic,
                        drawback: NeowDrawbackIdentityV1::Curse,
                    },
                    availability: ActionAvailabilityV1::Unknown {
                        gap_id: "neow-closed-transition-unverified-v1".to_owned(),
                    },
                },
            ],
        }
    }

    #[test]
    fn reward_choices_are_semantically_ordered_and_keep_availability() {
        let rendered = render_parity_lab_neow_v1(&input()).unwrap();
        assert!(rendered
            .text
            .contains("A0 choose reward=three_enemy_kill drawback=none"));
        assert!(rendered.text.contains(
            "A1 choose reward=one_rare_relic drawback=curse unavailable=unknown gap=neow-closed-transition-unverified-v1"
        ));
        assert_eq!(rendered.actions.len(), 2);
        assert!(rendered.actions[0].available);
        assert!(!rendered.actions[1].available);
        assert!(!rendered.text.contains("lament"));
    }

    #[test]
    fn completed_stage_exposes_only_leave() {
        let mut input = input();
        input.stage = NeowStageV1::Leave;
        input.actions = vec![NeowActionV1 {
            action_id: "leave".to_owned(),
            kind: NeowActionKindV1::Leave,
            availability: ActionAvailabilityV1::Verified,
        }];
        let rendered = render_parity_lab_neow_v1(&input).unwrap();
        assert!(rendered.text.contains("[neow stage=leave]"));
        assert!(rendered.text.contains("A0 leave"));
        assert_eq!(rendered.actions.len(), 1);
    }

    #[test]
    fn room_stage_and_action_shape_must_agree() {
        let mut input = input();
        input.actions.push(NeowActionV1 {
            action_id: "leave".to_owned(),
            kind: NeowActionKindV1::Leave,
            availability: ActionAvailabilityV1::Verified,
        });
        assert!(render_parity_lab_neow_v1(&input).is_err());
    }
}
