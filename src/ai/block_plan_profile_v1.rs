use crate::ai::card_analysis_v1::card_analysis_profile_v1;
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatCard;
use crate::state::run::RunState;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockPlanReadinessV1 {
    #[default]
    Absent,
    PlainCoverage,
    Latent,
    Supported,
    Ready,
}

pub const SECOND_WIND_BLOCK_ENGINE_MIN_NON_ATTACK_CARDS: u8 = 4;

pub const fn has_second_wind_block_engine_fuel_v1(non_attack_cards: u8) -> bool {
    non_attack_cards >= SECOND_WIND_BLOCK_ENGINE_MIN_NON_ATTACK_CARDS
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockPlanProfileV1 {
    pub plain_block_cards: u8,
    pub medium_block_chunks: u8,
    pub high_quality_block_chunks: u8,
    pub retention_sources: u8,
    pub multipliers: u8,
    pub payoffs: u8,
    pub feel_no_pain_sources: u8,
    pub controlled_exhaust_sources: u8,
    pub broad_exhaust_sources: u8,
    pub second_wind_sources: u8,
    pub non_attack_cards: u8,
    pub setup_debt: u8,
    pub access_support: u8,
    pub stasis_sensitive_key_cards: u8,
    pub readiness: BlockPlanReadinessV1,
    pub diagnosis: Vec<String>,
}

impl BlockPlanProfileV1 {
    pub fn has_fnp_exhaust_engine(&self) -> bool {
        self.feel_no_pain_sources > 0 && self.controlled_exhaust_sources > 0
    }

    pub fn has_second_wind_block_engine(&self) -> bool {
        self.second_wind_sources > 0 && has_second_wind_block_engine_fuel_v1(self.non_attack_cards)
    }

    pub fn has_supported_retention_plan(&self) -> bool {
        self.retention_sources > 0
            && (self.multipliers > 0
                || self.high_quality_block_chunks > 0
                || self.has_fnp_exhaust_engine()
                || self.has_second_wind_block_engine())
    }

    pub fn has_execute_block_plan(&self) -> bool {
        self.high_quality_block_chunks > 0
            || self.has_fnp_exhaust_engine()
            || self.has_second_wind_block_engine()
            || self.has_supported_retention_plan()
    }

    pub fn has_dark_echo_block_plan(&self) -> bool {
        self.has_execute_block_plan()
    }

    pub fn has_hyperbeam_block_plan(&self) -> bool {
        self.high_quality_block_chunks > 0 || self.has_supported_retention_plan()
    }

    pub fn engine_support_score(&self) -> usize {
        match self.readiness {
            BlockPlanReadinessV1::Absent | BlockPlanReadinessV1::PlainCoverage => 0,
            BlockPlanReadinessV1::Latent => 1,
            BlockPlanReadinessV1::Supported => 2,
            BlockPlanReadinessV1::Ready => 3,
        }
    }
}

pub fn block_plan_profile_v1(run_state: &RunState) -> BlockPlanProfileV1 {
    let mut profile = block_plan_profile_from_deck_v1(&run_state.master_deck);

    for relic in &run_state.relics {
        if matches!(relic.id, RelicId::Calipers) {
            profile.retention_sources = profile.retention_sources.saturating_add(1);
        }
    }

    finalize_block_plan_profile_v1(&mut profile);
    profile
}

pub fn block_plan_profile_from_deck_v1(deck: &[CombatCard]) -> BlockPlanProfileV1 {
    let mut profile = BlockPlanProfileV1::default();
    for card in deck {
        let analysis = card_analysis_profile_v1(card.id, card.upgrades);
        if analysis.is_block_plan_plain_coverage {
            profile.plain_block_cards = profile.plain_block_cards.saturating_add(1);
        }
        if analysis.is_block_plan_medium_chunk {
            profile.medium_block_chunks = profile.medium_block_chunks.saturating_add(1);
        }
        if analysis.is_block_plan_high_quality_chunk {
            profile.high_quality_block_chunks = profile.high_quality_block_chunks.saturating_add(1);
        }
        if analysis.is_block_retention_source {
            profile.retention_sources = profile.retention_sources.saturating_add(1);
            profile.setup_debt = profile.setup_debt.saturating_add(1);
        }
        if analysis.is_block_multiplier {
            profile.multipliers = profile.multipliers.saturating_add(1);
        }
        if analysis.is_block_payoff {
            profile.payoffs = profile.payoffs.saturating_add(1);
        }
        if analysis.is_feel_no_pain_source {
            profile.feel_no_pain_sources = profile.feel_no_pain_sources.saturating_add(1);
            profile.setup_debt = profile.setup_debt.saturating_add(1);
        }
        if analysis.is_second_wind_source {
            profile.second_wind_sources = profile.second_wind_sources.saturating_add(1);
        }
        if analysis.is_block_plan_controlled_exhaust_source {
            profile.controlled_exhaust_sources =
                profile.controlled_exhaust_sources.saturating_add(1);
        }
        if analysis.is_block_plan_broad_exhaust_source {
            profile.broad_exhaust_sources = profile.broad_exhaust_sources.saturating_add(1);
        }
        if analysis.is_non_attack {
            profile.non_attack_cards = profile.non_attack_cards.saturating_add(1);
        }
        if analysis.is_block_plan_access_support {
            profile.access_support = profile.access_support.saturating_add(1);
        }
        if analysis.is_stasis_sensitive_key_card {
            profile.stasis_sensitive_key_cards =
                profile.stasis_sensitive_key_cards.saturating_add(1);
        }
    }

    finalize_block_plan_profile_v1(&mut profile);
    profile
}

fn finalize_block_plan_profile_v1(profile: &mut BlockPlanProfileV1) {
    profile.readiness = block_plan_readiness_v1(profile);
    profile.diagnosis = block_plan_diagnosis_v1(profile);
}

fn block_plan_readiness_v1(profile: &BlockPlanProfileV1) -> BlockPlanReadinessV1 {
    if profile.has_supported_retention_plan()
        && (profile.multipliers > 0
            || profile.has_fnp_exhaust_engine()
            || profile.has_second_wind_block_engine())
    {
        return BlockPlanReadinessV1::Ready;
    }
    if profile.has_supported_retention_plan()
        || profile.has_fnp_exhaust_engine()
        || profile.high_quality_block_chunks >= 2
    {
        return BlockPlanReadinessV1::Supported;
    }
    if profile.retention_sources > 0
        || profile.multipliers > 0
        || profile.payoffs > 0
        || profile.feel_no_pain_sources > 0
    {
        return BlockPlanReadinessV1::Latent;
    }
    if profile.plain_block_cards >= 3
        || profile.medium_block_chunks > 0
        || profile.high_quality_block_chunks > 0
    {
        return BlockPlanReadinessV1::PlainCoverage;
    }
    BlockPlanReadinessV1::Absent
}

fn block_plan_diagnosis_v1(profile: &BlockPlanProfileV1) -> Vec<String> {
    let mut diagnosis = Vec::new();
    if profile.retention_sources > 0 && !profile.has_supported_retention_plan() {
        diagnosis.push("retention_without_block_engine_support".to_string());
    }
    if profile.multipliers > 0
        && profile.retention_sources == 0
        && profile.high_quality_block_chunks == 0
    {
        diagnosis.push("multiplier_without_retained_or_large_block".to_string());
    }
    if profile.payoffs > 0 && profile.readiness < BlockPlanReadinessV1::Supported {
        diagnosis.push("block_payoff_without_supported_block_plan".to_string());
    }
    if profile.feel_no_pain_sources > 0 && profile.controlled_exhaust_sources == 0 {
        diagnosis.push("fnp_without_controlled_exhaust_access".to_string());
    }
    if profile.medium_block_chunks > 0 && profile.high_quality_block_chunks == 0 {
        diagnosis.push("medium_block_chunk_not_terminal_plan".to_string());
    }
    if profile.stasis_sensitive_key_cards > 0 {
        diagnosis.push("stasis_can_target_key_block_or_engine_cards".to_string());
    }
    diagnosis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;

    #[test]
    fn barricade_alone_is_latent_not_supported_block_plan() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Barricade);

        let profile = block_plan_profile_v1(&run_state);

        assert_eq!(profile.readiness, BlockPlanReadinessV1::Latent);
        assert!(!profile.has_execute_block_plan());
        assert!(profile
            .diagnosis
            .contains(&"retention_without_block_engine_support".to_string()));
    }

    #[test]
    fn barricade_with_multiplier_and_big_block_is_ready() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::Barricade);
        run_state.add_card_to_deck(CardId::Entrench);
        run_state.add_card_to_deck(CardId::Impervious);

        let profile = block_plan_profile_v1(&run_state);

        assert_eq!(profile.readiness, BlockPlanReadinessV1::Ready);
        assert!(profile.has_execute_block_plan());
        assert!(profile.has_hyperbeam_block_plan());
    }

    #[test]
    fn flame_barrier_is_medium_block_not_terminal_plan() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.add_card_to_deck(CardId::FlameBarrier);

        let profile = block_plan_profile_v1(&run_state);

        assert_eq!(profile.readiness, BlockPlanReadinessV1::PlainCoverage);
        assert_eq!(profile.medium_block_chunks, 1);
        assert_eq!(profile.high_quality_block_chunks, 0);
        assert!(!profile.has_hyperbeam_block_plan());
    }
}
