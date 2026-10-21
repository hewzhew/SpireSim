//! Encounter scheduling system.
//!
//! Mirrors Java's encounter list generation:
//! - `MonsterInfo` with weighted selection (`roll()`)
//! - `populate_monster_list()` with no-repeat constraints
//! - `populate_first_strong_enemy()` with exclusion table
//! - Per-act encounter definitions (Exordium, TheCity, TheBeyond)

use super::factory::EncounterId;
use crate::runtime::rng::StsRng;
use serde::{Deserialize, Serialize};

pub const PUBLIC_ENCOUNTER_POOL_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncounterPoolTier {
    Weak,
    Strong,
    Elite,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PublicEncounterPoolEntry {
    pub encounter: EncounterId,
    pub weight: f32,
}

const ACT1_WEAK: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::Cultist, 2.0),
    pool_entry(EncounterId::JawWorm, 2.0),
    pool_entry(EncounterId::TwoLouse, 2.0),
    pool_entry(EncounterId::SmallSlimes, 2.0),
];
const ACT1_STRONG: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::BlueSlaver, 2.0),
    pool_entry(EncounterId::GremlinGang, 1.0),
    pool_entry(EncounterId::Looter, 2.0),
    pool_entry(EncounterId::LargeSlime, 2.0),
    pool_entry(EncounterId::LotsOfSlimes, 1.0),
    pool_entry(EncounterId::ExordiumThugs, 1.5),
    pool_entry(EncounterId::ExordiumWildlife, 1.5),
    pool_entry(EncounterId::RedSlaver, 1.0),
    pool_entry(EncounterId::ThreeLouse, 2.0),
    pool_entry(EncounterId::TwoFungiBeasts, 2.0),
];
const ACT1_ELITE: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::GremlinNob, 1.0),
    pool_entry(EncounterId::Lagavulin, 1.0),
    pool_entry(EncounterId::ThreeSentries, 1.0),
];
const ACT2_WEAK: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::SphericGuardian, 2.0),
    pool_entry(EncounterId::ChosenAlone, 2.0),
    pool_entry(EncounterId::ShellParasite, 2.0),
    pool_entry(EncounterId::ThreeByrds, 2.0),
    pool_entry(EncounterId::TwoThieves, 2.0),
];
const ACT2_STRONG: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::ChosenAndByrds, 2.0),
    pool_entry(EncounterId::SentryAndSphere, 2.0),
    pool_entry(EncounterId::SnakePlant, 6.0),
    pool_entry(EncounterId::Snecko, 4.0),
    pool_entry(EncounterId::CenturionAndHealer, 6.0),
    pool_entry(EncounterId::CultistAndChosen, 3.0),
    pool_entry(EncounterId::ThreeCultists, 3.0),
    pool_entry(EncounterId::ShelledParasiteAndFungi, 3.0),
];
const ACT2_ELITE: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::GremlinLeader, 1.0),
    pool_entry(EncounterId::Slavers, 1.0),
    pool_entry(EncounterId::BookOfStabbing, 1.0),
];
const ACT3_WEAK: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::ThreeDarklings, 2.0),
    pool_entry(EncounterId::OrbWalker, 2.0),
    pool_entry(EncounterId::ThreeShapes, 2.0),
];
const ACT3_STRONG: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::SpireGrowth, 1.0),
    pool_entry(EncounterId::Transient, 1.0),
    pool_entry(EncounterId::FourShapes, 1.0),
    pool_entry(EncounterId::TheMaw, 1.0),
    pool_entry(EncounterId::SphereAndTwoShapes, 1.0),
    pool_entry(EncounterId::JawWormHorde, 1.0),
    pool_entry(EncounterId::ThreeDarklings, 1.0),
    pool_entry(EncounterId::WrithingMass, 1.0),
];
const ACT3_ELITE: &[PublicEncounterPoolEntry] = &[
    pool_entry(EncounterId::GiantHead, 2.0),
    pool_entry(EncounterId::TheNemesis, 2.0),
    pool_entry(EncounterId::Reptomancer, 2.0),
];
const ACT4_FIXED: &[PublicEncounterPoolEntry] = &[pool_entry(EncounterId::ShieldAndSpear, 1.0)];

