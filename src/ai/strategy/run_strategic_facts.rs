use crate::content::cards::{get_card_definition, is_starter_basic, CardType};
use crate::content::relics::{energy_master_delta, RelicId};
use crate::runtime::combat::CombatCard;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunStrategicFacts {
    pub entering_act: u8,
    pub starter_basic_count: usize,
    pub curse_count: usize,
    pub has_energy_relic: bool,
    pub has_runic_pyramid: bool,
}

impl RunStrategicFacts {
    pub fn from_run_state(run_state: &RunState) -> Self {
        let relics = run_state
            .relics
            .iter()
            .map(|relic| relic.id)
            .collect::<Vec<_>>();
        Self::from_public_state(run_state.act_num, &run_state.master_deck, &relics)
    }

    /// Rebuilds the same strategic facts from state that is safe to expose to
    /// a public actor. Card and relic occurrence identities are irrelevant to
    /// these facts; only their public definitions and the current act matter.
    pub fn from_public_state(current_act: u8, deck: &[CombatCard], relics: &[RelicId]) -> Self {
        Self {
            entering_act: current_act.saturating_add(1),
            starter_basic_count: deck.iter().filter(|card| is_starter_basic(card.id)).count(),
            curse_count: deck
                .iter()
                .filter(|card| get_card_definition(card.id).card_type == CardType::Curse)
                .count(),
            has_energy_relic: relics.iter().any(|relic| energy_master_delta(*relic) > 0),
            has_runic_pyramid: relics.contains(&RelicId::RunicPyramid),
        }
    }

    pub fn has_act2_energy_gap(&self) -> bool {
        self.entering_act == 2 && !self.has_energy_relic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::{RelicId, RelicState};

    #[test]
    fn strategic_facts_report_runic_pyramid_without_policy_judgment() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.relics.push(RelicState::new(RelicId::RunicPyramid));

        assert!(RunStrategicFacts::from_run_state(&run).has_runic_pyramid);
    }
}
