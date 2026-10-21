use crate::ai::block_plan_profile_v1::{block_plan_profile_v1, BlockPlanProfileV1};
use crate::ai::card_analysis_v1::{card_analysis_profile_v1, CardAnalysisBossTransitionBurstV1};
use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::state::run::RunState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BossMechanicPressureProfileV1 {
    pub boss: EncounterId,
    pub target_topology: BossEncounterTargetTopologyV1,
    pub pressure_points: Vec<BossMechanicPressurePointV1>,
    pub red_flags: Vec<BossMechanicRedFlagV1>,
    pub missing_answers: Vec<BossMechanicMissingAnswerV1>,
    pub biases: Vec<BossMechanicBiasV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BossEncounterTargetTopologyV1 {
    SingleOpponent,
    MultipleOpponentsOrSummons,
    SplitIntoMultipleOpponents,
    Unknown,
}

impl BossMechanicPressureProfileV1 {
    pub fn has_pressure(&self, pressure: BossMechanicPressurePointV1) -> bool {
        self.pressure_points.contains(&pressure)
    }

    pub fn has_red_flag(&self, flag: BossMechanicRedFlagV1) -> bool {
        self.red_flags.contains(&flag)
    }

    pub fn has_missing_answer(&self, answer: BossMechanicMissingAnswerV1) -> bool {
        self.missing_answers.contains(&answer)
    }

    pub fn has_bias(&self, bias: BossMechanicBiasV1) -> bool {
        self.biases.contains(&bias)
    }

    pub fn summary_labels(&self) -> Vec<String> {
        let mut labels = Vec::new();
        labels.extend(
            self.pressure_points
                .iter()
                .map(|value| format!("pressure:{}", value.label())),
        );
        labels.extend(
            self.red_flags
                .iter()
                .map(|value| format!("red:{}", value.label())),
        );
        labels.extend(
            self.missing_answers
                .iter()
                .map(|value| format!("missing:{}", value.label())),
        );
        labels.extend(
            self.biases
                .iter()
                .map(|value| format!("bias:{}", value.label())),
        );
        labels
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BossMechanicPressurePointV1 {
    OpeningBurstCheck,
    SplitControlWindow,
    PostSplitControl,
    PowerPlayPenalty,
    DarkEchoBlockCheck,
    EnemyStrengthMultiHitExposure,
    CultistGrowthPressure,
    TimeWarpCounterControl,
    HasteTransitionWindow,
    ArtifactBarrier,
    StrengthClock,
    DazedPollution,
    ChampTransitionWindow,
    ExecuteBlockCheck,
    CollectorMinionPressure,
    CollectorTurnFourDebuff,
    HyperbeamTurnSixCheck,
    StasisKeyCardAccess,
}

impl BossMechanicPressurePointV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::OpeningBurstCheck => "opening_burst_check",
            Self::SplitControlWindow => "split_control_window",
            Self::PostSplitControl => "post_split_control",
            Self::PowerPlayPenalty => "power_play_penalty",
            Self::DarkEchoBlockCheck => "dark_echo_block_check",
            Self::EnemyStrengthMultiHitExposure => "enemy_strength_multi_hit_exposure",
            Self::CultistGrowthPressure => "cultist_growth_pressure",
            Self::TimeWarpCounterControl => "time_warp_counter_control",
            Self::HasteTransitionWindow => "haste_transition_window",
            Self::ArtifactBarrier => "artifact_barrier",
            Self::StrengthClock => "strength_clock",
            Self::DazedPollution => "dazed_pollution",
            Self::ChampTransitionWindow => "champ_transition_window",
            Self::ExecuteBlockCheck => "execute_block_check",
            Self::CollectorMinionPressure => "collector_minion_pressure",
            Self::CollectorTurnFourDebuff => "collector_turn4_debuff",
            Self::HyperbeamTurnSixCheck => "hyperbeam_turn6_check",
            Self::StasisKeyCardAccess => "stasis_key_card_access",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BossMechanicRedFlagV1 {
    EnemyStrengthMultiHitRisk,
    NoDarkEchoBlockPlan,
    LowValueCardSpamRisk,
    NoHalfHpBurstPlan,
    SplitDamageWithoutAoe,
    DebuffBlockedByArtifact,
    PrematureChampTransitionRisk,
    NoExecuteBlockPlan,
    NoAoeForCollectorMinions,
    NoHyperbeamBlockPlan,
    StolenKeyCardRisk,
}

impl BossMechanicRedFlagV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::EnemyStrengthMultiHitRisk => "enemy_strength_multi_hit_risk",
            Self::NoDarkEchoBlockPlan => "no_dark_echo_block_plan",
            Self::LowValueCardSpamRisk => "low_value_card_spam_risk",
            Self::NoHalfHpBurstPlan => "no_half_hp_burst_plan",
            Self::SplitDamageWithoutAoe => "split_damage_without_aoe",
            Self::DebuffBlockedByArtifact => "debuff_blocked_by_artifact",
            Self::PrematureChampTransitionRisk => "premature_champ_transition_risk",
            Self::NoExecuteBlockPlan => "no_execute_block_plan",
            Self::NoAoeForCollectorMinions => "no_aoe_for_collector_minions",
            Self::NoHyperbeamBlockPlan => "no_hyperbeam_block_plan",
            Self::StolenKeyCardRisk => "stolen_key_card_risk",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BossMechanicMissingAnswerV1 {
    SplitTimingPlan,
    PostSplitDamagePlan,
    PhasePowerPlan,
    DarkEchoBlockPlan,
    TimeWarpCounterPlan,
    HasteBurstOrSetupPlan,
    ArtifactStripPlan,
    FocusedKillOrderPlan,
    ChampTransitionBurst,
    ExecuteBlockPlan,
    CollectorMinionPlan,
    TurnFourDebuffPlan,
    Block50OrKillBeforeBeam,
    StasisRecoveryPlan,
}

impl BossMechanicMissingAnswerV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::SplitTimingPlan => "split_timing_plan",
            Self::PostSplitDamagePlan => "post_split_damage_plan",
            Self::PhasePowerPlan => "phase_power_plan",
            Self::DarkEchoBlockPlan => "dark_echo_block_plan",
            Self::TimeWarpCounterPlan => "time_warp_counter_plan",
            Self::HasteBurstOrSetupPlan => "haste_burst_or_setup_plan",
            Self::ArtifactStripPlan => "artifact_strip_plan",
            Self::FocusedKillOrderPlan => "focused_kill_order_plan",
            Self::ChampTransitionBurst => "champ_transition_burst",
            Self::ExecuteBlockPlan => "execute_block_plan",
            Self::CollectorMinionPlan => "collector_minion_plan",
            Self::TurnFourDebuffPlan => "turn4_debuff_plan",
            Self::Block50OrKillBeforeBeam => "block50_or_kill_before_beam",
            Self::StasisRecoveryPlan => "stasis_recovery_plan",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BossMechanicBiasV1 {
    PreferSplitControl,
    PreferPostSplitAoe,
    DelayMinorPowers,
    PreferHighImpactPerCard,
    PreferArtifactStrip,
    PreferFocusedSingleTarget,
    PreferAoeMinionControl,
    PreferTransitionBurst,
    PreferBigBlock,
    DefensivePotionPriority,
    RecoverStolenKeyCards,
}

impl BossMechanicBiasV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::PreferSplitControl => "prefer_split_control",
            Self::PreferPostSplitAoe => "prefer_post_split_aoe",
            Self::DelayMinorPowers => "delay_minor_powers",
            Self::PreferHighImpactPerCard => "prefer_high_impact_per_card",
            Self::PreferArtifactStrip => "prefer_artifact_strip",
            Self::PreferFocusedSingleTarget => "prefer_focused_single_target",
            Self::PreferAoeMinionControl => "prefer_aoe_minion_control",
            Self::PreferTransitionBurst => "prefer_transition_burst",
            Self::PreferBigBlock => "prefer_big_block",
            Self::DefensivePotionPriority => "defensive_potion_priority",
            Self::RecoverStolenKeyCards => "recover_stolen_key_cards",
        }
    }
}

