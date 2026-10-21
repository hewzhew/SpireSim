use crate::runtime::combat::CollectorRuntimeState;
use crate::runtime::combat::{
    AwakenedOneRuntimeState, BookOfStabbingRuntimeState, BronzeAutomatonRuntimeState,
    BronzeOrbRuntimeState, ByrdRuntimeState, ChampRuntimeState, ChosenRuntimeState,
    CorruptHeartRuntimeState, CultistRuntimeState, DarklingRuntimeState, DecaRuntimeState,
    DonuRuntimeState, ExploderRuntimeState, GiantHeadRuntimeState, GremlinLeaderRuntimeState,
    GremlinNobRuntimeState, GremlinWizardRuntimeState, GuardianRuntimeState, HexaghostRuntimeState,
    JawWormRuntimeState, LagavulinRuntimeState, LargeSlimeRuntimeState, LouseRuntimeState,
    MawRuntimeState, NemesisRuntimeState, ReptomancerRuntimeState, SentryRuntimeState,
    ShelledParasiteRuntimeState, SlaverRedRuntimeState, SlimeBossRuntimeState,
    SnakeDaggerRuntimeState, SneckoRuntimeState, SphericGuardianRuntimeState, SpikerRuntimeState,
    SpireShieldRuntimeState, SpireSpearRuntimeState, ThiefRuntimeState, TimeEaterRuntimeState,
    TransientRuntimeState, WrithingMassRuntimeState,
};
pub(super) fn stable_hexaghost_signature(state: &HexaghostRuntimeState) -> String {
    format!(
        "act{}:orbs{}:burn_upg{}:divider{}",
        state.activated,
        state.orb_active_count,
        state.burn_upgraded,
        stable_json(&state.divider_damage)
    )
}

pub(super) fn stable_louse_signature(state: &LouseRuntimeState) -> String {
    format!("bite{}", stable_json(&state.bite_damage))
}

pub(super) fn stable_jaw_worm_signature(state: &JawWormRuntimeState) -> String {
    format!(
        "seed{}:first{}:hard{}",
        state.protocol_seeded, state.first_move, state.hard_mode
    )
}

pub(super) fn stable_thief_signature(state: &ThiefRuntimeState) -> String {
    format!(
        "seed{}:slash{}:gold{}",
        state.protocol_seeded, state.slash_count, state.stolen_gold
    )
}

pub(super) fn stable_byrd_signature(state: &ByrdRuntimeState) -> String {
    format!(
        "seed{}:first{}:flying{}",
        state.protocol_seeded, state.first_move, state.is_flying
    )
}

pub(super) fn stable_chosen_signature(state: &ChosenRuntimeState) -> String {
    format!(
        "seed{}:first{}:hex{}",
        state.protocol_seeded, state.first_turn, state.used_hex
    )
}

pub(super) fn stable_snecko_signature(state: &SneckoRuntimeState) -> String {
    format!("seed{}:first{}", state.protocol_seeded, state.first_turn)
}

pub(super) fn stable_shelled_parasite_signature(state: &ShelledParasiteRuntimeState) -> String {
    format!("seed{}:first{}", state.protocol_seeded, state.first_move)
}

pub(super) fn stable_bronze_automaton_signature(state: &BronzeAutomatonRuntimeState) -> String {
    format!(
        "seed{}:first{}:turns{}",
        state.protocol_seeded, state.first_turn, state.num_turns
    )
}

pub(super) fn stable_bronze_orb_signature(state: &BronzeOrbRuntimeState) -> String {
    format!("seed{}:stasis{}", state.protocol_seeded, state.used_stasis)
}

pub(super) fn stable_book_of_stabbing_signature(state: &BookOfStabbingRuntimeState) -> String {
    format!("seed{}:stab{}", state.protocol_seeded, state.stab_count)
}

pub(super) fn stable_collector_signature(state: &CollectorRuntimeState) -> String {
    format!(
        "seed{}:spawn{}:ult{}:turns{}:slots{}",
        state.protocol_seeded,
        state.initial_spawn,
        state.ult_used,
        state.turns_taken,
        stable_json(&state.enemy_slots)
    )
}

pub(super) fn stable_champ_signature(state: &ChampRuntimeState) -> String {
    format!(
        "seed{}:first{}:turns{}:forge{}:threshold{}",
        state.protocol_seeded,
        state.first_turn,
        state.num_turns,
        state.forge_times,
        state.threshold_reached
    )
}

pub(super) fn stable_awakened_one_signature(state: &AwakenedOneRuntimeState) -> String {
    format!(
        "seed{}:form1{}:first{}",
        state.protocol_seeded, state.form1, state.first_turn
    )
}

pub(super) fn stable_corrupt_heart_signature(state: &CorruptHeartRuntimeState) -> String {
    format!(
        "seed{}:first{}:moves{}:buff{}:blood_hits{}",
        state.protocol_seeded,
        state.first_move,
        state.move_count,
        state.buff_count,
        state.blood_hit_count
    )
}

pub(super) fn stable_darkling_signature(state: &DarklingRuntimeState) -> String {
    format!(
        "seed{}:first{}:nip{}",
        state.protocol_seeded, state.first_move, state.nip_dmg
    )
}

