use super::{RelicId, RelicState};
use crate::runtime::combat::CardLineageId;

/// Resolves the only run-persistent meaning of `RelicState::amount` without
/// exposing its compact integer encoding outside the simulator owner.
pub fn persistent_relic_bound_card_lineage_v1(
    relic: &RelicState,
) -> Result<Option<CardLineageId>, String> {
    let is_bottle = matches!(
        relic.id,
        RelicId::BottledFlame | RelicId::BottledLightning | RelicId::BottledTornado
    );
    if is_bottle {
        if relic.amount == 0 {
            return Ok(None);
        }
        return CardLineageId::from_runtime_i32_payload(relic.amount)
            .map(Some)
            .ok_or_else(|| "bottled relic contains an invalid card lineage".to_owned());
    }
    if relic.amount == 0 || relic_amount_is_combat_local_v1(relic.id) {
        Ok(None)
    } else {
        Err("persistent relic contains unclassified auxiliary state".to_owned())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentRelicCounterSemanticsV1 {
    Exact,
    CyclicProgress { trigger_at: i32 },
}

pub const fn persistent_relic_counter_semantics_v1(
    relic: RelicId,
) -> PersistentRelicCounterSemanticsV1 {
    match relic {
        RelicId::Sundial => PersistentRelicCounterSemanticsV1::CyclicProgress {
            trigger_at: super::sundial::SHUFFLES_PER_TRIGGER,
        },
        RelicId::HappyFlower => PersistentRelicCounterSemanticsV1::CyclicProgress { trigger_at: 3 },
        RelicId::InkBottle | RelicId::Nunchaku | RelicId::PenNib => {
            PersistentRelicCounterSemanticsV1::CyclicProgress { trigger_at: 10 }
        }
        _ => PersistentRelicCounterSemanticsV1::Exact,
    }
}

/// Removes combat/turn-local implementation fields from a relic state before
/// it participates in post-combat resource identity. Unknown relic fields stay
/// exact; only fields whose owner explicitly resets them before the next
/// combat are canonicalized here.
pub fn post_combat_persistent_relic_state_v1(relic: &RelicState) -> RelicState {
    let mut projected = relic.clone();
    if relic_used_up_is_combat_local_v1(relic.id) {
        projected.used_up = false;
    }
    if relic_amount_is_combat_local_v1(relic.id) {
        projected.amount = 0;
    }
    if relic_counter_is_combat_local_v1(relic.id) {
        projected.counter = 0;
    }
    projected
}

pub fn post_combat_persistent_relic_states_equal_v1(
    left: &[RelicState],
    right: &[RelicState],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            post_combat_persistent_relic_state_v1(left)
                == post_combat_persistent_relic_state_v1(right)
        })
}

const fn relic_used_up_is_combat_local_v1(relic: RelicId) -> bool {
    matches!(
        relic,
        RelicId::AncientTeaSet
            | RelicId::CentennialPuzzle
            | RelicId::GamblingChip
            | RelicId::HornCleat
            | RelicId::HoveringKite
            | RelicId::Lantern
            | RelicId::Necronomicon
            | RelicId::RedSkull
            | RelicId::UnceasingTop
    )
}

const fn relic_amount_is_combat_local_v1(relic: RelicId) -> bool {
    matches!(
        relic,
        RelicId::Pocketwatch
            | RelicId::RunicCapacitor
            | RelicId::UnceasingTop
            | RelicId::SlaversCollar
    )
}

/// Whether this counter resets before the next combat. Decision projections
/// use this same owner classification when displaying persistent resources.
pub const fn relic_counter_is_combat_local_v1(relic: RelicId) -> bool {
    matches!(
        relic,
        RelicId::ArtOfWar
            | RelicId::CaptainsWheel
            | RelicId::EmotionChip
            | RelicId::HornCleat
            | RelicId::Kunai
            | RelicId::LetterOpener
            | RelicId::OrangePellets
            | RelicId::OrnamentalFan
            | RelicId::Pocketwatch
            | RelicId::Shuriken
            | RelicId::StoneCalendar
            | RelicId::VelvetChoker
    )
}

/// Returns whether two terminal relic states are equivalent for hard-resource
/// dominance. Known combat-local implementation fields are ignored; unknown
/// counters, charges, `used_up`, and `amount` remain exact. Known cyclic
/// accumulators may differ only inside the same readiness band; being near or
/// immediately ready to trigger is never erased.
pub fn persistent_relic_states_equivalent_for_resource_dominance_v1(
    left: &[RelicState],
    right: &[RelicState],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| persistent_relic_state_equivalent_v1(left, right))
}

/// Returns whether `left` has Pareto-better cyclic progress than `right`
/// without crossing the readiness bands used by hard-resource comparison.
/// Exact/unknown counters must remain equal, and a gain on one cyclic relic
/// never compensates for losing progress on another.
pub fn persistent_relic_states_strictly_dominate_within_resource_equivalence_v1(
    left: &[RelicState],
    right: &[RelicState],
) -> bool {
    if !persistent_relic_states_equivalent_for_resource_dominance_v1(left, right) {
        return false;
    }
    let mut strictly_better = false;
    for (left, right) in left.iter().zip(right) {
        match persistent_relic_counter_semantics_v1(left.id) {
            PersistentRelicCounterSemanticsV1::Exact => {}
            PersistentRelicCounterSemanticsV1::CyclicProgress { trigger_at } => {
                if progress_readiness_band_v1(left.counter, trigger_at).is_none() {
                    continue;
                }
                if left.counter < right.counter {
                    return false;
                }
                strictly_better |= left.counter > right.counter;
            }
        }
    }
    strictly_better
}