#[derive(Clone, Debug)]
struct BossMechanicDeckFactsV1 {
    block_plan: BlockPlanProfileV1,
    power_count: usize,
    minor_power_count: usize,
    aoe_count: usize,
    artifact_strip_count: usize,
    big_block_count: usize,
    key_card_count: usize,
    low_value_spam_count: usize,
    transition_burst_count: usize,
    execute_block_plan_count: usize,
    dark_echo_block_plan_count: usize,
    feel_no_pain_count: usize,
    exhaust_access_count: usize,
    second_wind_count: usize,
    non_attack_count: usize,
    enemy_strength_relic_pressure: bool,
}

pub fn boss_mechanic_pressure_profile_v1(
    run_state: &RunState,
    boss: EncounterId,
) -> BossMechanicPressureProfileV1 {
    let facts = BossMechanicDeckFactsV1::from_run_state(run_state);
    let mut profile = BossMechanicPressureProfileV1 {
        boss,
        target_topology: boss_target_topology_v1(boss),
        pressure_points: Vec::new(),
        red_flags: Vec::new(),
        missing_answers: Vec::new(),
        biases: Vec::new(),
    };

    match boss {
        EncounterId::SlimeBoss => slime_boss_pressure(&facts, &mut profile),
        EncounterId::AwakenedOne => awakened_one_pressure(&facts, &mut profile),
        EncounterId::TimeEater => time_eater_pressure(&facts, &mut profile),
        EncounterId::DonuAndDeca => donu_deca_pressure(&facts, &mut profile),
        EncounterId::TheChamp => champ_pressure(&facts, &mut profile),
        EncounterId::Collector => collector_pressure(&facts, &mut profile),
        EncounterId::Automaton => automaton_pressure(&facts, &mut profile),
        _ => {}
    }

    profile.dedup();
    profile
}