pub(super) fn stable_reptomancer_signature(state: &ReptomancerRuntimeState) -> String {
    format!(
        "seed{}:first{}:daggers{}",
        state.protocol_seeded,
        state.first_move,
        stable_json(&state.dagger_slots)
    )
}

pub(super) fn stable_nemesis_signature(state: &NemesisRuntimeState) -> String {
    format!(
        "seed{}:first{}:scythe_cd{}",
        state.protocol_seeded, state.first_move, state.scythe_cooldown
    )
}

pub(super) fn stable_giant_head_signature(state: &GiantHeadRuntimeState) -> String {
    format!("seed{}:count{}", state.protocol_seeded, state.count)
}

pub(super) fn stable_time_eater_signature(state: &TimeEaterRuntimeState) -> String {
    format!("seed{}:haste{}", state.protocol_seeded, state.used_haste)
}

pub(super) fn stable_donu_signature(state: &DonuRuntimeState) -> String {
    format!("seed{}:atk{}", state.protocol_seeded, state.is_attacking)
}

pub(super) fn stable_deca_signature(state: &DecaRuntimeState) -> String {
    format!("seed{}:atk{}", state.protocol_seeded, state.is_attacking)
}

pub(super) fn stable_transient_signature(state: &TransientRuntimeState) -> String {
    format!("seed{}:count{}", state.protocol_seeded, state.count)
}

pub(super) fn stable_exploder_signature(state: &ExploderRuntimeState) -> String {
    format!("seed{}:turn{}", state.protocol_seeded, state.turn_count)
}

pub(super) fn stable_maw_signature(state: &MawRuntimeState) -> String {
    format!(
        "seed{}:roared{}:turn{}",
        state.protocol_seeded, state.roared, state.turn_count
    )
}

pub(super) fn stable_writhing_mass_signature(state: &WrithingMassRuntimeState) -> String {
    format!(
        "seed{}:first{}:used_mega{}",
        state.protocol_seeded, state.first_move, state.used_mega_debuff
    )
}

pub(super) fn stable_snake_dagger_signature(state: &SnakeDaggerRuntimeState) -> String {
    format!("seed{}:first{}", state.protocol_seeded, state.first_move)
}

pub(super) fn stable_spiker_signature(state: &SpikerRuntimeState) -> String {
    format!("seed{}:thorns{}", state.protocol_seeded, state.thorns_count)
}

pub(super) fn stable_spire_shield_signature(state: &SpireShieldRuntimeState) -> String {
    format!("seed{}:moves{}", state.protocol_seeded, state.move_count)
}

pub(super) fn stable_spire_spear_signature(state: &SpireSpearRuntimeState) -> String {
    format!(
        "seed{}:moves{}:skewer{}",
        state.protocol_seeded, state.move_count, state.skewer_count
    )
}

pub(super) fn stable_slaver_red_signature(state: &SlaverRedRuntimeState) -> String {
    format!(
        "seed{}:first{}:entangle{}",
        state.protocol_seeded, state.first_turn, state.used_entangle
    )
}

pub(super) fn stable_gremlin_leader_signature(state: &GremlinLeaderRuntimeState) -> String {
    format!(
        "seed{}:slots{}",
        state.protocol_seeded,
        stable_json(&state.gremlin_slots)
    )
}

pub(super) fn stable_gremlin_nob_signature(state: &GremlinNobRuntimeState) -> String {
    format!("seed{}:bellow{}", state.protocol_seeded, state.used_bellow)
}

pub(super) fn stable_gremlin_wizard_signature(state: &GremlinWizardRuntimeState) -> String {
    format!(
        "seed{}:charge{}",
        state.protocol_seeded, state.current_charge
    )
}

pub(super) fn stable_cultist_signature(state: &CultistRuntimeState) -> String {
    format!("seed{}:first{}", state.protocol_seeded, state.first_move)
}

pub(super) fn stable_sentry_signature(state: &SentryRuntimeState) -> String {
    format!("seed{}:first{}", state.protocol_seeded, state.first_move)
}

pub(super) fn stable_slime_boss_signature(state: &SlimeBossRuntimeState) -> String {
    format!("seed{}:first{}", state.protocol_seeded, state.first_turn)
}

pub(super) fn stable_large_slime_signature(state: &LargeSlimeRuntimeState) -> String {
    format!(
        "seed{}:split{}",
        state.protocol_seeded, state.split_triggered
    )
}

pub(super) fn stable_spheric_guardian_signature(state: &SphericGuardianRuntimeState) -> String {
    format!(
        "seed{}:first{}:second{}",
        state.protocol_seeded, state.first_move, state.second_move
    )
}

pub(super) fn stable_lagavulin_signature(state: &LagavulinRuntimeState) -> String {
    format!(
        "out{}:idle{}:debuff{}:trigger{}",
        state.is_out, state.idle_count, state.debuff_turn_count, state.is_out_triggered
    )
}

pub(super) fn stable_guardian_signature(state: &GuardianRuntimeState) -> String {
    format!(
        "threshold{}:taken{}:open{}:close{}",
        state.damage_threshold, state.damage_taken, state.is_open, state.close_up_triggered
    )
}

fn stable_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("stable monster runtime field should serialize")
}
