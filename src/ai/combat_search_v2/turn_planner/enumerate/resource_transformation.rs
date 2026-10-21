use std::collections::BTreeMap;

use crate::runtime::action::Action;
use crate::state::core::ClientInput;

use super::work::TurnPlanWorkNode;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct PartialContinuationProjectionScore {
    pub(super) primary: i32,
    pub(super) secondary: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PartialContinuationProjection {
    pub(super) family: &'static str,
    pub(super) score: PartialContinuationProjectionScore,
}

/// Project only runtime transformations whose input and direct output are
/// completely determined by the current state without replaying another card.
/// Additional transformations belong here only after their card-cost and
/// trigger ordering has an equally explicit contract.
pub(super) fn continuation_resource_projections(
    work: &TurnPlanWorkNode,
) -> Vec<PartialContinuationProjection> {
    let Some(actions) = work.cached_eligible_actions.as_ref() else {
        return Vec::new();
    };
    let combat = &work.node.combat;
    let mut best_by_family = BTreeMap::<&'static str, PartialContinuationProjectionScore>::new();
    for choice in actions {
        let ClientInput::PlayCard { card_index, target } = &choice.input else {
            continue;
        };
        let Some(card) = combat.zones.hand.get(*card_index) else {
            continue;
        };
        for action in crate::content::cards::resolve_card_play(card.id, combat, card, *target) {
            let projection = match action.action {
                Action::DoubleBlock { target } if target == combat.entities.player.id => {
                    let block = combat.entities.player.block.max(0);
                    Some((
                        "double_block",
                        PartialContinuationProjectionScore {
                            primary: block.saturating_mul(2).min(999),
                            secondary: block,
                        },
                    ))
                }
                Action::BlockPerNonAttack { block_per_card } => {
                    let target_count = combat
                        .zones
                        .hand
                        .iter()
                        .enumerate()
                        .filter(|(index, target)| {
                            *index != *card_index
                                && crate::content::cards::get_card_definition(target.id).card_type
                                    != crate::content::cards::CardType::Attack
                        })
                        .count() as i32;
                    let feel_no_pain = crate::content::powers::store::power_amount(
                        combat,
                        combat.entities.player.id,
                        crate::content::powers::PowerId::FeelNoPain,
                    )
                    .max(0);
                    let block_per_target = block_per_card.max(0).saturating_add(feel_no_pain);
                    let projected_block = combat
                        .entities
                        .player
                        .block
                        .saturating_add(block_per_target.saturating_mul(target_count))
                        .min(999);
                    Some((
                        "non_attack_hand_to_block",
                        PartialContinuationProjectionScore {
                            primary: projected_block,
                            secondary: target_count,
                        },
                    ))
                }
                _ => None,
            };
            let Some((family, score)) = projection else {
                continue;
            };
            let entry = best_by_family.entry(family).or_insert(score);
            *entry = (*entry).max(score);
        }
    }
    best_by_family
        .into_iter()
        .map(|(family, score)| PartialContinuationProjection { family, score })
        .collect()
}