fn slime_boss_pressure(
    facts: &BossMechanicDeckFactsV1,
    profile: &mut BossMechanicPressureProfileV1,
) {
    profile.push_pressure(BossMechanicPressurePointV1::OpeningBurstCheck);
    profile.push_pressure(BossMechanicPressurePointV1::SplitControlWindow);
    profile.push_pressure(BossMechanicPressurePointV1::PostSplitControl);
    profile.push_bias(BossMechanicBiasV1::PreferSplitControl);

    if facts.transition_burst_count == 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::SplitTimingPlan);
        profile.push_red_flag(BossMechanicRedFlagV1::NoHalfHpBurstPlan);
    }
    if facts.aoe_count == 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::PostSplitDamagePlan);
        profile.push_red_flag(BossMechanicRedFlagV1::SplitDamageWithoutAoe);
    } else {
        profile.push_bias(BossMechanicBiasV1::PreferPostSplitAoe);
    }
}

pub fn boss_target_topology_v1(boss: EncounterId) -> BossEncounterTargetTopologyV1 {
    match boss {
        EncounterId::TheGuardian
        | EncounterId::Hexaghost
        | EncounterId::TheChamp
        | EncounterId::TimeEater
        | EncounterId::TheHeart => BossEncounterTargetTopologyV1::SingleOpponent,
        EncounterId::Automaton
        | EncounterId::Collector
        | EncounterId::AwakenedOne
        | EncounterId::DonuAndDeca
        | EncounterId::ShieldAndSpear => BossEncounterTargetTopologyV1::MultipleOpponentsOrSummons,
        EncounterId::SlimeBoss => BossEncounterTargetTopologyV1::SplitIntoMultipleOpponents,
        _ => BossEncounterTargetTopologyV1::Unknown,
    }
}

pub fn relic_creates_enemy_strength_pressure_v1(relic: RelicId) -> bool {
    matches!(relic, RelicId::Brimstone | RelicId::PhilosopherStone)
}

impl BossMechanicPressureProfileV1 {
    fn push_pressure(&mut self, value: BossMechanicPressurePointV1) {
        self.pressure_points.push(value);
    }

    fn push_red_flag(&mut self, value: BossMechanicRedFlagV1) {
        self.red_flags.push(value);
    }

    fn push_missing_answer(&mut self, value: BossMechanicMissingAnswerV1) {
        self.missing_answers.push(value);
    }

    fn push_bias(&mut self, value: BossMechanicBiasV1) {
        self.biases.push(value);
    }

    fn dedup(&mut self) {
        self.pressure_points.sort();
        self.pressure_points.dedup();
        self.red_flags.sort();
        self.red_flags.dedup();
        self.missing_answers.sort();
        self.missing_answers.dedup();
        self.biases.sort();
        self.biases.dedup();
    }
}

