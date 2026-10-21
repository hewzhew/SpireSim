//! Shared public-only admission for the deterministic model and task surfaces.
//!
//! Runtime legality remains owned by the simulator. This module only removes
//! mechanically dominated clicks from the policy surface, using facts present
//! in both public projections. It deliberately returns ordinals from the
//! caller's own surface instead of assuming two projections share ordinals.

use crate::agent::information::action::PublicCombatAtomicActionV1;
use crate::agent::information::state::PublicCombatStateV1;
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublicStrategicAdmissionKindV1 {
    FreeGoldReward,
    PotionReward,
    OpenCardReward,
    DiscardRunPotion,
    /// The learning projection commits a route. Browsing task actors keep
    /// their ordinary Cancel action so they can return to pending rewards.
    MapCancel,
    Other,
}

/// Preserves the historical learned-policy reward reduction without binding
/// it to planner ids, private resolution, or one candidate representation.
pub(crate) fn admitted_strategic_candidate_ordinals_v1(
    is_reward_context: bool,
    defer_potion_discard: bool,
    has_empty_potion_slot: bool,
    has_sozu: bool,
    candidates: impl IntoIterator<Item = (usize, PublicStrategicAdmissionKindV1)>,
) -> Vec<usize> {
    let candidates = candidates.into_iter().collect::<Vec<_>>();
    if is_reward_context {
        if let Some((ordinal, _)) = candidates
            .iter()
            .find(|(_, kind)| *kind == PublicStrategicAdmissionKindV1::FreeGoldReward)
        {
            return vec![*ordinal];
        }

        if has_empty_potion_slot && !has_sozu {
            let potion_rewards = candidates
                .iter()
                .filter(|(_, kind)| *kind == PublicStrategicAdmissionKindV1::PotionReward)
                .map(|(ordinal, _)| *ordinal)
                .collect::<Vec<_>>();
            if potion_rewards.len() == 1 {
                return potion_rewards;
            }
        }
    }

    // Card reveal is reversible: Skip closes the page but leaves the item.
    // Keep reveal and navigation together; runtime legality is unchanged.
    // A free slot can hold the existing potion without cost. Sozu prevents
    // obtaining a replacement at all. Map and card-choice pages have no current
    // potion acquisition to make room for; later rewards can be handled then.
    // Keep engine legality for manual play.
    let useful = candidates
        .iter()
        .filter(|(_, kind)| {
            *kind != PublicStrategicAdmissionKindV1::MapCancel
                && (*kind != PublicStrategicAdmissionKindV1::DiscardRunPotion
                    || (!defer_potion_discard && !has_empty_potion_slot && !has_sozu))
        })
        .map(|(ordinal, _)| *ordinal)
        .collect::<Vec<_>>();
    if !useful.is_empty() {
        return useful;
    }

    // An exceptional surface with only discard actions must remain executable.
    candidates.into_iter().map(|(ordinal, _)| ordinal).collect()
}

/// Discarding a potion has no immediate combat effect. It enters the public
/// policy surface only when another currently admitted public action can refill
/// the opened slot at this unchanged decision.
pub(crate) fn public_combat_atomic_candidate_allowed_v1(
    observation: &PublicCombatStateV1,
    action: &PublicCombatAtomicActionV1,
    admitted_atomic_actions: &[&PublicCombatAtomicActionV1],
) -> bool {
    let PublicCombatAtomicActionV1::DiscardPotion {
        potion_index: discarded_slot,
    } = action
    else {
        return true;
    };
    if observation
        .player
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Sozu)
    {
        return false;
    }

    admitted_atomic_actions
        .iter()
        .any(|candidate| match candidate {
            PublicCombatAtomicActionV1::UsePotion { potion_index, .. }
                if potion_index != discarded_slot =>
            {
                observation
                    .potions
                    .get(*potion_index)
                    .and_then(Option::as_ref)
                    .is_some_and(|potion| potion.potion_id == PotionId::EntropicBrew)
            }
            PublicCombatAtomicActionV1::PlayCard { hand_index, .. } => observation
                .cards
                .hand
                .cards
                .get(*hand_index)
                .is_some_and(|card| card.card_id == CardId::Alchemize),
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_close_is_navigation_while_other_cancel_actions_remain_choices() {
        assert_eq!(
            admitted_strategic_candidate_ordinals_v1(
                false,
                true,
                false,
                false,
                [
                    (0, PublicStrategicAdmissionKindV1::Other),
                    (1, PublicStrategicAdmissionKindV1::MapCancel),
                    (2, PublicStrategicAdmissionKindV1::Other),
                ],
            ),
            vec![0, 2]
        );
    }

    #[test]
    fn strategic_empty_slot_excludes_discard_without_removing_progress_actions() {
        let candidates = [
            (0, PublicStrategicAdmissionKindV1::Other),
            (1, PublicStrategicAdmissionKindV1::DiscardRunPotion),
            (2, PublicStrategicAdmissionKindV1::OpenCardReward),
        ];
        for is_reward in [false, true] {
            assert_eq!(
                admitted_strategic_candidate_ordinals_v1(is_reward, false, true, false, candidates),
                vec![0, 2]
            );
        }
        assert_eq!(
            admitted_strategic_candidate_ordinals_v1(false, false, false, false, candidates),
            vec![0, 1, 2]
        );
        assert_eq!(
            admitted_strategic_candidate_ordinals_v1(false, true, false, false, candidates),
            vec![0, 2]
        );
        assert_eq!(
            admitted_strategic_candidate_ordinals_v1(false, false, false, true, candidates),
            vec![0, 2]
        );
    }
}
