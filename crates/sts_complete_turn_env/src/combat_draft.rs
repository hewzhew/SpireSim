use std::fmt;

use sts_agent::agent::information::action::{
    resolve_public_combat_action_v1, PublicCombatActionChoiceV1,
};

use sts_combat_planner::{
    CombatDecisionRoot, CombatDecisionRootError, CombatDraftError, CombatDraftOptionCoverage,
    CombatDraftOptionHandle, CombatDraftSession, CombatPlanningQuantum, CompleteTurnOptionBoundary,
    ExactAtomicWitness, SelectionIntent, SelectionSource, TurnOptionGenerationReport,
    TurnOptionGeneratorConfig,
};
use sts_complete_turn_view::{
    project_combat_draft_option_v1, project_combat_draft_selection_v1,
    project_combat_draft_turn_v1, render_combat_draft_option_v1, render_combat_draft_turn_v1,
    CombatDraftOptionViewV1, CombatDraftSelectionMemberV1, CombatDraftSelectionViewV1,
    CombatDraftTurnViewV1,
};
use sts_core::sim::combat::{CombatPosition, EngineCombatStepper};
use sts_core::state::core::ClientInput;

/// Result of exactly one caller-budgeted generation quantum.
///
/// The view is taken immediately after that single advance. No internal loop
/// waits for the first option, complete coverage, or a wall-clock deadline.
#[derive(Clone, Debug)]
pub struct CombatDraftAdvanceV1 {
    pub generation: TurnOptionGenerationReport,
    pub view: CombatDraftTurnViewV1,
}