impl BossMechanicDeckFactsV1 {
    fn from_run_state(run_state: &RunState) -> Self {
        let strength_profile = crate::ai::strength_profile_v1::strength_profile_v1(run_state);
        let block_plan = block_plan_profile_v1(run_state);
        let mut facts = Self {
            big_block_count: block_plan.high_quality_block_chunks as usize,
            execute_block_plan_count: usize::from(block_plan.has_execute_block_plan()),
            dark_echo_block_plan_count: usize::from(block_plan.has_dark_echo_block_plan()),
            key_card_count: block_plan.stasis_sensitive_key_cards as usize,
            block_plan,
            power_count: 0,
            minor_power_count: 0,
            aoe_count: 0,
            artifact_strip_count: 0,
            low_value_spam_count: 0,
            transition_burst_count: 0,
            feel_no_pain_count: 0,
            exhaust_access_count: 0,
            second_wind_count: 0,
            non_attack_count: 0,
            enemy_strength_relic_pressure: run_state
                .relics
                .iter()
                .any(|relic| relic_creates_enemy_strength_pressure_v1(relic.id)),
        };

        for card in &run_state.master_deck {
            let analysis = card_analysis_profile_v1(card.id, card.upgrades);
            if matches!(analysis.card_type, crate::content::cards::CardType::Power) {
                facts.power_count = facts.power_count.saturating_add(1);
            }
            if analysis.is_non_attack {
                facts.non_attack_count = facts.non_attack_count.saturating_add(1);
            }
            if analysis.is_boss_minor_power {
                facts.minor_power_count = facts.minor_power_count.saturating_add(1);
            }
            if analysis.aoe_support > Default::default() {
                facts.aoe_count = facts.aoe_count.saturating_add(1);
            }
            if analysis.is_boss_artifact_strip {
                facts.artifact_strip_count = facts.artifact_strip_count.saturating_add(1);
            }
            if analysis.is_boss_low_value_spam {
                facts.low_value_spam_count = facts.low_value_spam_count.saturating_add(1);
            }
            if is_transition_burst(&analysis, &strength_profile) {
                facts.transition_burst_count = facts.transition_burst_count.saturating_add(1);
            }
            if analysis.is_feel_no_pain_source {
                facts.feel_no_pain_count = facts.feel_no_pain_count.saturating_add(1);
            }
            if analysis.is_second_wind_source {
                facts.second_wind_count = facts.second_wind_count.saturating_add(1);
            }
            if analysis.is_boss_exhaust_access {
                facts.exhaust_access_count = facts.exhaust_access_count.saturating_add(1);
            }
        }
        facts
    }
}

fn awakened_one_pressure(
    facts: &BossMechanicDeckFactsV1,
    profile: &mut BossMechanicPressureProfileV1,
) {
    profile.push_pressure(BossMechanicPressurePointV1::PowerPlayPenalty);
    profile.push_pressure(BossMechanicPressurePointV1::DarkEchoBlockCheck);
    profile.push_pressure(BossMechanicPressurePointV1::CultistGrowthPressure);

    if facts.power_count > 1 || facts.minor_power_count > 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::PhasePowerPlan);
    }
    if facts.minor_power_count > 0 {
        profile.push_bias(BossMechanicBiasV1::DelayMinorPowers);
    }
    if facts.dark_echo_block_plan_count == 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::DarkEchoBlockPlan);
        profile.push_red_flag(BossMechanicRedFlagV1::NoDarkEchoBlockPlan);
    }
    if facts.big_block_count > 0 {
        profile.push_bias(BossMechanicBiasV1::PreferBigBlock);
    }
    if facts.enemy_strength_relic_pressure {
        profile.push_pressure(BossMechanicPressurePointV1::EnemyStrengthMultiHitExposure);
        profile.push_red_flag(BossMechanicRedFlagV1::EnemyStrengthMultiHitRisk);
    }
}

fn time_eater_pressure(
    facts: &BossMechanicDeckFactsV1,
    profile: &mut BossMechanicPressureProfileV1,
) {
    profile.push_pressure(BossMechanicPressurePointV1::TimeWarpCounterControl);
    profile.push_pressure(BossMechanicPressurePointV1::HasteTransitionWindow);
    profile.push_missing_answer(BossMechanicMissingAnswerV1::TimeWarpCounterPlan);
    profile.push_missing_answer(BossMechanicMissingAnswerV1::HasteBurstOrSetupPlan);
    profile.push_bias(BossMechanicBiasV1::PreferHighImpactPerCard);

    if facts.low_value_spam_count >= 3 {
        profile.push_red_flag(BossMechanicRedFlagV1::LowValueCardSpamRisk);
    }
    if facts.transition_burst_count == 0 {
        profile.push_red_flag(BossMechanicRedFlagV1::NoHalfHpBurstPlan);
    }
}