const fn pool_entry(encounter: EncounterId, weight: f32) -> PublicEncounterPoolEntry {
    PublicEncounterPoolEntry { encounter, weight }
}

/// Public content eligibility only. This does not roll, condition on, or expose
/// the hidden scheduled encounter queue.
pub fn public_encounter_pool(
    act: u8,
    tier: EncounterPoolTier,
) -> &'static [PublicEncounterPoolEntry] {
    match (act, tier) {
        (1, EncounterPoolTier::Weak) => ACT1_WEAK,
        (1, EncounterPoolTier::Strong) => ACT1_STRONG,
        (1, EncounterPoolTier::Elite) => ACT1_ELITE,
        (2, EncounterPoolTier::Weak) => ACT2_WEAK,
        (2, EncounterPoolTier::Strong) => ACT2_STRONG,
        (2, EncounterPoolTier::Elite) => ACT2_ELITE,
        (3, EncounterPoolTier::Weak) => ACT3_WEAK,
        (3, EncounterPoolTier::Strong) => ACT3_STRONG,
        (3, EncounterPoolTier::Elite) => ACT3_ELITE,
        (4, EncounterPoolTier::Strong | EncounterPoolTier::Elite) => ACT4_FIXED,
        _ => &[],
    }
}

/// A weighted encounter entry, mirroring Java's `MonsterInfo`.
struct MonsterInfo {
    encounter: EncounterId,
    weight: f32,
}

fn normalized_monster_infos(entries: &[PublicEncounterPoolEntry]) -> Vec<MonsterInfo> {
    let mut infos = entries
        .iter()
        .map(|entry| MonsterInfo {
            encounter: entry.encounter,
            weight: entry.weight,
        })
        .collect::<Vec<_>>();
    normalize_weights(&mut infos);
    infos
}

/// Normalize weights so they sum to 1.0, sorted ascending by weight.
/// Java: `MonsterInfo.normalizeWeights()` — sorts by weight then divides by total.
fn normalize_weights(list: &mut [MonsterInfo]) {
    list.sort_by(|a, b| a.weight.partial_cmp(&b.weight).unwrap());
    let total: f32 = list.iter().map(|m| m.weight).sum();
    for m in list.iter_mut() {
        m.weight /= total;
    }
}

/// Weighted roll selection.
/// Java: `MonsterInfo.roll(list, roll)` — cumulative weight comparison.
fn roll(list: &[MonsterInfo], roll_value: f32) -> EncounterId {
    let mut current_weight = 0.0f32;
    for m in list {
        current_weight += m.weight;
        if roll_value < current_weight {
            return m.encounter;
        }
    }
    // Fallback (should not happen with properly normalized weights)
    list.last().unwrap().encounter
}

/// Populate monster list with no-repeat constraints.
/// Java: `AbstractDungeon.populateMonsterList()`
///
/// For normal fights: no repeat of last 1 OR last 2 entries.
/// For elite fights: no repeat of last 1 entry.
fn populate_monster_list(
    target: &mut Vec<EncounterId>,
    monsters: &[MonsterInfo],
    count: usize,
    elites: bool,
    monster_rng: &mut StsRng,
) {
    let mut i = 0;
    while i < count {
        let to_add = roll(monsters, monster_rng.random_f32());

        if target.is_empty() {
            target.push(to_add);
            i += 1;
            continue;
        }

        if elites {
            // Elite: must differ from last 1
            if to_add == *target.last().unwrap() {
                continue; // re-roll
            }
            target.push(to_add);
            i += 1;
        } else {
            // Normal: must differ from last 1 AND last 2
            if to_add == *target.last().unwrap() {
                continue; // re-roll
            }
            if target.len() > 1 && to_add == target[target.len() - 2] {
                continue; // re-roll
            }
            target.push(to_add);
            i += 1;
        }
    }
}

/// Populate the first strong enemy, re-rolling if result is in the exclusion set.
/// Java: `AbstractDungeon.populateFirstStrongEnemy()`
fn populate_first_strong_enemy(
    target: &mut Vec<EncounterId>,
    monsters: &[MonsterInfo],
    exclusions: &[EncounterId],
    monster_rng: &mut StsRng,
) {
    loop {
        let m = roll(monsters, monster_rng.random_f32());
        if !exclusions.contains(&m) {
            target.push(m);
            return;
        }
    }
}

