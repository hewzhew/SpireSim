use crate::state::core::EngineState;
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn on_equip(run_state: &mut RunState) -> Option<EngineState> {
    use crate::content::cards::{get_card_definition, CardId, CardType};
    use crate::content::relics::RelicId;
    let mut upgradable_indices: Vec<usize> = run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let def = get_card_definition(c.id);
            def.card_type == CardType::Attack && (c.id == CardId::SearingBlow || c.upgrades == 0)
        })
        .map(|(i, _)| i)
        .collect();

    if !upgradable_indices.is_empty() {
        crate::runtime::rng::shuffle_with_random_long(
            &mut upgradable_indices,
            &mut run_state.rng_pool.misc_rng,
        );
        let selected_lineages: Vec<crate::runtime::combat::CardLineageId> = upgradable_indices
            .iter()
            .take(2)
            .map(|&idx| run_state.master_deck[idx].lineage)
            .collect();
        for lineage in selected_lineages {
            run_state
                .upgrade_card_with_source(lineage, DomainEventSource::Relic(RelicId::Whetstone));
        }
    }
    None
}