fn donu_deca_pressure(
    facts: &BossMechanicDeckFactsV1,
    profile: &mut BossMechanicPressureProfileV1,
) {
    profile.push_pressure(BossMechanicPressurePointV1::StrengthClock);
    profile.push_pressure(BossMechanicPressurePointV1::DazedPollution);
    profile.push_pressure(BossMechanicPressurePointV1::ArtifactBarrier);
    profile.push_missing_answer(BossMechanicMissingAnswerV1::FocusedKillOrderPlan);
    profile.push_bias(BossMechanicBiasV1::PreferFocusedSingleTarget);

    if facts.artifact_strip_count == 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::ArtifactStripPlan);
        profile.push_red_flag(BossMechanicRedFlagV1::DebuffBlockedByArtifact);
    } else {
        profile.push_bias(BossMechanicBiasV1::PreferArtifactStrip);
    }
    if facts.aoe_count == 0 {
        profile.push_red_flag(BossMechanicRedFlagV1::SplitDamageWithoutAoe);
    }
}

fn champ_pressure(facts: &BossMechanicDeckFactsV1, profile: &mut BossMechanicPressureProfileV1) {
    profile.push_pressure(BossMechanicPressurePointV1::ChampTransitionWindow);
    profile.push_pressure(BossMechanicPressurePointV1::ExecuteBlockCheck);
    profile.push_bias(BossMechanicBiasV1::PreferTransitionBurst);

    if facts.transition_burst_count == 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::ChampTransitionBurst);
        profile.push_red_flag(BossMechanicRedFlagV1::PrematureChampTransitionRisk);
    }
    if facts.execute_block_plan_count == 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::ExecuteBlockPlan);
        profile.push_red_flag(BossMechanicRedFlagV1::NoExecuteBlockPlan);
    }
    if facts.big_block_count > 0 {
        profile.push_bias(BossMechanicBiasV1::PreferBigBlock);
    }
}

fn collector_pressure(
    facts: &BossMechanicDeckFactsV1,
    profile: &mut BossMechanicPressureProfileV1,
) {
    profile.push_pressure(BossMechanicPressurePointV1::CollectorMinionPressure);
    profile.push_pressure(BossMechanicPressurePointV1::CollectorTurnFourDebuff);
    profile.push_missing_answer(BossMechanicMissingAnswerV1::CollectorMinionPlan);
    profile.push_missing_answer(BossMechanicMissingAnswerV1::TurnFourDebuffPlan);
    profile.push_bias(BossMechanicBiasV1::PreferAoeMinionControl);

    if facts.aoe_count == 0 {
        profile.push_red_flag(BossMechanicRedFlagV1::NoAoeForCollectorMinions);
    }
    if facts.big_block_count == 0 {
        profile.push_bias(BossMechanicBiasV1::DefensivePotionPriority);
    }
}

fn automaton_pressure(
    facts: &BossMechanicDeckFactsV1,
    profile: &mut BossMechanicPressureProfileV1,
) {
    profile.push_pressure(BossMechanicPressurePointV1::HyperbeamTurnSixCheck);
    profile.push_pressure(BossMechanicPressurePointV1::StasisKeyCardAccess);
    profile.push_missing_answer(BossMechanicMissingAnswerV1::StasisRecoveryPlan);
    profile.push_bias(BossMechanicBiasV1::RecoverStolenKeyCards);

    if !facts.block_plan.has_hyperbeam_block_plan() {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::Block50OrKillBeforeBeam);
        profile.push_red_flag(BossMechanicRedFlagV1::NoHyperbeamBlockPlan);
        profile.push_bias(BossMechanicBiasV1::DefensivePotionPriority);
    } else {
        profile.push_bias(BossMechanicBiasV1::PreferBigBlock);
    }
    if facts.key_card_count > 0 {
        profile.push_red_flag(BossMechanicRedFlagV1::StolenKeyCardRisk);
    }
    if facts.artifact_strip_count == 0 {
        profile.push_missing_answer(BossMechanicMissingAnswerV1::ArtifactStripPlan);
    }
}

