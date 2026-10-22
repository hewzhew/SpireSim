use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::DarkEmbrace,
        round_duration: false,
    };

pub fn on_exhaust(state: &CombatState, amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    if state.are_monsters_basically_dead_java() {
        return actions;
    }
    actions.push(Action::DrawCards(amount as u32));
    actions
}
