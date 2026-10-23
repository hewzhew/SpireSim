use serde::{Deserialize, Serialize};
use sts_combat_planner::{
    CombatDecisionRoot, CombatPlanningCounters, CombatPlanningQuantum, CompleteTurnOptionBoundary,
    ExactAtomicWitness, SelectionSource, TurnOptionGenerationReport, TurnOptionGenerationStatus,
    TurnOptionGeneratorConfig,
};
#[cfg(test)]
use sts_complete_turn_env::CombatDraftEnvStatusV1;
use sts_complete_turn_env::CombatDraftEnvV1;
use sts_complete_turn_view::{CombatDraftSelectionMemberV1, CombatDraftSelectionViewV1};
use sts_oracle_learning::agent::task::{
    TaskPrivilegedSupportAccessV1, TaskPrivilegedSupportProvenanceV1,
    TaskResearchPlanAttemptExitV1, TaskResearchPlanAttemptV1, TaskResearchPlanCandidateV1,
    TaskResearchPlanLaneAttemptV1, TaskResearchPlanStepV1, TaskResearchPlanV1,
    TaskResearchPlanWorkUnitV1,
};
use sts_oracle_learning::sim::combat::CombatPosition;

use crate::driver::{RunPlayCombatResearchAttemptV1, RunPlayControllerV1};

mod opening;
mod reading;
mod selection_reading;
use reading::DraftReadingV1;

fn message(locale: sts_presentation::Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("draft", key, parameters)
        .expect("combat draft message")
}

