//! Neutral contracts for combat-local tactical guidance.
//!
//! This crate owns no search, continuation value, or runtime execution.
//! Implementations may name preferred selection members, rank legal actions and
//! already materialized exact states, but cannot remove actions or claim a
//! terminal result.

use std::sync::Arc;

use sts_core::sim::combat::CombatPosition;
use sts_core::sim::combat_action_surface::CombatSelectionActionFamilyV2;
use sts_core::state::core::ClientInput;

/// One exact choice on a concrete simulator action surface.
///
/// Structured selections remain a family here: their potentially large
/// member language is scheduled lazily by the consumer.
#[derive(Clone, Copy)]
pub enum CombatPolicyChoice<'a> {
    Atomic(&'a ClientInput),
    StructuredSelection(&'a CombatSelectionActionFamilyV2),
}

/// A bounded preference over concrete members of the supplied current family.
/// Consumers must validate membership and retain every unpreferred member.
/// These inputs are ephemeral guidance, not an executable or durable plan.
#[derive(Clone, Debug, Default)]
pub struct CombatSelectionPreferences(Box<[ClientInput]>);

impl CombatSelectionPreferences {
    pub const LIMIT: usize = 4;

    /// Retains at most four suggestions without enumerating the whole family.
    pub fn new(members: impl IntoIterator<Item = ClientInput>) -> Self {
        Self(members.into_iter().take(Self::LIMIT).collect())
    }

    pub fn members(&self) -> &[ClientInput] {
        &self.0
    }
}

/// Opaque, lexicographically ordered guidance for an exact combat state.
///
/// Consumers assign no meaning or units to the components. Higher components
/// are preferred only inside the guide's own independent lane.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct CombatStateGuideRank(Arc<[i32]>);

impl CombatStateGuideRank {
    pub fn new(components: impl Into<Vec<i32>>) -> Self {
        Self(components.into().into())
    }

    /// Read-only diagnostic view of the policy-owned rank.
    pub fn components(&self) -> &[i32] {
        self.0.as_ref()
    }
}

/// Stable identity for one independent guide queue.
///
/// Equal IDs mean equal semantics even when boundary and partial-turn guide
/// sets are requested separately. Different IDs are never compared merely
/// because they occupy the same vector position.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CombatGuideLaneId(u32);

impl CombatGuideLaneId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Stable lane identities shared by the retained tactical implementation and
/// independent decorators which target one semantic view. Keeping these IDs
/// in the contract crate prevents separately compiled owners from silently
/// disagreeing about which guide a typed bias addresses.
pub const COMBAT_PROGRESS_GUIDE_LANE_V1: CombatGuideLaneId = CombatGuideLaneId::new(1);
pub const COMBAT_SURVIVAL_GUIDE_LANE_V1: CombatGuideLaneId = CombatGuideLaneId::new(2);
pub const COMBAT_HORIZON_GUIDE_LANE_V1: CombatGuideLaneId = CombatGuideLaneId::new(3);
pub const COMBAT_SETUP_GUIDE_LANE_V1: CombatGuideLaneId = CombatGuideLaneId::new(4);
pub const COMBAT_TURN_DEPTH_GUIDE_LANE_V1: CombatGuideLaneId = CombatGuideLaneId::new(5);
/// Explicit laboratory guidance for preserving and cashing a Feed fatal payoff.
/// Production policies do not install this lane implicitly.
pub const COMBAT_FEED_PAYOFF_GUIDE_LANE_V1: CombatGuideLaneId = CombatGuideLaneId::new(6);

/// One opaque rank in one explicitly identified guide lane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatStateGuide {
    pub lane: CombatGuideLaneId,
    pub rank: CombatStateGuideRank,
}

impl CombatStateGuide {
    pub fn new(lane: CombatGuideLaneId, components: impl Into<Vec<i32>>) -> Self {
        Self {
            lane,
            rank: CombatStateGuideRank::new(components),
        }
    }

    pub fn from_rank(lane: CombatGuideLaneId, rank: CombatStateGuideRank) -> Self {
        Self { lane, rank }
    }
}

/// Guidance over legal actions and materialized exact states.
///
/// Returning a small weight never changes legality. Invalid weights are
/// treated as neutral by consumers rather than trusted as a hard exclusion.
pub trait CombatActionPolicy: Send + Sync {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64>;

    /// Sparse preferences for a lazy multi-card selection family. Empty is neutral.
    /// Inputs must use the current family's source-bound lineages/indices and
    /// order. Invalid or repeated suggestions receive no extra probability.
    /// Consumers own validation, the uniform fallback and work accounting.
    fn structured_selection_preferences(
        &self,
        _position: &CombatPosition,
        _family: &CombatSelectionActionFamilyV2,
    ) -> CombatSelectionPreferences {
        CombatSelectionPreferences::default()
    }

    /// Optional ordering inside a finite structured family after its concrete
    /// legal members are known. The default preserves uniform cursor order.
    fn structured_selection_member_weights(
        &self,
        _position: &CombatPosition,
        _family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        vec![1.0; members.len()]
    }

    /// Backward-compatible single-lane guidance for already materialized exact
    /// boundaries. It cannot change duplicate ownership or terminal truth.
    fn state_guide_rank(&self, _position: &CombatPosition) -> Option<CombatStateGuideRank> {
        None
    }

    /// Independent guide lanes over one shared exact-state graph. Keeping
    /// ranks separate avoids inventing calibration between unlike semantics.
    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.state_guide_rank(position)
            .map(|rank| CombatStateGuide::from_rank(CombatGuideLaneId::new(0), rank))
            .into_iter()
            .collect()
    }

    /// Guidance for partial states while constructing one complete player
    /// turn. This remains distinct from turn-boundary guidance because a good
    /// boundary value may discourage the investments needed to reach it.
    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.state_guides(position)
    }
}

pub type SharedCombatActionPolicy = Arc<dyn CombatActionPolicy>;

#[derive(Clone, Copy, Debug, Default)]
pub struct UniformCombatActionPolicy;

impl CombatActionPolicy for UniformCombatActionPolicy {
    fn weights(&self, _position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        vec![1.0; choices.len()]
    }
}

/// Typed relative service bias for one already defined guide lane.
///
/// This value does not define a scheduler or authorize extra total work. A
/// search consumer may use it only to redistribute an explicitly owned guide
/// allowance among lanes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatGuideServiceBias {
    pub lane: CombatGuideLaneId,
    pub extra_services_per_cycle: usize,
}

#[cfg(test)]
mod tests;
