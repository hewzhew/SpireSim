use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};

use crate::eval::fingerprint::{combat_state_fingerprint_v3, StateFingerprintV3};
use crate::fixtures::combat_start_spec::compile_combat_start_spec_with_rng_overrides;
use crate::runtime::rng::{RngPool, StsRng};
use crate::sim::combat::CombatPosition;

use super::{derive_shuffle_seed_v1, ResolvedCombatLabSpecV1};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabCompiledSampleV1 {
    pub sample_index: u64,
    pub shuffle_seed: u64,
    pub start: CombatPosition,
    pub state_fingerprint: StateFingerprintV3,
    pub non_shuffle_rng_hash: String,
    pub shuffle_rng_hash: String,
    pub monster_snapshot_hash: String,
}

pub struct CombatLabScenarioCompilerV1 {
    resolved: ResolvedCombatLabSpecV1,
    baseline: CombatPosition,
    baseline_non_shuffle_rng_hash: String,
    baseline_monster_snapshot_hash: String,
}

pub fn preflight_combat_lab_scenario_v1(
    resolved: &ResolvedCombatLabSpecV1,
) -> Result<CombatLabScenarioCompilerV1, String> {
    let (engine, combat) = compile_combat_start_spec_with_rng_overrides(
        &resolved.start_spec_snapshot,
        resolved.start_spec_snapshot.seed,
        None,
    )?;
    let baseline = CombatPosition::new(engine, combat);
    let baseline_non_shuffle_rng_hash = non_shuffle_rng_hash(&baseline.combat.rng.pool)?;
    let baseline_monster_snapshot_hash = monster_snapshot_hash(&baseline)?;
    Ok(CombatLabScenarioCompilerV1 {
        resolved: resolved.clone(),
        baseline,
        baseline_non_shuffle_rng_hash,
        baseline_monster_snapshot_hash,
    })
}

impl CombatLabScenarioCompilerV1 {
    pub fn compile_bank(
        &self,
        sample_count: u64,
    ) -> Result<Vec<CombatLabCompiledSampleV1>, String> {
        if sample_count == 0 {
            return Err("combat laboratory scenario bank must be nonempty".to_string());
        }
        (0..sample_count)
            .map(|sample_index| self.compile_sample(sample_index))
            .collect()
    }

    pub fn compile_sample(&self, sample_index: u64) -> Result<CombatLabCompiledSampleV1, String> {
        let shuffle_seed = derive_shuffle_seed_v1(&self.resolved.schedule, sample_index);
        let (engine, combat) = compile_combat_start_spec_with_rng_overrides(
            &self.resolved.start_spec_snapshot,
            self.resolved.start_spec_snapshot.seed,
            Some(shuffle_seed),
        )?;
        let start = CombatPosition::new(engine, combat);
        validate_combat_lab_sample_invariants_v1(&self.baseline, &start)?;

        let non_shuffle_rng_hash = non_shuffle_rng_hash(&start.combat.rng.pool)?;
        if non_shuffle_rng_hash != self.baseline_non_shuffle_rng_hash {
            return Err(invariant_error("non_shuffle_rng_hash"));
        }
        let monster_snapshot_hash = monster_snapshot_hash(&start)?;
        if monster_snapshot_hash != self.baseline_monster_snapshot_hash {
            return Err(invariant_error("monster_snapshot_hash"));
        }
        let shuffle_rng_hash = hash_serializable(&start.combat.rng.pool.shuffle_rng)?;
        let state_fingerprint = combat_state_fingerprint_v3(&start);

        Ok(CombatLabCompiledSampleV1 {
            sample_index,
            shuffle_seed,
            start,
            state_fingerprint,
            non_shuffle_rng_hash,
            shuffle_rng_hash,
            monster_snapshot_hash,
        })
    }
}

pub(super) fn validate_combat_lab_sample_invariants_v1(
    baseline: &CombatPosition,
    sampled: &CombatPosition,
) -> Result<(), String> {
    let baseline_monsters = &baseline.combat.entities.monsters;
    let sampled_monsters = &sampled.combat.entities.monsters;
    if baseline_monsters.len() != sampled_monsters.len() {
        return Err(invariant_error("monsters.length"));
    }
    for (index, (baseline, sampled)) in baseline_monsters.iter().zip(sampled_monsters).enumerate() {
        if baseline.id != sampled.id {
            return Err(invariant_error(&format!("monsters[{index}].id")));
        }
        if baseline.enemy_id != sampled.enemy_id {
            return Err(invariant_error(&format!("monsters[{index}].enemy_id")));
        }
        if baseline.slot != sampled.slot {
            return Err(invariant_error(&format!("monsters[{index}].slot")));
        }
        if baseline.current_hp != sampled.current_hp {
            return Err(invariant_error(&format!("monsters[{index}].current_hp")));
        }
        if baseline.max_hp != sampled.max_hp {
            return Err(invariant_error(&format!("monsters[{index}].max_hp")));
        }
        if baseline.move_state != sampled.move_state {
            return Err(invariant_error(&format!("monsters[{index}].move_state")));
        }
    }

    validate_non_shuffle_rng_fields(&baseline.combat.rng.pool, &sampled.combat.rng.pool)
}

fn validate_non_shuffle_rng_fields(baseline: &RngPool, sampled: &RngPool) -> Result<(), String> {
    let fields = non_shuffle_rng_fields(baseline)
        .into_iter()
        .zip(non_shuffle_rng_fields(sampled));
    for ((name, baseline), (_, sampled)) in fields {
        if baseline != sampled {
            return Err(invariant_error(&format!("rng.{name}")));
        }
    }
    Ok(())
}

fn non_shuffle_rng_fields(pool: &RngPool) -> Vec<(&'static str, &StsRng)> {
    vec![
        ("monster_rng", &pool.monster_rng),
        ("event_rng", &pool.event_rng),
        ("merchant_rng", &pool.merchant_rng),
        ("card_rng", &pool.card_rng),
        ("treasure_rng", &pool.treasure_rng),
        ("relic_rng", &pool.relic_rng),
        ("potion_rng", &pool.potion_rng),
        ("monster_hp_rng", &pool.monster_hp_rng),
        ("ai_rng", &pool.ai_rng),
        ("card_random_rng", &pool.card_random_rng),
        ("misc_rng", &pool.misc_rng),
        ("math_rng", &pool.math_rng),
    ]
}

fn non_shuffle_rng_hash(pool: &RngPool) -> Result<String, String> {
    hash_serializable(&non_shuffle_rng_fields(pool))
}

fn monster_snapshot_hash(position: &CombatPosition) -> Result<String, String> {
    let snapshot = position
        .combat
        .entities
        .monsters
        .iter()
        .map(|monster| {
            (
                monster.id,
                monster.enemy_id,
                monster.slot,
                monster.current_hp,
                monster.max_hp,
                &monster.move_state,
            )
        })
        .collect::<Vec<_>>();
    hash_serializable(&snapshot)
}

fn hash_serializable<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to serialize combat laboratory fingerprint: {error}"))?;
    let mut hasher = Blake2b512::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    Ok(digest[..32]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn invariant_error(field: &str) -> String {
    format!("combat laboratory sample invariant mismatch at field '{field}'")
}
