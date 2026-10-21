//! Test-only builders shared by behavior and semantic integration tests.

use crate::content::monsters::EnemyId;
use crate::runtime::combat::{
    CardZones, CombatCard, CombatMeta, CombatPhase, CombatRng, CombatRuntimeHints, CombatState,
    EngineRuntime, EntityState, EphemeralCounters, MonsterEntity, MonsterMoveState, PlayerEntity,
    StanceId, TurnRuntime,
};
use crate::runtime::rng::RngPool;
use crate::state::run::RunState;
use std::collections::VecDeque;

/// Names one logical card slot in a hand-authored test scenario.
///
/// This helper is deliberately test-only: it makes fixture provenance
/// explicit without teaching production APIs to accept historical bare
/// integer card identifiers.
pub fn scenario_lineage(slot: u32) -> crate::runtime::combat::CardLineageId {
    crate::runtime::combat::CardLineageId::constructed_scenario(slot)
}

/// Names several logical card slots in a hand-authored test scenario.
///
/// Like [`scenario_lineage`], this is deliberately unavailable to production
/// code and does not infer identity from card contents or pile position.
pub fn scenario_lineages(
    slots: impl IntoIterator<Item = u32>,
) -> Vec<crate::runtime::combat::CardLineageId> {
    slots.into_iter().map(scenario_lineage).collect()
}

/// Builds a fully initialized run with the character's natural starting
/// loadout. This is the explicit fixture for tests whose assertions depend on
/// starter cards or the starter relic.
pub fn natural_test_run(
    seed: u64,
    ascension_level: u8,
    final_act: bool,
    player_class: &'static str,
) -> RunState {
    RunState::new(seed, ascension_level, final_act, player_class)
}

/// Builds a fully initialized run whose deck is exactly `master_deck`.
///
/// Prefer this over calling `RunState::new` and then clearing or appending to
/// its natural starter deck. The replacement is deliberately part of the
/// constructor contract, so a synthetic fixture cannot accidentally contain
/// both the natural loadout and hand-authored cards.
pub fn synthetic_deck_test_run(
    seed: u64,
    ascension_level: u8,
    final_act: bool,
    player_class: &'static str,
    master_deck: Vec<CombatCard>,
) -> RunState {
    let mut run = RunState::new(seed, ascension_level, final_act, player_class);
    run.master_deck = master_deck;
    run
}

pub fn blank_test_combat() -> CombatState {
    CombatState {
        meta: CombatMeta {
            ascension_level: 0,
            player_class: "Ironclad".to_string(),
            is_boss_fight: false,
            is_elite_fight: false,
            master_deck_snapshot: Vec::new().into(),
            master_deck_lineage_allocator: Default::default(),
            meta_changes: Vec::new(),
        },
        turn: TurnRuntime {
            turn_count: 1,
            current_phase: CombatPhase::PlayerTurn,
            energy: 3,
            turn_start_draw_modifier: 0,
            counters: EphemeralCounters::default(),
        },
        zones: CardZones {
            draw_pile: vec![].into(),
            hand: vec![],
            discard_pile: vec![].into(),
            exhaust_pile: vec![].into(),
            limbo: vec![],
            queued_cards: VecDeque::new(),
            combat_lineage_allocator: Default::default(),
        },
        entities: EntityState {
            player: PlayerEntity {
                id: 0,
                current_hp: 80,
                max_hp: 80,
                block: 0,
                facing_left: false,
                gold_delta_this_combat: 0,
                gold: 99,
                max_orbs: 0,
                orbs: vec![],
                stance: StanceId::Neutral,
                relics: vec![],
                relic_buses: Default::default(),
                energy_master: 3,
            },
            monsters: vec![],
            potions: vec![],
            power_db: Default::default(),
        },
        engine: EngineRuntime::new(),
        rng: CombatRng::new(RngPool::new(123)),
        runtime: CombatRuntimeHints::default(),
    }
}

pub fn combat_with_monsters(monsters: Vec<MonsterEntity>) -> CombatState {
    let mut state = blank_test_combat();
    state.entities.monsters = monsters;
    state
}