/// Cheap lifecycle state that never generates or replays an option.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatDraftEnvStatusV1 {
    Active {
        selected_turn_count: usize,
        available_option_count: usize,
        coverage: CombatDraftOptionCoverage,
    },
    Terminal {
        selected_turn_count: usize,
        boundary: CompleteTurnOptionBoundary,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CombatDraftEnvErrorV1 {
    InvalidOptionLabel { label: String },
    InvalidRoot(CombatDecisionRootError),
    Draft(CombatDraftError),
    View(String),
}

impl fmt::Display for CombatDraftEnvErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptionLabel { label } => write!(
                formatter,
                "invalid combat draft option label {label:?}; expected canonical T0, T1, ..."
            ),
            Self::InvalidRoot(error) => write!(formatter, "invalid combat draft root: {error:?}"),
            Self::Draft(CombatDraftError::UnknownOption { ordinal }) => {
                write!(
                    formatter,
                    "current combat draft turn has no option T{ordinal}"
                )
            }
            Self::Draft(CombatDraftError::StaleOptionHandle) => {
                formatter.write_str("combat draft option belongs to a previous turn frame")
            }
            Self::Draft(error) => write!(formatter, "combat draft operation failed: {error:?}"),
            Self::View(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for CombatDraftEnvErrorV1 {}

impl From<CombatDecisionRootError> for CombatDraftEnvErrorV1 {
    fn from(error: CombatDecisionRootError) -> Self {
        Self::InvalidRoot(error)
    }
}

impl From<CombatDraftError> for CombatDraftEnvErrorV1 {
    fn from(error: CombatDraftError) -> Self {
        Self::Draft(error)
    }
}

/// Thin in-process adapter over one analyst-owned combat draft.
///
/// It owns no second combat state and introduces no search policy. Typed
/// handles from observations preserve frame identity. Short labels such as
/// `T1` are a human-facing convenience and always resolve against the current
/// observed turn only.
pub struct CombatDraftEnvV1 {
    draft: CombatDraftSession,
    max_engine_steps_per_action: usize,
}

impl CombatDraftEnvV1 {
    pub fn from_exact_prefix(
        position: CombatPosition,
        config: TurnOptionGeneratorConfig,
        prefix: Vec<sts_combat_planner::TurnOptionAction>,
    ) -> Result<Self, CombatDraftEnvErrorV1> {
        let root = CombatDecisionRoot::new(position)?;
        Ok(Self {
            draft: CombatDraftSession::from_exact_prefix(
                root,
                config,
                prefix,
                &EngineCombatStepper,
            )?,
            max_engine_steps_per_action: config.max_engine_steps_per_transition,
        })
    }

    pub fn from_position(
        position: CombatPosition,
        config: TurnOptionGeneratorConfig,
    ) -> Result<Self, CombatDraftEnvErrorV1> {
        let max_engine_steps_per_action = config.max_engine_steps_per_transition;
        let root = CombatDecisionRoot::new(position)?;
        Ok(Self {
            draft: CombatDraftSession::new(root, config),
            max_engine_steps_per_action,
        })
    }

    pub fn status(&self) -> CombatDraftEnvStatusV1 {
        if let Some(boundary) = self.draft.terminal_boundary() {
            return CombatDraftEnvStatusV1::Terminal {
                selected_turn_count: self.draft.selected_turn_count(),
                boundary,
            };
        }
        CombatDraftEnvStatusV1::Active {
            selected_turn_count: self.draft.selected_turn_count(),
            available_option_count: self.draft.option_handles().len(),
            coverage: self
                .draft
                .current_option_coverage()
                .expect("a non-terminal draft has option coverage"),
        }
    }

    pub fn observe(&self) -> Result<CombatDraftTurnViewV1, CombatDraftEnvErrorV1> {
        project_combat_draft_turn_v1(&self.draft).map_err(CombatDraftEnvErrorV1::View)
    }

    pub fn current_frame_handle(&self) -> Option<sts_combat_planner::CombatDraftFrameHandle> {
        self.draft.current_frame_handle()
    }

    pub fn root_selection_intent(&self) -> Option<SelectionIntent> {
        self.draft.root_selection_intent()
    }

    pub fn selection_view(&self) -> Result<CombatDraftSelectionViewV1, CombatDraftEnvErrorV1> {
        project_combat_draft_selection_v1(&self.draft).map_err(CombatDraftEnvErrorV1::View)
    }

    pub fn selection_sources(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<sts_combat_planner::SelectionSourcePage, CombatDraftEnvErrorV1> {
        self.draft
            .selection_sources(&EngineCombatStepper, offset, limit)
            .map_err(Into::into)
    }

    pub fn selection_summary(
        &self,
        source: &SelectionSource,
        locale: sts_presentation::Locale,
    ) -> Result<String, CombatDraftEnvErrorV1> {
        sts_complete_turn_view::summarize_combat_draft_selection_source_v1(
            &self.draft,
            source,
            locale,
        )
        .map_err(CombatDraftEnvErrorV1::View)
    }

    pub fn selection_context(
        &self,
        source: &SelectionSource,
    ) -> Result<sts_complete_turn_view::CombatDraftSelectionContextV1, CombatDraftEnvErrorV1> {
        sts_complete_turn_view::project_combat_draft_selection_context_v1(
            &self.draft,
            source,
            self.max_engine_steps_per_action,
        )
        .map_err(CombatDraftEnvErrorV1::View)
    }

    pub fn selection_continuation_work(
        &self,
        source: &SelectionSource,
    ) -> Result<usize, CombatDraftEnvErrorV1> {
        self.draft
            .selection_continuation_work(source)
            .map_err(Into::into)
    }

    pub fn selection_view_at(
        &self,
        source: &SelectionSource,
    ) -> Result<CombatDraftSelectionViewV1, CombatDraftEnvErrorV1> {
        sts_complete_turn_view::project_combat_draft_selection_at_v1(&self.draft, source)
            .map_err(CombatDraftEnvErrorV1::View)
    }

    pub fn selection_outcomes(
        &self,
        source: &SelectionSource,
        offset: usize,
    ) -> Result<sts_complete_turn_view::CombatDraftSelectionOutcomesViewV1, CombatDraftEnvErrorV1>
    {
        sts_complete_turn_view::project_combat_draft_selection_outcomes_v1(
            &self.draft,
            source,
            offset,
        )
        .map_err(CombatDraftEnvErrorV1::View)
    }

    /// Resolve public occurrences through the existing typed projection. A
    /// source check precedes resolution; all members resolve before any edit.
    pub fn replace_selection_preferences(
        &mut self,
        source: &SelectionSource,
        preferences: &[CombatDraftSelectionMemberV1],
    ) -> Result<CombatDraftSelectionViewV1, CombatDraftEnvErrorV1> {
        self.draft.validate_selection_source(source)?;
        if preferences.len() > sts_combat_planner::CombatSelectionPreferences::LIMIT {
            return Err(CombatDraftError::SelectionIntent(
                sts_combat_planner::SelectionIntentError::TooManyMembers {
                    limit: sts_combat_planner::CombatSelectionPreferences::LIMIT,
                },
            )
            .into());
        }
        let projection = sts_complete_turn_view::project_combat_draft_selection_surface_at_v1(
            &self.draft,
            source,
        )
        .map_err(CombatDraftEnvErrorV1::View)?;
        let members = preferences
            .iter()
            .map(|preference| {
                resolve_public_combat_action_v1(
                    &projection,
                    &PublicCombatActionChoiceV1::Selection {
                        family_ordinal: preference.family,
                        selected_domain_ordinals: preference.choices.clone(),
                    },
                )
                .map_err(|error| CombatDraftEnvErrorV1::View(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut unique = Vec::new();
        for preference in preferences {
            if !unique.contains(preference) {
                unique.push(preference.clone());
            }
        }
        let view = CombatDraftSelectionViewV1 {
            families: projection.public.selection_families,
            preferences: unique,
        };
        self.draft
            .replace_selection_intent(source, &members, &EngineCombatStepper)?;
        Ok(view)
    }

    pub fn replace_root_selection_intent(
        &mut self,
        source: &SelectionSource,
        members: &[ClientInput],
    ) -> Result<(), CombatDraftEnvErrorV1> {
        self.draft
            .replace_root_selection_intent(source, members, &EngineCombatStepper)
            .map_err(CombatDraftEnvErrorV1::Draft)
    }

    pub fn render_observation(&self) -> Result<String, CombatDraftEnvErrorV1> {
        render_combat_draft_turn_v1(&self.observe()?).map_err(CombatDraftEnvErrorV1::View)
    }

    /// Advances the current generator once using exactly the supplied quantum.
    pub fn advance(
        &mut self,
        quantum: CombatPlanningQuantum,
    ) -> Result<CombatDraftAdvanceV1, CombatDraftEnvErrorV1> {
        let generation = self.draft.advance_options(&EngineCombatStepper, quantum)?;
        let view = self.observe()?;
        Ok(CombatDraftAdvanceV1 { generation, view })
    }

    pub fn inspect(&self, label: &str) -> Result<CombatDraftOptionViewV1, CombatDraftEnvErrorV1> {
        let handle = self.handle_for_current_label(label)?;
        self.inspect_handle(handle)
    }

    pub fn inspect_handle(
        &self,
        handle: CombatDraftOptionHandle,
    ) -> Result<CombatDraftOptionViewV1, CombatDraftEnvErrorV1> {
        // Preserve the core's typed unknown/stale distinction instead of
        // letting the presentation layer flatten it into a string.
        self.draft.option(handle)?;
        project_combat_draft_option_v1(&self.draft, handle, self.max_engine_steps_per_action)
            .map_err(CombatDraftEnvErrorV1::View)
    }

    pub fn render_inspection(&self, label: &str) -> Result<String, CombatDraftEnvErrorV1> {
        render_combat_draft_option_v1(&self.inspect(label)?).map_err(CombatDraftEnvErrorV1::View)
    }

    pub fn choose(
        &mut self,
        label: &str,
    ) -> Result<CompleteTurnOptionBoundary, CombatDraftEnvErrorV1> {
        let handle = self.handle_for_current_label(label)?;
        self.choose_handle(handle)
    }

    pub fn choose_handle(
        &mut self,
        handle: CombatDraftOptionHandle,
    ) -> Result<CompleteTurnOptionBoundary, CombatDraftEnvErrorV1> {
        self.draft
            .choose(handle, &EngineCombatStepper)
            .map_err(CombatDraftEnvErrorV1::Draft)
    }

    pub fn undo(&mut self) -> Result<(), CombatDraftEnvErrorV1> {
        self.draft.undo().map_err(CombatDraftEnvErrorV1::Draft)
    }

    pub fn terminal_boundary(&self) -> Option<CompleteTurnOptionBoundary> {
        self.draft.terminal_boundary()
    }

    pub fn victory_witness(&self) -> Result<Option<ExactAtomicWitness>, CombatDraftEnvErrorV1> {
        self.draft
            .victory_witness(&EngineCombatStepper)
            .map_err(CombatDraftEnvErrorV1::Draft)
    }

    fn handle_for_current_label(
        &self,
        label: &str,
    ) -> Result<CombatDraftOptionHandle, CombatDraftEnvErrorV1> {
        let ordinal = parse_canonical_option_label(label)?;
        self.draft
            .option_handle(ordinal)
            .map_err(CombatDraftEnvErrorV1::Draft)
    }
}

fn parse_canonical_option_label(label: &str) -> Result<usize, CombatDraftEnvErrorV1> {
    let digits = label
        .strip_prefix('T')
        .filter(|digits| !digits.is_empty())
        .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_digit()))
        .filter(|digits| digits.len() == 1 || !digits.starts_with('0'))
        .ok_or_else(|| CombatDraftEnvErrorV1::InvalidOptionLabel {
            label: label.to_owned(),
        })?;
    let ordinal =
        digits
            .parse::<usize>()
            .map_err(|_| CombatDraftEnvErrorV1::InvalidOptionLabel {
                label: label.to_owned(),
            })?;
    if format!("T{ordinal}") != label {
        return Err(CombatDraftEnvErrorV1::InvalidOptionLabel {
            label: label.to_owned(),
        });
    }
    Ok(ordinal)
}

#[cfg(test)]
mod tests {
    use std::cmp::Reverse;

    use sts_combat_planner::{
        CombatDraftOptionCoverage, GenerationInterruption, TurnOptionGenerationStatus,
    };
    use sts_core::content::{
        cards::CardId,
        monsters::{factory::EncounterId, EnemyId},
    };
    use sts_core::runtime::combat::CombatCard;
    use sts_core::sim::combat_start::build_natural_combat_start;
    use sts_core::state::core::EngineState;
    use sts_core::state::map::node::RoomType;
    use sts_core::state::run::RunState;
    use sts_core::test_support::{blank_test_combat, planned_monster};

    use super::*;

    fn config() -> TurnOptionGeneratorConfig {
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            allow_potion_expenditure: false,
            allow_potion_discard: false,
            ..TurnOptionGeneratorConfig::default()
        }
    }

    fn defensive_env() -> CombatDraftEnvV1 {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 11)];
        combat.turn.energy = 1;
        CombatDraftEnvV1::from_position(
            CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            config(),
        )
        .unwrap()
    }

    #[test]
    fn public_selection_preferences_preserve_occurrences_order_and_zero_card_meaning() {
        use sts_agent::agent::information::action::PublicCombatSelectionDomainCandidateV1;
        use sts_core::content::potions::{Potion, PotionId};
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 11),
            CombatCard::for_scenario(CardId::Wound, 22),
            CombatCard::for_scenario(CardId::Wound, 33),
        ];
        combat.entities.potions = vec![Some(Potion::new(PotionId::GamblersBrew, 1))];
        let prepared = sts_combat_planner::materialize_exact_action_line(
            &EngineCombatStepper,
            &CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            &[ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            }],
            250,
        )
        .unwrap();
        let mut env = CombatDraftEnvV1::from_position(prepared.final_position, config()).unwrap();
        let source = env.root_selection_intent().unwrap().source;
        let view = env.selection_view().unwrap();
        assert_eq!(
            view.families[0]
                .domain
                .iter()
                .filter(|candidate| matches!(
                    candidate,
                    PublicCombatSelectionDomainCandidateV1::Card {
                        card_id: Some(CardId::Wound),
                        ..
                    }
                ))
                .count(),
            2
        );
        let member = |choices| CombatDraftSelectionMemberV1 { family: 0, choices };
        let advice = vec![member(vec![1, 2]), member(vec![2, 1])];
        let before = env.status();
        env.replace_selection_preferences(&source, &advice).unwrap();
        assert_eq!(env.status(), before);
        assert_eq!(env.selection_view().unwrap().preferences, advice);
        let exact = env.root_selection_intent().unwrap().members;
        assert_ne!(exact[0], exact[1]);
        for invalid in [
            member(vec![1, 1]),
            member(vec![99]),
            CombatDraftSelectionMemberV1 {
                family: 99,
                choices: vec![],
            },
        ] {
            assert!(env
                .replace_selection_preferences(&source, &[advice[0].clone(), invalid])
                .is_err());
            assert_eq!(env.selection_view().unwrap().preferences, advice);
        }
        env.replace_selection_preferences(&source, &[member(vec![])])
            .unwrap();
        assert_eq!(env.root_selection_intent().unwrap().members.len(), 1);
        env.replace_selection_preferences(&source, &[]).unwrap();
        assert!(env.root_selection_intent().unwrap().members.is_empty());
        assert_eq!(
            env.advance(CombatPlanningQuantum::deterministic(0, 0))
                .unwrap()
                .generation
                .after
                .generation_work,
            0
        );
        let rendered = sts_complete_turn_view::render_combat_draft_selection_v1(&view);
        assert!(rendered.contains("Wound"));
        assert!(!rendered.contains("lineage"));
    }

    #[test]
    fn selection_display_and_resolution_share_generator_admission_bounds() {
        use sts_core::content::potions::{Potion, PotionId};
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Wound, 11)];
        combat.entities.potions = vec![Some(Potion::new(PotionId::Elixir, 1))];
        let prepared = sts_combat_planner::materialize_exact_action_line(
            &EngineCombatStepper,
            &CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            &[ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            }],
            250,
        )
        .unwrap();
        let mut env = CombatDraftEnvV1::from_position(prepared.final_position, config()).unwrap();
        let source = env.root_selection_intent().unwrap().source;
        let view = env.selection_view().unwrap();
        assert_eq!(view.families[0].declared_min, 1);
        assert!(env
            .replace_selection_preferences(
                &source,
                &[CombatDraftSelectionMemberV1 {
                    family: 0,
                    choices: vec![]
                }]
            )
            .is_err());
        env.replace_selection_preferences(
            &source,
            &[CombatDraftSelectionMemberV1 {
                family: 0,
                choices: vec![0],
            }],
        )
        .unwrap();
        assert_eq!(env.root_selection_intent().unwrap().members.len(), 1);
    }

    fn finish_turn(env: &mut CombatDraftEnvV1) -> CombatDraftTurnViewV1 {
        let advanced = env
            .advance(CombatPlanningQuantum::deterministic(1_000, 16_384))
            .unwrap();
        assert_eq!(
            advanced.generation.status,
            TurnOptionGenerationStatus::Complete
        );
        assert!(!advanced.view.options.is_empty());
        advanced.view
    }

    #[test]
    fn zero_quantum_returns_open_observation_without_hidden_work() {
        let mut env = defensive_env();

        let advanced = env
            .advance(CombatPlanningQuantum::deterministic(0, 0))
            .unwrap();

        assert_eq!(
            advanced.generation.status,
            TurnOptionGenerationStatus::Partial(GenerationInterruption::GenerationWorkBudget)
        );
        assert_eq!(
            env.status(),
            CombatDraftEnvStatusV1::Active {
                selected_turn_count: 0,
                available_option_count: 0,
                coverage: CombatDraftOptionCoverage::Open {
                    mechanics_gap_count: 0,
                },
            }
        );
        assert!(advanced.view.options.is_empty());
    }

    #[test]
    fn editable_selection_intent_uses_the_resident_draft_at_a_real_potion_boundary() {
        use sts_core::content::potions::{Potion, PotionId};
        use sts_core::state::selection::{SelectionResolution, SelectionScope};
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 11),
            CombatCard::for_scenario(CardId::Wound, 22),
        ];
        combat.entities.potions = vec![Some(Potion::new(PotionId::GamblersBrew, 1))];
        let prepared = sts_combat_planner::materialize_exact_action_line(
            &EngineCombatStepper,
            &CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            &[ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            }],
            250,
        )
        .unwrap();
        let members = vec![ClientInput::SubmitSelection(
            SelectionResolution::card_lineages(
                SelectionScope::Hand,
                prepared
                    .final_position
                    .combat
                    .zones
                    .hand
                    .iter()
                    .filter(|card| card.id == CardId::Wound)
                    .map(|card| card.lineage),
            ),
        )];
        let mut env = CombatDraftEnvV1::from_position(prepared.final_position, config()).unwrap();
        let source = env.root_selection_intent().unwrap().source;
        let before = env.status();
        env.replace_root_selection_intent(&source, &members)
            .unwrap();
        assert_eq!(env.root_selection_intent().unwrap().members, members);
        assert_eq!(env.status(), before);
        let idle = env
            .advance(CombatPlanningQuantum::deterministic(0, 0))
            .unwrap();
        assert_eq!(idle.generation.after.generation_work, 0);
        let view = finish_turn(&mut env);
        let handle = view.options[0].handle;
        let inspected = env.inspect_handle(handle).unwrap();
        env.replace_root_selection_intent(&source, &[]).unwrap();
        assert_eq!(env.inspect_handle(handle).unwrap(), inspected);
        env.choose_handle(handle).unwrap();
        assert!(env.root_selection_intent().unwrap().members.is_empty());
        assert!(matches!(
            env.replace_root_selection_intent(&source, &members),
            Err(CombatDraftEnvErrorV1::Draft(
                CombatDraftError::SelectionIntent(
                    sts_combat_planner::SelectionIntentError::StaleSource
                )
            ))
        ));
        env.undo().unwrap();
        let mut restored = env.observe().unwrap();
        let explored = restored
            .options
            .iter_mut()
            .find(|o| o.handle == handle)
            .unwrap();
        assert!(explored.continuation.take().is_some());
        assert_eq!(restored, view);
        env.replace_root_selection_intent(&source, &members)
            .unwrap();
        assert_eq!(env.inspect_handle(handle).unwrap(), inspected);
    }

    #[test]
    fn inspection_uses_one_short_label_and_returns_public_successor() {
        let mut env = defensive_env();
        let view = finish_turn(&mut env);
        let label = view.options[0].handle.to_string();

        let inspection = env.inspect(&label).unwrap();
        let rendered = env.render_inspection(&label).unwrap();

        assert_eq!(inspection.handle.to_string(), label);
        assert!(rendered.contains(&format!("OPTION {label}")));
        assert!(rendered.contains("ACTION A0"));
        assert!(rendered.contains("[next]"));
        assert!(!rendered.contains(['{', '}']));
    }

    #[test]
    fn short_label_parser_rejects_action_ids_aliases_and_missing_options() {
        let env = defensive_env();

        for label in ["A0", "t0", "T", "T01", " T0", "T0 ", "T+0"] {
            assert!(matches!(
                env.inspect(label),
                Err(CombatDraftEnvErrorV1::InvalidOptionLabel { .. })
            ));
        }
        assert_eq!(
            env.inspect("T9"),
            Err(CombatDraftEnvErrorV1::Draft(
                CombatDraftError::UnknownOption { ordinal: 9 }
            ))
        );
    }

    #[test]
    fn choose_and_undo_retain_both_frames_and_report_explored_options() {
        let mut env = defensive_env();
        let parent_view = finish_turn(&mut env);
        let parent = parent_view
            .options
            .iter()
            .find(|option| option.boundary == CompleteTurnOptionBoundary::NextPlayerTurn)
            .unwrap()
            .handle;
        let parent_label = parent.to_string();

        assert_eq!(
            env.choose(&parent_label).unwrap(),
            CompleteTurnOptionBoundary::NextPlayerTurn
        );
        let child_view = finish_turn(&mut env);
        let child = child_view.options[0].handle;
        env.undo().unwrap();

        let mut restored = env.observe().unwrap();
        let explored = restored
            .options
            .iter_mut()
            .find(|o| o.handle == parent)
            .unwrap();
        let continuation = explored.continuation.take().unwrap();
        assert_eq!(continuation.frame, child_view.frame);
        assert_eq!(
            continuation.available_option_count,
            child_view.options.len()
        );
        assert_eq!(restored, parent_view);
        assert_eq!(
            env.inspect_handle(child),
            Err(CombatDraftEnvErrorV1::Draft(
                CombatDraftError::StaleOptionHandle
            ))
        );
        assert!(env.inspect_handle(parent).is_ok());
        env.choose_handle(parent).unwrap();
        assert_eq!(env.observe().unwrap(), child_view);
        assert!(env.inspect_handle(child).is_ok());
    }

    #[test]
    fn exact_engine_win_is_the_only_terminal_state_that_exports_a_witness() {
        let mut combat = blank_test_combat();
        let mut monster = planned_monster(EnemyId::JawWorm, 1);
        monster.current_hp = 6;
        monster.max_hp = 6;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 11)];
        combat.turn.energy = 1;
        let mut env = CombatDraftEnvV1::from_position(
            CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            config(),
        )
        .unwrap();
        let view = finish_turn(&mut env);
        let winning = view
            .options
            .iter()
            .find(|option| option.boundary == CompleteTurnOptionBoundary::TerminalWin)
            .unwrap()
            .handle;

        env.choose_handle(winning).unwrap();

        assert_eq!(
            env.status(),
            CombatDraftEnvStatusV1::Terminal {
                selected_turn_count: 1,
                boundary: CompleteTurnOptionBoundary::TerminalWin,
            }
        );
        assert!(env.victory_witness().unwrap().is_some());
        env.undo().unwrap();
        assert!(env.victory_witness().unwrap().is_none());
    }

    #[test]
    fn natural_a20_starter_combat_forms_a_multiturn_exact_witness() {
        let mut run = RunState::new(2_026_090_401, 20, true, "Ironclad");
        let (engine, combat) =
            build_natural_combat_start(&mut run, EncounterId::Cultist, RoomType::MonsterRoom)
                .expect("natural Cultist combat should initialize");
        let mut env =
            CombatDraftEnvV1::from_position(CombatPosition::new(engine, combat), config())
                .expect("natural combat should be a valid draft root");

        for _ in 0..8 {
            let advanced = env
                .advance(CombatPlanningQuantum::deterministic(1_024, 65_536))
                .expect("natural turn generation should advance");
            assert_eq!(
                advanced.generation.status,
                TurnOptionGenerationStatus::Complete,
                "the natural turn surface must be complete before selection"
            );

            let selected = advanced
                .view
                .options
                .iter()
                .filter_map(|option| {
                    let label = option.handle.to_string();
                    let detail = env
                        .inspect(&label)
                        .expect("every indexed option should have a public exact projection");
                    (!matches!(
                        detail.boundary,
                        CompleteTurnOptionBoundary::TerminalLoss
                            | CompleteTurnOptionBoundary::Escape
                    ))
                    .then_some((
                        detail.boundary != CompleteTurnOptionBoundary::TerminalWin,
                        detail
                            .successor
                            .monsters
                            .iter()
                            .filter(|monster| monster.alive)
                            .map(|monster| monster.hp.max(0))
                            .sum::<i32>(),
                        Reverse(detail.successor.player.hp),
                        option.handle.ordinal(),
                        label,
                    ))
                })
                .min()
                .expect("a living natural combat should expose a non-loss complete turn");

            let boundary = env
                .choose(&selected.4)
                .expect("the observed current-frame option should remain selectable");
            if boundary == CompleteTurnOptionBoundary::TerminalWin {
                break;
            }
        }

        assert!(matches!(
            env.status(),
            CombatDraftEnvStatusV1::Terminal {
                selected_turn_count: 2..,
                boundary: CompleteTurnOptionBoundary::TerminalWin,
            }
        ));
        let witness = env
            .victory_witness()
            .expect("the selected natural line should replay exactly")
            .expect("the natural terminal win should export a witness");
        assert!(!witness.actions.is_empty());
        assert!(witness.replay_engine_steps > 0);
    }
}
