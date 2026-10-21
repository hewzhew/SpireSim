use serde::{Deserialize, Serialize};

use crate::agent::information::effect::{
    PublicCombatActionEffectEnvelopeV1, PublicCombatEffectProjectionErrorV1,
};
use crate::agent::information::state::public_combat_state_replay_compatible_v2;
use crate::ai::planner_core::{PlannerDecisionContext, PublicInformationSnapshotV1};

use super::{
    build_public_information_snapshot_v1, GameOutcomeV1, TaskDecisionSiteV1, TaskPublicCandidateV1,
    TaskPublicObservationV1, TaskStrategicActionV1,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TaskScopeOrdinalV1(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TaskAttemptOrdinalV1(pub u32);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPublicDecisionV1 {
    pub site: TaskDecisionSiteV1,
    pub observation: TaskPublicObservationV1,
    pub candidates: Vec<TaskPublicCandidateV1>,
    pub information: PublicInformationSnapshotV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPublicCombatCandidateEffectV1 {
    pub public_snapshot_id: String,
    pub candidate_ordinal: usize,
    pub effect: PublicCombatActionEffectEnvelopeV1,
}

impl TaskPublicDecisionV1 {
    pub fn new(
        site: TaskDecisionSiteV1,
        observation: TaskPublicObservationV1,
        candidates: Vec<TaskPublicCandidateV1>,
    ) -> Result<Self, String> {
        let information = build_public_information_snapshot_v1(site, &observation, &candidates)?;
        Ok(Self {
            site,
            observation,
            candidates,
            information,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let expected =
            build_public_information_snapshot_v1(self.site, &self.observation, &self.candidates)?;
        if self.information != expected {
            return Err("task public information identity disagrees with its payload".to_string());
        }
        Ok(())
    }

    pub fn validate_choice(&self, choice: &TaskPublicChoiceV1) -> Result<(), String> {
        let Some(first) = choice.ordinal_path.first().copied() else {
            return Err("task public choice path must not be empty".to_string());
        };
        if first >= self.candidates.len() {
            return Err(format!(
                "task public choice ordinal {first} is outside candidate surface of length {}",
                self.candidates.len()
            ));
        }
        Ok(())
    }

    /// Equality used only while replaying a durable archive across an explicit
    /// public-observation schema migration. Candidate semantics and every
    /// non-migrated observation field must still match exactly.
    pub fn replay_compatible_v2(&self, observed: &Self) -> bool {
        self.replay_incompatibility_v2(observed).is_none()
    }

    /// Returns the first semantic component that prevents durable replay.
    /// This deliberately reports only typed public paths, never the complete
    /// observation or private checkpoint payload.
    #[doc(hidden)]
    pub fn replay_incompatibility_v2(&self, observed: &Self) -> Option<String> {
        if self == observed {
            return None;
        }
        if self.site != observed.site {
            return Some("site".to_string());
        }
        if self.candidates.len() != observed.candidates.len() {
            return Some("candidates.length".to_string());
        }
        if let Some((candidate_ordinal, path)) = self
            .candidates
            .iter()
            .zip(&observed.candidates)
            .enumerate()
            .find_map(|(candidate_ordinal, (expected, observed))| {
                task_public_candidate_replay_incompatibility_v2(expected, observed)
                    .map(|path| (candidate_ordinal, path))
            })
        {
            return Some(format!("candidates[{candidate_ordinal}].{path}"));
        }
        match (&self.observation, &observed.observation) {
            (
                TaskPublicObservationV1::Strategic {
                    observation: expected,
                },
                TaskPublicObservationV1::Strategic {
                    observation: observed,
                },
            ) => strategic_observation_replay_incompatibility_v2(expected, observed)
                .map(|path| format!("observation.strategic.{path}")),
            (
                TaskPublicObservationV1::Combat {
                    run_context: expected_context,
                    observation: expected_observation,
                },
                TaskPublicObservationV1::Combat {
                    run_context: observed_context,
                    observation: observed_observation,
                },
            ) => {
                if expected_context != observed_context {
                    Some("observation.combat.run_context".to_string())
                } else if !public_combat_state_replay_compatible_v2(
                    expected_observation,
                    observed_observation,
                ) {
                    Some("observation.combat.state".to_string())
                } else {
                    None
                }
            }
            _ => Some("observation.kind".to_string()),
        }
    }

    /// Projects pre-execution mechanics for one candidate against this
    /// decision's own public combat observation. It never executes the action
    /// and cannot be paired with a caller-supplied observation.
    pub fn combat_candidate_effect_v1(
        &self,
        candidate_ordinal: usize,
    ) -> Result<TaskPublicCombatCandidateEffectV1, PublicCombatEffectProjectionErrorV1> {
        if self.validate().is_err() {
            return Err(PublicCombatEffectProjectionErrorV1::NonCanonicalTaskDecision);
        }
        let candidate = self.candidates.get(candidate_ordinal).ok_or(
            PublicCombatEffectProjectionErrorV1::CandidateOrdinalOutOfRange(candidate_ordinal),
        )?;
        let TaskPublicObservationV1::Combat { observation, .. } = &self.observation else {
            return Err(PublicCombatEffectProjectionErrorV1::NotCombatDecision);
        };
        Ok(TaskPublicCombatCandidateEffectV1 {
            public_snapshot_id: self.information.snapshot_id.clone(),
            candidate_ordinal,
            effect: candidate.combat_effect_v1(observation)?,
        })
    }
}

fn strategic_observation_replay_incompatibility_v2(
    expected: &super::TaskStrategicObservationV1,
    observed: &super::TaskStrategicObservationV1,
) -> Option<&'static str> {
    if expected == observed {
        return None;
    }
    let reward_context_compatible = match (&expected.context, &observed.context) {
        (
            PlannerDecisionContext::Reward {
                pending_rewards: expected,
            },
            PlannerDecisionContext::Reward {
                pending_rewards: observed,
            },
        ) => expected == observed || (expected.is_empty() && !observed.is_empty()),
        _ => expected.context == observed.context,
    };
    if !reward_context_compatible {
        Some("context")
    } else if expected.mechanics != observed.mechanics {
        Some("mechanics")
    } else if expected.run_goal != observed.run_goal {
        Some("run_goal")
    } else if expected.decision_site != observed.decision_site {
        Some("decision_site")
    } else if expected.run != observed.run {
        Some("run")
    } else if expected.cards != observed.cards {
        Some("cards")
    } else if expected.relics != observed.relics {
        Some("relics")
    } else if expected.potions != observed.potions {
        Some("potions")
    } else if expected.public_map != observed.public_map {
        Some("public_map")
    } else if expected.public_history.shop_purge_count != observed.public_history.shop_purge_count {
        Some("public_history.shop_purge_count")
    } else if expected.public_history.recent_combat_hp_loss
        != observed.public_history.recent_combat_hp_loss
    {
        Some("public_history.recent_combat_hp_loss")
    } else if expected.public_history.recent_combat_turns
        != observed.public_history.recent_combat_turns
    {
        Some("public_history.recent_combat_turns")
    } else if expected.public_history.recent_combat_potions_used
        != observed.public_history.recent_combat_potions_used
    {
        Some("public_history.recent_combat_potions_used")
    } else {
        None
    }
}

fn task_public_candidate_replay_incompatibility_v2(
    expected: &TaskPublicCandidateV1,
    observed: &TaskPublicCandidateV1,
) -> Option<&'static str> {
    if expected == observed {
        return None;
    }
    if let (
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseRouteNode {
                    x: expected_x,
                    y: expected_y,
                    flight: expected_flight,
                    question_mark_outcomes: expected_outcomes,
                },
        },
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseRouteNode {
                    x: observed_x,
                    y: observed_y,
                    flight: observed_flight,
                    question_mark_outcomes: observed_outcomes,
                },
        },
    ) = (expected, observed)
    {
        return if expected_x != observed_x {
            Some("strategic.choose_route.x")
        } else if expected_y != observed_y {
            Some("strategic.choose_route.y")
        } else if expected_flight != observed_flight {
            Some("strategic.choose_route.flight")
        } else if expected_outcomes != observed_outcomes {
            Some("strategic.choose_route.question_mark_outcomes")
        } else {
            Some("strategic.choose_route")
        };
    }
    let (
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    event: expected_event,
                    screen: expected_screen,
                    option_index: expected_option,
                    action: expected_action,
                    public_semantics: expected_semantics,
                },
        },
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    event: observed_event,
                    screen: observed_screen,
                    option_index: observed_option,
                    action: observed_action,
                    public_semantics: observed_semantics,
                },
        },
    ) = (expected, observed)
    else {
        return Some("kind_or_action");
    };
    if expected_event != observed_event {
        return Some("strategic.choose_event.event");
    }
    if expected_screen != observed_screen {
        return Some("strategic.choose_event.screen");
    }
    if expected_option != observed_option {
        return Some("strategic.choose_event.option_index");
    }
    if expected_action != observed_action {
        return Some("strategic.choose_event.action");
    }
    if !expected_semantics.specific_cards_gained.is_empty()
        && !observed_semantics.specific_cards_gained.is_empty()
        && expected_semantics.specific_cards_gained != observed_semantics.specific_cards_gained
    {
        return Some("strategic.choose_event.specific_cards_gained");
    }
    if !expected_semantics.specific_cards_removed.is_empty()
        && !observed_semantics.specific_cards_removed.is_empty()
        && expected_semantics.specific_cards_removed != observed_semantics.specific_cards_removed
    {
        return Some("strategic.choose_event.specific_cards_removed");
    }
    let legacy_random_outcome_enrichment = !expected_semantics.random_outcome
        && observed_semantics.random_outcome
        && expected_semantics.structured_semantics.is_none()
        && observed_semantics
            .structured_semantics
            .as_ref()
            .is_some_and(structured_event_mechanics_imply_random_outcome_v1);
    let mut expected_semantics = expected_semantics.clone();
    let mut observed_semantics = observed_semantics.clone();
    expected_semantics.specific_cards_gained.clear();
    observed_semantics.specific_cards_gained.clear();
    expected_semantics.specific_cards_removed.clear();
    observed_semantics.specific_cards_removed.clear();
    if expected_semantics.public_label.is_none() {
        observed_semantics.public_label = None;
    }
    if expected_semantics.structured_semantics.is_none() {
        observed_semantics.structured_semantics = None;
    }
    if legacy_random_outcome_enrichment {
        observed_semantics.random_outcome = false;
    }
    if expected_semantics.random_outcome != observed_semantics.random_outcome {
        return Some("strategic.choose_event.public_semantics.random_outcome");
    }
    (expected_semantics != observed_semantics).then_some("strategic.choose_event.public_semantics")
}