/// Generate the full encounter schedule for a given act.
/// Returns (monster_list, elite_monster_list).
///
/// Called at dungeon initialization. The lists are consumed in order
/// as the player progresses through combat rooms.
pub fn generate_encounter_lists(
    act: u8,
    monster_rng: &mut StsRng,
) -> (Vec<EncounterId>, Vec<EncounterId>) {
    let mut monster_list: Vec<EncounterId> = Vec::new();
    let mut elite_list: Vec<EncounterId> = Vec::new();

    match act {
        1 => generate_exordium(&mut monster_list, &mut elite_list, monster_rng),
        2 => generate_the_city(&mut monster_list, &mut elite_list, monster_rng),
        3 => generate_the_beyond(&mut monster_list, &mut elite_list, monster_rng),
        4 => generate_the_ending(&mut monster_list, &mut elite_list),
        _ => {}
    }

    (monster_list, elite_list)
}

fn generate_the_ending(monster_list: &mut Vec<EncounterId>, elite_list: &mut Vec<EncounterId>) {
    // Java: TheEnding.generateMonsters() fills both normal and elite lists with
    // Shield and Spear. The fixed Act 4 map uses the elite room, but both lists
    // are populated in the source.
    monster_list.extend([EncounterId::ShieldAndSpear; 3]);
    elite_list.extend([EncounterId::ShieldAndSpear; 3]);
}

// ============================================================================
// Act 1: Exordium
// ============================================================================

fn generate_exordium(
    monster_list: &mut Vec<EncounterId>,
    elite_list: &mut Vec<EncounterId>,
    monster_rng: &mut StsRng,
) {
    // Weak enemies (3)
    // Java: Cultist(2.0), JawWorm(2.0), 2Louse(2.0), SmallSlimes(2.0)
    let weak = normalized_monster_infos(public_encounter_pool(1, EncounterPoolTier::Weak));
    populate_monster_list(monster_list, &weak, 3, false, monster_rng);

    // Strong enemies (12)
    // Java: BlueSlaver(2.0), GremlinGang(1.0), Looter(2.0), LargeSlime(2.0),
    //       LotsOfSlimes(1.0), ExordiumThugs(1.5), ExordiumWildlife(1.5),
    //       RedSlaver(1.0), 3Louse(2.0), 2FungiBeasts(2.0)
    let strong = normalized_monster_infos(public_encounter_pool(1, EncounterPoolTier::Strong));

    // First strong enemy with exclusions based on last weak enemy
    let exclusions = exordium_exclusions(monster_list);
    populate_first_strong_enemy(monster_list, &strong, &exclusions, monster_rng);
    populate_monster_list(monster_list, &strong, 12, false, monster_rng);

    // Elites (10)
    // Java: GremlinNob(1.0), Lagavulin(1.0), 3Sentries(1.0)
    let elites = normalized_monster_infos(public_encounter_pool(1, EncounterPoolTier::Elite));
    populate_monster_list(elite_list, &elites, 10, true, monster_rng);
}

/// Java: Exordium.generateExclusions()
fn exordium_exclusions(monster_list: &[EncounterId]) -> Vec<EncounterId> {
    let last = monster_list.last().copied();
    match last {
        Some(EncounterId::Looter) => vec![EncounterId::ExordiumThugs],
        Some(EncounterId::BlueSlaver) => vec![EncounterId::RedSlaver, EncounterId::ExordiumThugs],
        Some(EncounterId::TwoLouse) => vec![EncounterId::ThreeLouse],
        Some(EncounterId::SmallSlimes) => vec![EncounterId::LargeSlime, EncounterId::LotsOfSlimes],
        _ => vec![],
    }
}

// ============================================================================
// Act 2: The City
// ============================================================================

