use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{QueuedCardPlay, QueuedCardSource};
use crate::state::selection::DomainEventSource;
use smallvec::SmallVec;

/// Necronomicon: The first Attack you play each turn that costs 2 or more is played twice.
/// Java uses an internal `activated` boolean rather than the visible relic counter.
/// We model this with `used_up = false` meaning available this turn.
pub fn at_turn_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.used_up = false;
}

pub fn on_equip(run_state: &mut crate::state::run::RunState) {
    run_state.add_card_to_deck_with_upgrades_from(
        crate::content::cards::CardId::Necronomicurse,
        0,
        DomainEventSource::Relic(crate::content::relics::RelicId::Necronomicon),
    );
}

pub fn on_unequip(run_state: &mut crate::state::run::RunState, source: DomainEventSource) {
    if let Some(pos) = run_state
        .master_deck
        .iter()
        .position(|card| card.id == crate::content::cards::CardId::Necronomicurse)
    {
        let lineage = run_state.master_deck[pos].lineage;
        run_state.remove_card_from_deck_without_removal_hooks_with_source(lineage, source);
    }
}

pub fn on_use_card(
    card_id: crate::content::cards::CardId,
    card_cost_for_turn: i32,
    relic_state: &mut crate::content::relics::RelicState,
    card: &crate::runtime::combat::CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[crate::runtime::action::ActionInfo; 4]> {
    let def = crate::content::cards::get_card_definition(card_id);
    let mut actions = SmallVec::new();

    let meets_cost_threshold = (card_cost_for_turn >= 2 && !card.free_to_play_once)
        || (def.cost == -1 && card.energy_on_use >= 2);

    if !relic_state.used_up
        && def.card_type == crate::content::cards::CardType::Attack
        && meets_cost_threshold
    {
        relic_state.used_up = true;
        actions.push(ActionInfo {
            action: Action::EnqueueCardPlay {
                item: Box::new(QueuedCardPlay::same_instance_replay_java(
                    card,
                    target,
                    QueuedCardSource::Necronomicon,
                )),
                in_front: true,
            },
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}