fn structured_event_mechanics_imply_random_outcome_v1(
    semantics: &crate::ai::planner_core::PlannerEventOptionStructuredSemanticsV1,
) -> bool {
    use crate::ai::planner_core::{
        PlannerEventCardPoolV1 as CardPool, PlannerEventPublicEffectV1 as Effect,
        PlannerEventRelicPoolV1 as RelicPool,
    };

    semantics.effects.iter().any(|effect| match effect {
        Effect::GainGoldRange { .. }
        | Effect::ObtainRandomPotions { .. }
        | Effect::TransformCard { .. }
        | Effect::RandomOutcome { .. } => true,
        Effect::ObtainRelic { pool, .. } => !matches!(pool, RelicPool::Specific { .. }),
        Effect::ObtainCard { pool, .. }
        | Effect::ObtainColorlessCard { pool, .. }
        | Effect::OfferCards { pool, .. }
        | Effect::ObtainCurse { pool, .. } => !matches!(pool, CardPool::Specific { .. }),
        _ => false,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPublicChoiceV1 {
    /// Ordinals are interpreted only against the bound public snapshot. The
    /// first ordinal selects the top-level candidate; later ordinals are an
    /// optional autoregressive payload owned by that candidate family.
    pub ordinal_path: Vec<usize>,
}

impl TaskPublicChoiceV1 {
    pub fn one(ordinal: usize) -> Self {
        Self {
            ordinal_path: vec![ordinal],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPublicStepV1 {
    pub source: TaskPublicDecisionV1,
    pub choice: TaskPublicChoiceV1,
    pub observed_boundary: TaskObservedBoundaryV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskObservedBoundaryV1 {
    Decision {
        decision: TaskPublicDecisionV1,
    },
    CommitReady,
    ExploredScopeExit {
        exit: super::TaskExploredScopeExitV1,
    },
    CommittedNextScope {
        decision: TaskPublicDecisionV1,
    },
    CommittedGameTerminal {
        game_outcome: GameOutcomeV1,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeAttemptStatusV1 {
    InProgress,
    AbandonedByReload,
    AwaitingCommit,
    SelectedAndCommitted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeAttemptV1 {
    pub attempt: TaskAttemptOrdinalV1,
    pub root: TaskPublicDecisionV1,
    pub steps: Vec<TaskPublicStepV1>,
    pub status: NodeAttemptStatusV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMemoryV1 {
    pub scope: TaskScopeOrdinalV1,
    pub entry_site: TaskDecisionSiteV1,
    pub attempts: Vec<NodeAttemptV1>,
}

impl NodeMemoryV1 {
    pub(super) fn new(scope: TaskScopeOrdinalV1, root: TaskPublicDecisionV1) -> Self {
        Self {
            scope,
            entry_site: root.site,
            attempts: vec![NodeAttemptV1 {
                attempt: TaskAttemptOrdinalV1(0),
                root,
                steps: Vec::new(),
                status: NodeAttemptStatusV1::InProgress,
            }],
        }
    }

    pub(super) fn current(&self) -> &NodeAttemptV1 {
        self.attempts
            .last()
            .expect("node memory always contains a current attempt")
    }

    pub(super) fn current_mut(&mut self) -> &mut NodeAttemptV1 {
        self.attempts
            .last_mut()
            .expect("node memory always contains a current attempt")
    }

    pub(super) fn begin_reloaded_attempt(&mut self, root: TaskPublicDecisionV1) {
        let ordinal = TaskAttemptOrdinalV1(self.attempts.len() as u32);
        self.attempts.push(NodeAttemptV1 {
            attempt: ordinal,
            root,
            steps: Vec::new(),
            status: NodeAttemptStatusV1::InProgress,
        });
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LegalKnowledgeLedgerV1 {
    pub completed_scopes: Vec<ArchivedNodeKnowledgeV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedNodeKnowledgeV1 {
    pub scope: TaskScopeOrdinalV1,
    pub entry_site: TaskDecisionSiteV1,
    pub attempts: Vec<NodeAttemptV1>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedPublicHistoryV1 {
    pub steps: Vec<CommittedPublicStepV1>,
    pub completed_scopes: Vec<CommittedScopeBoundaryV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedPublicStepV1 {
    pub scope: TaskScopeOrdinalV1,
    pub site: TaskDecisionSiteV1,
    pub step: TaskPublicStepV1,
}

/// One scope-addressed replay-observed public step. Unlike
/// `CommittedPublicStepV1`, this wrapper does not claim that its enclosing
/// scope was selected and committed; the run archive uses it for both the
/// committed path and reachable active-scope frontiers.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskScopedPublicStepV1 {
    pub scope: TaskScopeOrdinalV1,
    pub site: TaskDecisionSiteV1,
    pub step: TaskPublicStepV1,
}

impl From<CommittedPublicStepV1> for TaskScopedPublicStepV1 {
    fn from(step: CommittedPublicStepV1) -> Self {
        Self {
            scope: step.scope,
            site: step.site,
            step: step.step,
        }
    }
}

impl From<TaskScopedPublicStepV1> for CommittedPublicStepV1 {
    fn from(step: TaskScopedPublicStepV1) -> Self {
        Self {
            scope: step.scope,
            site: step.site,
            step: step.step,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedScopeBoundaryV1 {
    pub scope: TaskScopeOrdinalV1,
    pub entry_site: TaskDecisionSiteV1,
    pub exit: TaskCommittedScopeExitV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskCommittedScopeExitV1 {
    NextScope { decision: TaskPublicDecisionV1 },
    GameTerminal { game_outcome: GameOutcomeV1 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::planner_core::{PlannerEventOptionPublicSemanticsV1, PlannerOfferedCard};
    use crate::content::cards::CardId;
    use crate::state::events::{EventActionKind, EventId};

    fn library_candidate(
        card: Option<CardId>,
        public_label: Option<&str>,
    ) -> TaskPublicCandidateV1 {
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::ChooseEventOption {
                event: EventId::TheLibrary,
                screen: 1,
                option_index: 0,
                action: EventActionKind::Gain,
                public_semantics: PlannerEventOptionPublicSemanticsV1 {
                    public_label: public_label.map(str::to_string),
                    cards_gained: 1,
                    specific_cards_gained: card
                        .map(|card| PlannerOfferedCard { card, upgrades: 0 })
                        .into_iter()
                        .collect(),
                    ..PlannerEventOptionPublicSemanticsV1::default()
                },
            },
        }
    }

    fn event_semantics_mut(
        candidate: &mut TaskPublicCandidateV1,
    ) -> &mut PlannerEventOptionPublicSemanticsV1 {
        let TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    public_semantics, ..
                },
        } = candidate
        else {
            panic!("event fixture");
        };
        public_semantics
    }

    #[test]
    fn event_card_identity_allows_old_missing_evidence_but_not_conflicting_cards() {
        let legacy = library_candidate(None, None);
        let shrug = library_candidate(Some(CardId::ShrugItOff), None);
        let same_shrug = library_candidate(Some(CardId::ShrugItOff), None);
        let pommel = library_candidate(Some(CardId::PommelStrike), None);

        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&legacy, &shrug),
            None
        );
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&shrug, &legacy),
            None
        );
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&shrug, &same_shrug),
            None
        );
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&shrug, &pommel),
            Some("strategic.choose_event.specific_cards_gained")
        );
    }

    #[test]
    fn event_public_label_is_legacy_optional_but_exact_once_recorded() {
        let legacy = library_candidate(Some(CardId::ShrugItOff), None);
        let labelled = library_candidate(Some(CardId::ShrugItOff), Some("Take Shrug It Off."));
        let conflicting =
            library_candidate(Some(CardId::ShrugItOff), Some("Take a different card."));

        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&legacy, &labelled),
            None
        );
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&labelled, &legacy),
            Some("strategic.choose_event.public_semantics")
        );
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&labelled, &conflicting),
            Some("strategic.choose_event.public_semantics")
        );
    }

    #[test]
    fn event_structured_mechanics_are_legacy_optional_but_exact_once_recorded() {
        use crate::ai::planner_core::{
            PlannerEventOptionStructuredSemanticsV1, PlannerEventPublicEffectV1,
            PlannerEventTransitionV1,
        };

        let legacy = library_candidate(Some(CardId::ShrugItOff), None);
        let mut current = legacy.clone();
        let mut conflicting = legacy.clone();
        let structured = PlannerEventOptionStructuredSemanticsV1 {
            effects: vec![PlannerEventPublicEffectV1::GainGold { amount: 10 }],
            transition: PlannerEventTransitionV1::Complete,
        };
        let conflict = PlannerEventOptionStructuredSemanticsV1 {
            effects: vec![PlannerEventPublicEffectV1::GainGold { amount: 11 }],
            transition: PlannerEventTransitionV1::Complete,
        };
        for (candidate, mechanics) in [(&mut current, structured), (&mut conflicting, conflict)] {
            event_semantics_mut(candidate).structured_semantics = Some(mechanics);
        }

        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&legacy, &current),
            None
        );
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&current, &legacy),
            Some("strategic.choose_event.public_semantics")
        );
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&current, &conflicting),
            Some("strategic.choose_event.public_semantics")
        );
    }

    #[test]
    fn legacy_random_flag_enrichment_requires_a_typed_random_source() {
        use crate::ai::planner_core::{
            PlannerEventOptionStructuredSemanticsV1, PlannerEventPublicEffectV1,
            PlannerEventRelicPoolV1, PlannerEventTransitionV1,
        };

        let mut legacy = library_candidate(Some(CardId::ShrugItOff), None);
        event_semantics_mut(&mut legacy).relics_gained = 1;
        let mut changed_randomness = legacy.clone();
        event_semantics_mut(&mut changed_randomness).random_outcome = true;
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&legacy, &changed_randomness),
            Some("strategic.choose_event.public_semantics.random_outcome")
        );

        let mut enriched_random_relic = legacy.clone();
        let public_semantics = event_semantics_mut(&mut enriched_random_relic);
        public_semantics.relics_gained = 1;
        public_semantics.random_outcome = true;
        public_semantics.structured_semantics = Some(PlannerEventOptionStructuredSemanticsV1 {
            effects: vec![PlannerEventPublicEffectV1::ObtainRelic {
                count: 1,
                pool: PlannerEventRelicPoolV1::RandomRelic,
            }],
            transition: PlannerEventTransitionV1::AdvanceScreen,
        });
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&legacy, &enriched_random_relic),
            None,
            "a typed random source may enrich only a pre-structured legacy false flag"
        );

        let mut deterministic_relic = enriched_random_relic.clone();
        event_semantics_mut(&mut deterministic_relic).structured_semantics =
            Some(PlannerEventOptionStructuredSemanticsV1 {
                effects: vec![PlannerEventPublicEffectV1::ObtainRelic {
                    count: 1,
                    pool: PlannerEventRelicPoolV1::Specific {
                        relic: crate::content::relics::RelicId::Anchor,
                    },
                }],
                transition: PlannerEventTransitionV1::AdvanceScreen,
            });
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&legacy, &deterministic_relic),
            Some("strategic.choose_event.public_semantics.random_outcome"),
            "a deterministic typed effect cannot excuse a changed randomness flag"
        );

        let mut recorded_random_relic = enriched_random_relic.clone();
        event_semantics_mut(&mut recorded_random_relic).random_outcome = false;
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(
                &recorded_random_relic,
                &enriched_random_relic,
            ),
            Some("strategic.choose_event.public_semantics.random_outcome"),
            "once structured mechanics were recorded the coarse flag remains exact"
        );

        let mut changed_relic_count = enriched_random_relic.clone();
        event_semantics_mut(&mut changed_relic_count).relics_gained = 2;
        assert_eq!(
            task_public_candidate_replay_incompatibility_v2(&legacy, &changed_relic_count),
            Some("strategic.choose_event.public_semantics"),
            "the migration must not hide any independent coarse semantic change"
        );
    }
}
