use sts_oracle_runtime::ai::combat_search_v2::{CombatLineLabReport, SearchTerminalLabel};

use super::super::search_types::SearchReview;
use super::line_tags::line_quality_tags;
use super::types::BossLineReview;

pub(super) fn collect_line_reviews(
    ladder: &[SearchReview],
    line_lab: Option<&CombatLineLabReport>,
) -> Vec<BossLineReview> {
    let mut reviews: Vec<_> = ladder.iter().map(line_review_from_search).collect();
    if let Some(turn_pool) = line_lab.and_then(|report| report.turn_pool.as_ref()) {
        reviews.extend(turn_pool.lanes.iter().map(|line| BossLineReview {
            source: format!("turn_pool:{}", line.lane),
            terminal: line.terminal,
            final_hp: Some(line.final_hp),
            hp_loss: None,
            turns: Some(line.turns),
            potions_used: Some(line.potions_used),
            tags: line_quality_tags(
                line.terminal,
                Some(line.final_hp),
                Some(line.turns),
                Some(line.potions_used),
                line.living_enemy_count,
                line.total_enemy_hp,
            ),
        }));
    }
    reviews
}

fn line_review_from_search(review: &SearchReview) -> BossLineReview {
    let progress = review.facts.diagnostic_progress.as_ref();
    BossLineReview {
        source: review.label.to_string(),
        terminal: progress
            .map(|facts| facts.terminal)
            .unwrap_or(SearchTerminalLabel::Unresolved),
        final_hp: review
            .final_hp
            .or_else(|| progress.map(|facts| facts.final_hp)),
        hp_loss: review
            .hp_loss
            .or_else(|| progress.map(|facts| facts.hp_loss)),
        turns: review.turns.or_else(|| progress.map(|facts| facts.turns)),
        potions_used: review
            .potions_used
            .or_else(|| progress.map(|facts| facts.potions_used)),
        tags: line_quality_tags(
            progress
                .map(|facts| facts.terminal)
                .unwrap_or(SearchTerminalLabel::Unresolved),
            review
                .final_hp
                .or_else(|| progress.map(|facts| facts.final_hp)),
            review.turns.or_else(|| progress.map(|facts| facts.turns)),
            review
                .potions_used
                .or_else(|| progress.map(|facts| facts.potions_used)),
            progress
                .map(|facts| facts.living_enemy_count)
                .unwrap_or_default(),
            progress
                .map(|facts| facts.total_enemy_hp)
                .unwrap_or_default(),
        ),
    }
}
