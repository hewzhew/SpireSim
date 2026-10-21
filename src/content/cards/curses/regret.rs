use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

/// Regret: Unplayable. At the end of your turn, lose HP equal to the number of cards in your hand.
///
/// Java snapshots `hand.size()` into this card's base/magic number when it is
/// enqueued.  Other end-turn queued cards may leave the hand before this one
/// resolves, so resolution must use the captured card value rather than count
/// the then-current hand again.
pub fn on_end_turn_in_hand(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    damage_actions(state.zones.hand.len() as i32)
}

pub fn on_end_turn_queued_card(card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    damage_actions(card.base_magic_num_mut.max(0))
}

fn damage_actions(hand_size: i32) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::Damage(DamageInfo {
            source: 0,
            target: 0, // 0 is player
            base: hand_size,
            output: hand_size,
            damage_type: DamageType::HpLoss,
            is_modified: false,
        }),
        insertion_mode: AddTo::Bottom,
    }]
}
