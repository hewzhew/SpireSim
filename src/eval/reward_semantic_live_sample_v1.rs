use std::collections::HashMap;

use crate::ai::strategy::package_state::PackageStateReport;
use crate::ai::strategy::reward_semantic_probe::{
    assess_reward_semantics_from_cards, explain_reward_semantics_v1,
    RewardCandidateSemanticExplanationV1, RewardSemanticCoverageStatusV1,
};
use crate::content::cards::{get_card_definition, CardId};
use crate::content::identity::gameplay_display_label_v1;
use crate::eval::run_control::RunControlSession;
use crate::state::core::EngineState;
use crate::state::rewards::{RewardCard, RewardState};

#[derive(Clone, Debug, PartialEq)]
pub struct RewardSemanticLiveSampleV1 {
    pub branch_id: String,
    pub branch_choices: Vec<String>,
    pub act: u8,
    pub floor: i32,
    pub boss: Option<String>,
    pub deck_size: usize,
    pub deck_package: PackageStateReport,
    pub candidates: Vec<RewardSemanticLiveCandidateV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RewardSemanticLiveCandidateV1 {
    pub label: String,
    pub card: CardId,
    pub explanation: RewardCandidateSemanticExplanationV1,
}

pub fn reward_semantic_live_sample_from_session_v1(
    session: &RunControlSession,
    branch_id: String,
    branch_choices: Vec<String>,
) -> Option<RewardSemanticLiveSampleV1> {
    let cards = active_or_visible_reward_cards_for_live_sample_v1(session)?;
    let deck_cards = session
        .run_state
        .master_deck
        .iter()
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let reward_cards = cards.iter().map(|card| card.id).collect::<Vec<_>>();
    let semantic_probe = assess_reward_semantics_from_cards(&deck_cards, &reward_cards);
    let semantic_explanation = explain_reward_semantics_v1(&semantic_probe);
    let candidates = cards
        .iter()
        .zip(semantic_explanation.candidates)
        .map(|(card, explanation)| RewardSemanticLiveCandidateV1 {
            label: reward_card_label_v1(card),
            card: card.id,
            explanation,
        })
        .collect::<Vec<_>>();
    Some(RewardSemanticLiveSampleV1 {
        branch_id,
        branch_choices,
        act: session.run_state.act_num,
        floor: session.run_state.floor_num,
        boss: session
            .run_state
            .boss_key
            .map(|boss| boss.stable_id().to_string()),
        deck_size: session.run_state.master_deck.len(),
        deck_package: semantic_explanation.deck_package,
        candidates,
    })
}

pub fn render_reward_semantic_live_sample_v1(
    sample: &RewardSemanticLiveSampleV1,
    sample_number: usize,
    sample_limit: Option<usize>,
) -> String {
    let mut lines = Vec::new();
    let total = sample_limit
        .map(|limit| limit.to_string())
        .unwrap_or_else(|| "?".to_string());
    lines.push(format!(
        "reward_semantic_sample {sample_number}/{total} A{}F{} boss={} branch={} deck_size={}",
        sample.act,
        sample.floor,
        sample.boss.as_deref().unwrap_or("-"),
        sample.branch_id,
        sample.deck_size
    ));
    lines.push(format!(
        "choices: {}",
        if sample.branch_choices.is_empty() {
            "-".to_string()
        } else {
            sample.branch_choices.join(" -> ")
        }
    ));
    lines.push(format!(
        "deck_package strength={:?} exhaust={:?} self_damage={:?} block={:?}",
        sample.deck_package.strength,
        sample.deck_package.exhaust,
        sample.deck_package.self_damage,
        sample.deck_package.block
    ));
    for candidate in &sample.candidates {
        let explanation = &candidate.explanation;
        lines.push(String::new());
        lines.push(format!(
            "candidate={} card={}",
            candidate.label,
            gameplay_display_label_v1(candidate.card)
        ));
        lines.push(format!(
            "  coverage={:?} fields=[{}]",
            explanation.coverage.status,
            render_static_list_v1(&explanation.coverage.explained_fields)
        ));
        lines.push(format!(
            "  package_changes=[{}] closes=[{}] opens=[{}]",
            render_list_v1(&explanation.package_changes),
            render_list_v1(&explanation.closes),
            render_list_v1(&explanation.opens)
        ));
        lines.push(format!(
            "  facts provides=[{}] damage=[{}] damage_uses=[{}] emits=[{}] rules=[{}] handlers=[{}]",
            render_list_v1(&explanation.provides),
            render_list_v1(&explanation.damage),
            render_list_v1(&explanation.damage_uses),
            render_list_v1(&explanation.emits),
            render_list_v1(&explanation.rules),
            render_list_v1(&explanation.handlers)
        ));
        lines.push(format!(
            "  burdens=[{}] duplicates=[{}]",
            render_list_v1(&explanation.burdens),
            render_list_v1(&explanation.duplicates)
        ));
    }
    lines.join("\n")
}

pub fn render_reward_semantic_live_sample_summary_v1(
    samples: &[RewardSemanticLiveSampleV1],
) -> String {
    let summary = summarize_reward_semantic_live_samples_v1(samples);
    let mut lines = Vec::new();
    lines.push("reward_semantic_live_sample_summary:".to_string());
    lines.push(format!("  reward_surfaces={}", summary.reward_surfaces));
    lines.push(format!("  candidates={}", summary.candidates));
    lines.push("  coverage:".to_string());
    lines.push(format!("    Explained={}", summary.explained));
    lines.push(format!("    Empty={}", summary.empty));
    lines.push(format!(
        "    DeferredSequenceTactical={}",
        summary.deferred_sequence_tactical
    ));
    lines.push("  empty_candidates:".to_string());
    append_candidate_counts_v1(&mut lines, &summary.empty_candidates);
    lines.push("  deferred_candidates:".to_string());
    append_candidate_counts_v1(&mut lines, &summary.deferred_candidates);
    lines.join("\n")
}

#[derive(Debug, Default)]
struct RewardSemanticLiveSampleSummaryV1 {
    reward_surfaces: usize,
    candidates: usize,
    explained: usize,
    empty: usize,
    deferred_sequence_tactical: usize,
    empty_candidates: Vec<(CardId, usize)>,
    deferred_candidates: Vec<(CardId, usize)>,
}

fn summarize_reward_semantic_live_samples_v1(
    samples: &[RewardSemanticLiveSampleV1],
) -> RewardSemanticLiveSampleSummaryV1 {
    let mut summary = RewardSemanticLiveSampleSummaryV1 {
        reward_surfaces: samples.len(),
        ..Default::default()
    };
    let mut empty_candidates = HashMap::<CardId, usize>::new();
    let mut deferred_candidates = HashMap::<CardId, usize>::new();
    for sample in samples {
        for candidate in &sample.candidates {
            summary.candidates += 1;
            match candidate.explanation.coverage.status {
                RewardSemanticCoverageStatusV1::Explained => summary.explained += 1,
                RewardSemanticCoverageStatusV1::Empty => {
                    summary.empty += 1;
                    *empty_candidates.entry(candidate.card).or_default() += 1;
                }
                RewardSemanticCoverageStatusV1::DeferredSequenceTactical => {
                    summary.deferred_sequence_tactical += 1;
                    *deferred_candidates.entry(candidate.card).or_default() += 1;
                }
            }
        }
    }
    summary.empty_candidates = sorted_candidate_counts_v1(empty_candidates);
    summary.deferred_candidates = sorted_candidate_counts_v1(deferred_candidates);
    summary
}

fn active_or_visible_reward_cards_for_live_sample_v1(
    session: &RunControlSession,
) -> Option<Vec<RewardCard>> {
    match &session.engine_state {
        EngineState::RewardScreen(reward) => reward
            .pending_card_choice
            .clone()
            .or_else(|| first_visible_card_reward_for_live_sample_v1(reward)),
        EngineState::RewardOverlay { reward_state, .. } => reward_state
            .pending_card_choice
            .clone()
            .or_else(|| first_visible_card_reward_for_live_sample_v1(reward_state)),
        _ => None,
    }
}

fn first_visible_card_reward_for_live_sample_v1(reward: &RewardState) -> Option<Vec<RewardCard>> {
    reward.items.iter().find_map(|item| match item {
        crate::state::rewards::RewardItem::Card { cards } => Some(cards.clone()),
        _ => None,
    })
}

fn reward_card_label_v1(card: &RewardCard) -> String {
    let name = get_card_definition(card.id).name;
    if card.upgrades == 0 {
        name.to_string()
    } else {
        format!("{name}+{}", card.upgrades)
    }
}

fn render_list_v1(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(", ")
    }
}

fn render_static_list_v1(items: &[&'static str]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(", ")
    }
}

fn sorted_candidate_counts_v1(counts: HashMap<CardId, usize>) -> Vec<(CardId, usize)> {
    let mut counts = counts.into_iter().collect::<Vec<_>>();
    counts.sort_by(|(left_card, left_count), (right_card, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| card_name_v1(*left_card).cmp(card_name_v1(*right_card)))
    });
    counts
}

fn append_candidate_counts_v1(lines: &mut Vec<String>, counts: &[(CardId, usize)]) {
    if counts.is_empty() {
        lines.push("    -".to_string());
        return;
    }
    for (card, count) in counts {
        lines.push(format!("    {} x{count}", card_name_v1(*card)));
    }
}

fn card_name_v1(card: CardId) -> &'static str {
    get_card_definition(card).name
}
