use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchTerminalLabel {
    Win,
    Escape,
    Loss,
    Unresolved,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCoverageStatus {
    Exhaustive,
    AcceptedCompleteCandidate,
    NodeBudgetLimited,
    ActionPrefixBudgetLimited,
    ActionSurfaceIncomplete,
    TimeBudgetLimited,
    FrontierOpen,
}

#[cfg(test)]
mod tests {
    use super::SearchCoverageStatus;

    #[test]
    fn search_coverage_status_deserializes_snake_case_artifact_json() {
        let status: SearchCoverageStatus =
            serde_json::from_str("\"node_budget_limited\"").expect("artifact status should parse");

        assert_eq!(status, SearchCoverageStatus::NodeBudgetLimited);

        let incomplete: SearchCoverageStatus =
            serde_json::from_str("\"action_surface_incomplete\"")
                .expect("new action-surface status should parse");
        assert_eq!(incomplete, SearchCoverageStatus::ActionSurfaceIncomplete);
    }
}