fn generate_the_city(
    monster_list: &mut Vec<EncounterId>,
    elite_list: &mut Vec<EncounterId>,
    monster_rng: &mut StsRng,
) {
    // Weak enemies (2)
    // Java: SphericGuardian(2.0), Chosen(2.0), ShellParasite(2.0), 3Byrds(2.0), 2Thieves(2.0)
    let weak = normalized_monster_infos(public_encounter_pool(2, EncounterPoolTier::Weak));
    populate_monster_list(monster_list, &weak, 2, false, monster_rng);

    // Strong enemies (12)
    // Java: ChosenAndByrds(2.0), SentryAndSphere(2.0), SnakePlant(6.0), Snecko(4.0),
    //       CenturionAndHealer(6.0), CultistAndChosen(3.0), 3Cultists(3.0), ShelledParasiteAndFungi(3.0)
    let strong = normalized_monster_infos(public_encounter_pool(2, EncounterPoolTier::Strong));
    let exclusions = city_exclusions(monster_list);
    populate_first_strong_enemy(monster_list, &strong, &exclusions, monster_rng);
    populate_monster_list(monster_list, &strong, 12, false, monster_rng);

    // Elites (10)
    // Java: GremlinLeader(1.0), Slavers(1.0), BookOfStabbing(1.0)
    let elites = normalized_monster_infos(public_encounter_pool(2, EncounterPoolTier::Elite));
    populate_monster_list(elite_list, &elites, 10, true, monster_rng);
}

/// Java: TheCity.generateExclusions()
fn city_exclusions(monster_list: &[EncounterId]) -> Vec<EncounterId> {
    let last = monster_list.last().copied();
    match last {
        Some(EncounterId::SphericGuardian) => vec![EncounterId::SentryAndSphere],
        Some(EncounterId::ThreeByrds) => vec![EncounterId::ChosenAndByrds],
        Some(EncounterId::ChosenAlone) => {
            vec![EncounterId::ChosenAndByrds, EncounterId::CultistAndChosen]
        }
        _ => vec![],
    }
}

// ============================================================================
// Act 3: The Beyond
// ============================================================================

fn generate_the_beyond(
    monster_list: &mut Vec<EncounterId>,
    elite_list: &mut Vec<EncounterId>,
    monster_rng: &mut StsRng,
) {
    // Weak enemies (2)
    // Java: 3Darklings(2.0), OrbWalker(2.0), 3Shapes(2.0)
    let weak = normalized_monster_infos(public_encounter_pool(3, EncounterPoolTier::Weak));
    populate_monster_list(monster_list, &weak, 2, false, monster_rng);

    // Strong enemies (12)
    // Java: SpireGrowth(1.0), Transient(1.0), 4Shapes(1.0), Maw(1.0),
    //       SphereAndTwoShapes(1.0), JawWormHorde(1.0), 3Darklings(1.0), WrithingMass(1.0)
    let strong = normalized_monster_infos(public_encounter_pool(3, EncounterPoolTier::Strong));
    let exclusions = beyond_exclusions(monster_list);
    populate_first_strong_enemy(monster_list, &strong, &exclusions, monster_rng);
    populate_monster_list(monster_list, &strong, 12, false, monster_rng);

    // Elites (10)
    // Java: GiantHead(2.0), Nemesis(2.0), Reptomancer(2.0)
    let elites = normalized_monster_infos(public_encounter_pool(3, EncounterPoolTier::Elite));
    populate_monster_list(elite_list, &elites, 10, true, monster_rng);
}

