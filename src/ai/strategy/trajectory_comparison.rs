use serde::{Deserialize, Serialize};

use crate::ai::strategy::challenger_signature::DeckBurdenBand;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryTerminal {
    Running,
    Victory,
    Defeat,
    CoverageLimited,
    Gap,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrajectoryProgress {
    pub act: u8,
    pub floor: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrajectoryPressureEvidence {
    Unknown,
    Comparable { open: u16, covered: u16 },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrajectoryDeployabilityEvidence {
    Unknown,
    Comparable {
        claimed_answers: u16,
        timely_playable: u16,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrajectoryResources {
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub potion_count: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrajectoryConstruction {
    pub burden: DeckBurdenBand,
    pub completed_commitments: u16,
    pub active_commitments: u16,
    pub failed_commitments: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectorySearchComparabilityStatus {
    Comparable,
    WallSafetyLimited,
    InsufficientEvidence,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrajectorySearchComparability {
    pub status: TrajectorySearchComparabilityStatus,
    pub total_attempts: u32,
    pub exact_accepted_attempts: u32,
    pub node_bounded_attempts: u32,
    pub exhaustive_attempts: u32,
    pub wall_limited_attempts: u32,
    pub insufficient_attempts: u32,
}

impl TrajectorySearchComparability {
    pub const fn comparable_without_attempts() -> Self {
        Self {
            status: TrajectorySearchComparabilityStatus::Comparable,
            total_attempts: 0,
            exact_accepted_attempts: 0,
            node_bounded_attempts: 0,
            exhaustive_attempts: 0,
            wall_limited_attempts: 0,
            insufficient_attempts: 0,
        }
    }
}

impl Default for TrajectorySearchComparability {
    fn default() -> Self {
        Self {
            status: TrajectorySearchComparabilityStatus::InsufficientEvidence,
            total_attempts: 0,
            exact_accepted_attempts: 0,
            node_bounded_attempts: 0,
            exhaustive_attempts: 0,
            wall_limited_attempts: 0,
            insufficient_attempts: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrajectorySnapshot {
    pub lane: String,
    pub terminal: TrajectoryTerminal,
    pub progress: TrajectoryProgress,
    pub pressure: TrajectoryPressureEvidence,
    pub deployability: TrajectoryDeployabilityEvidence,
    pub resources: TrajectoryResources,
    pub construction: TrajectoryConstruction,
    #[serde(default)]
    pub search_comparability: TrajectorySearchComparability,
    #[serde(default)]
    pub full_search_comparability: TrajectorySearchComparability,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerComparison {
    BaselineBetter,
    ChallengerBetter,
    Equal,
    Unknown,
    Conflict,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryVerdict {
    BaselineBetter,
    ChallengerBetter,
    Equivalent,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TrajectoryPairEligibility {
    Comparable,
    ExcludedWallSafetyLimited,
    #[default]
    ExcludedInsufficientEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrajectoryComparison {
    pub baseline_lane: String,
    pub challenger_lane: String,
    pub progression: LayerComparison,
    pub pressure: LayerComparison,
    pub deployability: LayerComparison,
    pub resources: LayerComparison,
    pub construction: LayerComparison,
    #[serde(default)]
    pub eligibility: TrajectoryPairEligibility,
    pub verdict: TrajectoryVerdict,
}

pub fn compare_trajectories(
    baseline: &TrajectorySnapshot,
    challenger: &TrajectorySnapshot,
) -> TrajectoryComparison {
    let progression = compare_progression(baseline, challenger);
    let pressure = compare_pressure(baseline.pressure, challenger.pressure);
    let deployability = compare_deployability(baseline.deployability, challenger.deployability);
    let resources = compare_resources(baseline.resources, challenger.resources);
    let construction = compare_construction(baseline.construction, challenger.construction);

    let terminal_verdict = match progression {
        LayerComparison::BaselineBetter
            if terminal_decides(baseline.terminal, challenger.terminal) =>
        {
            Some(TrajectoryVerdict::BaselineBetter)
        }
        LayerComparison::ChallengerBetter
            if terminal_decides(baseline.terminal, challenger.terminal) =>
        {
            Some(TrajectoryVerdict::ChallengerBetter)
        }
        _ => None,
    };
    let layers = [
        progression,
        pressure,
        deployability,
        resources,
        construction,
    ];
    let eligibility = pair_eligibility(
        baseline.search_comparability.status,
        challenger.search_comparability.status,
    );
    let computed_verdict = terminal_verdict.unwrap_or_else(|| aggregate_nonterminal(&layers));
    let verdict = if eligibility == TrajectoryPairEligibility::Comparable {
        computed_verdict
    } else {
        TrajectoryVerdict::Inconclusive
    };

    TrajectoryComparison {
        baseline_lane: baseline.lane.clone(),
        challenger_lane: challenger.lane.clone(),
        progression,
        pressure,
        deployability,
        resources,
        construction,
        eligibility,
        verdict,
    }
}

fn pair_eligibility(
    baseline: TrajectorySearchComparabilityStatus,
    challenger: TrajectorySearchComparabilityStatus,
) -> TrajectoryPairEligibility {
    use TrajectorySearchComparabilityStatus::{
        Comparable, InsufficientEvidence, WallSafetyLimited,
    };
    match (baseline, challenger) {
        (WallSafetyLimited, _) | (_, WallSafetyLimited) => {
            TrajectoryPairEligibility::ExcludedWallSafetyLimited
        }
        (InsufficientEvidence, _) | (_, InsufficientEvidence) => {
            TrajectoryPairEligibility::ExcludedInsufficientEvidence
        }
        (Comparable, Comparable) => TrajectoryPairEligibility::Comparable,
    }
}

fn compare_progression(
    baseline: &TrajectorySnapshot,
    challenger: &TrajectorySnapshot,
) -> LayerComparison {
    use TrajectoryTerminal::{Defeat, Victory};

    match (baseline.terminal, challenger.terminal) {
        (Victory, Victory) | (Defeat, Defeat) => compare_ord(
            (baseline.progress.act, baseline.progress.floor),
            (challenger.progress.act, challenger.progress.floor),
        ),
        (Victory, _) | (_, Defeat) => LayerComparison::BaselineBetter,
        (_, Victory) | (Defeat, _) => LayerComparison::ChallengerBetter,
        _ => compare_ord(
            (baseline.progress.act, baseline.progress.floor),
            (challenger.progress.act, challenger.progress.floor),
        ),
    }
}

fn terminal_decides(baseline: TrajectoryTerminal, challenger: TrajectoryTerminal) -> bool {
    use TrajectoryTerminal::{Defeat, Victory};
    matches!(
        (baseline, challenger),
        (Victory, _) | (_, Victory) | (Defeat, _) | (_, Defeat)
    ) && baseline != challenger
}

fn compare_pressure(
    baseline: TrajectoryPressureEvidence,
    challenger: TrajectoryPressureEvidence,
) -> LayerComparison {
    match (baseline, challenger) {
        (
            TrajectoryPressureEvidence::Comparable {
                open: baseline_open,
                covered: baseline_covered,
            },
            TrajectoryPressureEvidence::Comparable {
                open: challenger_open,
                covered: challenger_covered,
            },
        ) => pareto_compare([
            baseline_open.cmp(&challenger_open).reverse(),
            baseline_covered.cmp(&challenger_covered),
        ]),
        _ => LayerComparison::Unknown,
    }
}

fn compare_deployability(
    baseline: TrajectoryDeployabilityEvidence,
    challenger: TrajectoryDeployabilityEvidence,
) -> LayerComparison {
    match (baseline, challenger) {
        (
            TrajectoryDeployabilityEvidence::Comparable {
                claimed_answers: baseline_claimed,
                timely_playable: baseline_timely,
            },
            TrajectoryDeployabilityEvidence::Comparable {
                claimed_answers: challenger_claimed,
                timely_playable: challenger_timely,
            },
        ) => pareto_compare([
            baseline_claimed.cmp(&challenger_claimed),
            baseline_timely.cmp(&challenger_timely),
        ]),
        _ => LayerComparison::Unknown,
    }
}

fn compare_resources(
    baseline: TrajectoryResources,
    challenger: TrajectoryResources,
) -> LayerComparison {
    pareto_compare([
        baseline.hp.max(0).cmp(&challenger.hp.max(0)),
        baseline.max_hp.max(0).cmp(&challenger.max_hp.max(0)),
        baseline.gold.max(0).cmp(&challenger.gold.max(0)),
        baseline.potion_count.cmp(&challenger.potion_count),
    ])
}

fn compare_construction(
    baseline: TrajectoryConstruction,
    challenger: TrajectoryConstruction,
) -> LayerComparison {
    if baseline == challenger {
        return LayerComparison::Equal;
    }
    if challenger.active_commitments > baseline.active_commitments
        && challenger.completed_commitments == baseline.completed_commitments
        && challenger.failed_commitments == baseline.failed_commitments
        && challenger.burden == baseline.burden
    {
        return LayerComparison::Unknown;
    }

    let comparison = pareto_compare([
        baseline.burden.cmp(&challenger.burden).reverse(),
        baseline
            .completed_commitments
            .cmp(&challenger.completed_commitments),
        baseline
            .active_commitments
            .cmp(&challenger.active_commitments)
            .reverse(),
        baseline
            .failed_commitments
            .cmp(&challenger.failed_commitments)
            .reverse(),
    ]);
    match comparison {
        LayerComparison::Equal => LayerComparison::Equal,
        LayerComparison::BaselineBetter | LayerComparison::ChallengerBetter => comparison,
        LayerComparison::Conflict | LayerComparison::Unknown => LayerComparison::Conflict,
    }
}

fn compare_ord<T: Ord>(baseline: T, challenger: T) -> LayerComparison {
    use std::cmp::Ordering;
    match baseline.cmp(&challenger) {
        Ordering::Greater => LayerComparison::BaselineBetter,
        Ordering::Less => LayerComparison::ChallengerBetter,
        Ordering::Equal => LayerComparison::Equal,
    }
}

fn pareto_compare<const N: usize>(dimensions: [std::cmp::Ordering; N]) -> LayerComparison {
    use std::cmp::Ordering;
    let baseline_better = dimensions.contains(&Ordering::Greater);
    let challenger_better = dimensions.contains(&Ordering::Less);
    match (baseline_better, challenger_better) {
        (true, true) => LayerComparison::Conflict,
        (true, false) => LayerComparison::BaselineBetter,
        (false, true) => LayerComparison::ChallengerBetter,
        (false, false) => LayerComparison::Equal,
    }
}

fn aggregate_nonterminal(layers: &[LayerComparison]) -> TrajectoryVerdict {
    if layers
        .iter()
        .any(|layer| matches!(layer, LayerComparison::Unknown | LayerComparison::Conflict))
    {
        return TrajectoryVerdict::Inconclusive;
    }
    let baseline_better = layers.contains(&LayerComparison::BaselineBetter);
    let challenger_better = layers.contains(&LayerComparison::ChallengerBetter);
    match (baseline_better, challenger_better) {
        (true, false) => TrajectoryVerdict::BaselineBetter,
        (false, true) => TrajectoryVerdict::ChallengerBetter,
        (false, false) => TrajectoryVerdict::Equivalent,
        (true, true) => TrajectoryVerdict::Inconclusive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::challenger_signature::DeckBurdenBand;

    fn snapshot(lane: &str) -> TrajectorySnapshot {
        TrajectorySnapshot {
            lane: lane.to_string(),
            terminal: TrajectoryTerminal::Running,
            progress: TrajectoryProgress { act: 2, floor: 20 },
            pressure: TrajectoryPressureEvidence::Unknown,
            deployability: TrajectoryDeployabilityEvidence::Unknown,
            resources: TrajectoryResources {
                hp: 40,
                max_hp: 80,
                gold: 100,
                potion_count: 1,
            },
            construction: TrajectoryConstruction {
                burden: DeckBurdenBand::Watch,
                completed_commitments: 0,
                active_commitments: 0,
                failed_commitments: 0,
            },
            search_comparability: TrajectorySearchComparability::comparable_without_attempts(),
            full_search_comparability: TrajectorySearchComparability::comparable_without_attempts(),
        }
    }

    #[test]
    fn comparable_pair_keeps_existing_terminal_verdict() {
        let baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        challenger.terminal = TrajectoryTerminal::Victory;

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(
            comparison.eligibility,
            TrajectoryPairEligibility::Comparable
        );
        assert_eq!(comparison.verdict, TrajectoryVerdict::ChallengerBetter);
    }

    #[test]
    fn wall_limited_pair_is_explicitly_excluded() {
        let baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        challenger.terminal = TrajectoryTerminal::Victory;
        challenger.search_comparability = TrajectorySearchComparability {
            status: TrajectorySearchComparabilityStatus::WallSafetyLimited,
            total_attempts: 1,
            exact_accepted_attempts: 0,
            node_bounded_attempts: 0,
            exhaustive_attempts: 0,
            wall_limited_attempts: 1,
            insufficient_attempts: 0,
        };

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(
            comparison.eligibility,
            TrajectoryPairEligibility::ExcludedWallSafetyLimited
        );
        assert_eq!(comparison.verdict, TrajectoryVerdict::Inconclusive);
        assert_eq!(comparison.progression, LayerComparison::ChallengerBetter);
    }

    #[test]
    fn insufficient_pair_is_explicitly_excluded() {
        let baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        challenger.search_comparability = TrajectorySearchComparability::default();

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(
            comparison.eligibility,
            TrajectoryPairEligibility::ExcludedInsufficientEvidence
        );
        assert_eq!(comparison.verdict, TrajectoryVerdict::Inconclusive);
    }

    #[test]
    fn legacy_trajectory_json_defaults_to_excluded_search_evidence() {
        let baseline = snapshot("baseline");
        let challenger = snapshot("challenger-1");
        let comparison = compare_trajectories(&baseline, &challenger);

        let mut snapshot_value = serde_json::to_value(&baseline).expect("serialize snapshot");
        snapshot_value
            .as_object_mut()
            .expect("snapshot object")
            .remove("search_comparability");
        snapshot_value
            .as_object_mut()
            .expect("snapshot object")
            .remove("full_search_comparability");
        let restored_snapshot: TrajectorySnapshot =
            serde_json::from_value(snapshot_value).expect("legacy snapshot");

        let mut comparison_value = serde_json::to_value(comparison).expect("serialize comparison");
        comparison_value
            .as_object_mut()
            .expect("comparison object")
            .remove("eligibility");
        let restored_comparison: TrajectoryComparison =
            serde_json::from_value(comparison_value).expect("legacy comparison");

        assert_eq!(
            restored_snapshot.search_comparability.status,
            TrajectorySearchComparabilityStatus::InsufficientEvidence
        );
        assert_eq!(
            restored_snapshot.full_search_comparability.status,
            TrajectorySearchComparabilityStatus::InsufficientEvidence
        );
        assert_eq!(
            restored_comparison.eligibility,
            TrajectoryPairEligibility::ExcludedInsufficientEvidence
        );
    }

    #[test]
    fn terminal_victory_is_decisive_even_with_fewer_resources() {
        let baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        challenger.terminal = TrajectoryTerminal::Victory;
        challenger.resources.hp = 1;
        challenger.resources.gold = 0;

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(comparison.verdict, TrajectoryVerdict::ChallengerBetter);
        assert_eq!(comparison.progression, LayerComparison::ChallengerBetter);
    }

    #[test]
    fn more_hp_cannot_resolve_unknown_pressure_and_deployability() {
        let baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        challenger.resources.hp = 70;

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(comparison.resources, LayerComparison::ChallengerBetter);
        assert_eq!(comparison.pressure, LayerComparison::Unknown);
        assert_eq!(comparison.verdict, TrajectoryVerdict::Inconclusive);
    }

    #[test]
    fn mixed_nonterminal_directions_are_inconclusive() {
        let mut baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        challenger.progress.floor = 21;
        baseline.resources.hp = 60;
        challenger.resources.hp = 30;

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(comparison.progression, LayerComparison::ChallengerBetter);
        assert_eq!(comparison.resources, LayerComparison::BaselineBetter);
        assert_eq!(comparison.verdict, TrajectoryVerdict::Inconclusive);
    }

    #[test]
    fn resource_layer_uses_pareto_dominance_instead_of_a_sum() {
        let mut baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        baseline.resources.hp = 60;
        baseline.resources.gold = 20;
        challenger.resources.hp = 40;
        challenger.resources.gold = 200;

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(comparison.resources, LayerComparison::Conflict);
        assert_eq!(comparison.verdict, TrajectoryVerdict::Inconclusive);
    }

    #[test]
    fn complete_equal_evidence_is_equivalent_but_unknown_is_not() {
        let mut baseline = snapshot("baseline");
        let mut challenger = snapshot("challenger-1");
        baseline.pressure = TrajectoryPressureEvidence::Comparable {
            open: 1,
            covered: 2,
        };
        challenger.pressure = baseline.pressure;
        baseline.deployability = TrajectoryDeployabilityEvidence::Comparable {
            claimed_answers: 2,
            timely_playable: 1,
        };
        challenger.deployability = baseline.deployability;

        let comparison = compare_trajectories(&baseline, &challenger);

        assert_eq!(comparison.verdict, TrajectoryVerdict::Equivalent);
    }
}
