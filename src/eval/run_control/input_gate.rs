use crate::sim::combat_legal_actions::is_legal_move;
use crate::state::core::{ClientInput, EngineState};
use crate::state::selection::{SelectionResolution, SelectionScope, SelectionTargetRef};

use super::session::RunControlSession;

impl RunControlSession {
    pub(super) fn validate_input_for_current_state(
        &self,
        input: &ClientInput,
    ) -> Result<(), String> {
        if self.visible_candidate_allows_input(input)
            || self.current_screen_allows_extra_input(input)
            || self.run_level_potion_input_is_allowed(input)
        {
            return Ok(());
        }
        Err(format!(
            "input `{}` is not valid on the current screen: {}",
            crate::eval::run_control::view_model::client_input_hint(input),
            crate::eval::run_control::decision_surface::build_decision_surface(self)
                .view
                .header
                .title
        ))
    }

    fn visible_candidate_allows_input(&self, input: &ClientInput) -> bool {
        let surface = crate::eval::run_control::decision_surface::build_decision_surface(self);
        crate::eval::run_control::decision_surface::surface_allows_visible_input(&surface, input)
    }

    fn current_screen_allows_extra_input(&self, input: &ClientInput) -> bool {
        if let Some(allowed) =
            crate::eval::run_control::selection_surface::current_selection_input_is_allowed(
                self, input,
            )
        {
            return allowed;
        }
        match (&self.engine_state, input) {
            (
                EngineState::CombatPlayerTurn
                | EngineState::CombatProcessing
                | EngineState::PendingChoice(_),
                _,
            ) => self
                .current_combat_position_for_actions()
                .map(|position| is_legal_move(&position.engine, &position.combat, input))
                .unwrap_or(false),
            (EngineState::MapOverlay { .. }, ClientInput::Cancel) => true,
            (EngineState::RunPendingChoice(choice), ClientInput::SubmitSelection(resolution)) => {
                self.run_pending_resolution_is_allowed(choice, resolution)
            }
            (EngineState::RunPendingChoice(_), ClientInput::Cancel) => true,
            (EngineState::Shop(shop), ClientInput::PurgeCard(idx)) => {
                self.shop_purge_is_allowed(shop, *idx)
            }
            (EngineState::RewardScreen(reward), ClientInput::Cancel) => {
                reward.skippable || reward.pending_card_choice.is_some()
            }
            (EngineState::RewardOverlay { reward_state, .. }, ClientInput::Cancel) => {
                reward_state.skippable || reward_state.pending_card_choice.is_some()
            }
            _ => false,
        }
    }

    pub(in crate::eval::run_control) fn run_pending_selection_is_allowed(
        &self,
        choice: &crate::state::core::RunPendingChoiceState,
        indices: &[usize],
    ) -> bool {
        if indices.len() < choice.min_choices.saturating_sub(choice.selected.len())
            || indices.len() > choice.max_choices.saturating_sub(choice.selected.len())
        {
            return false;
        }
        let mut seen = Vec::new();
        for &idx in indices {
            let Some(card) = self.run_state.master_deck.get(idx) else {
                return false;
            };
            if seen.contains(&idx)
                || choice.selected.contains(&card.lineage)
                || !crate::state::core::run_pending_choice_allows_card_for_run(
                    &choice.reason,
                    card,
                    &self.run_state,
                )
            {
                return false;
            }
            seen.push(idx);
        }
        true
    }

    pub(in crate::eval::run_control) fn run_pending_resolution_is_allowed(
        &self,
        choice: &crate::state::core::RunPendingChoiceState,
        resolution: &SelectionResolution,
    ) -> bool {
        if resolution.scope != SelectionScope::Deck {
            return false;
        }
        let indices = resolution
            .selected
            .iter()
            .filter_map(|target| match target {
                SelectionTargetRef::CardLineage(lineage) => self
                    .run_state
                    .master_deck
                    .iter()
                    .position(|card| card.lineage == *lineage),
            })
            .collect::<Vec<_>>();
        indices.len() == resolution.selected.len()
            && self.run_pending_selection_is_allowed(choice, &indices)
    }

    fn shop_purge_is_allowed(&self, shop: &crate::state::shop::ShopState, idx: usize) -> bool {
        shop.purge_available
            && self.run_state.gold >= shop.purge_cost
            && self.run_state.master_deck.get(idx).is_some_and(|card| {
                crate::state::core::master_deck_card_is_purgeable(card)
                    && !crate::state::core::master_deck_card_is_bottled(
                        card,
                        &self.run_state.relics,
                    )
            })
    }

    fn run_level_potion_input_is_allowed(&self, input: &ClientInput) -> bool {
        if !matches!(
            self.engine_state,
            EngineState::MapNavigation
                | EngineState::MapOverlay { .. }
                | EngineState::EventRoom
                | EngineState::RewardScreen(_)
                | EngineState::RewardOverlay { .. }
                | EngineState::TreasureRoom(_)
                | EngineState::Campfire
                | EngineState::Shop(_)
                | EngineState::RunPendingChoice(_)
                | EngineState::BossRelicSelect(_)
        ) {
            return false;
        }
        let is_we_meet_again = self
            .run_state
            .event_state
            .as_ref()
            .is_some_and(|event| event.id == crate::state::events::EventId::WeMeetAgain);
        match input {
            ClientInput::DiscardPotion(slot) => {
                crate::content::potions::potion_can_discard_in_event(is_we_meet_again)
                    && self
                        .run_state
                        .potions
                        .get(*slot)
                        .and_then(|slot| slot.as_ref())
                        .is_some_and(|potion| potion.can_discard)
            }
            ClientInput::UsePotion {
                potion_index,
                target,
            } if target.is_none() => self
                .run_state
                .potions
                .get(*potion_index)
                .and_then(|slot| slot.as_ref())
                .is_some_and(|potion| {
                    potion.can_use
                        && crate::content::potions::potion_can_use_out_of_combat(
                            potion.id,
                            is_we_meet_again,
                        )
                }),
            _ => false,
        }
    }
}
