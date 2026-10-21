use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::content::cards::{get_card_definition, CardId};
use crate::content::relics::RelicId;
use crate::eval::run_control::RunControlSession;
use crate::state::core::EngineState;
use crate::state::rewards::{RewardCard, RewardItem, RewardScreenContext, RewardState};

pub const REWARD_BOUNDARY_PACKET_SCHEMA_NAME: &str = "RewardBoundaryPacketV2";
pub const REWARD_BOUNDARY_PACKET_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RewardBoundaryPacketV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub surface: String,
    pub screen_context: String,
    pub skippable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_card_reward_index: Option<usize>,
    pub boundary_class: String,
    pub candidates: Vec<RewardCandidateSnapshotV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RewardCandidateSnapshotV1 {
    pub candidate_id: String,
    pub command: String,
    pub display_label: String,
    pub reward_kind: String,
    pub role: String,
    pub consumes_reward: bool,
    pub information_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<RewardCardSnapshotV1>,
    pub effects: Vec<RewardEffectSnapshotV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RewardCardSnapshotV1 {
    pub card_id: String,
    pub display_label: String,
    pub card_type: String,
    pub rarity: String,
    pub cost: String,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RewardEffectSnapshotV1 {
    pub kind: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

pub fn reward_boundary_packet_from_session_v1(
    session: &RunControlSession,
) -> Option<RewardBoundaryPacketV1> {
    let (reward, surface, overlay_return_state) = active_reward_state_v1(&session.engine_state)?;
    let mut candidates = if let Some(cards) = reward.pending_card_choice.as_ref() {
        pending_card_choice_candidates_v1(session, reward, cards, surface, overlay_return_state)
    } else {
        reward_item_candidates_v1(session, reward, surface, overlay_return_state)
    };
    candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    Some(RewardBoundaryPacketV1 {
        schema_name: REWARD_BOUNDARY_PACKET_SCHEMA_NAME.to_string(),
        schema_version: REWARD_BOUNDARY_PACKET_SCHEMA_VERSION,
        surface: surface.to_string(),
        screen_context: reward_screen_context_v1(reward.screen_context).to_string(),
        skippable: reward.skippable,
        pending_card_reward_index: reward.pending_card_reward_index,
        boundary_class: reward_boundary_class_v1(reward, surface, &candidates),
        candidates,
    })
}

pub fn reward_boundary_detail_lines_v1(
    packet: &RewardBoundaryPacketV1,
    max_candidates: usize,
) -> Vec<String> {
    let mut lines = vec![format!(
        "reward surface={} context={} class={}",
        packet.surface, packet.screen_context, packet.boundary_class
    )];
    lines.extend(
        packet
            .candidates
            .iter()
            .take(max_candidates)
            .map(reward_candidate_detail_line_v1),
    );
    let remaining = packet.candidates.len().saturating_sub(max_candidates);
    if remaining > 0 {
        lines.push(format!("reward candidates omitted={remaining}"));
    }
    lines
}

fn active_reward_state_v1(
    engine: &EngineState,
) -> Option<(&RewardState, &'static str, Option<&EngineState>)> {
    match engine {
        EngineState::RewardScreen(reward) => Some((reward, "reward_screen", None)),
        EngineState::RewardOverlay {
            reward_state,
            return_state,
        } => Some((reward_state, "reward_overlay", Some(return_state.as_ref()))),
        _ => None,
    }
}

fn pending_card_choice_candidates_v1(
    session: &RunControlSession,
    reward: &RewardState,
    cards: &[RewardCard],
    surface: &str,
    overlay_return_state: Option<&EngineState>,
) -> Vec<RewardCandidateSnapshotV1> {
    let item_index = reward.pending_card_reward_index.unwrap_or(0);
    let mut candidates = cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            let card_snapshot = reward_card_snapshot_v1(card);
            let mut effects = Vec::new();
            effects.push(effect_v1(
                "add_card",
                [("card", card_snapshot.card_id.clone())],
            ));
            RewardCandidateSnapshotV1 {
                candidate_id: format!("{surface}:card_reward:{item_index}:pick:{index}"),
                command: format!("pick {index}"),
                display_label: card_snapshot.display_label.clone(),
                reward_kind: "card_pick".to_string(),
                role: "card_choice".to_string(),
                consumes_reward: true,
                information_tags: vec!["public_visible_card_choice".to_string()],
                cards: vec![card_snapshot],
                effects,
            }
        })
        .collect::<Vec<_>>();
    if session
        .run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::SingingBowl)
    {
        candidates.push(RewardCandidateSnapshotV1 {
            candidate_id: format!("{surface}:card_reward:{item_index}:bowl"),
            command: "bowl".to_string(),
            display_label: "Singing Bowl | gain 2 max HP".to_string(),
            reward_kind: "singing_bowl".to_string(),
            role: "max_hp_alternative".to_string(),
            consumes_reward: true,
            information_tags: vec!["relic_reward_alternative".to_string()],
            cards: Vec::new(),
            effects: vec![effect_v1("gain_max_hp", [("amount", "2".to_string())])],
        });
    }
    candidates.push(RewardCandidateSnapshotV1 {
        candidate_id: format!("{surface}:card_reward:{item_index}:back"),
        command: "back".to_string(),
        display_label: reward_back_label_v1(overlay_return_state, true),
        reward_kind: "navigation".to_string(),
        role: "return_without_consuming_card_reward".to_string(),
        consumes_reward: false,
        information_tags: vec!["card_reward_remains".to_string()],
        cards: Vec::new(),
        effects: vec![effect_v1("return_to_reward_screen", [])],
    });
    candidates
}

