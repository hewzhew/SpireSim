use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub const MECHANICS: &str = "Unplayable. If still in hand at the end of your turn, deal 2 damage to yourself before the ordinary hand discard. Block can absorb this damage. The card is discarded normally and can return in later draws; discarding or Exhausting it before this trigger avoids the damage. Combat Exhaust does not remove it from your permanent deck.";

pub fn on_end_turn_in_hand(_state: &CombatState) -> smallvec::SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::Damage(crate::runtime::action::DamageInfo {
            source: 0,
            target: 0,
            base: 2,
            output: 2,
            damage_type: crate::runtime::action::DamageType::Thorns,
            is_modified: false,
        }),
        insertion_mode: AddTo::Bottom,
    }]
}
