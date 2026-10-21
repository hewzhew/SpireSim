use crate::content::monsters::EnemyId;
use crate::runtime::combat::{CombatState, MonsterEntity};

use super::super::types::{CombatMonsterKey, CombatMonsterRuntimeKey};

pub(super) fn monsters_key(combat: &CombatState) -> Vec<CombatMonsterKey> {
    combat.entities.monsters.iter().map(monster_key).collect()
}

fn monster_key(monster: &MonsterEntity) -> CombatMonsterKey {
    CombatMonsterKey {
        entity_id: monster.id,
        enemy_id: monster.enemy_id,
        current_hp: monster.current_hp,
        max_hp: monster.max_hp,
        block: monster.block,
        slot: monster.slot,
        logical_position: monster.logical_position,
        is_dying: monster.is_dying,
        is_escaped: monster.is_escaped,
        half_dead: monster.half_dead,
        move_state: monster.move_state.clone(),
        runtime: monster_runtime_key(monster),
    }
}

fn monster_runtime_key(monster: &MonsterEntity) -> CombatMonsterRuntimeKey {
    match monster.enemy_id {
        EnemyId::Hexaghost => CombatMonsterRuntimeKey::Hexaghost(monster.hexaghost.clone()),
        EnemyId::LouseNormal | EnemyId::LouseDefensive => {
            CombatMonsterRuntimeKey::Louse(monster.louse.clone())
        }
        EnemyId::JawWorm => CombatMonsterRuntimeKey::JawWorm(monster.jaw_worm.clone()),
        EnemyId::Looter | EnemyId::Mugger => CombatMonsterRuntimeKey::Thief(monster.thief.clone()),
        EnemyId::Byrd => CombatMonsterRuntimeKey::Byrd(monster.byrd.clone()),
        EnemyId::Chosen => CombatMonsterRuntimeKey::Chosen(monster.chosen.clone()),
        EnemyId::Snecko => CombatMonsterRuntimeKey::Snecko(monster.snecko.clone()),
        EnemyId::ShelledParasite => {
            CombatMonsterRuntimeKey::ShelledParasite(monster.shelled_parasite.clone())
        }
        EnemyId::BronzeAutomaton => {
            CombatMonsterRuntimeKey::BronzeAutomaton(monster.bronze_automaton.clone())
        }
        EnemyId::BronzeOrb => CombatMonsterRuntimeKey::BronzeOrb(monster.bronze_orb.clone()),
        EnemyId::BookOfStabbing => {
            CombatMonsterRuntimeKey::BookOfStabbing(monster.book_of_stabbing.clone())
        }
        EnemyId::TheCollector => CombatMonsterRuntimeKey::Collector(monster.collector.clone()),
        EnemyId::Champ => CombatMonsterRuntimeKey::Champ(monster.champ.clone()),
        EnemyId::AwakenedOne => CombatMonsterRuntimeKey::AwakenedOne(monster.awakened_one.clone()),
        EnemyId::CorruptHeart => {
            CombatMonsterRuntimeKey::CorruptHeart(monster.corrupt_heart.clone())
        }
        EnemyId::WrithingMass => {
            CombatMonsterRuntimeKey::WrithingMass(monster.writhing_mass.clone())
        }
        EnemyId::Spiker => CombatMonsterRuntimeKey::Spiker(monster.spiker.clone()),
        EnemyId::SpireShield => CombatMonsterRuntimeKey::SpireShield(monster.spire_shield.clone()),
        EnemyId::SpireSpear => CombatMonsterRuntimeKey::SpireSpear(monster.spire_spear.clone()),
        EnemyId::SlaverRed => CombatMonsterRuntimeKey::SlaverRed(monster.slaver_red.clone()),
        EnemyId::GremlinLeader => {
            CombatMonsterRuntimeKey::GremlinLeader(monster.gremlin_leader.clone())
        }
        EnemyId::GremlinNob => CombatMonsterRuntimeKey::GremlinNob(monster.gremlin_nob.clone()),
        EnemyId::GremlinWizard => {
            CombatMonsterRuntimeKey::GremlinWizard(monster.gremlin_wizard.clone())
        }
        EnemyId::Cultist => CombatMonsterRuntimeKey::Cultist(monster.cultist.clone()),
        EnemyId::Sentry => CombatMonsterRuntimeKey::Sentry(monster.sentry.clone()),
        EnemyId::SlimeBoss => CombatMonsterRuntimeKey::SlimeBoss(monster.slime_boss.clone()),
        EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL => {
            CombatMonsterRuntimeKey::LargeSlime(monster.large_slime.clone())
        }
        EnemyId::SphericGuardian => {
            CombatMonsterRuntimeKey::SphericGuardian(monster.spheric_guardian.clone())
        }
        EnemyId::Reptomancer => CombatMonsterRuntimeKey::Reptomancer(monster.reptomancer.clone()),
        EnemyId::Darkling => CombatMonsterRuntimeKey::Darkling(monster.darkling.clone()),
        EnemyId::Nemesis => CombatMonsterRuntimeKey::Nemesis(monster.nemesis.clone()),
        EnemyId::GiantHead => CombatMonsterRuntimeKey::GiantHead(monster.giant_head.clone()),
        EnemyId::TimeEater => CombatMonsterRuntimeKey::TimeEater(monster.time_eater.clone()),
        EnemyId::Donu => CombatMonsterRuntimeKey::Donu(monster.donu.clone()),
        EnemyId::Deca => CombatMonsterRuntimeKey::Deca(monster.deca.clone()),
        EnemyId::Transient => CombatMonsterRuntimeKey::Transient(monster.transient.clone()),
        EnemyId::Exploder => CombatMonsterRuntimeKey::Exploder(monster.exploder.clone()),
        EnemyId::Maw => CombatMonsterRuntimeKey::Maw(monster.maw.clone()),
        EnemyId::SnakeDagger => CombatMonsterRuntimeKey::SnakeDagger(monster.snake_dagger.clone()),
        EnemyId::Lagavulin => CombatMonsterRuntimeKey::Lagavulin(monster.lagavulin.clone()),
        EnemyId::TheGuardian => CombatMonsterRuntimeKey::Guardian(monster.guardian.clone()),
        _ => CombatMonsterRuntimeKey::None,
    }
}
