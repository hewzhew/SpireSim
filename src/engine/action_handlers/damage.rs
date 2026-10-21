// action_handlers/damage.rs - Combat damage domain facade
//
// The implementation is split by action semantics rather than by card class:
// - core: canonical damage pipeline and shared hooks
// - variants: regular, random, multi-target, monster, and orb damage actions
// - special_attacks: attacks with kill/resource/card-progress side effects
// - health/gold/vampire/card_effects: non-core mutations historically routed here

mod card_effects;
mod core;
mod gold;
mod health;
mod special_attacks;
mod vampire;
mod variants;

pub use card_effects::{
    handle_block_per_non_attack, handle_exhaust_all_non_attack, handle_exhaust_random_card,
    handle_limit_break,
};
pub use core::{
    apply_raw_damage_to_monster, deduct_block, handle_damage, resolve_player_damage,
    PlayerDamageResolution,
};
pub use gold::{handle_gain_gold, handle_steal_player_gold};
pub use health::{
    block_after_gain, handle_double_block, handle_gain_block, handle_gain_block_random_monster,
    handle_heal, handle_lose_block, handle_lose_hp, handle_poison_lose_hp, handle_remove_all_block,
    handle_set_current_hp, increase_player_max_hp_like_java,
};
pub use special_attacks::{
    handle_dropkick, handle_feed, handle_fiend_fire, handle_ftl, handle_hand_of_greed,
    handle_instant_kill, handle_judgement, handle_lesson_learned, handle_ritual_dagger,
    handle_skewer, handle_sunder, handle_trigger_marks, handle_whirlwind,
};
pub use vampire::{handle_vampire_damage, handle_vampire_damage_all_enemies};
pub use variants::{
    handle_attack_damage_random_enemy_card, handle_bane_damage, handle_damage_all_enemies,
    handle_damage_per_attack_played, handle_damage_random_enemy, handle_flechettes,
    handle_heel_hook, handle_monster_attack, handle_orb_damage, handle_orb_damage_all_enemies,
    handle_orb_damage_random_enemy, handle_pummel_damage, handle_wallop_damage,
};