/// Java: TheBeyond.generateExclusions()
fn beyond_exclusions(monster_list: &[EncounterId]) -> Vec<EncounterId> {
    let last = monster_list.last().copied();
    match last {
        Some(EncounterId::ThreeDarklings) => vec![EncounterId::ThreeDarklings],
        Some(EncounterId::OrbWalker) => vec![EncounterId::OrbWalker],
        Some(EncounterId::ThreeShapes) => vec![EncounterId::FourShapes],
        _ => vec![],
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BossSeenState {
    pub guardian: bool,
    pub hexaghost: bool,
    pub slime_boss: bool,
    pub champ: bool,
    pub automaton: bool,
    pub collector: bool,
    pub awakened_one: bool,
    pub donu_and_deca: bool,
    pub time_eater: bool,
}

impl BossSeenState {
    pub const fn all_seen() -> Self {
        Self {
            guardian: true,
            hexaghost: true,
            slime_boss: true,
            champ: true,
            automaton: true,
            collector: true,
            awakened_one: true,
            donu_and_deca: true,
            time_eater: true,
        }
    }

    #[cfg(test)]
    const fn none_seen() -> Self {
        Self {
            guardian: false,
            hexaghost: false,
            slime_boss: false,
            champ: false,
            automaton: false,
            collector: false,
            awakened_one: false,
            donu_and_deca: false,
            time_eater: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BossGenerationSettings {
    pub is_daily_run: bool,
    pub is_demo: bool,
    pub seen: BossSeenState,
}

impl BossGenerationSettings {
    pub const fn standard_all_seen() -> Self {
        Self {
            is_daily_run: false,
            is_demo: false,
            seen: BossSeenState::all_seen(),
        }
    }
}

fn shuffle_bosses(bosses: &mut [EncounterId], monster_rng: &mut StsRng) {
    crate::runtime::rng::shuffle_with_random_long(bosses, monster_rng);
}

fn duplicate_single_boss_or_fallback(
    bosses: &mut Vec<EncounterId>,
    fallback: &[EncounterId],
    monster_rng: &mut StsRng,
) {
    if bosses.len() == 1 {
        bosses.push(bosses[0]);
    } else if bosses.is_empty() {
        bosses.extend_from_slice(fallback);
        shuffle_bosses(bosses, monster_rng);
    }
}

fn initialize_exordium_bosses(
    settings: BossGenerationSettings,
    monster_rng: &mut StsRng,
) -> Vec<EncounterId> {
    let fallback = [
        EncounterId::TheGuardian,
        EncounterId::Hexaghost,
        EncounterId::SlimeBoss,
    ];
    let mut bosses = Vec::new();
    if settings.is_daily_run {
        bosses.extend_from_slice(&fallback);
        shuffle_bosses(&mut bosses, monster_rng);
    } else if !settings.seen.guardian {
        bosses.push(EncounterId::TheGuardian);
    } else if !settings.seen.hexaghost {
        bosses.push(EncounterId::Hexaghost);
    } else if !settings.seen.slime_boss {
        bosses.push(EncounterId::SlimeBoss);
    } else {
        bosses.extend_from_slice(&fallback);
        shuffle_bosses(&mut bosses, monster_rng);
    }

    duplicate_single_boss_or_fallback(&mut bosses, &fallback, monster_rng);

    if settings.is_demo {
        bosses.clear();
        bosses.push(EncounterId::Hexaghost);
    }

    bosses
}

fn initialize_city_bosses(
    settings: BossGenerationSettings,
    monster_rng: &mut StsRng,
) -> Vec<EncounterId> {
    let fallback = [
        EncounterId::Automaton,
        EncounterId::Collector,
        EncounterId::TheChamp,
    ];
    let mut bosses = Vec::new();
    if settings.is_daily_run {
        bosses.extend_from_slice(&fallback);
        shuffle_bosses(&mut bosses, monster_rng);
    } else if !settings.seen.champ {
        bosses.push(EncounterId::TheChamp);
    } else if !settings.seen.automaton {
        bosses.push(EncounterId::Automaton);
    } else if !settings.seen.collector {
        bosses.push(EncounterId::Collector);
    } else {
        bosses.extend_from_slice(&fallback);
        shuffle_bosses(&mut bosses, monster_rng);
    }

    duplicate_single_boss_or_fallback(&mut bosses, &fallback, monster_rng);
    bosses
}

fn initialize_beyond_bosses(
    settings: BossGenerationSettings,
    monster_rng: &mut StsRng,
) -> Vec<EncounterId> {
    let fallback = [
        EncounterId::AwakenedOne,
        EncounterId::TimeEater,
        EncounterId::DonuAndDeca,
    ];
    let mut bosses = Vec::new();
    if settings.is_daily_run {
        bosses.extend_from_slice(&fallback);
        shuffle_bosses(&mut bosses, monster_rng);
    } else if !settings.seen.awakened_one {
        bosses.push(EncounterId::AwakenedOne);
    } else if !settings.seen.donu_and_deca {
        bosses.push(EncounterId::DonuAndDeca);
    } else if !settings.seen.time_eater {
        bosses.push(EncounterId::TimeEater);
    } else {
        bosses.extend_from_slice(&fallback);
        shuffle_bosses(&mut bosses, monster_rng);
    }

    duplicate_single_boss_or_fallback(&mut bosses, &fallback, monster_rng);
    bosses
}

/// Generate the boss list for the given act under the simulator's standard
/// profile: a normal, all-unlocked/all-bosses-seen run.
pub fn generate_boss_list(
    act: u8,
    monster_rng: &mut crate::runtime::rng::StsRng,
) -> Vec<EncounterId> {
    generate_boss_list_with_settings(
        act,
        monster_rng,
        BossGenerationSettings::standard_all_seen(),
    )
}

/// Java: `initializeBoss()` in Exordium/TheCity/TheBeyond/TheEnding.
///
/// Non-daily runs force the first unseen boss in each act-specific unlock
/// order. If exactly one boss is selected, Java duplicates it so the boss list
/// has at least two entries. Once every boss in the act has been seen, or in
/// daily runs, Java shuffles all three bosses with
/// `Collections.shuffle(bossList, new Random(monsterRng.randomLong()))`.
pub fn generate_boss_list_with_settings(
    act: u8,
    monster_rng: &mut crate::runtime::rng::StsRng,
    settings: BossGenerationSettings,
) -> Vec<EncounterId> {
    match act {
        1 => initialize_exordium_bosses(settings, monster_rng),
        2 => initialize_city_bosses(settings, monster_rng),
        3 => initialize_beyond_bosses(settings, monster_rng),
        4 => vec![
            EncounterId::TheHeart,
            EncounterId::TheHeart,
            EncounterId::TheHeart,
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_act3_pools_expose_content_weights_without_realizing_hidden_queue() {
        let strong = public_encounter_pool(3, EncounterPoolTier::Strong);
        let elite = public_encounter_pool(3, EncounterPoolTier::Elite);

        assert_eq!(
            strong
                .iter()
                .map(|entry| (entry.encounter, entry.weight))
                .collect::<Vec<_>>(),
            vec![
                (EncounterId::SpireGrowth, 1.0),
                (EncounterId::Transient, 1.0),
                (EncounterId::FourShapes, 1.0),
                (EncounterId::TheMaw, 1.0),
                (EncounterId::SphereAndTwoShapes, 1.0),
                (EncounterId::JawWormHorde, 1.0),
                (EncounterId::ThreeDarklings, 1.0),
                (EncounterId::WrithingMass, 1.0),
            ]
        );
        assert_eq!(
            elite
                .iter()
                .map(|entry| (entry.encounter, entry.weight))
                .collect::<Vec<_>>(),
            vec![
                (EncounterId::GiantHead, 2.0),
                (EncounterId::TheNemesis, 2.0),
                (EncounterId::Reptomancer, 2.0),
            ]
        );
        assert!(public_encounter_pool(99, EncounterPoolTier::Strong).is_empty());
    }

    fn assert_java_normal_repeat_rule(monsters: &[EncounterId], first_strong_index: usize) {
        for window in monsters.windows(2) {
            assert_ne!(
                window[0], window[1],
                "Java normal encounter list rejects immediate repeats"
            );
        }
        for (start, window) in monsters.windows(3).enumerate() {
            let end = start + 2;
            if end == first_strong_index {
                continue;
            }
            assert_ne!(
                window[0], window[2],
                "Java normal encounter list rejects repeats from two encounters ago"
            );
        }
    }

    fn assert_java_elite_repeat_rule(elites: &[EncounterId]) {
        for window in elites.windows(2) {
            assert_ne!(
                window[0], window[1],
                "Java elite encounter list rejects immediate repeats"
            );
        }
    }

    fn assert_exordium_first_strong_exclusions(monsters: &[EncounterId]) {
        let last_weak = monsters[2];
        let first_strong = monsters[3];
        assert!(
            !match last_weak {
                EncounterId::Looter => [EncounterId::ExordiumThugs].contains(&first_strong),
                EncounterId::BlueSlaver => {
                    [EncounterId::RedSlaver, EncounterId::ExordiumThugs].contains(&first_strong)
                }
                EncounterId::TwoLouse => [EncounterId::ThreeLouse].contains(&first_strong),
                EncounterId::SmallSlimes => {
                    [EncounterId::LargeSlime, EncounterId::LotsOfSlimes].contains(&first_strong)
                }
                _ => false,
            },
            "Java populateFirstStrongEnemy rerolls act-specific exclusions"
        );
    }

    fn assert_city_first_strong_exclusions(monsters: &[EncounterId]) {
        let last_weak = monsters[1];
        let first_strong = monsters[2];
        assert!(
            !match last_weak {
                EncounterId::SphericGuardian => {
                    [EncounterId::SentryAndSphere].contains(&first_strong)
                }
                EncounterId::ThreeByrds => [EncounterId::ChosenAndByrds].contains(&first_strong),
                EncounterId::ChosenAlone => {
                    [EncounterId::ChosenAndByrds, EncounterId::CultistAndChosen]
                        .contains(&first_strong)
                }
                _ => false,
            },
            "Java TheCity.generateExclusions rerolls the first strong encounter"
        );
    }

    fn assert_beyond_first_strong_exclusions(monsters: &[EncounterId]) {
        let last_weak = monsters[1];
        let first_strong = monsters[2];
        assert!(
            !match last_weak {
                EncounterId::ThreeDarklings =>
                    [EncounterId::ThreeDarklings].contains(&first_strong),
                EncounterId::OrbWalker => [EncounterId::OrbWalker].contains(&first_strong),
                EncounterId::ThreeShapes => [EncounterId::FourShapes].contains(&first_strong),
                _ => false,
            },
            "Java TheBeyond.generateExclusions rerolls the first strong encounter"
        );
    }

    #[test]
    fn first_strong_exclusion_tables_match_java_sources() {
        assert_eq!(
            exordium_exclusions(&[EncounterId::Looter]),
            vec![EncounterId::ExordiumThugs]
        );
        assert_eq!(
            exordium_exclusions(&[EncounterId::BlueSlaver]),
            vec![EncounterId::RedSlaver, EncounterId::ExordiumThugs]
        );
        assert_eq!(
            exordium_exclusions(&[EncounterId::TwoLouse]),
            vec![EncounterId::ThreeLouse]
        );
        assert_eq!(
            exordium_exclusions(&[EncounterId::SmallSlimes]),
            vec![EncounterId::LargeSlime, EncounterId::LotsOfSlimes]
        );
        assert!(exordium_exclusions(&[EncounterId::JawWorm]).is_empty());
        assert!(exordium_exclusions(&[EncounterId::Cultist]).is_empty());

        assert_eq!(
            city_exclusions(&[EncounterId::SphericGuardian]),
            vec![EncounterId::SentryAndSphere]
        );
        assert_eq!(
            city_exclusions(&[EncounterId::ThreeByrds]),
            vec![EncounterId::ChosenAndByrds]
        );
        assert_eq!(
            city_exclusions(&[EncounterId::ChosenAlone]),
            vec![EncounterId::ChosenAndByrds, EncounterId::CultistAndChosen]
        );
        assert!(city_exclusions(&[EncounterId::ShellParasite]).is_empty());
        assert!(city_exclusions(&[EncounterId::TwoThieves]).is_empty());

        assert_eq!(
            beyond_exclusions(&[EncounterId::ThreeDarklings]),
            vec![EncounterId::ThreeDarklings]
        );
        assert_eq!(
            beyond_exclusions(&[EncounterId::OrbWalker]),
            vec![EncounterId::OrbWalker]
        );
        assert_eq!(
            beyond_exclusions(&[EncounterId::ThreeShapes]),
            vec![EncounterId::FourShapes]
        );
    }

    #[test]
    fn encounter_lists_preserve_java_generation_invariants() {
        for seed in [1, 7, 17, 42, 5201, 99991] {
            let mut rng = StsRng::new(seed);

            let (act1_monsters, act1_elites) = generate_encounter_lists(1, &mut rng);
            assert_eq!(act1_monsters.len(), 16);
            assert_eq!(act1_elites.len(), 10);
            assert_java_normal_repeat_rule(&act1_monsters, 3);
            assert_java_elite_repeat_rule(&act1_elites);
            assert_exordium_first_strong_exclusions(&act1_monsters);

            let (act2_monsters, act2_elites) = generate_encounter_lists(2, &mut rng);
            assert_eq!(act2_monsters.len(), 15);
            assert_eq!(act2_elites.len(), 10);
            assert_java_normal_repeat_rule(&act2_monsters, 2);
            assert_java_elite_repeat_rule(&act2_elites);
            assert_city_first_strong_exclusions(&act2_monsters);

            let (act3_monsters, act3_elites) = generate_encounter_lists(3, &mut rng);
            assert_eq!(act3_monsters.len(), 15);
            assert_eq!(act3_elites.len(), 10);
            assert_java_normal_repeat_rule(&act3_monsters, 2);
            assert_java_elite_repeat_rule(&act3_elites);
            assert_beyond_first_strong_exclusions(&act3_monsters);

            let (act4_monsters, act4_elites) = generate_encounter_lists(4, &mut rng);
            assert_eq!(act4_monsters, vec![EncounterId::ShieldAndSpear; 3]);
            assert_eq!(act4_elites, vec![EncounterId::ShieldAndSpear; 3]);
        }
    }

    #[test]
    fn unknown_act_does_not_fall_back_to_exordium_encounter_lists() {
        let mut rng = StsRng::new(7);

        let (monsters, elites) = generate_encounter_lists(99, &mut rng);

        assert!(monsters.is_empty());
        assert!(elites.is_empty());
    }

    #[test]
    fn boss_lists_preserve_java_seen_boss_unlock_order() {
        let mut rng = StsRng::new(7);
        let settings = BossGenerationSettings {
            is_daily_run: false,
            is_demo: false,
            seen: BossSeenState::none_seen(),
        };

        assert_eq!(
            generate_boss_list_with_settings(1, &mut rng, settings),
            vec![EncounterId::TheGuardian, EncounterId::TheGuardian]
        );
        assert_eq!(
            rng.counter, 0,
            "Java does not call monsterRng.randomLong() when it forces a single unseen boss"
        );

        let mut seen = BossSeenState::none_seen();
        seen.guardian = true;
        let mut rng = StsRng::new(7);
        assert_eq!(
            generate_boss_list_with_settings(
                1,
                &mut rng,
                BossGenerationSettings { seen, ..settings },
            ),
            vec![EncounterId::Hexaghost, EncounterId::Hexaghost]
        );
        assert_eq!(rng.counter, 0);

        let mut rng = StsRng::new(7);
        assert_eq!(
            generate_boss_list_with_settings(2, &mut rng, settings),
            vec![EncounterId::TheChamp, EncounterId::TheChamp]
        );
        assert_eq!(rng.counter, 0);

        let mut rng = StsRng::new(7);
        assert_eq!(
            generate_boss_list_with_settings(3, &mut rng, settings),
            vec![EncounterId::AwakenedOne, EncounterId::AwakenedOne]
        );
        assert_eq!(rng.counter, 0);
    }

    #[test]
    fn boss_lists_shuffle_all_three_only_for_daily_or_all_seen_paths() {
        let mut rng = StsRng::new(7);
        let bosses = generate_boss_list_with_settings(
            1,
            &mut rng,
            BossGenerationSettings::standard_all_seen(),
        );
        assert_eq!(bosses.len(), 3);
        assert!(bosses.contains(&EncounterId::TheGuardian));
        assert!(bosses.contains(&EncounterId::Hexaghost));
        assert!(bosses.contains(&EncounterId::SlimeBoss));
        assert_eq!(
            rng.counter, 1,
            "Java all-seen boss generation consumes one monsterRng.randomLong() for Collections.shuffle"
        );

        let mut rng = StsRng::new(7);
        let bosses = generate_boss_list_with_settings(
            2,
            &mut rng,
            BossGenerationSettings {
                is_daily_run: true,
                is_demo: false,
                seen: BossSeenState::none_seen(),
            },
        );
        assert_eq!(bosses.len(), 3);
        assert!(bosses.contains(&EncounterId::Automaton));
        assert!(bosses.contains(&EncounterId::Collector));
        assert!(bosses.contains(&EncounterId::TheChamp));
        assert_eq!(
            rng.counter, 1,
            "Java daily boss generation shuffles all bosses without checking UnlockTracker"
        );
    }

    #[test]
    fn exordium_demo_overrides_after_java_boss_generation_branch() {
        let mut rng = StsRng::new(7);
        let bosses = generate_boss_list_with_settings(
            1,
            &mut rng,
            BossGenerationSettings {
                is_daily_run: false,
                is_demo: true,
                seen: BossSeenState::all_seen(),
            },
        );

        assert_eq!(bosses, vec![EncounterId::Hexaghost]);
        assert_eq!(
            rng.counter, 1,
            "Java Exordium.initializeBoss runs the all-seen shuffle before Settings.isDemo clears the list"
        );
    }
}
