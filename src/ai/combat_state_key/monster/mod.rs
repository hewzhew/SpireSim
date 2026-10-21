mod move_state;
mod runtime;

use crate::content::monsters::EnemyId;
use crate::runtime::combat::MonsterEntity;

use move_state::stable_move_state_signature;
use runtime::*;
pub(super) fn stable_monster_signature(monster: &MonsterEntity) -> String {
    format!(
        "{}:hp{}:{}:blk{}:dy{}:esc{}:half{}:move={}:rt={}",
        stable_monster_kind_signature(monster.enemy_id),
        monster.current_hp,
        monster.max_hp,
        monster.block,
        monster.is_dying,
        monster.is_escaped,
        monster.half_dead,
        stable_move_state_signature(&monster.move_state),
        stable_monster_runtime_signature(monster),
    )
}

fn stable_monster_kind_signature(enemy_id: EnemyId) -> String {
    enemy_id.stable_id().to_string()
}

fn stable_monster_runtime_signature(monster: &MonsterEntity) -> String {
    match monster.enemy_id {
        EnemyId::Hexaghost => {
            format!("hex:{}", stable_hexaghost_signature(&monster.hexaghost))
        }
        EnemyId::LouseNormal | EnemyId::LouseDefensive => {
            format!("louse:{}", stable_louse_signature(&monster.louse))
        }
        EnemyId::JawWorm => format!("jaw:{}", stable_jaw_worm_signature(&monster.jaw_worm)),
        EnemyId::Looter | EnemyId::Mugger => {
            format!("thief:{}", stable_thief_signature(&monster.thief))
        }
        EnemyId::Byrd => format!("byrd:{}", stable_byrd_signature(&monster.byrd)),
        EnemyId::Chosen => format!("chosen:{}", stable_chosen_signature(&monster.chosen)),
        EnemyId::Snecko => format!("snecko:{}", stable_snecko_signature(&monster.snecko)),
        EnemyId::ShelledParasite => format!(
            "parasite:{}",
            stable_shelled_parasite_signature(&monster.shelled_parasite)
        ),
        EnemyId::BronzeAutomaton => format!(
            "bronze_auto:{}",
            stable_bronze_automaton_signature(&monster.bronze_automaton)
        ),
        EnemyId::BronzeOrb => {
            format!(
                "bronze_orb:{}",
                stable_bronze_orb_signature(&monster.bronze_orb)
            )
        }
        EnemyId::BookOfStabbing => {
            format!(
                "book:{}",
                stable_book_of_stabbing_signature(&monster.book_of_stabbing)
            )
        }
        EnemyId::TheCollector => {
            format!(
                "collector:{}",
                stable_collector_signature(&monster.collector)
            )
        }
        EnemyId::Champ => format!("champ:{}", stable_champ_signature(&monster.champ)),
        EnemyId::AwakenedOne => {
            format!(
                "awakened:{}",
                stable_awakened_one_signature(&monster.awakened_one)
            )
        }
        EnemyId::CorruptHeart => {
            format!(
                "heart:{}",
                stable_corrupt_heart_signature(&monster.corrupt_heart)
            )
        }
        EnemyId::Darkling => {
            format!("darkling:{}", stable_darkling_signature(&monster.darkling))
        }
        EnemyId::Reptomancer => {
            format!(
                "reptomancer:{}",
                stable_reptomancer_signature(&monster.reptomancer)
            )
        }
        EnemyId::Nemesis => {
            format!("nemesis:{}", stable_nemesis_signature(&monster.nemesis))
        }
        EnemyId::GiantHead => {
            format!(
                "giant_head:{}",
                stable_giant_head_signature(&monster.giant_head)
            )
        }
        EnemyId::TimeEater => {
            format!(
                "time_eater:{}",
                stable_time_eater_signature(&monster.time_eater)
            )
        }
        EnemyId::Donu => format!("donu:{}", stable_donu_signature(&monster.donu)),
        EnemyId::Deca => format!("deca:{}", stable_deca_signature(&monster.deca)),
        EnemyId::Transient => {
            format!(
                "transient:{}",
                stable_transient_signature(&monster.transient)
            )
        }
        EnemyId::Exploder => {
            format!("exploder:{}", stable_exploder_signature(&monster.exploder))
        }
        EnemyId::Maw => format!("maw:{}", stable_maw_signature(&monster.maw)),
        EnemyId::SnakeDagger => {
            format!(
                "snake_dagger:{}",
                stable_snake_dagger_signature(&monster.snake_dagger)
            )
        }
        EnemyId::WrithingMass => {
            format!(
                "writhing_mass:{}",
                stable_writhing_mass_signature(&monster.writhing_mass)
            )
        }
        EnemyId::Spiker => {
            format!("spiker:{}", stable_spiker_signature(&monster.spiker))
        }
        EnemyId::SpireShield => {
            format!(
                "spire_shield:{}",
                stable_spire_shield_signature(&monster.spire_shield)
            )
        }
        EnemyId::SpireSpear => {
            format!(
                "spire_spear:{}",
                stable_spire_spear_signature(&monster.spire_spear)
            )
        }
        EnemyId::SlaverRed => {
            format!(
                "slaver_red:{}",
                stable_slaver_red_signature(&monster.slaver_red)
            )
        }
        EnemyId::GremlinLeader => {
            format!(
                "gremlin_leader:{}",
                stable_gremlin_leader_signature(&monster.gremlin_leader)
            )
        }
        EnemyId::GremlinNob => {
            format!(
                "gremlin_nob:{}",
                stable_gremlin_nob_signature(&monster.gremlin_nob)
            )
        }
        EnemyId::GremlinWizard => {
            format!(
                "gremlin_wizard:{}",
                stable_gremlin_wizard_signature(&monster.gremlin_wizard)
            )
        }
        EnemyId::Cultist => {
            format!("cultist:{}", stable_cultist_signature(&monster.cultist))
        }
        EnemyId::Sentry => {
            format!("sentry:{}", stable_sentry_signature(&monster.sentry))
        }
        EnemyId::SlimeBoss => {
            format!(
                "slime_boss:{}",
                stable_slime_boss_signature(&monster.slime_boss)
            )
        }
        EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL => {
            format!(
                "large_slime:{}",
                stable_large_slime_signature(&monster.large_slime)
            )
        }
        EnemyId::SphericGuardian => {
            format!(
                "spheric_guardian:{}",
                stable_spheric_guardian_signature(&monster.spheric_guardian)
            )
        }
        EnemyId::Lagavulin => {
            format!(
                "lagavulin:{}",
                stable_lagavulin_signature(&monster.lagavulin)
            )
        }
        EnemyId::TheGuardian => {
            format!("guardian:{}", stable_guardian_signature(&monster.guardian))
        }
        _ => "_".to_string(),
    }
}