pub fn test_monster(enemy_id: EnemyId) -> MonsterEntity {
    let mut monster = MonsterEntity {
        id: 1,
        enemy_id,
        current_hp: 20,
        max_hp: 20,
        block: 0,
        slot: 0,
        is_dying: false,
        is_escaped: false,
        half_dead: false,
        move_state: MonsterMoveState::default(),
        logical_position: 0,
        runtime: Default::default(),
    };

    if enemy_id == EnemyId::Byrd {
        monster.byrd.first_move = true;
        monster.byrd.is_flying = true;
        monster.byrd.protocol_seeded = true;
    }
    if enemy_id == EnemyId::Chosen {
        monster.chosen.first_turn = true;
        monster.chosen.used_hex = false;
        monster.chosen.protocol_seeded = true;
    }
    if enemy_id == EnemyId::Snecko {
        monster.snecko.first_turn = true;
        monster.snecko.protocol_seeded = true;
    }
    if enemy_id == EnemyId::ShelledParasite {
        monster.shelled_parasite.first_move = true;
        monster.shelled_parasite.protocol_seeded = true;
    }
    if enemy_id == EnemyId::SphericGuardian {
        crate::content::monsters::city::spheric_guardian::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::JawWorm {
        crate::content::monsters::exordium::jaw_worm::initialize_runtime_state(&mut monster, false);
    }
    if enemy_id == EnemyId::BronzeAutomaton {
        monster.bronze_automaton.protocol_seeded = true;
        monster.bronze_automaton.first_turn = true;
        monster.bronze_automaton.num_turns = 0;
    }
    if enemy_id == EnemyId::BronzeOrb {
        monster.bronze_orb.protocol_seeded = true;
        monster.bronze_orb.used_stasis = false;
    }
    if enemy_id == EnemyId::BookOfStabbing {
        monster.book_of_stabbing.protocol_seeded = true;
        monster.book_of_stabbing.stab_count = 1;
    }
    if enemy_id == EnemyId::TheCollector {
        monster.collector.protocol_seeded = true;
        monster.collector.initial_spawn = true;
        monster.collector.ult_used = false;
        monster.collector.turns_taken = 0;
    }
    if enemy_id == EnemyId::Champ {
        monster.champ.protocol_seeded = true;
        monster.champ.first_turn = true;
        monster.champ.num_turns = 0;
        monster.champ.forge_times = 0;
        monster.champ.threshold_reached = false;
    }
    if enemy_id == EnemyId::AwakenedOne {
        monster.awakened_one.protocol_seeded = true;
        monster.awakened_one.form1 = true;
        monster.awakened_one.first_turn = true;
    }
    if enemy_id == EnemyId::CorruptHeart {
        crate::content::monsters::ending::corrupt_heart::initialize_runtime_state(&mut monster, 0);
    }
    if enemy_id == EnemyId::WrithingMass {
        monster.writhing_mass.protocol_seeded = true;
        monster.writhing_mass.first_move = true;
        monster.writhing_mass.used_mega_debuff = false;
    }
    if enemy_id == EnemyId::Darkling {
        monster.darkling.protocol_seeded = true;
        monster.darkling.first_move = true;
        monster.darkling.nip_dmg = 9;
    }
    if enemy_id == EnemyId::Reptomancer {
        crate::content::monsters::beyond::reptomancer::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Nemesis {
        crate::content::monsters::beyond::nemesis::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::GiantHead {
        crate::content::monsters::beyond::giant_head::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::TimeEater {
        crate::content::monsters::beyond::time_eater::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Donu {
        crate::content::monsters::beyond::donu::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Deca {
        crate::content::monsters::beyond::deca::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Transient {
        crate::content::monsters::beyond::transient::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Exploder {
        crate::content::monsters::beyond::exploder::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Maw {
        crate::content::monsters::beyond::maw::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::SnakeDagger {
        crate::content::monsters::beyond::snake_dagger::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Spiker {
        monster.spiker.protocol_seeded = true;
        monster.spiker.thorns_count = 0;
    }
    if enemy_id == EnemyId::SpireShield {
        monster.spire_shield.protocol_seeded = true;
        monster.spire_shield.move_count = 0;
    }
    if enemy_id == EnemyId::SpireSpear {
        crate::content::monsters::ending::spire_spear::initialize_runtime_state(&mut monster, 0);
    }
    if enemy_id == EnemyId::SlaverRed {
        monster.slaver_red.protocol_seeded = true;
        monster.slaver_red.first_turn = true;
        monster.slaver_red.used_entangle = false;
    }
    if enemy_id == EnemyId::GremlinLeader {
        monster.gremlin_leader.protocol_seeded = true;
        monster.gremlin_leader.gremlin_slots = [None, None, None];
    }
    if enemy_id == EnemyId::GremlinNob {
        monster.gremlin_nob.protocol_seeded = true;
        monster.gremlin_nob.used_bellow = false;
    }
    if enemy_id == EnemyId::GremlinWizard {
        crate::content::monsters::exordium::gremlin_wizard::initialize_runtime_state(&mut monster);
    }
    if enemy_id == EnemyId::Cultist {
        monster.cultist.protocol_seeded = true;
        monster.cultist.first_move = true;
    }
    if enemy_id == EnemyId::Sentry {
        monster.sentry.protocol_seeded = true;
        monster.sentry.first_move = true;
    }
    if enemy_id == EnemyId::SlimeBoss {
        crate::content::monsters::exordium::slime_boss::initialize_runtime_state(&mut monster);
    }
    if matches!(enemy_id, EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL) {
        crate::content::monsters::exordium::initialize_large_slime_runtime_state(&mut monster);
    }
    if matches!(enemy_id, EnemyId::Looter | EnemyId::Mugger) {
        monster.thief.protocol_seeded = true;
        monster.thief.slash_count = 0;
        monster.thief.stolen_gold = 0;
    }

    monster
}

pub fn planned_monster(enemy_id: EnemyId, move_id: u8) -> MonsterEntity {
    let mut monster = test_monster(enemy_id);
    monster.set_planned_move_id(move_id);
    monster
}

pub fn monster_with_history(enemy_id: EnemyId, move_id: u8, history: &[u8]) -> MonsterEntity {
    let mut monster = planned_monster(enemy_id, move_id);
    monster.move_history_mut().extend(history.iter().copied());
    monster
}

#[cfg(test)]
mod run_fixture_tests {
    use super::*;
    use crate::content::cards::CardId;

    #[test]
    fn natural_run_fixture_keeps_exact_ironclad_starting_deck() {
        let run = natural_test_run(1, 0, false, "Ironclad");

        assert_eq!(run.master_deck.len(), 10);
        assert_eq!(
            run.master_deck
                .iter()
                .filter(|card| card.id == CardId::Strike)
                .count(),
            5
        );
        assert_eq!(
            run.master_deck
                .iter()
                .filter(|card| card.id == CardId::Bash)
                .count(),
            1
        );
    }

    #[test]
    fn synthetic_deck_fixture_replaces_the_natural_deck_atomically() {
        let run = synthetic_deck_test_run(
            1,
            0,
            false,
            "Ironclad",
            vec![CombatCard::for_scenario(CardId::Carnage, 7)],
        );

        assert_eq!(run.master_deck.len(), 1);
        assert_eq!(run.master_deck[0].id, CardId::Carnage);
        assert_eq!(
            run.master_deck[0].lineage,
            crate::runtime::combat::CardLineageId::constructed_scenario(7)
        );
    }
}

/// An explicitly constructed acquisition receipt for state-key/outcome fixtures.
pub fn test_master_deck_acquisition(
    card_id: crate::content::cards::CardId,
    slot: u32,
) -> crate::runtime::combat::MasterDeckAcquisition {
    crate::runtime::combat::MasterDeckAcquisition {
        card: crate::runtime::combat::CombatCard::for_scenario(card_id, slot),
        monster_id: crate::content::monsters::EnemyId::WrithingMass,
        monster_slot: 0,
    }
}
