use crate::runtime::combat::{CombatState, MonsterEntity};

pub(super) fn initialize_spawned_monster_runtime(
    enemy_id: crate::content::monsters::EnemyId,
    new_monster: &mut MonsterEntity,
    state: &mut CombatState,
) {
    if enemy_id == crate::content::monsters::EnemyId::TorchHead {
        crate::content::monsters::city::torch_head::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Darkling {
        crate::content::monsters::beyond::darkling::initialize_runtime_state(
            new_monster,
            &mut state.rng.monster_hp_rng,
            state.meta.ascension_level,
        );
    }
    if enemy_id == crate::content::monsters::EnemyId::Reptomancer {
        crate::content::monsters::beyond::reptomancer::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Nemesis {
        crate::content::monsters::beyond::nemesis::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::GiantHead {
        crate::content::monsters::beyond::giant_head::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::TimeEater {
        crate::content::monsters::beyond::time_eater::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Donu {
        crate::content::monsters::beyond::donu::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Deca {
        crate::content::monsters::beyond::deca::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Transient {
        crate::content::monsters::beyond::transient::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Exploder {
        crate::content::monsters::beyond::exploder::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Maw {
        crate::content::monsters::beyond::maw::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::SnakeDagger {
        crate::content::monsters::beyond::snake_dagger::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::JawWorm {
        crate::content::monsters::exordium::jaw_worm::initialize_runtime_state(new_monster, false);
    }
    if enemy_id == crate::content::monsters::EnemyId::Byrd {
        new_monster.byrd.first_move = true;
        new_monster.byrd.is_flying = true;
        new_monster.byrd.protocol_seeded = true;
    }
    if enemy_id == crate::content::monsters::EnemyId::Chosen {
        new_monster.chosen.first_turn = true;
        new_monster.chosen.used_hex = false;
        new_monster.chosen.protocol_seeded = true;
    }
    if enemy_id == crate::content::monsters::EnemyId::Snecko {
        new_monster.snecko.first_turn = true;
        new_monster.snecko.protocol_seeded = true;
    }
    if enemy_id == crate::content::monsters::EnemyId::ShelledParasite {
        new_monster.shelled_parasite.first_move = true;
        new_monster.shelled_parasite.protocol_seeded = true;
    }
    if enemy_id == crate::content::monsters::EnemyId::SphericGuardian {
        crate::content::monsters::city::spheric_guardian::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::BronzeAutomaton {
        new_monster.bronze_automaton.protocol_seeded = true;
        new_monster.bronze_automaton.first_turn = true;
        new_monster.bronze_automaton.num_turns = 0;
    }
    if enemy_id == crate::content::monsters::EnemyId::BronzeOrb {
        new_monster.bronze_orb.protocol_seeded = true;
        new_monster.bronze_orb.used_stasis = false;
    }
    if enemy_id == crate::content::monsters::EnemyId::BookOfStabbing {
        new_monster.book_of_stabbing.protocol_seeded = true;
        new_monster.book_of_stabbing.stab_count = 1;
    }
    if enemy_id == crate::content::monsters::EnemyId::TheCollector {
        new_monster.collector.protocol_seeded = true;
        new_monster.collector.initial_spawn = true;
        new_monster.collector.ult_used = false;
        new_monster.collector.turns_taken = 0;
    }
    if enemy_id == crate::content::monsters::EnemyId::Champ {
        new_monster.champ.protocol_seeded = true;
        new_monster.champ.first_turn = true;
        new_monster.champ.num_turns = 0;
        new_monster.champ.forge_times = 0;
        new_monster.champ.threshold_reached = false;
    }
    if enemy_id == crate::content::monsters::EnemyId::AwakenedOne {
        new_monster.awakened_one.protocol_seeded = true;
        new_monster.awakened_one.form1 = true;
        new_monster.awakened_one.first_turn = true;
    }
    if enemy_id == crate::content::monsters::EnemyId::CorruptHeart {
        crate::content::monsters::ending::corrupt_heart::initialize_runtime_state(
            new_monster,
            state.meta.ascension_level,
        );
    }
    if enemy_id == crate::content::monsters::EnemyId::WrithingMass {
        new_monster.writhing_mass.protocol_seeded = true;
        new_monster.writhing_mass.first_move = true;
        new_monster.writhing_mass.used_mega_debuff = false;
    }
    if enemy_id == crate::content::monsters::EnemyId::Spiker {
        new_monster.spiker.protocol_seeded = true;
        new_monster.spiker.thorns_count = 0;
    }
    if enemy_id == crate::content::monsters::EnemyId::SpireShield {
        new_monster.spire_shield.protocol_seeded = true;
        new_monster.spire_shield.move_count = 0;
        new_monster.logical_position = -1;
    }
    if enemy_id == crate::content::monsters::EnemyId::SpireSpear {
        crate::content::monsters::ending::spire_spear::initialize_runtime_state(
            new_monster,
            state.meta.ascension_level,
        );
        new_monster.logical_position = 1;
    }
    if enemy_id == crate::content::monsters::EnemyId::SlaverRed {
        new_monster.slaver_red.protocol_seeded = true;
        new_monster.slaver_red.first_turn = true;
        new_monster.slaver_red.used_entangle = false;
    }
    if enemy_id == crate::content::monsters::EnemyId::GremlinLeader {
        new_monster.gremlin_leader.protocol_seeded = true;
        new_monster.gremlin_leader.gremlin_slots = [None, None, None];
    }
    if enemy_id == crate::content::monsters::EnemyId::GremlinNob {
        new_monster.gremlin_nob.protocol_seeded = true;
        new_monster.gremlin_nob.used_bellow = false;
    }
    if enemy_id == crate::content::monsters::EnemyId::GremlinWizard {
        crate::content::monsters::exordium::gremlin_wizard::initialize_runtime_state(new_monster);
    }
    if enemy_id == crate::content::monsters::EnemyId::Cultist {
        new_monster.cultist.protocol_seeded = true;
        new_monster.cultist.first_move = true;
    }
    if enemy_id == crate::content::monsters::EnemyId::Sentry {
        new_monster.sentry.protocol_seeded = true;
        new_monster.sentry.first_move = true;
    }
    if enemy_id == crate::content::monsters::EnemyId::SlimeBoss {
        crate::content::monsters::exordium::slime_boss::initialize_runtime_state(new_monster);
    }
    if matches!(
        enemy_id,
        crate::content::monsters::EnemyId::AcidSlimeL
            | crate::content::monsters::EnemyId::SpikeSlimeL
    ) {
        crate::content::monsters::exordium::initialize_large_slime_runtime_state(new_monster);
    }
    if matches!(
        enemy_id,
        crate::content::monsters::EnemyId::Looter | crate::content::monsters::EnemyId::Mugger
    ) {
        new_monster.thief.protocol_seeded = true;
        new_monster.thief.slash_count = 0;
        new_monster.thief.stolen_gold = 0;
    }
    if enemy_id == crate::content::monsters::EnemyId::TheGuardian {
        crate::content::monsters::exordium::the_guardian::initialize_runtime_state(
            new_monster,
            state.meta.ascension_level,
        );
    }
}
