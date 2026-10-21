use crate::content::monsters::factory::EncounterId;
use crate::state::run::RunState;

use crate::ai::route_window_facts::{
    build_route_window_facts, RouteWindowFactsConfig, RouteWindowModality, RouteWindowPredicate,
    RouteWindowSubject,
};

use super::types::{
    StrategyThreatProfileV1, StrategyThreatSourceRecordV1, StrategyThreatSourceV1,
    StrategyThreatTagV1,
};

pub fn threat_profile_from_run_state_v1(run_state: &RunState) -> StrategyThreatProfileV1 {
    let current_act_cleared = current_act_cleared_at_strategic_boundary_v1(run_state);
    let current_boss = (!current_act_cleared)
        .then_some(run_state.boss_key)
        .flatten()
        .filter(|boss| boss_act_v1(*boss) == Some(run_state.act_num));
    let mut profile = StrategyThreatProfileV1 {
        current_act: run_state.act_num,
        boss: current_boss.map(|boss| boss.stable_id().to_string()),
        tags: Vec::new(),
        sources: Vec::new(),
        evidence: Vec::new(),
    };

    if let Some(boss) = current_boss {
        add_boss_threats(run_state.act_num, boss, &mut profile);
    }
    let future_pools = if current_act_cleared {
        FutureCurrentActCombatPoolsV1 {
            elite: false,
            hallway: false,
        }
    } else {
        future_current_act_combat_pools_v1(run_state)
    };
    if future_pools.elite {
        add_act_elite_pool_threats(run_state.act_num, &mut profile);
    }
    if future_pools.hallway {
        add_act_hallway_pool_threats(run_state.act_num, &mut profile);
    }

    profile
}

