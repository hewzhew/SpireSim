use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Decay,
    };

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
