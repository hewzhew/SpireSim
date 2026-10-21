use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{
    CombatSearchV2ChildRolloutPolicy, CombatSearchV2Config, CombatSearchV2ExpansionPolicy,
    CombatSearchV2PhaseGuardPolicy, CombatSearchV2PotionPolicy, CombatSearchV2RolloutPolicy,
    CombatSearchV2SetupBiasPolicy, CombatSearchV2TurnPlanPolicy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatSearchBudgetSpec {
    pub max_nodes: usize,
    pub wall_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CombatSearchEngineProfile {
    pub budget: CombatSearchBudgetSpec,
    pub plugins: CombatSearchPluginStack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CombatSearchRolloutBudgetSpec {
    max_evaluations: usize,
    max_actions: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CombatSearchAttemptPolicy {
    pub acceptance: CombatSearchAcceptancePluginId,
    pub artifacts: CombatSearchArtifactPluginId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CombatSearchProfile {
    pub label: &'static str,
    pub engine: CombatSearchEngineProfile,
    pub policy: CombatSearchAttemptPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatSearchPluginStack {
    pub action_prior: CombatSearchActionPriorPluginId,
    pub node_evaluator: CombatSearchNodeEvaluatorPluginId,
    pub expansion: CombatSearchExpansionPluginId,
    pub turn_plan: CombatSearchTurnPlanPluginId,
    pub child_rollout: CombatSearchChildRolloutPluginId,
    pub rollout: CombatSearchRolloutPluginId,
    pub potion: CombatSearchPotionPlugin,
    pub phase_guard: CombatSearchPhaseGuardPluginId,
}

impl Default for CombatSearchPluginStack {
    fn default() -> Self {
        Self {
            action_prior: CombatSearchActionPriorPluginId::Default,
            node_evaluator: CombatSearchNodeEvaluatorPluginId::CombatOutcomeScore,
            expansion: CombatSearchExpansionPluginId::AtomicActions,
            turn_plan: CombatSearchTurnPlanPluginId::Disabled,
            child_rollout: CombatSearchChildRolloutPluginId::LazyOnPop,
            rollout: CombatSearchRolloutPluginId::EnemyMechanicsAdaptiveNoPotion,
            potion: CombatSearchPotionPlugin::default(),
            phase_guard: CombatSearchPhaseGuardPluginId::Default,
        }
    }
}

impl CombatSearchPluginStack {
    pub fn from_config(config: &CombatSearchV2Config) -> Self {
        Self {
            action_prior: config.setup_bias_policy.into(),
            node_evaluator: CombatSearchNodeEvaluatorPluginId::CombatOutcomeScore,
            expansion: config.expansion_policy.into(),
            turn_plan: config.turn_plan_policy.into(),
            child_rollout: config.child_rollout_policy.into(),
            rollout: config.rollout_policy.into(),
            potion: CombatSearchPotionPlugin {
                policy: config.potion_policy,
                max_potions_used: config.max_potions_used,
                allowed_potion_slots: config.allowed_potion_slots,
                allow_potion_discard: config.allow_potion_discard,
            },
            phase_guard: config.phase_guard_policy.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CombatSearchPotionPlugin {
    pub policy: CombatSearchV2PotionPolicy,
    pub max_potions_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_potion_slots: Option<u64>,
    /// Explicit admission for discard actions. `None` preserves the legacy
    /// policy-derived surface (`All` admits discard); semantic production
    /// profiles set `Some(false)` even when every selected potion may be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_potion_discard: Option<bool>,
}

impl Default for CombatSearchPotionPlugin {
    fn default() -> Self {
        Self {
            policy: CombatSearchV2PotionPolicy::Never,
            max_potions_used: None,
            allowed_potion_slots: None,
            allow_potion_discard: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchActionPriorPluginId {
    Default,
    KeyCardOnline,
}

impl CombatSearchActionPriorPluginId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::KeyCardOnline => "key_card_online",
        }
    }

    pub(in crate::ai::combat_search_v2) fn prioritizes_key_card_online(self) -> bool {
        matches!(self, Self::KeyCardOnline)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchNodeEvaluatorPluginId {
    CombatOutcomeScore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchExpansionPluginId {
    AtomicActions,
    HierarchicalTurnBoundary,
}

impl CombatSearchExpansionPluginId {
    pub fn label(self) -> &'static str {
        CombatSearchV2ExpansionPolicy::from(self).label()
    }

    pub(in crate::ai::combat_search_v2) fn owns_turn_boundaries(self) -> bool {
        matches!(self, Self::HierarchicalTurnBoundary)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchTurnPlanPluginId {
    Disabled,
    DiagnosticOnly,
    RootFrontierSeed,
    TurnBoundaryFrontierSeed,
    TacticalEnemyTurnBoundaryFrontierSeed,
}

impl CombatSearchTurnPlanPluginId {
    pub(in crate::ai::combat_search_v2) fn seeds_root_frontier(self) -> bool {
        CombatSearchV2TurnPlanPolicy::from(self).seeds_root_frontier()
    }

    pub(in crate::ai::combat_search_v2) fn seeds_turn_boundary_frontier(self) -> bool {
        CombatSearchV2TurnPlanPolicy::from(self).seeds_turn_boundary_frontier()
    }

    pub(in crate::ai::combat_search_v2) fn requires_tactical_enemy_gate(self) -> bool {
        CombatSearchV2TurnPlanPolicy::from(self).requires_tactical_enemy_gate()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchChildRolloutPluginId {
    Immediate,
    LazyOnPop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchRolloutPluginId {
    Disabled,
    #[default]
    EnemyMechanicsAdaptiveNoPotion,
    ConservativeNoPotion,
    PhaseAwareNoPotion,
    TurnBeamNoPotion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchPhaseGuardPluginId {
    Default,
    ChampSplitGuard,
    TimeEaterClockHint,
}

impl CombatSearchPhaseGuardPluginId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::ChampSplitGuard => "champ_split_guard",
            Self::TimeEaterClockHint => "time_eater_clock_hint",
        }
    }

    pub(in crate::ai::combat_search_v2) fn guards_champ_split(self) -> bool {
        matches!(self, Self::ChampSplitGuard)
    }

    pub(in crate::ai::combat_search_v2) fn guards_time_eater_clock(self) -> bool {
        matches!(self, Self::TimeEaterClockHint)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchAcceptancePluginId {
    AcceptedLineOnly,
    AcceptedLineOrPrimaryChunk,
    CleanAcceptedLineNoNewCurse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchArtifactPluginId {
    None,
    PortfolioAttempt,
    DecisionMicroscope,
    FullTrace,
}

pub trait CombatSearchActionPriorPlugin {
    fn id(&self) -> CombatSearchActionPriorPluginId;
}

pub trait CombatSearchNodeEvaluatorPlugin {
    fn id(&self) -> CombatSearchNodeEvaluatorPluginId;
}

pub trait CombatSearchExpansionPlugin {
    fn id(&self) -> CombatSearchExpansionPluginId;
}

pub trait CombatSearchTurnPlanPlugin {
    fn id(&self) -> CombatSearchTurnPlanPluginId;
}

pub trait CombatSearchChildRolloutPlugin {
    fn id(&self) -> CombatSearchChildRolloutPluginId;
}

pub trait CombatSearchRolloutPlugin {
    fn id(&self) -> CombatSearchRolloutPluginId;
}

pub trait CombatSearchPhaseGuardPlugin {
    fn id(&self) -> CombatSearchPhaseGuardPluginId;
}

pub trait CombatSearchAcceptancePlugin {
    fn id(&self) -> CombatSearchAcceptancePluginId;
}

pub trait CombatSearchArtifactPlugin {
    fn id(&self) -> CombatSearchArtifactPluginId;
}

#[derive(Clone, Copy)]
pub struct CombatSearchActionOrderingPlugins<'a> {
    pub root_action_prior: Option<&'a super::CombatSearchV2RootActionPrior>,
    pub action_prior: CombatSearchActionPriorPluginId,
    pub phase_guard: CombatSearchPhaseGuardPluginId,
}

impl<'a> CombatSearchActionOrderingPlugins<'a> {
    pub fn from_stack(
        root_action_prior: Option<&'a super::CombatSearchV2RootActionPrior>,
        stack: &CombatSearchPluginStack,
    ) -> Self {
        Self {
            root_action_prior,
            action_prior: stack.action_prior,
            phase_guard: stack.phase_guard,
        }
    }
}

impl Default for CombatSearchActionOrderingPlugins<'_> {
    fn default() -> Self {
        Self {
            root_action_prior: None,
            action_prior: CombatSearchActionPriorPluginId::Default,
            phase_guard: CombatSearchPhaseGuardPluginId::Default,
        }
    }
}

impl CombatSearchActionPriorPlugin for CombatSearchActionPriorPluginId {
    fn id(&self) -> CombatSearchActionPriorPluginId {
        *self
    }
}

impl CombatSearchNodeEvaluatorPlugin for CombatSearchNodeEvaluatorPluginId {
    fn id(&self) -> CombatSearchNodeEvaluatorPluginId {
        *self
    }
}

impl CombatSearchExpansionPlugin for CombatSearchExpansionPluginId {
    fn id(&self) -> CombatSearchExpansionPluginId {
        *self
    }
}

impl CombatSearchTurnPlanPlugin for CombatSearchTurnPlanPluginId {
    fn id(&self) -> CombatSearchTurnPlanPluginId {
        *self
    }
}

impl CombatSearchChildRolloutPlugin for CombatSearchChildRolloutPluginId {
    fn id(&self) -> CombatSearchChildRolloutPluginId {
        *self
    }
}

impl CombatSearchRolloutPlugin for CombatSearchRolloutPluginId {
    fn id(&self) -> CombatSearchRolloutPluginId {
        *self
    }
}

impl CombatSearchPhaseGuardPlugin for CombatSearchPhaseGuardPluginId {
    fn id(&self) -> CombatSearchPhaseGuardPluginId {
        *self
    }
}

impl CombatSearchAcceptancePlugin for CombatSearchAcceptancePluginId {
    fn id(&self) -> CombatSearchAcceptancePluginId {
        *self
    }
}

impl CombatSearchArtifactPlugin for CombatSearchArtifactPluginId {
    fn id(&self) -> CombatSearchArtifactPluginId {
        *self
    }
}

impl CombatSearchProfile {
    pub fn with_acceptance(mut self, acceptance: CombatSearchAcceptancePluginId) -> Self {
        self.policy.acceptance = acceptance;
        self
    }

    pub fn with_action_prior_plugin(
        mut self,
        action_prior: CombatSearchActionPriorPluginId,
    ) -> Self {
        self.engine.plugins.action_prior = action_prior;
        self
    }

    pub fn with_turn_plan_plugin(mut self, turn_plan: CombatSearchTurnPlanPluginId) -> Self {
        self.engine.plugins.turn_plan = turn_plan;
        self
    }

    pub fn with_expansion_plugin(mut self, expansion: CombatSearchExpansionPluginId) -> Self {
        self.engine.plugins.expansion = expansion;
        self
    }

    pub fn with_child_rollout_plugin(
        mut self,
        child_rollout: CombatSearchChildRolloutPluginId,
    ) -> Self {
        self.engine.plugins.child_rollout = child_rollout;
        self
    }

    pub fn with_rollout_plugin(mut self, rollout: CombatSearchRolloutPluginId) -> Self {
        self.engine.plugins.rollout = rollout;
        self
    }

    pub fn with_potion_policy(mut self, policy: CombatSearchV2PotionPolicy) -> Self {
        self.engine.plugins.potion.policy = policy;
        self
    }

    pub fn with_max_potions_used(mut self, max_potions_used: u32) -> Self {
        self.engine.plugins.potion.max_potions_used = Some(max_potions_used);
        self
    }

    pub fn with_phase_guard_plugin(mut self, phase_guard: CombatSearchPhaseGuardPluginId) -> Self {
        self.engine.plugins.phase_guard = phase_guard;
        self
    }

    pub fn to_config(self) -> CombatSearchV2Config {
        self.engine.to_config()
    }

    pub fn engine_fingerprint(self) -> String {
        self.engine.fingerprint()
    }

    /// Stable identity for search behavior. Work budgets are deliberately
    /// excluded because advancing an existing session must not pretend to
    /// create a different search engine.
    pub fn semantics_fingerprint(self) -> String {
        self.engine.semantics_fingerprint()
    }
}

impl CombatSearchEngineProfile {
    pub fn fingerprint(self) -> String {
        serde_json::to_string(&self).expect("combat search engine profile should serialize")
    }

    pub fn semantics_fingerprint(self) -> String {
        serde_json::to_string(&self.plugins).expect("combat search plugin stack should serialize")
    }

    pub fn to_config(self) -> CombatSearchV2Config {
        let defaults = CombatSearchV2Config::default();
        let rollout_budget = CombatSearchRolloutBudgetSpec::for_engine_profile(self);
        CombatSearchV2Config {
            max_nodes: self.budget.max_nodes,
            wall_time: Some(Duration::from_millis(self.budget.wall_ms)),
            potion_policy: self.plugins.potion.policy,
            max_potions_used: self.plugins.potion.max_potions_used,
            allowed_potion_slots: self.plugins.potion.allowed_potion_slots,
            allow_potion_discard: self.plugins.potion.allow_potion_discard,
            rollout_policy: self.plugins.rollout.into(),
            child_rollout_policy: self.plugins.child_rollout.into(),
            expansion_policy: self.plugins.expansion.into(),
            turn_plan_policy: self.plugins.turn_plan.into(),
            phase_guard_policy: self.plugins.phase_guard.into(),
            setup_bias_policy: self.plugins.action_prior.into(),
            rollout_max_evaluations: rollout_budget.max_evaluations,
            rollout_max_actions: rollout_budget.max_actions,
            ..defaults
        }
    }
}

impl From<CombatSearchExpansionPluginId> for CombatSearchV2ExpansionPolicy {
    fn from(plugin: CombatSearchExpansionPluginId) -> Self {
        match plugin {
            CombatSearchExpansionPluginId::AtomicActions => Self::AtomicActions,
            CombatSearchExpansionPluginId::HierarchicalTurnBoundary => {
                Self::HierarchicalTurnBoundary
            }
        }
    }
}

impl From<CombatSearchV2ExpansionPolicy> for CombatSearchExpansionPluginId {
    fn from(policy: CombatSearchV2ExpansionPolicy) -> Self {
        match policy {
            CombatSearchV2ExpansionPolicy::AtomicActions => Self::AtomicActions,
            CombatSearchV2ExpansionPolicy::HierarchicalTurnBoundary => {
                Self::HierarchicalTurnBoundary
            }
        }
    }
}

impl CombatSearchRolloutBudgetSpec {
    fn for_engine_profile(profile: CombatSearchEngineProfile) -> Self {
        // Rollout is a bounded ordering hint, not the search itself. Long default
        // rollouts can consume an entire short wall-clock lane before the frontier
        // has a chance to expand, so named profiles scale rollout depth with the
        // wall budget that already participates in their engine identity.
        let max_actions = match profile.budget.wall_ms {
            0..=1_000 => 6,
            1_001..=2_500 => 8,
            2_501..=5_000 => 12,
            5_001..=10_000 => 16,
            _ => 24,
        };
        let max_evaluations = match profile.plugins.child_rollout {
            CombatSearchChildRolloutPluginId::Immediate => match profile.budget.wall_ms {
                0..=1_000 => 24,
                1_001..=2_500 => 48,
                2_501..=5_000 => 96,
                5_001..=10_000 => 128,
                _ => 256,
            },
            CombatSearchChildRolloutPluginId::LazyOnPop => match profile.budget.wall_ms {
                0..=1_000 => 64,
                1_001..=2_500 => 128,
                2_501..=5_000 => 256,
                _ => 384,
            },
        };
        Self {
            max_evaluations,
            max_actions,
        }
    }
}

impl From<CombatSearchActionPriorPluginId> for CombatSearchV2SetupBiasPolicy {
    fn from(plugin: CombatSearchActionPriorPluginId) -> Self {
        match plugin {
            CombatSearchActionPriorPluginId::Default => Self::Default,
            CombatSearchActionPriorPluginId::KeyCardOnline => Self::KeyCardOnline,
        }
    }
}

impl From<CombatSearchV2SetupBiasPolicy> for CombatSearchActionPriorPluginId {
    fn from(policy: CombatSearchV2SetupBiasPolicy) -> Self {
        match policy {
            CombatSearchV2SetupBiasPolicy::Default => Self::Default,
            CombatSearchV2SetupBiasPolicy::KeyCardOnline => Self::KeyCardOnline,
        }
    }
}

impl From<CombatSearchTurnPlanPluginId> for CombatSearchV2TurnPlanPolicy {
    fn from(plugin: CombatSearchTurnPlanPluginId) -> Self {
        match plugin {
            CombatSearchTurnPlanPluginId::Disabled => Self::Disabled,
            CombatSearchTurnPlanPluginId::DiagnosticOnly => Self::DiagnosticOnly,
            CombatSearchTurnPlanPluginId::RootFrontierSeed => Self::RootFrontierSeed,
            CombatSearchTurnPlanPluginId::TurnBoundaryFrontierSeed => {
                Self::TurnBoundaryFrontierSeed
            }
            CombatSearchTurnPlanPluginId::TacticalEnemyTurnBoundaryFrontierSeed => {
                Self::TacticalEnemyTurnBoundaryFrontierSeed
            }
        }
    }
}

impl From<CombatSearchV2TurnPlanPolicy> for CombatSearchTurnPlanPluginId {
    fn from(policy: CombatSearchV2TurnPlanPolicy) -> Self {
        match policy {
            CombatSearchV2TurnPlanPolicy::Disabled => Self::Disabled,
            CombatSearchV2TurnPlanPolicy::DiagnosticOnly => Self::DiagnosticOnly,
            CombatSearchV2TurnPlanPolicy::RootFrontierSeed => Self::RootFrontierSeed,
            CombatSearchV2TurnPlanPolicy::TurnBoundaryFrontierSeed => {
                Self::TurnBoundaryFrontierSeed
            }
            CombatSearchV2TurnPlanPolicy::TacticalEnemyTurnBoundaryFrontierSeed => {
                Self::TacticalEnemyTurnBoundaryFrontierSeed
            }
        }
    }
}

impl From<CombatSearchChildRolloutPluginId> for CombatSearchV2ChildRolloutPolicy {
    fn from(plugin: CombatSearchChildRolloutPluginId) -> Self {
        match plugin {
            CombatSearchChildRolloutPluginId::Immediate => Self::Immediate,
            CombatSearchChildRolloutPluginId::LazyOnPop => Self::LazyOnPop,
        }
    }
}

impl From<CombatSearchV2ChildRolloutPolicy> for CombatSearchChildRolloutPluginId {
    fn from(policy: CombatSearchV2ChildRolloutPolicy) -> Self {
        match policy {
            CombatSearchV2ChildRolloutPolicy::Immediate => Self::Immediate,
            CombatSearchV2ChildRolloutPolicy::LazyOnPop => Self::LazyOnPop,
        }
    }
}

impl From<CombatSearchRolloutPluginId> for CombatSearchV2RolloutPolicy {
    fn from(plugin: CombatSearchRolloutPluginId) -> Self {
        match plugin {
            CombatSearchRolloutPluginId::Disabled => Self::Disabled,
            CombatSearchRolloutPluginId::EnemyMechanicsAdaptiveNoPotion => {
                Self::EnemyMechanicsAdaptiveNoPotion
            }
            CombatSearchRolloutPluginId::ConservativeNoPotion => Self::ConservativeNoPotion,
            CombatSearchRolloutPluginId::PhaseAwareNoPotion => Self::PhaseAwareNoPotion,
            CombatSearchRolloutPluginId::TurnBeamNoPotion => Self::TurnBeamNoPotion,
        }
    }
}

impl From<CombatSearchV2RolloutPolicy> for CombatSearchRolloutPluginId {
    fn from(policy: CombatSearchV2RolloutPolicy) -> Self {
        match policy {
            CombatSearchV2RolloutPolicy::Disabled => Self::Disabled,
            CombatSearchV2RolloutPolicy::EnemyMechanicsAdaptiveNoPotion => {
                Self::EnemyMechanicsAdaptiveNoPotion
            }
            CombatSearchV2RolloutPolicy::ConservativeNoPotion => Self::ConservativeNoPotion,
            CombatSearchV2RolloutPolicy::PhaseAwareNoPotion => Self::PhaseAwareNoPotion,
            CombatSearchV2RolloutPolicy::TurnBeamNoPotion => Self::TurnBeamNoPotion,
        }
    }
}

impl From<CombatSearchPhaseGuardPluginId> for CombatSearchV2PhaseGuardPolicy {
    fn from(plugin: CombatSearchPhaseGuardPluginId) -> Self {
        match plugin {
            CombatSearchPhaseGuardPluginId::Default => Self::Default,
            CombatSearchPhaseGuardPluginId::ChampSplitGuard => Self::ChampSplitGuard,
            CombatSearchPhaseGuardPluginId::TimeEaterClockHint => Self::TimeEaterClockHint,
        }
    }
}

impl From<CombatSearchV2PhaseGuardPolicy> for CombatSearchPhaseGuardPluginId {
    fn from(policy: CombatSearchV2PhaseGuardPolicy) -> Self {
        match policy {
            CombatSearchV2PhaseGuardPolicy::Default => Self::Default,
            CombatSearchV2PhaseGuardPolicy::ChampSplitGuard => Self::ChampSplitGuard,
            CombatSearchV2PhaseGuardPolicy::TimeEaterClockHint => Self::TimeEaterClockHint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_profile(label: &'static str) -> CombatSearchProfile {
        CombatSearchProfile {
            label,
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 50,
                    wall_ms: 100,
                },
                plugins: CombatSearchPluginStack::default(),
            },
            policy: CombatSearchAttemptPolicy {
                acceptance: CombatSearchAcceptancePluginId::AcceptedLineOnly,
                artifacts: CombatSearchArtifactPluginId::None,
            },
        }
    }

    #[test]
    fn engine_identity_ignores_label_and_attempt_policy() {
        let base = test_profile("immediate");
        let renamed = CombatSearchProfile {
            label: "renamed",
            policy: CombatSearchAttemptPolicy {
                acceptance: CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
                artifacts: CombatSearchArtifactPluginId::FullTrace,
            },
            ..base
        };

        assert_eq!(base.engine, renamed.engine);
        assert_eq!(base.engine_fingerprint(), renamed.engine_fingerprint());
    }

    #[test]
    fn engine_identity_distinguishes_child_rollout_policy() {
        let immediate = test_profile("immediate")
            .with_child_rollout_plugin(CombatSearchChildRolloutPluginId::Immediate);
        let lazy = test_profile("lazy")
            .with_child_rollout_plugin(CombatSearchChildRolloutPluginId::LazyOnPop);

        assert_ne!(immediate.engine, lazy.engine);
        assert_ne!(immediate.engine_fingerprint(), lazy.engine_fingerprint());
    }

    #[test]
    fn semantics_identity_ignores_incremental_work_budget() {
        let base = test_profile("initial");
        let refined = CombatSearchProfile {
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 500,
                    wall_ms: 1_000,
                },
                ..base.engine
            },
            ..base
        };

        assert_ne!(base.engine_fingerprint(), refined.engine_fingerprint());
        assert_eq!(
            base.semantics_fingerprint(),
            refined.semantics_fingerprint()
        );
    }

    #[test]
    fn semantics_identity_distinguishes_explicit_potion_discard_admission() {
        let mut legacy_all_legal = test_profile("all_legal");
        legacy_all_legal.engine.plugins.potion.policy = CombatSearchV2PotionPolicy::All;
        let mut semantic_use_only = legacy_all_legal;
        semantic_use_only.engine.plugins.potion.allow_potion_discard = Some(false);

        assert_ne!(
            legacy_all_legal.semantics_fingerprint(),
            semantic_use_only.semantics_fingerprint()
        );
    }

    #[test]
    fn short_named_profiles_bound_rollout_horizon_below_legacy_defaults() {
        let config = test_profile("short").to_config();

        assert_eq!(config.rollout_max_evaluations, 64);
        assert_eq!(config.rollout_max_actions, 6);
        assert!(
            config.rollout_max_evaluations
                < crate::ai::combat_search_v2::rollout::DEFAULT_ROLLOUT_MAX_EVALUATIONS
        );
        assert!(
            config.rollout_max_actions
                < crate::ai::combat_search_v2::rollout::DEFAULT_ROLLOUT_MAX_ACTIONS
        );
    }

    #[test]
    fn immediate_rollout_profiles_receive_a_stricter_evaluation_budget() {
        let lazy = CombatSearchProfile {
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 50_000,
                    wall_ms: 2_000,
                },
                ..test_profile("lazy").engine
            },
            ..test_profile("lazy")
        };
        let immediate = lazy.with_child_rollout_plugin(CombatSearchChildRolloutPluginId::Immediate);

        assert_eq!(lazy.to_config().rollout_max_evaluations, 128);
        assert_eq!(immediate.to_config().rollout_max_evaluations, 48);
        assert_eq!(lazy.to_config().rollout_max_actions, 8);
        assert_eq!(immediate.to_config().rollout_max_actions, 8);
    }

    #[test]
    fn medium_immediate_profiles_stay_below_the_legacy_quality_lane_volume() {
        let immediate = CombatSearchProfile {
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 200_000,
                    wall_ms: 7_500,
                },
                ..test_profile("medium_immediate").engine
            },
            ..test_profile("medium_immediate")
        }
        .with_child_rollout_plugin(CombatSearchChildRolloutPluginId::Immediate);
        let config = immediate.to_config();

        assert_eq!(config.rollout_max_evaluations, 128);
        assert_eq!(config.rollout_max_actions, 16);
    }

    #[test]
    fn longer_named_profiles_expand_rollout_horizon_without_restoring_unbounded_depth() {
        let profile = CombatSearchProfile {
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 500_000,
                    wall_ms: 20_000,
                },
                ..test_profile("long").engine
            },
            ..test_profile("long")
        };
        let config = profile.to_config();

        assert_eq!(config.rollout_max_evaluations, 384);
        assert_eq!(config.rollout_max_actions, 24);
    }

    #[test]
    fn plugin_ids_implement_their_role_traits() {
        fn action_prior_id(
            plugin: impl CombatSearchActionPriorPlugin,
        ) -> CombatSearchActionPriorPluginId {
            plugin.id()
        }
        fn evaluator_id(
            plugin: impl CombatSearchNodeEvaluatorPlugin,
        ) -> CombatSearchNodeEvaluatorPluginId {
            plugin.id()
        }
        fn rollout_id(plugin: impl CombatSearchRolloutPlugin) -> CombatSearchRolloutPluginId {
            plugin.id()
        }
        fn acceptance_id(
            plugin: impl CombatSearchAcceptancePlugin,
        ) -> CombatSearchAcceptancePluginId {
            plugin.id()
        }

        assert_eq!(
            action_prior_id(CombatSearchActionPriorPluginId::KeyCardOnline),
            CombatSearchActionPriorPluginId::KeyCardOnline
        );
        assert_eq!(
            evaluator_id(CombatSearchNodeEvaluatorPluginId::CombatOutcomeScore),
            CombatSearchNodeEvaluatorPluginId::CombatOutcomeScore
        );
        assert_eq!(
            rollout_id(CombatSearchRolloutPluginId::TurnBeamNoPotion),
            CombatSearchRolloutPluginId::TurnBeamNoPotion
        );
        assert_eq!(
            acceptance_id(CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse),
            CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse
        );
    }

    #[test]
    fn plugin_stack_can_be_projected_from_legacy_config() {
        let config = CombatSearchV2Config {
            setup_bias_policy: CombatSearchV2SetupBiasPolicy::KeyCardOnline,
            phase_guard_policy: CombatSearchV2PhaseGuardPolicy::TimeEaterClockHint,
            rollout_policy: CombatSearchV2RolloutPolicy::TurnBeamNoPotion,
            child_rollout_policy: CombatSearchV2ChildRolloutPolicy::Immediate,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::RootFrontierSeed,
            potion_policy: CombatSearchV2PotionPolicy::SemanticBudgeted,
            max_potions_used: Some(2),
            ..CombatSearchV2Config::default()
        };

        let stack = CombatSearchPluginStack::from_config(&config);

        assert_eq!(
            stack.action_prior,
            CombatSearchActionPriorPluginId::KeyCardOnline
        );
        assert_eq!(
            stack.phase_guard,
            CombatSearchPhaseGuardPluginId::TimeEaterClockHint
        );
        assert_eq!(stack.rollout, CombatSearchRolloutPluginId::TurnBeamNoPotion);
        assert_eq!(
            stack.child_rollout,
            CombatSearchChildRolloutPluginId::Immediate
        );
        assert_eq!(
            stack.turn_plan,
            CombatSearchTurnPlanPluginId::RootFrontierSeed
        );
        assert_eq!(
            stack.potion.policy,
            CombatSearchV2PotionPolicy::SemanticBudgeted
        );
        assert_eq!(stack.potion.max_potions_used, Some(2));
    }

    #[test]
    fn profile_builder_can_set_core_search_plugins() {
        let profile = CombatSearchProfile {
            label: "test",
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 7,
                    wall_ms: 11,
                },
                plugins: CombatSearchPluginStack::default(),
            },
            policy: CombatSearchAttemptPolicy {
                acceptance: CombatSearchAcceptancePluginId::AcceptedLineOnly,
                artifacts: CombatSearchArtifactPluginId::None,
            },
        }
        .with_action_prior_plugin(CombatSearchActionPriorPluginId::KeyCardOnline)
        .with_turn_plan_plugin(CombatSearchTurnPlanPluginId::RootFrontierSeed)
        .with_child_rollout_plugin(CombatSearchChildRolloutPluginId::Immediate);

        let config = profile.to_config();

        assert_eq!(
            config.setup_bias_policy,
            CombatSearchV2SetupBiasPolicy::KeyCardOnline
        );
        assert_eq!(
            config.turn_plan_policy,
            CombatSearchV2TurnPlanPolicy::RootFrontierSeed
        );
        assert_eq!(
            config.child_rollout_policy,
            CombatSearchV2ChildRolloutPolicy::Immediate
        );
    }
}