fn persistent_relic_state_equivalent_v1(left: &RelicState, right: &RelicState) -> bool {
    let left = post_combat_persistent_relic_state_v1(left);
    let right = post_combat_persistent_relic_state_v1(right);
    if left.id != right.id || left.used_up != right.used_up || left.amount != right.amount {
        return false;
    }
    match persistent_relic_counter_semantics_v1(left.id) {
        PersistentRelicCounterSemanticsV1::Exact => left.counter == right.counter,
        PersistentRelicCounterSemanticsV1::CyclicProgress { trigger_at } => {
            match (
                progress_readiness_band_v1(left.counter, trigger_at),
                progress_readiness_band_v1(right.counter, trigger_at),
            ) {
                (Some(left), Some(right)) => left == right,
                (None, None) => left.counter == right.counter,
                _ => false,
            }
        }
    }
}

fn progress_readiness_band_v1(counter: i32, trigger_at: i32) -> Option<u8> {
    if counter < 0 || counter >= trigger_at {
        return None;
    }
    let remaining = trigger_at - counter;
    Some(if remaining == 1 {
        2
    } else if remaining <= 3 {
        1
    } else {
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nunchaku(counter: i32) -> RelicState {
        let mut relic = RelicState::new(RelicId::Nunchaku);
        relic.counter = counter;
        relic
    }

    #[test]
    fn bottled_lineage_is_decoded_only_by_the_relic_state_owner() {
        let lineage = CardLineageId::master_deck(7);
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = lineage.to_runtime_i32_payload();

        assert_eq!(
            persistent_relic_bound_card_lineage_v1(&bottle).unwrap(),
            Some(lineage)
        );
        assert_eq!(
            persistent_relic_bound_card_lineage_v1(&RelicState::new(RelicId::BurningBlood))
                .unwrap(),
            None
        );

        let mut invalid = RelicState::new(RelicId::BurningBlood);
        invalid.amount = lineage.to_runtime_i32_payload();
        assert!(persistent_relic_bound_card_lineage_v1(&invalid).is_err());

        let mut combat_local = RelicState::new(RelicId::RunicCapacitor);
        combat_local.amount = 1;
        assert_eq!(
            persistent_relic_bound_card_lineage_v1(&combat_local).unwrap(),
            None
        );
    }

    #[test]
    fn remote_cyclic_progress_does_not_block_hard_resource_comparison() {
        assert!(
            persistent_relic_states_equivalent_for_resource_dominance_v1(
                &[nunchaku(1)],
                &[nunchaku(3)],
            )
        );
    }

    #[test]
    fn higher_progress_strictly_dominates_only_within_one_readiness_band() {
        assert!(
            persistent_relic_states_strictly_dominate_within_resource_equivalence_v1(
                &[nunchaku(5)],
                &[nunchaku(3)],
            )
        );
        assert!(
            !persistent_relic_states_strictly_dominate_within_resource_equivalence_v1(
                &[nunchaku(3)],
                &[nunchaku(5)],
            )
        );
        assert!(
            !persistent_relic_states_strictly_dominate_within_resource_equivalence_v1(
                &[nunchaku(8)],
                &[nunchaku(6)],
            )
        );
    }

    #[test]
    fn cyclic_progress_tradeoff_remains_incomparable() {
        let mut ink_bottle_left = RelicState::new(RelicId::InkBottle);
        ink_bottle_left.counter = 2;
        let mut ink_bottle_right = ink_bottle_left.clone();
        ink_bottle_right.counter = 4;

        assert!(
            !persistent_relic_states_strictly_dominate_within_resource_equivalence_v1(
                &[nunchaku(5), ink_bottle_left],
                &[nunchaku(3), ink_bottle_right],
            )
        );
    }

    #[test]
    fn near_and_ready_progress_remain_materially_distinct() {
        assert!(
            !persistent_relic_states_equivalent_for_resource_dominance_v1(
                &[nunchaku(6)],
                &[nunchaku(8)],
            )
        );
        assert!(
            !persistent_relic_states_equivalent_for_resource_dominance_v1(
                &[nunchaku(8)],
                &[nunchaku(9)],
            )
        );
    }

    #[test]
    fn unknown_relic_counters_remain_exact() {
        let mut left = RelicState::new(RelicId::IncenseBurner);
        left.counter = 1;
        let mut right = left.clone();
        right.counter = 2;

        assert!(!persistent_relic_states_equivalent_for_resource_dominance_v1(&[left], &[right],));

        assert!(
            !persistent_relic_states_equivalent_for_resource_dominance_v1(
                &[nunchaku(-1)],
                &[nunchaku(-2)],
            )
        );
    }

    #[test]
    fn combat_and_turn_local_fields_are_not_post_combat_resources() {
        let inactive = RelicState::new(RelicId::RedSkull);
        let mut active = inactive.clone();
        active.used_up = true;

        let pellets_clear = RelicState::new(RelicId::OrangePellets);
        let mut pellets_partial = pellets_clear.clone();
        pellets_partial.counter = 0b011;

        assert_eq!(
            post_combat_persistent_relic_state_v1(&inactive),
            post_combat_persistent_relic_state_v1(&active),
        );
        assert_eq!(
            post_combat_persistent_relic_state_v1(&pellets_clear),
            post_combat_persistent_relic_state_v1(&pellets_partial),
        );
        assert!(
            persistent_relic_states_equivalent_for_resource_dominance_v1(&[inactive], &[active],)
        );
    }
}