fn is_transition_burst(
    card: &crate::ai::card_analysis_v1::CardAnalysisProfileV1,
    strength: &crate::ai::strength_profile_v1::StrengthProfileV1,
) -> bool {
    match card.boss_transition_burst {
        CardAnalysisBossTransitionBurstV1::Unconditional => true,
        CardAnalysisBossTransitionBurstV1::StrengthPayoff => {
            strength.stable_sources > 0
                || strength.temporary_bursts > 0
                || strength.convertible_potential_count > 0
        }
        CardAnalysisBossTransitionBurstV1::StrengthConverter => {
            strength.stable_sources > 0 || strength.temporary_bursts > 0
        }
        CardAnalysisBossTransitionBurstV1::None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::state::run::RunState;

    fn replace_deck(run: &mut RunState, cards: &[CardId]) {
        run.master_deck = cards
            .iter()
            .enumerate()
            .map(|(index, &card)| CombatCard::for_scenario(card, 10_000 + index as u32))
            .collect();
    }

    #[test]
    fn awakened_one_profile_exposes_power_and_multi_hit_strength_pressure() {
        let mut run = RunState::new(744270980, 0, false, "Ironclad");
        run.act_num = 3;
        run.floor_num = 48;
        run.boss_key = Some(EncounterId::AwakenedOne);
        replace_deck(
            &mut run,
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::DarkEmbrace,
                CardId::FireBreathing,
                CardId::Inflame,
                CardId::FlameBarrier,
                CardId::Impervious,
            ],
        );
        run.relics.push(RelicState::new(RelicId::Brimstone));

        let profile = boss_mechanic_pressure_profile_v1(&run, EncounterId::AwakenedOne);

        assert_eq!(profile.boss, EncounterId::AwakenedOne);
        assert!(profile.has_pressure(BossMechanicPressurePointV1::PowerPlayPenalty));
        assert!(profile.has_pressure(BossMechanicPressurePointV1::DarkEchoBlockCheck));
        assert!(profile.has_red_flag(BossMechanicRedFlagV1::EnemyStrengthMultiHitRisk));
        assert!(profile.has_bias(BossMechanicBiasV1::DelayMinorPowers));
        assert!(profile.has_missing_answer(BossMechanicMissingAnswerV1::PhasePowerPlan));
    }

    #[test]
    fn awakened_one_profile_only_marks_dark_echo_missing_when_no_block_plan_exists() {
        let mut supported = RunState::new(744270980, 0, false, "Ironclad");
        supported.act_num = 3;
        supported.floor_num = 48;
        supported.boss_key = Some(EncounterId::AwakenedOne);
        replace_deck(
            &mut supported,
            &[
                CardId::FeelNoPain,
                CardId::SecondWind,
                CardId::TrueGrit,
                CardId::PowerThrough,
                CardId::Defend,
                CardId::Strike,
            ],
        );

        let supported_profile =
            boss_mechanic_pressure_profile_v1(&supported, EncounterId::AwakenedOne);

        assert!(supported_profile.has_pressure(BossMechanicPressurePointV1::DarkEchoBlockCheck));
        assert!(
            !supported_profile.has_missing_answer(BossMechanicMissingAnswerV1::DarkEchoBlockPlan)
        );
        assert!(!supported_profile.has_red_flag(BossMechanicRedFlagV1::NoDarkEchoBlockPlan));
        assert!(!supported_profile.has_missing_answer(BossMechanicMissingAnswerV1::PhasePowerPlan));

        let mut unsupported = RunState::new(744270980, 0, false, "Ironclad");
        unsupported.act_num = 3;
        unsupported.floor_num = 48;
        unsupported.boss_key = Some(EncounterId::AwakenedOne);
        replace_deck(
            &mut unsupported,
            &[CardId::Strike, CardId::Strike, CardId::Defend, CardId::Bash],
        );

        let unsupported_profile =
            boss_mechanic_pressure_profile_v1(&unsupported, EncounterId::AwakenedOne);

        assert!(
            unsupported_profile.has_missing_answer(BossMechanicMissingAnswerV1::DarkEchoBlockPlan)
        );
        assert!(unsupported_profile.has_red_flag(BossMechanicRedFlagV1::NoDarkEchoBlockPlan));
    }

    #[test]
    fn automaton_profile_exposes_hyperbeam_and_stasis_pressure() {
        let mut run = RunState::new(744270980, 0, false, "Ironclad");
        run.act_num = 2;
        run.floor_num = 32;
        run.boss_key = Some(EncounterId::Automaton);
        replace_deck(
            &mut run,
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::Bash,
                CardId::DemonForm,
                CardId::Offering,
                CardId::Shockwave,
            ],
        );

        let profile = boss_mechanic_pressure_profile_v1(&run, EncounterId::Automaton);

        assert_eq!(profile.boss, EncounterId::Automaton);
        assert!(profile.has_pressure(BossMechanicPressurePointV1::HyperbeamTurnSixCheck));
        assert!(profile.has_pressure(BossMechanicPressurePointV1::StasisKeyCardAccess));
        assert!(profile.has_red_flag(BossMechanicRedFlagV1::NoHyperbeamBlockPlan));
        assert!(profile.has_missing_answer(BossMechanicMissingAnswerV1::Block50OrKillBeforeBeam));
        assert!(profile.has_bias(BossMechanicBiasV1::DefensivePotionPriority));
    }

    #[test]
    fn champ_profile_only_marks_answers_missing_when_no_plan_exists() {
        let mut run = RunState::new(744270980, 0, false, "Ironclad");
        run.act_num = 2;
        run.floor_num = 31;
        run.boss_key = Some(EncounterId::TheChamp);
        replace_deck(
            &mut run,
            &[
                CardId::Carnage,
                CardId::SecondWind,
                CardId::Defend,
                CardId::Defend,
                CardId::GhostlyArmor,
                CardId::ShrugItOff,
            ],
        );

        let profile = boss_mechanic_pressure_profile_v1(&run, EncounterId::TheChamp);

        assert_eq!(profile.boss, EncounterId::TheChamp);
        assert_eq!(
            profile.target_topology,
            BossEncounterTargetTopologyV1::SingleOpponent
        );
        assert!(profile.has_pressure(BossMechanicPressurePointV1::ChampTransitionWindow));
        assert!(profile.has_pressure(BossMechanicPressurePointV1::ExecuteBlockCheck));
        assert!(!profile.has_missing_answer(BossMechanicMissingAnswerV1::ChampTransitionBurst));
        assert!(!profile.has_missing_answer(BossMechanicMissingAnswerV1::ExecuteBlockPlan));
        assert!(!profile.has_red_flag(BossMechanicRedFlagV1::PrematureChampTransitionRisk));
        assert!(!profile.has_red_flag(BossMechanicRedFlagV1::NoExecuteBlockPlan));
    }

    #[test]
    fn slime_boss_profile_exposes_split_timing_and_post_split_answers() {
        let mut run = RunState::new(744270980, 0, false, "Ironclad");
        run.act_num = 1;
        run.floor_num = 12;
        run.boss_key = Some(EncounterId::SlimeBoss);
        replace_deck(
            &mut run,
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::Bash,
                CardId::DarkEmbrace,
            ],
        );

        let profile = boss_mechanic_pressure_profile_v1(&run, EncounterId::SlimeBoss);

        assert!(profile.has_pressure(BossMechanicPressurePointV1::OpeningBurstCheck));
        assert!(profile.has_pressure(BossMechanicPressurePointV1::SplitControlWindow));
        assert!(profile.has_pressure(BossMechanicPressurePointV1::PostSplitControl));
        assert!(profile.has_missing_answer(BossMechanicMissingAnswerV1::SplitTimingPlan));
        assert!(profile.has_missing_answer(BossMechanicMissingAnswerV1::PostSplitDamagePlan));
        assert!(profile.has_red_flag(BossMechanicRedFlagV1::NoHalfHpBurstPlan));
        assert!(profile.has_red_flag(BossMechanicRedFlagV1::SplitDamageWithoutAoe));
    }

    #[test]
    fn boss_target_topology_distinguishes_single_random_target_and_split_fights() {
        assert_eq!(
            boss_target_topology_v1(EncounterId::TheChamp),
            BossEncounterTargetTopologyV1::SingleOpponent
        );
        assert_eq!(
            boss_target_topology_v1(EncounterId::DonuAndDeca),
            BossEncounterTargetTopologyV1::MultipleOpponentsOrSummons
        );
        assert_eq!(
            boss_target_topology_v1(EncounterId::SlimeBoss),
            BossEncounterTargetTopologyV1::SplitIntoMultipleOpponents
        );
    }

    #[test]
    fn champ_profile_marks_starter_like_deck_as_missing_burst_and_execute_plan() {
        let mut run = RunState::new(744270980, 0, false, "Ironclad");
        run.act_num = 2;
        run.floor_num = 31;
        run.boss_key = Some(EncounterId::TheChamp);
        replace_deck(
            &mut run,
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
            ],
        );

        let profile = boss_mechanic_pressure_profile_v1(&run, EncounterId::TheChamp);

        assert!(profile.has_missing_answer(BossMechanicMissingAnswerV1::ChampTransitionBurst));
        assert!(profile.has_missing_answer(BossMechanicMissingAnswerV1::ExecuteBlockPlan));
        assert!(profile.has_red_flag(BossMechanicRedFlagV1::PrematureChampTransitionRisk));
        assert!(profile.has_red_flag(BossMechanicRedFlagV1::NoExecuteBlockPlan));
    }

    #[test]
    fn barricade_alone_is_not_a_direct_boss_block_plan() {
        let mut champ = RunState::new(744270980, 0, false, "Ironclad");
        champ.act_num = 2;
        champ.floor_num = 31;
        champ.boss_key = Some(EncounterId::TheChamp);
        replace_deck(
            &mut champ,
            &[
                CardId::Barricade,
                CardId::Defend,
                CardId::Defend,
                CardId::Strike,
                CardId::Bash,
            ],
        );

        let champ_profile = boss_mechanic_pressure_profile_v1(&champ, EncounterId::TheChamp);

        assert!(champ_profile.has_missing_answer(BossMechanicMissingAnswerV1::ExecuteBlockPlan));
        assert!(champ_profile.has_red_flag(BossMechanicRedFlagV1::NoExecuteBlockPlan));

        let mut automaton = champ.clone();
        automaton.boss_key = Some(EncounterId::Automaton);

        let automaton_profile =
            boss_mechanic_pressure_profile_v1(&automaton, EncounterId::Automaton);

        assert!(automaton_profile
            .has_missing_answer(BossMechanicMissingAnswerV1::Block50OrKillBeforeBeam));
        assert!(automaton_profile.has_red_flag(BossMechanicRedFlagV1::NoHyperbeamBlockPlan));
    }

    #[test]
    fn barricade_with_block_multiplier_supports_long_block_plan() {
        let mut run = RunState::new(744270980, 0, false, "Ironclad");
        run.act_num = 2;
        run.floor_num = 31;
        run.boss_key = Some(EncounterId::TheChamp);
        replace_deck(
            &mut run,
            &[
                CardId::Barricade,
                CardId::Entrench,
                CardId::Defend,
                CardId::Defend,
                CardId::Carnage,
            ],
        );

        let profile = boss_mechanic_pressure_profile_v1(&run, EncounterId::TheChamp);

        assert!(!profile.has_missing_answer(BossMechanicMissingAnswerV1::ExecuteBlockPlan));
        assert!(!profile.has_red_flag(BossMechanicRedFlagV1::NoExecuteBlockPlan));
    }

    #[test]
    fn champ_profile_does_not_count_strength_payoff_as_burst_without_strength_source() {
        let mut payoff_only = RunState::new(744270980, 0, false, "Ironclad");
        payoff_only.act_num = 2;
        payoff_only.floor_num = 31;
        payoff_only.boss_key = Some(EncounterId::TheChamp);
        replace_deck(
            &mut payoff_only,
            &[CardId::HeavyBlade, CardId::PowerThrough],
        );

        let payoff_only_profile =
            boss_mechanic_pressure_profile_v1(&payoff_only, EncounterId::TheChamp);

        assert!(payoff_only_profile
            .has_missing_answer(BossMechanicMissingAnswerV1::ChampTransitionBurst));

        let mut supported = payoff_only.clone();
        replace_deck(
            &mut supported,
            &[CardId::HeavyBlade, CardId::JAX, CardId::PowerThrough],
        );

        let supported_profile =
            boss_mechanic_pressure_profile_v1(&supported, EncounterId::TheChamp);

        assert!(!supported_profile
            .has_missing_answer(BossMechanicMissingAnswerV1::ChampTransitionBurst));
    }
}