fn reward_item_candidates_v1(
    session: &RunControlSession,
    reward: &RewardState,
    surface: &str,
    overlay_return_state: Option<&EngineState>,
) -> Vec<RewardCandidateSnapshotV1> {
    let mut candidates = reward
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| reward_item_candidate_v1(surface, index, item))
        .collect::<Vec<_>>();
    if reward.has_card_reward_item()
        && session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SingingBowl)
    {
        candidates.push(RewardCandidateSnapshotV1 {
            candidate_id: format!("{surface}:bowl:first_card_reward"),
            command: "bowl".to_string(),
            display_label: "Singing Bowl | gain 2 max HP".to_string(),
            reward_kind: "singing_bowl".to_string(),
            role: "max_hp_alternative".to_string(),
            consumes_reward: true,
            information_tags: vec!["relic_reward_alternative".to_string()],
            cards: Vec::new(),
            effects: vec![effect_v1("gain_max_hp", [("amount", "2".to_string())])],
        });
    }
    if reward.skippable {
        candidates.push(reward_navigation_candidate_v1(
            surface,
            reward.items.is_empty(),
            overlay_return_state,
        ));
    }
    candidates
}

fn reward_item_candidate_v1(
    surface: &str,
    index: usize,
    item: &RewardItem,
) -> RewardCandidateSnapshotV1 {
    let (display_label, reward_kind, role, cards, effects, tags) = match item {
        RewardItem::Gold { amount } => (
            format!("{amount} gold"),
            "gold",
            "routine_claim",
            Vec::new(),
            vec![effect_v1("gain_gold", [("amount", amount.to_string())])],
            vec!["public_reward_item".to_string()],
        ),
        RewardItem::StolenGold { amount } => (
            format!("{amount} stolen gold"),
            "stolen_gold",
            "routine_claim",
            Vec::new(),
            vec![effect_v1(
                "gain_stolen_gold",
                [("amount", amount.to_string())],
            )],
            vec!["public_reward_item".to_string()],
        ),
        RewardItem::Card { cards } => (
            format!(
                "Card reward [{}]",
                cards
                    .iter()
                    .map(|card| reward_card_snapshot_v1(card).display_label)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "card_reward",
            "open_card_reward",
            cards.iter().map(reward_card_snapshot_v1).collect(),
            vec![effect_v1(
                "open_visible_card_reward_choices",
                [("count", cards.len().to_string())],
            )],
            vec![
                "public_reward_item".to_string(),
                "visible_card_reward_offer".to_string(),
            ],
        ),
        RewardItem::Relic { relic_id } => (
            format!("Relic {}", relic_id.stable_id()),
            "relic",
            "claim_reward",
            Vec::new(),
            vec![effect_v1(
                "obtain_relic",
                [("relic", relic_id.stable_id().to_owned())],
            )],
            vec!["public_reward_item".to_string()],
        ),
        RewardItem::Potion { potion_id } => (
            format!("Potion {}", potion_id.stable_id()),
            "potion",
            "claim_reward",
            Vec::new(),
            vec![effect_v1(
                "obtain_potion",
                [("potion", potion_id.stable_id().to_owned())],
            )],
            vec!["public_reward_item".to_string()],
        ),
        RewardItem::EmeraldKey => (
            "Emerald key".to_string(),
            "emerald_key",
            "claim_key",
            Vec::new(),
            vec![effect_v1("obtain_key", [("key", "emerald".to_string())])],
            vec!["public_reward_item".to_string()],
        ),
        RewardItem::SapphireKey => (
            "Sapphire key".to_string(),
            "sapphire_key",
            "claim_key",
            Vec::new(),
            vec![effect_v1("obtain_key", [("key", "sapphire".to_string())])],
            vec!["public_reward_item".to_string()],
        ),
    };
    RewardCandidateSnapshotV1 {
        candidate_id: format!("{surface}:item:{index}:{reward_kind}"),
        command: format!("claim {index}"),
        display_label,
        reward_kind: reward_kind.to_string(),
        role: role.to_string(),
        consumes_reward: true,
        information_tags: tags,
        cards,
        effects,
    }
}

fn reward_navigation_candidate_v1(
    surface: &str,
    empty_reward_screen: bool,
    overlay_return_state: Option<&EngineState>,
) -> RewardCandidateSnapshotV1 {
    let role = if overlay_return_state.is_some() {
        "return_to_previous_screen"
    } else if empty_reward_screen {
        "leave_reward_screen"
    } else {
        "map_preview_with_unclaimed_rewards"
    };
    let mut tags = vec!["navigation".to_string()];
    if !empty_reward_screen {
        tags.push("unclaimed_rewards_remain".to_string());
    }
    if matches!(overlay_return_state, Some(EngineState::Shop(_))) {
        tags.push("shop_overlay_rewards_remain_available".to_string());
    }
    RewardCandidateSnapshotV1 {
        candidate_id: format!("{surface}:navigation:{role}"),
        command: if overlay_return_state.is_some() {
            "back".to_string()
        } else {
            "skip".to_string()
        },
        display_label: reward_back_label_v1(overlay_return_state, empty_reward_screen),
        reward_kind: "navigation".to_string(),
        role: role.to_string(),
        consumes_reward: false,
        information_tags: tags,
        cards: Vec::new(),
        effects: vec![effect_v1(role, [])],
    }
}

fn reward_card_snapshot_v1(card: &RewardCard) -> RewardCardSnapshotV1 {
    let def = get_card_definition(card.id);
    RewardCardSnapshotV1 {
        card_id: card_id_v1(card.id),
        display_label: reward_card_label_v1(card),
        card_type: def.card_type.vocabulary_token_v1().to_string(),
        rarity: def.rarity.vocabulary_token_v1().to_string(),
        cost: card_cost_label_v1(def.cost),
        upgrades: card.upgrades,
    }
}

fn reward_card_label_v1(card: &RewardCard) -> String {
    let name = get_card_definition(card.id).name;
    if card.upgrades == 0 {
        name.to_string()
    } else {
        format!("{name}+{}", card.upgrades)
    }
}

fn card_id_v1(id: CardId) -> String {
    id.stable_id().to_string()
}

fn card_cost_label_v1(cost: i8) -> String {
    match cost {
        -2 => "unplayable".to_string(),
        -1 => "X".to_string(),
        value => value.to_string(),
    }
}

fn reward_back_label_v1(
    overlay_return_state: Option<&EngineState>,
    empty_reward_screen: bool,
) -> String {
    match overlay_return_state {
        Some(EngineState::Shop(_)) => "Return to shop".to_string(),
        Some(EngineState::EventRoom) => "Return to event".to_string(),
        Some(EngineState::Campfire) => "Return to campfire".to_string(),
        Some(_) => "Return to previous screen".to_string(),
        None if empty_reward_screen => "Leave reward screen".to_string(),
        None => "Open map preview".to_string(),
    }
}

fn reward_screen_context_v1(context: RewardScreenContext) -> &'static str {
    match context {
        RewardScreenContext::Standard => "standard",
        RewardScreenContext::TreasureRoom => "treasure_room",
        RewardScreenContext::MuggedCombat => "mugged_combat",
        RewardScreenContext::SmokedCombat => "smoked_combat",
        RewardScreenContext::NeowCardChoice => "neow_card_choice",
    }
}

