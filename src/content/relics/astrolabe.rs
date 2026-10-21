use crate::content::cards::CardId;
use crate::content::relics::RelicId;
use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn on_equip(run_state: &mut RunState, return_state: EngineState) -> Option<EngineState> {
    let purgeable_lineages = purgeable_card_lineages(run_state);
    let purgeable_count = purgeable_lineages.len();
    if purgeable_count == 0 {
        return None;
    }

    if purgeable_count <= 3 {
        run_state.transform_card_lineages_deferred_obtain_with_source(
            &purgeable_lineages,
            true,
            DomainEventSource::Relic(RelicId::Astrolabe),
        );
        return None;
    }

    Some(EngineState::RunPendingChoice(RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 3,
        max_choices: 3,
        reason: RunPendingChoiceReason::TransformUpgraded,
        source: DomainEventSource::Relic(RelicId::Astrolabe),
        return_state: Box::new(return_state),
    }))
}

fn purgeable_card_lineages(run_state: &RunState) -> Vec<crate::runtime::combat::CardLineageId> {
    run_state
        .master_deck
        .iter()
        .filter(|card| is_purgeable_for_astrolabe(card.id))
        .map(|card| card.lineage)
        .collect()
}

fn is_purgeable_for_astrolabe(card_id: CardId) -> bool {
    !matches!(
        card_id,
        CardId::AscendersBane | CardId::CurseOfTheBell | CardId::Necronomicurse
    )
}
