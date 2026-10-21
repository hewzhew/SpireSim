use crate::engine::action_handlers::{cards, stances};

use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn try_execute(action: Action, state: &mut CombatState) -> Result<(), Action> {
    match action {
        // === Card domain ===
        Action::DrawCards(amount) => cards::handle_draw_cards(amount, state),
        Action::InnerPeace { draw_amount } => stances::handle_inner_peace(draw_amount, state),
        Action::Indignation { amount } => stances::handle_indignation(amount, state),
        Action::DrawForUniqueOrbTypes {
            amount_per_orb_type,
        } => cards::handle_draw_for_unique_orb_types(amount_per_orb_type, state),
        Action::DrawCardsWithHistory {
            amount,
            clear_history,
        } => cards::handle_draw_cards_with_history(amount, clear_history, state),
        Action::EscapePlanBlockIfSkill { block } => {
            cards::handle_escape_plan_block_if_skill(block, state)
        }
        Action::ScrapeFollowUp => cards::handle_scrape_follow_up(state),
        Action::ExpertiseDraw { target_hand_size } => {
            cards::handle_expertise_draw(target_hand_size, state)
        }
        Action::CalculatedGamble { draw_extra } => {
            cards::handle_calculated_gamble(draw_extra, state)
        }
        Action::BladeFury { upgraded } => cards::handle_blade_fury(upgraded, state),
        Action::ApplyBulletTime => cards::handle_apply_bullet_time(state),
        Action::UnloadNonAttack => cards::handle_unload_non_attack(state),
        Action::EmptyDeckShuffle => cards::handle_empty_deck_shuffle(state),
        Action::ShuffleAllIntoDraw => cards::handle_shuffle_all_into_draw(state),
        Action::ShuffleDrawPile { trigger_relics } => {
            cards::handle_shuffle_draw_pile(trigger_relics, state)
        }
        Action::DiscardCard { card_lineage } => cards::handle_discard_card(card_lineage, state),
        Action::DiscardFromHand {
            amount,
            random,
            end_turn,
        } => cards::handle_discard_from_hand(amount, random, end_turn, state),
        Action::DiscardHandForTurnTransition => {
            crate::engine::core::discard_hand_for_turn_transition(state);
            state.turn.mark_end_turn_hand_discarded();
        }
        Action::DiscardToHand {
            card_lineage,
            cost_for_turn,
        } => cards::handle_discard_to_hand(card_lineage, cost_for_turn, state),
        Action::AllCostToHand { cost_target } => cards::handle_all_cost_to_hand(cost_target, state),
        Action::ExhaustCard {
            card_lineage,
            source_pile,
        } => cards::handle_exhaust_card(card_lineage, source_pile, state),
        Action::ExhaustFromHand {
            amount,
            random,
            any_number,
            can_pick_zero,
        } => cards::handle_exhaust_from_hand(amount, random, any_number, can_pick_zero, state),
        Action::Recycle => cards::handle_recycle(state),
        Action::MoveCard {
            card_lineage,
            from,
            to,
        } => cards::handle_move_card(card_lineage, from, to, state),
        Action::PutOnDeck { amount, random } => cards::handle_put_on_deck(amount, random, state),
        Action::Forethought { upgraded } => cards::handle_forethought(upgraded, state),
        Action::DiscardPileToTopOfDeck => cards::handle_discard_pile_to_top_of_deck(state),
        Action::RemoveCardFromPile { card_lineage, from } => {
            cards::handle_remove_card_from_pile(card_lineage, from, state)
        }
        Action::MakeTempCardInHand {
            card_id,
            amount,
            upgraded,
        } => cards::handle_make_temp_card_in_hand(card_id, amount, upgraded, state),
        Action::MakeTempCardInDiscard {
            card_id,
            amount,
            upgraded,
        } => cards::handle_make_temp_card_in_discard(card_id, amount, upgraded, state),
        Action::MakeTempCardInDrawPile {
            card_id,
            amount,
            random_spot,
            to_bottom,
            upgraded,
        } => cards::handle_make_temp_card_in_draw_pile(
            card_id,
            amount,
            random_spot,
            to_bottom,
            upgraded,
            state,
        ),
        Action::MakeCopyInHand { original, amount } => {
            cards::handle_make_copy_in_hand(*original, amount, state)
        }
        Action::MakeConstructedCopyInHand { original, amount } => {
            cards::handle_make_constructed_copy_in_hand(*original, amount, state)
        }
        Action::MakeCopyInDrawPile {
            original,
            amount,
            random_spot,
            to_bottom,
        } => cards::handle_make_copy_in_draw_pile(*original, amount, random_spot, to_bottom, state),
        Action::MakeCopyInDiscard { original, amount } => {
            cards::handle_make_copy_in_discard(*original, amount, state)
        }
        Action::ReturnStasisCard {
            card_lineage,
            to_hand,
        } => cards::handle_return_stasis_card(card_lineage, to_hand, state),
        Action::MakeTempCardInDiscardAndDeck { card_id, amount } => {
            cards::handle_make_temp_card_in_discard_and_deck(card_id, amount, state)
        }
        Action::ReduceAllHandCosts { amount } => cards::handle_reduce_all_hand_costs(amount, state),
        Action::ReduceRetainedHandCosts { amount } => {
            cards::handle_reduce_retained_hand_costs(amount, state)
        }
        Action::Enlightenment { permanent } => cards::handle_enlightenment(permanent, state),
        Action::Halt { block, additional } => cards::handle_halt(block, additional, state),
        Action::Madness => cards::handle_madness(state),
        Action::UpgradeAllInHand => cards::handle_upgrade_all_in_hand(state),
        Action::UpgradeAllCardsInCombat => cards::handle_upgrade_all_cards_in_combat(state),
        Action::UpgradeAllBurns => cards::handle_upgrade_all_burns(state),
        Action::UpgradeCard { card_lineage } => cards::handle_upgrade_card(card_lineage, state),
        Action::UpgradeRandomCard => cards::handle_upgrade_random_card(state),
        Action::ModifyCardMisc {
            card_lineage,
            amount,
        } => cards::handle_modify_card_misc(card_lineage, amount, state),
        Action::ModifyCardDamage {
            card_lineage,
            amount,
        } => cards::handle_modify_card_damage(card_lineage, amount, state),
        Action::Gash {
            card_lineage,
            amount,
        } => cards::handle_gash(card_lineage, amount, state),
        Action::ModifyCardBlock {
            card_lineage,
            amount,
        } => cards::handle_modify_card_block(card_lineage, amount, state),
        Action::ReduceCardCostForCombat {
            card_lineage,
            amount,
        } => cards::handle_reduce_card_cost_for_combat(card_lineage, amount, state),
        Action::RandomizeHandCosts => cards::handle_randomize_hand_costs(state),
        Action::MakeRandomCardInHand {
            card_type,
            cost_for_turn,
        } => cards::handle_make_random_card_in_hand(card_type, cost_for_turn, state),
        Action::Nightmare { amount } => cards::handle_nightmare(amount, state),
        Action::MakeRandomCardInDrawPile {
            card_type,
            cost_for_turn,
            random_spot,
        } => cards::handle_make_random_card_in_draw_pile(
            card_type,
            cost_for_turn,
            random_spot,
            state,
        ),
        Action::DrawPileToHandByType { amount, card_type } => {
            cards::handle_draw_pile_to_hand_by_type(amount, card_type, state)
        }
        Action::MakeRandomColorlessCardInHand {
            cost_for_turn,
            upgraded,
        } => cards::handle_make_random_colorless_card_in_hand(cost_for_turn, upgraded, state),
        Action::Transmutation {
            upgraded,
            free_to_play_once,
            energy_on_use,
        } => cards::handle_transmutation(upgraded, free_to_play_once, energy_on_use, state),
        Action::AggregateEnergy { divide_amount } => {
            cards::handle_aggregate_energy(divide_amount, state)
        }
        Action::Tempest {
            upgraded,
            free_to_play_once,
            energy_on_use,
        } => cards::handle_tempest(upgraded, free_to_play_once, energy_on_use, state),
        Action::MultiCast {
            upgraded,
            free_to_play_once,
            energy_on_use,
        } => cards::handle_multicast(upgraded, free_to_play_once, energy_on_use, state),
        Action::ReinforcedBody {
            block_amount,
            free_to_play_once,
            energy_on_use,
        } => cards::handle_reinforced_body(block_amount, free_to_play_once, energy_on_use, state),
        Action::UseCardDone {
            should_exhaust,
            trigger_after_use_hooks,
        } => cards::handle_use_card_done(should_exhaust, trigger_after_use_hooks, state),
        Action::UseCardAfterUseHooks { card } => {
            cards::handle_use_card_after_use_hooks(*card, state)
        }
        Action::QueueEarlyEndTurn => cards::handle_queue_early_end_turn(state),
        Action::SkipEnemiesTurn => cards::handle_skip_enemies_turn(state),
        Action::EnqueueCardPlay { item, in_front } => {
            cards::handle_enqueue_card_play(*item, in_front, state)
        }
        Action::PostDrawTrigger => cards::handle_post_draw_trigger(state),
        Action::FlushNextQueuedCard => cards::handle_flush_next_queued_card(state),
        Action::PlayCardDirect {
            card,
            target,
            purge,
        } => cards::handle_play_card_direct(*card, target, purge, state),
        Action::PlayTopCard { target, exhaust } => {
            cards::handle_play_top_card(target, exhaust, state)
        }
        Action::QueuePlayTopCardToBottom { target, exhaust } => {
            cards::handle_queue_play_top_card_to_bottom(target, exhaust, state)
        }
        Action::UsePotion { slot, target } => cards::handle_use_potion(slot, target, state),
        Action::DiscardPotion { slot } => {
            if let Some(potion_slot) = state.entities.potions.get_mut(slot) {
                if potion_slot
                    .as_ref()
                    .is_some_and(|potion| potion.can_discard)
                {
                    *potion_slot = None;
                }
            }
        }
        Action::ObtainPotion => cards::handle_obtain_potion(state),
        Action::ObtainSpecificPotion(potion_id) => {
            cards::obtain_specific_potion_if_allowed(state, potion_id);
        }
        Action::EndTurnTrigger => cards::handle_end_turn_trigger(state),
        Action::ClearCardQueue => cards::handle_clear_card_queue(state),
        Action::AddCardToMasterDeck { card_id, source } => {
            cards::handle_add_card_to_master_deck(card_id, source, state)
        }
        Action::BattleStartTrigger => cards::handle_battle_start_trigger(state),
        Action::PreBattleTrigger => cards::handle_pre_battle_trigger(state),
        Action::BattleStartPreDrawTrigger => cards::handle_battle_start_pre_draw_trigger(state),
        Action::RedSkullBattleStartCheck => {
            crate::content::relics::red_skull::battle_start_check(state);
        }
        Action::DodecahedronTurnStartCheck => {
            crate::content::relics::dodecahedron::Dodecahedron::turn_start_check(state);
        }

        other => return Err(other),
    }
    Ok(())
}
