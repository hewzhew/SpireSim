use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2WitnessLine;
use sts_oracle_runtime::sim::combat::CombatTerminal;

use super::super::quality_lanes::CombatLineQuality;
use super::super::search_types::{SearchDiagnosticProgressFacts, SearchReview};

#[derive(Serialize)]
pub(crate) struct CounterfactualHpProbe {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) original_hp: i32,
    pub(super) max_hp: i32,
    pub(super) levels: Vec<CounterfactualHpLevel>,
    pub(super) classification: CounterfactualHpClassification,
}

#[derive(Serialize)]
pub(super) struct CounterfactualHpLevel {
    pub(super) label: String,
    pub(super) hp: i32,
    pub(super) selected_lane: Option<&'static str>,
    pub(super) complete_win: bool,
    pub(super) quality: Option<CombatLineQuality>,
    pub(super) best_progress: Option<CounterfactualHpProgress>,
    pub(super) nodes_to_first_win: Option<u64>,
    pub(super) total_terminal_wins: u64,
    pub(super) replay_on_original_hp: Option<CounterfactualHpReplay>,
}

#[derive(Clone, Serialize)]
pub(super) struct CounterfactualHpProgress {
    pub(super) lane: &'static str,
    pub(super) facts: SearchDiagnosticProgressFacts,
}

#[derive(Serialize)]
pub(super) struct CounterfactualHpReplay {
    pub(super) terminal: CombatTerminal,
    pub(super) final_hp: i32,
    pub(super) total_enemy_hp: i32,
    pub(super) living_enemy_count: usize,
    pub(super) replayed_actions: usize,
    pub(super) action_count: Option<usize>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CounterfactualHpClassification {
    OriginalHpWin,
    CounterfactualLineStillWinsOriginalHp,
    CounterfactualOnlyWin,
    NoWinFound,
}

pub(super) struct CounterfactualHpCandidate {
    pub(super) lane: &'static str,
    pub(super) review: SearchReview,
    pub(super) quality: CombatLineQuality,
    pub(super) witness: CombatSearchV2WitnessLine,
}

impl CounterfactualHpProbe {
    pub(crate) fn classification_label(&self) -> &'static str {
        match self.classification {
            CounterfactualHpClassification::OriginalHpWin => "original_hp_win",
            CounterfactualHpClassification::CounterfactualLineStillWinsOriginalHp => {
                "counterfactual_line_still_wins_original_hp"
            }
            CounterfactualHpClassification::CounterfactualOnlyWin => "counterfactual_only_win",
            CounterfactualHpClassification::NoWinFound => "no_win_found",
        }
    }

    pub(crate) fn any_complete_win(&self) -> bool {
        self.levels.iter().any(|level| level.complete_win)
    }

    pub(crate) fn full_hp_best_progress_enemy_hp(&self) -> Option<i32> {
        self.full_hp_level()
            .and_then(|level| level.best_progress.as_ref())
            .map(|progress| progress.facts.total_enemy_hp)
    }

    pub(crate) fn full_hp_best_progress_turns(&self) -> Option<u32> {
        self.full_hp_level()
            .and_then(|level| level.best_progress.as_ref())
            .map(|progress| progress.facts.turns)
    }

    pub(crate) fn full_hp_complete_win(&self) -> Option<bool> {
        self.full_hp_level().map(|level| level.complete_win)
    }

    pub(crate) fn original_hp(&self) -> i32 {
        self.original_hp
    }

    pub(crate) fn max_hp(&self) -> i32 {
        self.max_hp
    }

    fn full_hp_level(&self) -> Option<&CounterfactualHpLevel> {
        self.levels.iter().find(|level| level.hp == self.max_hp)
    }
}