const MAX_GENERATION_WORK: usize = 65_536;
const MAX_ENGINE_STEPS: usize = 33_554_432;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCombatDraftGenerationV1 {
    pub status: &'static str,
    pub generation_work_consumed: usize,
    pub engine_steps_consumed: usize,
    pub newly_completed_options: usize,
    pub total_completed_options: usize,
    pub retained_work_items: usize,
    pub mechanics_gap_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCombatDraftStatusV1 {
    pub draft_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_ref: Option<String>,
    pub durable_head_unchanged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_head_revision: Option<u64>,
    #[serde(flatten)]
    pub reading: DraftReadingV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_candidate_ref: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCombatDraftAdvancedV1 {
    pub frame_ref: String,
    pub root_head_revision: u64,
    pub durable_head_unchanged: bool,
    pub new_draft: bool,
    pub generation: AgentCombatDraftGenerationV1,
    #[serde(flatten)]
    pub reading: DraftReadingV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCombatDraftInspectedV1 {
    pub frame_ref: String,
    pub root_head_revision: u64,
    pub durable_head_unchanged: bool,
    pub option_ref: String,
    pub view: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCombatDraftChosenV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_ref: Option<String>,
    pub root_head_revision: u64,
    pub durable_head_unchanged: bool,
    pub option_ref: String,
    pub boundary: &'static str,
    #[serde(flatten)]
    pub reading: DraftReadingV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_candidate_ref: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCombatDraftUndoneV1 {
    pub frame_ref: String,
    pub root_head_revision: u64,
    pub durable_head_unchanged: bool,
    #[serde(flatten)]
    pub reading: DraftReadingV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCombatDraftSelectionEditV1 {
    pub source_ref: String,
    pub preferences: Vec<CombatDraftSelectionMemberV1>,
}

impl AgentCombatDraftSelectionEditV1 {
    pub(crate) fn validate(&self, locale: sts_presentation::Locale) -> Result<(), String> {
        let digits = self
            .source_ref
            .strip_prefix('S')
            .ok_or_else(|| message(locale, "sourcePrefix", &[]))?;
        if digits.is_empty()
            || !digits.bytes().all(|b| b.is_ascii_digit())
            || digits.len() > 20
            || (digits.len() > 1 && digits.starts_with('0'))
        {
            return Err(message(locale, "sourceNumber", &[]));
        }
        if self.preferences.len() > sts_combat_planner::CombatSelectionPreferences::LIMIT {
            return Err(message(
                locale,
                "preferenceLimit",
                &[(
                    "max",
                    &(sts_combat_planner::CombatSelectionPreferences::LIMIT.to_string()),
                )],
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_selection_request(
    edit: Option<&AgentCombatDraftSelectionEditV1>,
    source_ref: Option<&str>,
    offset: Option<usize>,
    locale: sts_presentation::Locale,
) -> Result<(), String> {
    if edit.is_some() && (source_ref.is_some() || offset.is_some()) {
        return Err(message(locale, "editConflict", &[]));
    }
    if let Some(edit) = edit {
        edit.validate(locale)?;
    }
    if let Some(name) = source_ref {
        AgentCombatDraftSelectionEditV1 {
            source_ref: name.to_owned(),
            preferences: Vec::new(),
        }
        .validate(locale)?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct AgentCombatDraftSelectionV1 {
    pub frame_ref: String,
    pub root_head_revision: u64,
    pub durable_head_unchanged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
    #[serde(flatten)]
    pub reading: selection_reading::SelectionReadingV1,
}

/// Transport-local names for exact core sources. Clearing a draft expires
/// its token but does not recycle the number within this shell connection.
#[derive(Default)]
struct SelectionSourceBinding {
    entries: std::collections::VecDeque<(String, SelectionSource)>,
    next: u64,
}

impl SelectionSourceBinding {
    fn observe(&mut self, source: SelectionSource) -> Result<String, String> {
        if let Some(index) = self.entries.iter().position(|(_, bound)| *bound == source) {
            let entry = self.entries.remove(index).expect("observed entry");
            let name = entry.0.clone();
            self.entries.push_back(entry);
            return Ok(name);
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or("selection source identifiers exhausted")?;
        let name = format!("S{}", self.next);
        self.next = next;
        if self.entries.len() == 32 {
            self.entries.pop_front();
        }
        self.entries.push_back((name.clone(), source));
        Ok(name)
    }

    fn resolve(
        &self,
        name: &str,
        locale: sts_presentation::Locale,
    ) -> Result<SelectionSource, String> {
        self.entries
            .iter()
            .find(|(observed, _)| observed == name)
            .map(|(_, source)| source.clone())
            .ok_or_else(|| message(locale, "sourceExpired", &[]))
    }
}

pub(crate) struct AgentCombatDraftChoiceV1 {
    pub response: AgentCombatDraftChosenV1,
    pub commit_attempt: Option<RunPlayCombatResearchAttemptV1>,
}

/// Process-local owner for one analyst-selected combat draft.
///
/// The durable run remains owned by `RunPlayControllerV1`. This value retains
/// only one draft environment plus the identities needed to reject use after
/// the controller moves to another exact root.
pub(crate) struct AgentCombatDraftV1 {
    root_head_revision: u64,
    root_exact_state_hash: String,
    env: CombatDraftEnvV1,
    consumed: CombatPlanningCounters,
}

impl AgentCombatDraftV1 {
    pub(crate) fn new(
        root_head_revision: u64,
        position: CombatPosition,
        config: TurnOptionGeneratorConfig,
    ) -> Result<Self, String> {
        let root = CombatDecisionRoot::new(position.clone()).map_err(|error| {
            format!("agent combat draft requires a stable combat root: {error:?}")
        })?;
        Ok(Self {
            root_head_revision,
            root_exact_state_hash: root.exact_state_hash().to_owned(),
            env: CombatDraftEnvV1::from_position(position, config)
                .map_err(|error| error.to_string())?,
            consumed: CombatPlanningCounters::default(),
        })
    }

    pub(crate) fn root_head_revision(&self) -> u64 {
        self.root_head_revision
    }

    pub(crate) fn consumed(&self) -> CombatPlanningCounters {
        self.consumed
    }

    fn frame_ref(&self) -> Option<String> {
        self.env
            .current_frame_handle()
            .map(|frame| frame.to_string())
    }

    fn active_frame_ref(&self) -> String {
        self.frame_ref()
            .expect("an active draft has a frame reference")
    }

    /// Verifies both the durable revision and the private exact combat root.
    /// Exact identities never leave this process or enter the model surface.
    pub(crate) fn verify_root(
        &self,
        current_head_revision: u64,
        current_position: CombatPosition,
    ) -> Result<(), String> {
        if current_head_revision != self.root_head_revision {
            return Err(format!(
                "combat draft belongs to head {}, current head is {current_head_revision}",
                self.root_head_revision
            ));
        }
        let current = CombatDecisionRoot::new(current_position).map_err(|_| {
            "combat draft root is no longer an unresolved stable player turn".to_owned()
        })?;
        if current.exact_state_hash() != self.root_exact_state_hash {
            return Err(
                "combat draft exact root changed while its durable head stayed fixed".to_owned(),
            );
        }
        Ok(())
    }

    /// Forwards exactly one caller-supplied quantum. There is deliberately no
    /// retry, wait-until-option, or wait-until-complete loop in this owner.
    pub(crate) fn advance(
        &mut self,
        quantum: CombatPlanningQuantum,
    ) -> Result<TurnOptionGenerationReport, String> {
        let advanced = self
            .env
            .advance(quantum)
            .map_err(|error| error.to_string())?;
        self.consumed.generation_work = self.consumed.generation_work.saturating_add(
            advanced
                .generation
                .after
                .generation_work
                .saturating_sub(advanced.generation.before.generation_work),
        );
        self.consumed.engine_steps = self.consumed.engine_steps.saturating_add(
            advanced
                .generation
                .after
                .engine_steps
                .saturating_sub(advanced.generation.before.engine_steps),
        );
        Ok(advanced.generation)
    }

    #[cfg(test)]
    pub(crate) fn render_status(&self) -> Result<String, String> {
        match self.env.status() {
            CombatDraftEnvStatusV1::Active { .. } => self
                .env
                .render_observation()
                .map_err(|error| error.to_string()),
            CombatDraftEnvStatusV1::Terminal {
                selected_turn_count,
                boundary,
            } => Ok(format!(
                "combat-draft-v1\nselected_turns={selected_turn_count} terminal={}\n",
                boundary_token(boundary)
            )),
        }
    }

    pub(crate) fn verify_frame_ref(
        &self,
        expected: &str,
        locale: sts_presentation::Locale,
    ) -> Result<(), String> {
        match self.env.current_frame_handle() {
            Some(frame) if frame.to_string() == expected => Ok(()),
            Some(frame) => Err(message(
                locale,
                "wrongFrame",
                &[("frame", &(frame.to_string()))],
            )),
            None => Err(message(locale, "terminalOptions", &[])),
        }
    }

    pub(crate) fn choose(
        &mut self,
        option_ref: &str,
    ) -> Result<CompleteTurnOptionBoundary, String> {
        self.env
            .choose(option_ref)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn undo(&mut self) -> Result<(), String> {
        self.env.undo().map_err(|error| error.to_string())
    }

    pub(crate) fn selection_view(&self) -> Result<CombatDraftSelectionViewV1, String> {
        self.env.selection_view().map_err(|error| error.to_string())
    }

    fn selection_source(&self) -> Result<SelectionSource, String> {
        self.env
            .root_selection_intent()
            .map(|intent| intent.source)
            .ok_or_else(|| "combat draft is terminal".to_owned())
    }

    fn replace_selection(
        &mut self,
        source: &SelectionSource,
        preferences: &[CombatDraftSelectionMemberV1],
    ) -> Result<CombatDraftSelectionViewV1, String> {
        self.env
            .replace_selection_preferences(source, preferences)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn victory_witness(&self) -> Result<Option<ExactAtomicWitness>, String> {
        self.env
            .victory_witness()
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    pub(crate) fn test_env(&self) -> &CombatDraftEnvV1 {
        &self.env
    }
}

#[derive(Default)]
pub(crate) struct AgentCombatDraftRuntimeV1 {
    draft: Option<AgentCombatDraftV1>,
    selection_binding: SelectionSourceBinding,
}

impl AgentCombatDraftRuntimeV1 {
    pub(crate) fn clear(&mut self) {
        self.draft = None;
        self.selection_binding.entries.clear();
    }

    pub(crate) fn status(
        &mut self,
        controller: &mut RunPlayControllerV1,
        commit_ready: bool,
        locale: sts_presentation::Locale,
    ) -> Result<AgentCombatDraftStatusV1, String> {
        if self.draft.is_none() {
            return Ok(AgentCombatDraftStatusV1 {
                draft_present: false,
                frame_ref: None,
                durable_head_unchanged: true,
                root_head_revision: None,
                reading: DraftReadingV1 {
                    view: message(locale, "notStarted", &[]),
                    options: vec![],
                },
                commit_candidate_ref: None,
            });
        }
        self.verify_or_invalidate(controller, locale)?;
        let draft = self.draft.as_ref().expect("verified draft remains present");
        Ok(AgentCombatDraftStatusV1 {
            draft_present: true,
            frame_ref: draft.frame_ref(),
            durable_head_unchanged: true,
            root_head_revision: Some(draft.root_head_revision()),
            reading: reading::status(&draft.env, locale)?,
            commit_candidate_ref: commit_ready.then_some("c0"),
        })
    }

    pub(crate) fn advance(
        &mut self,
        controller: &mut RunPlayControllerV1,
        generation_work: usize,
        engine_steps: usize,
        locale: sts_presentation::Locale,
    ) -> Result<AgentCombatDraftAdvancedV1, String> {
        validate_quantum(generation_work, engine_steps, locale)?;
        let new_draft = self.draft.is_none();
        if new_draft {
            let root_head_revision = controller.head().head_revision;
            let max_engine_steps_per_transition = controller
                .head()
                .combat_plan_options
                .max_engine_steps_per_transition;
            let position = controller.current_combat_position_for_lab_v1()?;
            self.draft = Some(AgentCombatDraftV1::new(
                root_head_revision,
                position,
                TurnOptionGeneratorConfig {
                    max_engine_steps_per_transition,
                    allow_potion_discard: false,
                    ..TurnOptionGeneratorConfig::default()
                },
            )?);
        } else {
            self.verify_or_invalidate(controller, locale)?;
        }
        let draft = self
            .draft
            .as_mut()
            .expect("combat draft was created or verified above");
        let root_head_revision = draft.root_head_revision();
        let report = draft.advance(CombatPlanningQuantum::deterministic(
            generation_work,
            engine_steps,
        ))?;
        let response = AgentCombatDraftAdvancedV1 {
            frame_ref: draft.active_frame_ref(),
            root_head_revision,
            durable_head_unchanged: true,
            new_draft,
            generation: project_generation(&report),
            reading: reading::status(&draft.env, locale)?,
        };
        if controller.head().head_revision != root_head_revision {
            self.clear();
            return Err("combat draft advance changed the durable controller head".to_owned());
        }
        Ok(response)
    }

    pub(crate) fn inspect(
        &mut self,
        controller: &mut RunPlayControllerV1,
        frame_ref: &str,
        option_ref: &str,
        locale: sts_presentation::Locale,
    ) -> Result<AgentCombatDraftInspectedV1, String> {
        validate_option_ref(option_ref, locale)?;
        self.verify_or_invalidate(controller, locale)?;
        let draft = self.draft.as_ref().expect("verified draft remains present");
        draft.verify_frame_ref(frame_ref, locale)?;
        Ok(AgentCombatDraftInspectedV1 {
            frame_ref: draft.active_frame_ref(),
            root_head_revision: draft.root_head_revision(),
            durable_head_unchanged: true,
            option_ref: option_ref.to_owned(),
            view: reading::inspect(&draft.env, option_ref, locale)?,
        })
    }

    pub(crate) fn selection(
        &mut self,
        controller: &mut RunPlayControllerV1,
        edit: Option<&AgentCombatDraftSelectionEditV1>,
        source_ref: Option<&str>,
        offset: Option<usize>,
        locale: sts_presentation::Locale,
    ) -> Result<AgentCombatDraftSelectionV1, String> {
        validate_selection_request(edit, source_ref, offset, locale)?;
        self.verify_or_invalidate(controller, locale)?;
        self.selection_at_verified_root(edit, source_ref, offset, locale)
    }

    fn selection_at_verified_root(
        &mut self,
        edit: Option<&AgentCombatDraftSelectionEditV1>,
        source_ref: Option<&str>,
        offset: Option<usize>,
        locale: sts_presentation::Locale,
    ) -> Result<AgentCombatDraftSelectionV1, String> {
        validate_selection_request(edit, source_ref, offset, locale)?;
        let draft = self.draft.as_mut().expect("verified draft remains present");
        let requested = edit.map(|edit| edit.source_ref.as_str()).or(source_ref);
        let source = if let Some(name) = requested {
            Some(self.selection_binding.resolve(name, locale)?)
        } else if offset.is_none() && !draft.selection_view()?.families.is_empty() {
            Some(draft.selection_source()?)
        } else {
            None
        };
        let mut next_offset = None;
        let (source_ref, reading) = if let Some(source) = source {
            // Validate the source and any prefix replay before the atomic edit.
            let context = if edit.is_none() && offset.is_none() {
                Some(
                    draft
                        .env
                        .selection_context(&source)
                        .map_err(|e| e.to_string())?,
                )
            } else {
                None
            };
            let remaining = draft
                .env
                .selection_continuation_work(&source)
                .map_err(|e| e.to_string())?;
            let view = if let Some(edit) = edit {
                draft.replace_selection(&source, &edit.preferences)?
            } else {
                draft
                    .env
                    .selection_view_at(&source)
                    .map_err(|e| e.to_string())?
            };
            let outcomes = draft
                .env
                .selection_outcomes(&source, offset.unwrap_or(0))
                .map_err(|e| e.to_string())?;
            next_offset = outcomes.next_offset;
            let reading = selection_reading::focus(view, outcomes, context, remaining, locale)?;
            (Some(self.selection_binding.observe(source)?), reading)
        } else {
            let offset = offset.unwrap_or(0);
            let page = draft
                .env
                .selection_sources(offset, 8)
                .map_err(|e| e.to_string())?;
            let mut sources = Vec::new();
            for source in page.sources {
                let label = draft
                    .env
                    .selection_summary(&source, locale)
                    .map_err(|e| e.to_string())?;
                let action_count = source.action_count();
                sources.push(selection_reading::SelectionSourceV1 {
                    source_ref: self.selection_binding.observe(source)?,
                    label,
                    action_count,
                });
            }
            if offset.saturating_add(8) < page.total {
                next_offset = Some(offset + 8);
            }
            (
                None,
                selection_reading::index(sources, page.total, offset, locale),
            )
        };
        Ok(AgentCombatDraftSelectionV1 {
            frame_ref: draft.active_frame_ref(),
            root_head_revision: draft.root_head_revision(),
            durable_head_unchanged: true,
            source_ref,
            next_offset,
            reading,
        })
    }

    pub(crate) fn choose(
        &mut self,
        controller: &mut RunPlayControllerV1,
        frame_ref: &str,
        option_ref: &str,
        locale: sts_presentation::Locale,
    ) -> Result<AgentCombatDraftChoiceV1, String> {
        validate_option_ref(option_ref, locale)?;
        self.verify_or_invalidate(controller, locale)?;
        let (root_head_revision, boundary, reading, witness, consumed) = {
            let draft = self.draft.as_mut().expect("verified draft remains present");
            draft.verify_frame_ref(frame_ref, locale)?;
            let boundary = draft.choose(option_ref)?;
            self.selection_binding.entries.clear();
            let witness = if boundary == CompleteTurnOptionBoundary::TerminalWin {
                Some(draft.victory_witness()?.ok_or_else(|| {
                    "terminal winning combat draft did not export a victory witness".to_owned()
                })?)
            } else {
                None
            };
            (
                draft.root_head_revision(),
                boundary,
                reading::status(&draft.env, locale)?,
                witness,
                draft.consumed(),
            )
        };
        let commit_attempt = witness
            .as_ref()
            .map(|witness| prepare_commit_attempt(controller, consumed, witness))
            .transpose()?;
        if controller.head().head_revision != root_head_revision {
            self.clear();
            return Err("combat draft selection changed the durable controller head".to_owned());
        }
        Ok(AgentCombatDraftChoiceV1 {
            response: AgentCombatDraftChosenV1 {
                frame_ref: self
                    .draft
                    .as_ref()
                    .expect("selected draft remains present")
                    .frame_ref(),
                root_head_revision,
                durable_head_unchanged: true,
                option_ref: option_ref.to_owned(),
                boundary: boundary_token(boundary),
                reading,
                commit_candidate_ref: commit_attempt.as_ref().map(|_| "c0"),
            },
            commit_attempt,
        })
    }

    pub(crate) fn undo(
        &mut self,
        controller: &mut RunPlayControllerV1,
        locale: sts_presentation::Locale,
    ) -> Result<AgentCombatDraftUndoneV1, String> {
        self.verify_or_invalidate(controller, locale)?;
        let draft = self.draft.as_mut().expect("verified draft remains present");
        draft.undo()?;
        self.selection_binding.entries.clear();
        Ok(AgentCombatDraftUndoneV1 {
            frame_ref: draft.active_frame_ref(),
            root_head_revision: draft.root_head_revision(),
            durable_head_unchanged: true,
            reading: reading::status(&draft.env, locale)?,
        })
    }

    fn verify_or_invalidate(
        &mut self,
        controller: &mut RunPlayControllerV1,
        locale: sts_presentation::Locale,
    ) -> Result<(), String> {
        let Some(draft) = self.draft.as_ref() else {
            return Err(message(locale, "needAdvance", &[]));
        };
        let current_head_revision = controller.head().head_revision;
        let current_position = match controller.current_combat_position_for_lab_v1() {
            Ok(position) => position,
            Err(error) => {
                self.clear();
                return Err(message(
                    locale,
                    "invalidated",
                    &[("error", &(error.to_string()))],
                ));
            }
        };
        if let Err(error) = draft.verify_root(current_head_revision, current_position) {
            self.clear();
            return Err(message(
                locale,
                "invalidated",
                &[("error", &(error.to_string()))],
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_quantum(
    generation_work: usize,
    engine_steps: usize,
    locale: sts_presentation::Locale,
) -> Result<(), String> {
    if generation_work > MAX_GENERATION_WORK {
        return Err(message(
            locale,
            "workLimit",
            &[
                ("actual", &(generation_work.to_string())),
                ("max", &(MAX_GENERATION_WORK.to_string())),
            ],
        ));
    }
    if engine_steps > MAX_ENGINE_STEPS {
        return Err(message(
            locale,
            "engineLimit",
            &[
                ("actual", &(engine_steps.to_string())),
                ("max", &(MAX_ENGINE_STEPS.to_string())),
            ],
        ));
    }
    Ok(())
}

pub(crate) fn validate_option_ref(
    option_ref: &str,
    locale: sts_presentation::Locale,
) -> Result<(), String> {
    if option_ref.trim().is_empty() || option_ref.len() > 32 {
        return Err(message(locale, "optionRef", &[]));
    }
    Ok(())
}

fn project_generation(report: &TurnOptionGenerationReport) -> AgentCombatDraftGenerationV1 {
    let status = match report.status {
        TurnOptionGenerationStatus::Complete => "complete",
        TurnOptionGenerationStatus::PlanPrefixServiced => "plan_prefix_serviced",
        TurnOptionGenerationStatus::Partial(_) => "partial",
        TurnOptionGenerationStatus::PartialWithMechanicsGaps => "mechanics_gap",
    };
    AgentCombatDraftGenerationV1 {
        status,
        generation_work_consumed: report
            .after
            .generation_work
            .saturating_sub(report.before.generation_work),
        engine_steps_consumed: report
            .after
            .engine_steps
            .saturating_sub(report.before.engine_steps),
        newly_completed_options: report.newly_completed_options,
        total_completed_options: report.total_completed_options,
        retained_work_items: report.retained_work_items,
        mechanics_gap_count: report.gaps.len(),
    }
}

pub(crate) fn prepare_commit_attempt(
    controller: &RunPlayControllerV1,
    consumed: CombatPlanningCounters,
    witness: &ExactAtomicWitness,
) -> Result<RunPlayCombatResearchAttemptV1, String> {
    let inputs = witness
        .actions
        .iter()
        .map(|action| action.input.clone())
        .collect::<Vec<_>>();
    let provenance = TaskPrivilegedSupportProvenanceV1 {
        access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
        producer_name: "analyst_combat_draft".to_owned(),
        producer_revision: 1,
    };
    let (probe, outcome) = controller.exact_combat_replay_probe_v1(&inputs, provenance.clone())?;
    let first = probe
        .steps
        .first()
        .ok_or_else(|| "analyst combat draft produced an empty exact replay".to_owned())?;
    let candidate = TaskResearchPlanCandidateV1 {
        plan: TaskResearchPlanV1 {
            root_public_snapshot_id: first.source_decision.information.snapshot_id.clone(),
            steps: probe
                .steps
                .iter()
                .map(|step| TaskResearchPlanStepV1 {
                    source_public_snapshot_id: step.source_decision.information.snapshot_id.clone(),
                    choice: step.choice.clone(),
                })
                .collect(),
        },
        outcome,
        satisfies_requested_quality: true,
        supporting_provenances: vec![provenance.clone()],
    };
    let attempt = TaskResearchPlanAttemptV1 {
        candidates: vec![candidate],
        lanes: vec![TaskResearchPlanLaneAttemptV1 {
            provenance,
            work_unit: TaskResearchPlanWorkUnitV1::TurnGraphGenerationWork,
            work_consumed: u64::try_from(consumed.generation_work).unwrap_or(u64::MAX),
            exit: TaskResearchPlanAttemptExitV1::Witness,
        }],
    };
    attempt.validate()?;
    Ok(RunPlayCombatResearchAttemptV1 {
        attempt,
        witness_lines: vec![probe],
        milestone_candidates: Vec::new(),
        bounded_progress: Vec::new(),
    })
}

pub(crate) fn boundary_token(boundary: CompleteTurnOptionBoundary) -> &'static str {
    match boundary {
        CompleteTurnOptionBoundary::NextPlayerTurn => "next_player_turn",
        CompleteTurnOptionBoundary::TerminalWin => "terminal_win",
        CompleteTurnOptionBoundary::TerminalLoss => "terminal_loss",
        CompleteTurnOptionBoundary::Escape => "escape",
    }
}

#[cfg(test)]
#[path = "agent_combat_draft_selection_tests.rs"]
mod selection_tests;
