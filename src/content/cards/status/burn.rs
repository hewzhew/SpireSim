use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};

pub fn on_end_turn_in_hand(
    _state: &CombatState,
    card: &CombatCard,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let damage = if card.upgrades > 0 { 4 } else { 2 };

    smallvec::smallvec![ActionInfo {
        action: Action::Damage(DamageInfo {
            source: 0,
            target: 0,
            base: damage,
            output: damage,
            damage_type: DamageType::Thorns,
            is_modified: false,
        }),
        insertion_mode: AddTo::Bottom,
    }]
}