fn current_act_cleared_at_strategic_boundary_v1(run_state: &RunState) -> bool {
    run_state.pending_boss_reward
        || run_state.pending_boss_act_transition
        || run_state.map.current_y >= 15
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FutureCurrentActCombatPoolsV1 {
    elite: bool,
    hallway: bool,
}

fn future_current_act_combat_pools_v1(run_state: &RunState) -> FutureCurrentActCombatPoolsV1 {
    let facts = build_route_window_facts(
        run_state,
        RouteWindowFactsConfig {
            horizon_nodes: 15,
            path_budget: 2_000,
        },
    );
    FutureCurrentActCombatPoolsV1 {
        elite: route_subject_may_remain_v1(&facts.facts, RouteWindowSubject::Elite),
        hallway: route_subject_may_remain_v1(&facts.facts, RouteWindowSubject::HallwayCombat),
    }
}

fn route_subject_may_remain_v1(
    facts: &[crate::ai::route_window_facts::RouteWindowFact],
    subject: RouteWindowSubject,
) -> bool {
    facts
        .iter()
        .find(|fact| fact.predicate == RouteWindowPredicate::PresentInWindow { subject })
        .is_none_or(|fact| fact.modality != RouteWindowModality::Cannot)
}

pub fn empty_threat_profile_v1() -> StrategyThreatProfileV1 {
    StrategyThreatProfileV1::default()
}

fn boss_act_v1(boss: EncounterId) -> Option<u8> {
    match boss {
        EncounterId::TheGuardian | EncounterId::Hexaghost | EncounterId::SlimeBoss => Some(1),
        EncounterId::Automaton | EncounterId::Collector | EncounterId::TheChamp => Some(2),
        EncounterId::AwakenedOne | EncounterId::TimeEater | EncounterId::DonuAndDeca => Some(3),
        EncounterId::TheHeart => Some(4),
        _ => None,
    }
}

fn add_boss_threats(act: u8, boss: EncounterId, profile: &mut StrategyThreatProfileV1) {
    match boss {
        EncounterId::TheGuardian => {
            let evidence =
                "Act boss The Guardian: mode shift and large attacks reward controlled damage timing and mitigation";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "TheGuardian",
                &[
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::ModeShiftThreshold,
                    StrategyThreatTagV1::WeakValuable,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        EncounterId::Hexaghost => {
            let evidence =
                "Act boss Hexaghost: multi-hit attacks and Burn pressure reward mitigation and race damage";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "Hexaghost",
                &[
                    StrategyThreatTagV1::MultiHit,
                    StrategyThreatTagV1::StatusFlood,
                    StrategyThreatTagV1::HighIncomingDamage,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        EncounterId::SlimeBoss => {
            let evidence =
                "Act boss Slime Boss: split threshold and status pressure reward burst timing and post-split AoE";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "SlimeBoss",
                &[
                    StrategyThreatTagV1::SplitThreshold,
                    StrategyThreatTagV1::TimedDamageRace,
                    StrategyThreatTagV1::StatusFlood,
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::HighIncomingDamage,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        EncounterId::Automaton => {
            let evidence =
                "Act boss Bronze Automaton: artifact, minions, and Hyper Beam reward setup plus artifact-aware control";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "Automaton",
                &[
                    StrategyThreatTagV1::ArtifactBlocksDebuff,
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::SetupWindow,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        EncounterId::Collector => {
            let evidence =
                "Act boss Collector: repeated minion summons and long-fight pressure reward reliable multi-target control";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "Collector",
                &[
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::LongFightScaling,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        EncounterId::TheChamp => {
            let evidence =
                "Act boss Champ: long fight and execute phase reward strength down, weak, and scaling";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "TheChamp",
                &[
                    StrategyThreatTagV1::StrengthDebuffValuable,
                    StrategyThreatTagV1::WeakValuable,
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::LongFightScaling,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        EncounterId::AwakenedOne => {
            let evidence =
                "Act boss Awakened One: power scaling, cultists, and second phase reward careful power use and long-fight plans";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "AwakenedOne",
                &[
                    StrategyThreatTagV1::PowerPunish,
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::LongFightScaling,
                    StrategyThreatTagV1::HighIncomingDamage,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        EncounterId::TimeEater => {
            let evidence =
                "Act boss Time Eater: card play limit and long fight reward dense turns, mitigation, and scaling";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActBoss,
                "TimeEater",
                &[
                    StrategyThreatTagV1::CardPlayLimit,
                    StrategyThreatTagV1::LongFightScaling,
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::WeakValuable,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
        }
        _ => {}
    }
}

fn add_act_elite_pool_threats(act: u8, profile: &mut StrategyThreatProfileV1) {
    match act {
        1 => {
            let evidence =
                "Act 1 elite pool: Nob punishes skills, Sentries flood statuses, Lagavulin rewards setup/debuff answers";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActElitePool,
                "Act1ElitePool",
                &[
                    StrategyThreatTagV1::SkillPunish,
                    StrategyThreatTagV1::StatusFlood,
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::StrengthDebuffValuable,
                    StrategyThreatTagV1::SetupWindow,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
            push_elite_encounter_threats(
                profile,
                act,
                "GremlinNob",
                &[
                    StrategyThreatTagV1::SkillPunish,
                    StrategyThreatTagV1::HighIncomingDamage,
                ],
                "Act 1 elite Gremlin Nob: skills increase Strength and the fight rewards fast frontload",
            );
            push_elite_encounter_threats(
                profile,
                act,
                "Lagavulin",
                &[
                    StrategyThreatTagV1::SetupWindow,
                    StrategyThreatTagV1::StrengthDebuffValuable,
                    StrategyThreatTagV1::HighIncomingDamage,
                ],
                "Act 1 elite Lagavulin: sleep setup window and strength/dexterity debuffs reward setup plus mitigation",
            );
            push_elite_encounter_threats(
                profile,
                act,
                "ThreeSentries",
                &[
                    StrategyThreatTagV1::StatusFlood,
                    StrategyThreatTagV1::AoEValuable,
                ],
                "Act 1 elite Three Sentries: Dazed flood and three bodies reward status handling and AoE",
            );
        }
        2 => {
            let evidence =
                "Act 2 elite pool: Slavers/Gremlin Leader/Book reward frontload, AoE, and mitigation";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActElitePool,
                "Act2ElitePool",
                &[
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::MultiHit,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
            push_elite_encounter_threats(
                profile,
                act,
                "Slavers",
                &[
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::AoEValuable,
                ],
                "Act 2 elite Slavers: immediate multi-enemy damage pressure rewards frontload and AoE",
            );
            push_elite_encounter_threats(
                profile,
                act,
                "GremlinLeader",
                &[
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::SetupWindow,
                ],
                "Act 2 elite Gremlin Leader: minion summons reward AoE while some turns allow setup",
            );
            push_elite_encounter_threats(
                profile,
                act,
                "BookOfStabbing",
                &[
                    StrategyThreatTagV1::MultiHit,
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::StrengthDebuffValuable,
                ],
                "Act 2 elite Book of Stabbing: scaling multi-hit attacks reward strength down and mitigation",
            );
        }
        3 => {
            let evidence =
                "Act 3 elite pool: Reptomancer/Nemesis/Giant Head reward burst control, AoE, and long-fight scaling";
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActElitePool,
                "Act3ElitePool",
                &[
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::LongFightScaling,
                ],
                evidence,
            );
            profile.evidence.push(evidence.to_string());
            push_elite_encounter_threats(
                profile,
                act,
                "Reptomancer",
                &[
                    StrategyThreatTagV1::AoEValuable,
                    StrategyThreatTagV1::HighIncomingDamage,
                ],
                "Act 3 elite Reptomancer: dagger burst pressure rewards AoE and frontload",
            );
            push_elite_encounter_threats(
                profile,
                act,
                "TheNemesis",
                &[
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::SetupWindow,
                ],
                "Act 3 elite Nemesis: intangible turns and burst attacks reward setup timing and mitigation",
            );
            push_elite_encounter_threats(
                profile,
                act,
                "GiantHead",
                &[StrategyThreatTagV1::LongFightScaling],
                "Act 3 elite Giant Head: long fight rewards scaling and dense damage turns",
            );
        }
        _ => {}
    }
}

fn add_act_hallway_pool_threats(act: u8, profile: &mut StrategyThreatProfileV1) {
    match act {
        1 => push_tags(
            profile,
            act,
            StrategyThreatSourceV1::ActHallwayPool,
            "Act1HallwayPool",
            &[StrategyThreatTagV1::TimedDamageRace],
            "Act 1 hallway pool rewards early single-target frontload before enemies scale or inflict repeated chip damage",
        ),
        2 => push_tags(
            profile,
            act,
            StrategyThreatSourceV1::ActHallwayPool,
            "Act2HallwayPool",
            &[
                StrategyThreatTagV1::HighIncomingDamage,
                StrategyThreatTagV1::AoEValuable,
            ],
            "Act 2 hallway pool includes early multi-enemy and high-frontload pressure",
        ),
        3 => {
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActHallwayPool,
                "Spiker",
                &[StrategyThreatTagV1::RetaliationPunish],
                "Act 3 Spiker retaliation rewards high damage per triggering hit or non-attack damage",
            );
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActHallwayPool,
                "Transient",
                &[StrategyThreatTagV1::TimedDamageRace],
                "Act 3 Transient requires reliable damage each turn to suppress incoming damage",
            );
            push_tags(
                profile,
                act,
                StrategyThreatSourceV1::ActHallwayPool,
                "Act3HallwayPool",
                &[
                    StrategyThreatTagV1::HighIncomingDamage,
                    StrategyThreatTagV1::ArtifactBlocksDebuff,
                ],
                "Act 3 hallway pool combines large attacks with artifact-backed enemies",
            );
        }
        _ => {}
    }
}

fn push_elite_encounter_threats(
    profile: &mut StrategyThreatProfileV1,
    act: u8,
    subject: &str,
    tags: &[StrategyThreatTagV1],
    evidence: &str,
) {
    push_tags(
        profile,
        act,
        StrategyThreatSourceV1::ActEliteEncounter,
        subject,
        tags,
        evidence,
    );
}

fn push_tags(
    profile: &mut StrategyThreatProfileV1,
    source_act: u8,
    source: StrategyThreatSourceV1,
    subject: &str,
    tags: &[StrategyThreatTagV1],
    evidence: &str,
) {
    for tag in tags {
        if !profile.tags.contains(tag) {
            profile.tags.push(*tag);
        }
        let source_record = StrategyThreatSourceRecordV1 {
            source_act,
            tag: *tag,
            source,
            subject: subject.to_string(),
            evidence: evidence.to_string(),
        };
        if !profile.sources.contains(&source_record) {
            profile.sources.push(source_record);
        }
    }
}
