#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(in crate::ai::combat_search_v2::value::combat_eval) enum CombatEvalEvidenceKind {
    None,
    UnresolvedEstimate,
    SimulatedTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(in crate::ai::combat_search_v2) enum CombatEvalOutcomeClass {
    Loss,
    Unresolved,
    Win,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(in crate::ai::combat_search_v2) enum CombatEvalSurvivalBucket {
    DeadOrForcedLoss,
    LethalVisible,
    Critical,
    Stabilizing,
    Stable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(in crate::ai::combat_search_v2) enum CombatEvalProgressBucket {
    Regression,
    Stalled,
    AttritionFavored,
    RaceFavored,
    LethalNextTurnLikely,
    LethalNow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct CombatEvalV2 {
    pub(in crate::ai::combat_search_v2::value::combat_eval) evidence: CombatEvalEvidenceKind,
    pub(in crate::ai::combat_search_v2::value::combat_eval) outcome: CombatEvalOutcomeClass,
    pub(in crate::ai::combat_search_v2::value::combat_eval) survival: CombatEvalSurvivalBucket,
    pub(in crate::ai::combat_search_v2::value::combat_eval) progress: CombatEvalProgressBucket,
    pub(in crate::ai::combat_search_v2::value::combat_eval) risk_margin: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) persistent_adjusted_hp: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) final_hp: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) persistent_run_value: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) enemy_progress: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) phase_stability: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) resource_conservation: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) faster_turns: i32,
    pub(in crate::ai::combat_search_v2::value::combat_eval) fewer_cards_played: i32,
}

impl Default for CombatEvalV2 {
    fn default() -> Self {
        Self {
            evidence: CombatEvalEvidenceKind::None,
            outcome: CombatEvalOutcomeClass::Unresolved,
            survival: CombatEvalSurvivalBucket::DeadOrForcedLoss,
            progress: CombatEvalProgressBucket::Stalled,
            risk_margin: 0,
            persistent_adjusted_hp: 0,
            final_hp: 0,
            persistent_run_value: 0,
            enemy_progress: 0,
            phase_stability: 0,
            resource_conservation: 0,
            faster_turns: 0,
            fewer_cards_played: 0,
        }
    }
}

impl CombatEvalV2 {
    pub(in crate::ai::combat_search_v2) fn outcome_class(self) -> CombatEvalOutcomeClass {
        self.outcome
    }

    pub(in crate::ai::combat_search_v2) fn survival_bucket(self) -> CombatEvalSurvivalBucket {
        self.survival
    }

    pub(in crate::ai::combat_search_v2) fn progress_bucket(self) -> CombatEvalProgressBucket {
        self.progress
    }

    pub(in crate::ai::combat_search_v2) fn risk_margin(self) -> i32 {
        self.risk_margin
    }

    pub(in crate::ai::combat_search_v2) fn final_hp(self) -> i32 {
        self.final_hp
    }

    pub(in crate::ai::combat_search_v2) fn enemy_progress(self) -> i32 {
        self.enemy_progress
    }
}

impl CombatEvalOutcomeClass {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::Loss => "loss",
            Self::Unresolved => "unresolved",
            Self::Win => "win",
        }
    }
}

impl CombatEvalSurvivalBucket {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::DeadOrForcedLoss => "dead_or_forced_loss",
            Self::LethalVisible => "lethal_visible",
            Self::Critical => "critical",
            Self::Stabilizing => "stabilizing",
            Self::Stable => "stable",
        }
    }

    pub(in crate::ai::combat_search_v2::value::combat_eval) fn is_danger(self) -> bool {
        matches!(
            self,
            CombatEvalSurvivalBucket::DeadOrForcedLoss
                | CombatEvalSurvivalBucket::LethalVisible
                | CombatEvalSurvivalBucket::Critical
        )
    }
}

impl CombatEvalProgressBucket {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::Regression => "regression",
            Self::Stalled => "stalled",
            Self::AttritionFavored => "attrition_favored",
            Self::RaceFavored => "race_favored",
            Self::LethalNextTurnLikely => "lethal_next_turn_likely",
            Self::LethalNow => "lethal_now",
        }
    }
}
