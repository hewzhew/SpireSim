//! Opt-in private discard-group shuffle evidence. Never part of the public
//! draw receipt or combat identity. Other RNG uses (e.g. reshuffling the whole
//! draw pile) are not recorded here; callers must check their transition scope.

use super::{CardLineageId, CombatState};
use crate::{runtime::rng::StsRng, state::DomainCardSnapshot};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct OracleShuffleReceipt {
    /// Discard group in the exact order passed to the shuffle function.
    pub input: Vec<DomainCardSnapshot>,
    /// Shuffled group in Rust draw order (top first), before further effects.
    pub output: Vec<CardLineageId>,
    pub rng_before: StsRng,
    pub rng_after: StsRng,
}

/// One pointer when disabled; no receipt allocation or cloning in normal search.
#[derive(Debug, Default)]
pub(crate) struct OracleShuffleObservation(pub Option<Box<Vec<OracleShuffleReceipt>>>);

impl Clone for OracleShuffleObservation {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for OracleShuffleObservation {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl CombatState {
    pub(crate) fn begin_oracle_shuffle_observation(&mut self) {
        self.runtime.oracle_shuffle_observation.0 = Some(Box::default());
    }

    pub(crate) fn take_oracle_shuffle_observation(&mut self) -> Option<Vec<OracleShuffleReceipt>> {
        self.runtime.oracle_shuffle_observation.0.take().map(|v| *v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content::cards::CardId, runtime::combat::CombatCard};

    #[test]
    fn oracle_receipts_do_not_change_or_travel_with_combat_state() {
        let mut observed = crate::test_support::blank_test_combat();
        observed.zones.discard_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ]
        .into();
        observed.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Bash, 3)].into();
        let mut plain = observed.clone();
        observed.begin_oracle_shuffle_observation();
        let mut cloned = observed.clone();
        assert!(cloned.take_oracle_shuffle_observation().is_none());
        assert_eq!(observed, plain);
        assert_eq!(
            serde_json::to_value(&observed).unwrap(),
            serde_json::to_value(&plain).unwrap()
        );

        // Owner evidence records only the shuffled prefix, not the old draw tail.
        for state in [&mut observed, &mut plain] {
            state.shuffle_discard_pile_into_draw_pile();
            state
                .zones
                .discard_pile
                .push(CombatCard::for_scenario(CardId::Wound, 4));
            state.shuffle_discard_pile_into_draw_pile();
        }
        assert_eq!(observed, plain);
        assert_eq!(
            serde_json::to_value(&observed).unwrap(),
            serde_json::to_value(&plain).unwrap()
        );
        let receipts = observed.take_oracle_shuffle_observation().unwrap();
        assert_eq!(receipts.len(), 2);
        assert_eq!(
            receipts[0].input.iter().map(|c| c.id).collect::<Vec<_>>(),
            [CardId::Strike, CardId::Defend]
        );
        assert_eq!(receipts[1].input[0].id, CardId::Wound);
        assert_eq!(receipts[0].rng_after, receipts[1].rng_before);
        assert_eq!(receipts[1].rng_after, observed.rng.shuffle_rng);
        assert_eq!(
            observed
                .zones
                .draw_pile
                .iter()
                .map(|c| c.lineage)
                .collect::<Vec<_>>(),
            receipts[1]
                .output
                .iter()
                .chain(&receipts[0].output)
                .copied()
                .chain([CardLineageId::constructed_scenario(3)])
                .collect::<Vec<_>>()
        );
        assert!(plain.take_oracle_shuffle_observation().is_none());
        observed.begin_oracle_shuffle_observation();
        observed.clear_transition_observations();
        assert!(observed.take_oracle_shuffle_observation().is_none());
    }
}
