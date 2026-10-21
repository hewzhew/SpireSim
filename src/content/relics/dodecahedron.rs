use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub struct Dodecahedron;

impl Dodecahedron {
    pub fn at_turn_start(_state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
        smallvec::smallvec![ActionInfo {
            action: Action::DodecahedronTurnStartCheck,
            insertion_mode: AddTo::Bottom,
        }]
    }

    pub fn turn_start_check(state: &mut CombatState) {
        if state.entities.player.current_hp >= state.entities.player.max_hp {
            state.queue_action_back(Action::GainEnergy { amount: 1 });
        }
    }
}
