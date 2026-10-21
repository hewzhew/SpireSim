use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "cards drawn per card Exhausted",
    effect: "While active, whenever any card is Exhausted, draw amount cards. Multiple deployed copies add their amounts: amount 2 draws 2 per Exhaust. Does not give Energy. No Draw prevents the draw; no further draw is queued once combat has ended.",
    timing: "On Exhaust, after deployment; persists for the rest of combat. Merely owning or holding the card does not activate this trigger.",
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