fn reward_boundary_class_v1(
    reward: &RewardState,
    surface: &str,
    candidates: &[RewardCandidateSnapshotV1],
) -> String {
    if reward.pending_card_choice.is_some() {
        return "pending_card_choice".to_string();
    }
    if candidates.is_empty() {
        return "empty_reward_screen".to_string();
    }
    if surface == "reward_overlay" {
        return "reward_overlay".to_string();
    }
    if reward.has_card_reward_item() {
        return "contains_card_reward".to_string();
    }
    "reward_screen".to_string()
}

fn reward_candidate_detail_line_v1(candidate: &RewardCandidateSnapshotV1) -> String {
    let effects = reward_candidate_effect_summary_v1(candidate);
    let cards = if candidate.cards.is_empty() {
        String::new()
    } else {
        format!(
            " cards=[{}]",
            candidate
                .cards
                .iter()
                .map(|card| card.display_label.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    format!(
        "{} kind={} role={} consumes={} effects=[{}]{}",
        candidate.command,
        candidate.reward_kind,
        candidate.role,
        candidate.consumes_reward,
        effects,
        cards
    )
}

fn reward_candidate_effect_summary_v1(candidate: &RewardCandidateSnapshotV1) -> String {
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

fn effect_v1<const N: usize>(
    kind: impl Into<String>,
    params: [(&str, String); N],
) -> RewardEffectSnapshotV1 {
    let params = params
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect();
    RewardEffectSnapshotV1 {
        kind: kind.into(),
        params,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};

    #[test]
    fn unopened_card_reward_item_exposes_visible_offer_without_selecting() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: vec![
                RewardCard::new(CardId::PommelStrike, 0),
                RewardCard::new(CardId::ShrugItOff, 1),
                RewardCard::new(CardId::DemonForm, 0),
            ],
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let packet = reward_boundary_packet_from_session_v1(&session).unwrap();

        assert_eq!(packet.boundary_class, "contains_card_reward");
        let candidate = packet
            .candidates
            .iter()
            .find(|candidate| candidate.reward_kind == "card_reward")
            .unwrap();
        assert_eq!(candidate.command, "claim 0");
        assert_eq!(candidate.role, "open_card_reward");
        assert_eq!(candidate.cards.len(), 3);
        assert_eq!(candidate.cards[1].display_label, "Shrug It Off+1");
        assert!(candidate
            .information_tags
            .iter()
            .any(|tag| tag == "visible_card_reward_offer"));
    }

    #[test]
    fn pending_card_choice_exposes_pick_bowl_and_back_candidates() {
        let mut session = RunControlSession::new(Default::default());
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SingingBowl));
        let mut reward = RewardState::new();
        reward.pending_card_choice = Some(vec![RewardCard::new(CardId::Clothesline, 0)]);
        reward.pending_card_reward_index = Some(2);
        session.engine_state = EngineState::RewardScreen(reward);

        let packet = reward_boundary_packet_from_session_v1(&session).unwrap();

        assert_eq!(packet.boundary_class, "pending_card_choice");
        assert!(packet
            .candidates
            .iter()
            .any(|candidate| candidate.command == "pick 0"
                && candidate.role == "card_choice"
                && candidate.consumes_reward));
        assert!(
            packet
                .candidates
                .iter()
                .any(|candidate| candidate.command == "bowl"
                    && candidate.role == "max_hp_alternative")
        );
        assert!(packet
            .candidates
            .iter()
            .any(|candidate| candidate.command == "back"
                && candidate.role == "return_without_consuming_card_reward"
                && !candidate.consumes_reward));
    }

    #[test]
    fn reward_overlay_back_candidate_preserves_shop_return_semantics() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Gold { amount: 25 }];
        session.engine_state = EngineState::reward_overlay(
            reward,
            EngineState::Shop(crate::state::shop::ShopState::new()),
        );

        let packet = reward_boundary_packet_from_session_v1(&session).unwrap();

        assert_eq!(packet.surface, "reward_overlay");
        assert_eq!(packet.boundary_class, "reward_overlay");
        let back = packet
            .candidates
            .iter()
            .find(|candidate| candidate.command == "back")
            .unwrap();
        assert_eq!(back.display_label, "Return to shop");
        assert!(back
            .information_tags
            .iter()
            .any(|tag| tag == "shop_overlay_rewards_remain_available"));
    }

    #[test]
    fn detail_lines_are_structured_and_compact() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Gold { amount: 33 }];
        session.engine_state = EngineState::RewardScreen(reward);

        let packet = reward_boundary_packet_from_session_v1(&session).unwrap();
        let lines = reward_boundary_detail_lines_v1(&packet, 1);

        assert_eq!(
            lines[0],
            "reward surface=reward_screen context=standard class=reward_screen"
        );
        assert!(lines[1].contains("claim 0 kind=gold role=routine_claim"));
    }
}
