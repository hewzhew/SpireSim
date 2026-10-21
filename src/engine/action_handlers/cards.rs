// action_handlers/cards.rs — Card pile management domain
//
// Handles: DrawCards, EmptyDeckShuffle, DiscardCard, ExhaustCard, MoveCard, PutOnDeck,
//          MakeTempCard*, MakeCopy*, MakeRandom*, PlayCardDirect, PlayTopCard,
//          UseCardDone, UpgradeCard, UpgradeRandomCard, UpgradeAllInHand, UpgradeAllBurns,
//          ReduceAllHandCosts, RandomizeHandCosts, ModifyCardMisc,
//          UsePotion, DiscardPotion, ObtainPotion, ObtainSpecificPotion, Scry,
//          EndTurnTrigger, StartTurnTrigger, PostDrawTrigger, BattleStartTrigger, ClearCardQueue,
//          AddCardToMasterDeck, MakeTempCardInDiscardAndDeck, SuspendForCardReward

mod discard;
mod draw;
mod exhaust;
mod generated;
mod master_deck;
mod movement;
mod mutation;
mod pile_ops;
mod play_queue;
mod potions;
mod specials;
mod turn_triggers;
mod x_cost;
pub use discard::{
    handle_calculated_gamble, handle_discard_card, handle_discard_cards_with_order,
    handle_discard_from_hand, handle_discard_to_hand, handle_scrape_follow_up,
    move_card_to_discard_pile, DiscardHookOrder,
};
pub use draw::{
    handle_draw_cards, handle_draw_cards_with_history, handle_draw_for_unique_orb_types,
};
pub use exhaust::{
    handle_exhaust_card, handle_exhaust_from_hand, handle_recycle, handle_recycle_selected_card,
    move_card_to_exhaust_pile,
};
pub use generated::{
    handle_conjure_blade, handle_make_constructed_copy_in_hand, handle_make_copy_in_discard,
    handle_make_copy_in_draw_pile, handle_make_copy_in_hand, handle_make_random_card_in_draw_pile,
    handle_make_random_card_in_hand, handle_make_random_colorless_card_in_hand,
    handle_make_temp_card_in_discard, handle_make_temp_card_in_discard_and_deck,
    handle_make_temp_card_in_draw_pile, handle_make_temp_card_in_hand, handle_nightmare,
    handle_return_stasis_card, handle_transmutation, queue_nightmare_power_front,
};
pub use master_deck::handle_add_card_to_master_deck;
pub use movement::{
    handle_all_cost_to_hand, handle_discard_pile_to_top_of_deck, handle_draw_pile_to_hand_by_type,
    handle_exhume_card, handle_meditate, handle_move_card, handle_remove_card_from_pile,
};
pub use mutation::{
    handle_apply_bullet_time, handle_enlightenment, handle_gash, handle_madness,
    handle_modify_card_block, handle_modify_card_damage, handle_modify_card_misc,
    handle_randomize_hand_costs, handle_reduce_all_hand_costs, handle_reduce_card_cost_for_combat,
    handle_reduce_retained_hand_costs, handle_upgrade_all_burns,
    handle_upgrade_all_cards_in_combat, handle_upgrade_all_in_hand, handle_upgrade_card,
    handle_upgrade_random_card,
};
pub use pile_ops::{
    handle_empty_deck_shuffle, handle_forethought, handle_put_on_deck,
    handle_shuffle_all_into_draw, handle_shuffle_draw_pile,
};
pub use play_queue::{
    after_use_damage_requests, handle_enqueue_card_play, handle_flush_next_queued_card,
    handle_play_card_direct, handle_play_card_from_hand, handle_play_top_card,
    handle_queue_early_end_turn, handle_queue_play_top_card_to_bottom, handle_skip_enemies_turn,
    handle_use_card_after_use_hooks, handle_use_card_done,
};
pub use potions::{handle_obtain_potion, handle_use_potion, obtain_specific_potion_if_allowed};
pub use specials::{
    handle_barrage, handle_blade_fury, handle_escape_plan_block_if_skill, handle_expertise_draw,
    handle_halt, handle_unload_non_attack,
};
pub use turn_triggers::{
    end_turn_autoplay_card, handle_battle_start_pre_draw_trigger, handle_battle_start_trigger,
    handle_clear_card_queue, handle_end_turn_post_cards, handle_end_turn_trigger,
    handle_post_draw_trigger, handle_pre_battle_trigger, resolve_end_turn_triggered_autoplay,
};
pub use x_cost::{
    handle_aggregate_energy, handle_multicast, handle_reinforced_body, handle_tempest,
};
