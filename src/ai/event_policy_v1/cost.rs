use crate::content::relics::RelicId;
use crate::state::run::RunState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventCostProjectionV1 {
    pub nominal_hp_loss: i32,
    pub effective_hp_loss: i32,
    pub modifiers: Vec<EventCostModifierV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventCostModifierV1 {
    TungstenRod,
}

pub fn project_hp_loss_cost_v1(run_state: &RunState, losses: &[i32]) -> EventCostProjectionV1 {
    let has_tungsten_rod = run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::TungstenRod);
    let mut effective_hp_loss = 0;
    let mut nominal_hp_loss = 0;

    for loss in losses {
        let nominal = (*loss).max(0);
        nominal_hp_loss += nominal;
        let effective = if has_tungsten_rod && nominal > 0 {
            nominal - 1
        } else {
            nominal
        };
        effective_hp_loss += effective;
    }

    let modifiers = if has_tungsten_rod && losses.iter().any(|loss| *loss > 0) {
        vec![EventCostModifierV1::TungstenRod]
    } else {
        Vec::new()
    };

    EventCostProjectionV1 {
        nominal_hp_loss,
        effective_hp_loss,
        modifiers,
    }
}
