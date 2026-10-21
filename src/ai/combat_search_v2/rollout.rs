mod no_potion;
mod node;
mod turn_beam;

#[cfg(test)]
use super::rollout_pending_choice::RolloutPendingChoiceProgress;
#[cfg(test)]
use super::*;

pub(super) const DEFAULT_ROLLOUT_MAX_EVALUATIONS: usize = 384;
pub(super) const DEFAULT_ROLLOUT_MAX_ACTIONS: usize = 80;
pub(super) const DEFAULT_TURN_BEAM_WIDTH: usize = 3;

pub(super) use no_potion::{
    conservative_no_potion_rollout, phase_aware_no_potion_rollout,
    phase_aware_no_potion_rollout_for_contract,
};
#[cfg(test)]
pub(super) use turn_beam::turn_beam_no_potion_rollout;
pub(super) use turn_beam::{
    turn_beam_conservative_anchor_rollout, turn_beam_extension_rollout_with_attribution,
    TurnBeamExtensionAttribution,
};

#[cfg(test)]
mod tests;
