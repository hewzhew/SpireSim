//! Active compilation owner for AtomicExactV2.
//!
//! The retained tactical-policy adapter remains temporarily co-located while
//! its mature semantic evaluation is separated from atomic-private types.
//! That transitional adapter does not make this crate a general policy owner.

extern crate self as sts_simulator;

pub use sts_core::{content, engine, fixtures, runtime, sim, state, test_support, EntityId};

pub mod ai;
mod tactical_policy;

/// Narrow exact-turn kernel consumed by the complete-turn proposal owner.
///
/// The proposal crate must not depend on the AtomicExactV2 implementation
/// namespace or on whole-search reports. This facade exposes only the bounded
/// turn-prefix enumerator and the typed facts needed to project proposals.
pub mod complete_turn_kernel {
    pub use crate::ai::combat_search_v2::{
        enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided_with_deadline,
        CombatSearchV2ActionCardFacts, CombatSearchV2ActionExactDeltaFacts,
        CombatSearchV2ActionFacts, CombatSearchV2ActionImmediateFacts,
        CombatSearchV2ActionMechanicsFacts, CombatSearchV2ActionPendingChoiceSelectionFacts,
        CombatSearchV2ActionPendingChoiceSelectionRole, CombatSearchV2Config,
        CombatSearchV2PotionPolicy, CombatSearchV2TurnPlanPartialRetentionAuditV1,
        CombatSearchV2TurnPlanPartialRetentionLaneV1, CombatSearchV2TurnPlanPrefixSessionV1,
        CombatSearchV2TurnPlanProbeEnumeration, CombatSearchV2TurnPlanProbeGuideRankV1,
        SearchTerminalLabel,
    };
}

pub use tactical_policy::{retained_combat_tactical_policy_v1, RetainedCombatTacticalPolicyV1};
