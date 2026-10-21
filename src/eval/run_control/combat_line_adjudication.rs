use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::CombatSearchAcceptancePluginId;
use crate::sim::combat::CombatTerminal;

use super::transition_report::CardSnapshot;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLineCleanlinessV1 {
    Clean,
    Dirty,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatLineRejectionReasonV1 {
    NewCurse { cards: Vec<CardSnapshot> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatLineObservedOutcomeV1 {
    pub terminal: CombatTerminal,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub potions_used: u32,
    pub action_count: usize,
    pub gold_delta: i32,
    pub ritual_dagger_growth: i32,
    pub gained_curses: Vec<CardSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CombatLineAdjudicationV1 {
    Accepted {
        policy: CombatSearchAcceptancePluginId,
        cleanliness: CombatLineCleanlinessV1,
        observed_outcome: CombatLineObservedOutcomeV1,
    },
    Rejected {
        policy: CombatSearchAcceptancePluginId,
        reason: CombatLineRejectionReasonV1,
        observed_outcome: CombatLineObservedOutcomeV1,
    },
    ReplayFailed {
        policy: CombatSearchAcceptancePluginId,
        error: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CombatLineAcceptancePolicy {
    plugin: CombatSearchAcceptancePluginId,
    reject_gained_curses: bool,
}

impl CombatLineAcceptancePolicy {
    pub(super) fn from_plugin(plugin: CombatSearchAcceptancePluginId) -> Self {
        Self {
            plugin,
            reject_gained_curses: matches!(
                plugin,
                CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse
            ),
        }
    }

    pub(super) fn adjudicate(
        self,
        outcome: CombatLineObservedOutcomeV1,
    ) -> CombatLineAdjudicationV1 {
        if self.reject_gained_curses && !outcome.gained_curses.is_empty() {
            return CombatLineAdjudicationV1::Rejected {
                policy: self.plugin,
                reason: CombatLineRejectionReasonV1::NewCurse {
                    cards: outcome.gained_curses.clone(),
                },
                observed_outcome: outcome,
            };
        }
        CombatLineAdjudicationV1::Accepted {
            policy: self.plugin,
            cleanliness: if outcome.gained_curses.is_empty() {
                CombatLineCleanlinessV1::Clean
            } else {
                CombatLineCleanlinessV1::Dirty
            },
            observed_outcome: outcome,
        }
    }

    pub(super) fn requires_clean_line(self) -> bool {
        self.reject_gained_curses
    }

    pub(super) fn plugin(self) -> CombatSearchAcceptancePluginId {
        self.plugin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;

    fn parasite_outcome() -> CombatLineObservedOutcomeV1 {
        CombatLineObservedOutcomeV1 {
            terminal: CombatTerminal::Win,
            final_hp: 44,
            hp_loss: 0,
            potions_used: 0,
            action_count: 32,
            gold_delta: 0,
            ritual_dagger_growth: 0,
            gained_curses: vec![CardSnapshot {
                id: CardId::Parasite,
                lineage: crate::test_support::scenario_lineage(9001),
                upgrades: 0,
            }],
        }
    }

    #[test]
    fn acceptance_plugins_adjudicate_the_same_dirty_outcome_explicitly() {
        let outcome = parasite_outcome();

        for plugin in [
            CombatSearchAcceptancePluginId::AcceptedLineOnly,
            CombatSearchAcceptancePluginId::AcceptedLineOrPrimaryChunk,
        ] {
            assert_eq!(
                CombatLineAcceptancePolicy::from_plugin(plugin).adjudicate(outcome.clone()),
                CombatLineAdjudicationV1::Accepted {
                    policy: plugin,
                    cleanliness: CombatLineCleanlinessV1::Dirty,
                    observed_outcome: outcome.clone(),
                }
            );
        }

        assert_eq!(
            CombatLineAcceptancePolicy::from_plugin(
                CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
            )
            .adjudicate(outcome.clone()),
            CombatLineAdjudicationV1::Rejected {
                policy: CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
                reason: CombatLineRejectionReasonV1::NewCurse {
                    cards: outcome.gained_curses.clone(),
                },
                observed_outcome: outcome,
            }
        );
    }
}
