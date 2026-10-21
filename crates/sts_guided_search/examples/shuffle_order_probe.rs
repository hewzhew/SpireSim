//! Offline fixed-shuffle experiment over solver or run-play combat replays.
//! Proposals change play order, never edit the combat piles. This is Oracle
//! research: reading shuffle_rng is not knowledge obtained by a Native-SL player.
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use clap::Parser;
use serde_json::{json, Value};
use sts_combat_outcome::combat_witness_materialization::materialize_replay_checked_combat_outcome_v1;
use sts_core::{sim::combat::CombatPosition, state::core::ClientInput};
use sts_guided_search::shuffle_order::*;
use sts_guided_search::solver::{CombatSolver, SolverConfig};

#[derive(Parser)]
struct Args {
    /// Solver artifacts or run-play exact-prefix artifacts with combat_replay.
    #[arg(long, required = true, num_args = 1..)]
    input: Vec<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    /// Equal continuation work per verified alternative, including baseline.
    #[arg(long, default_value_t = 10000)]
    work: usize,
    /// Maximum consecutive append-only plays in a tested block.
    #[arg(long, default_value_t = 6, value_parser = clap::value_parser!(u8).range(2..=6))]
    max_block: u8,
    /// Also evaluate every permutation as a small-block reference. Normally
    /// only mapped proposals are evaluated; there is no factorial expansion.
    #[arg(long)]
    exhaustive_reference: bool,
}

fn continuation(root: &CombatPosition, work: usize) -> Result<Value, String> {
    let start = Instant::now();
    let mut solver = CombatSolver::new(
        root.clone(),
        SolverConfig {
            max_potion_uses: Some(0),
            shuffle_continuation_work: 0,
            ..Default::default()
        },
    )?;
    let stop = solver.advance(work, Duration::from_secs(10))?;
    let best = solver.plans().iter().max_by_key(|p| p.outcome.final_hp);
    Ok(
        json!({"stop":stop,"stats":solver.stats(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"best_hp":best.map(|p|p.outcome.final_hp),
        "hp_selected_witness":best.map(|p|json!({"source":"continuation_search", "inputs":p.inputs,
            "outcome":p.outcome,"successor":p.successor}))}),
    )
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let elapsed = Instant::now();
    if args.output.exists() {
        return Err("output already exists".into());
    }
    let mut cases = Vec::new();
    for path in &args.input {
        let case_elapsed = Instant::now();
        let artifact: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let payload = artifact.get("combat_replay").unwrap_or(&artifact);
        let root: CombatPosition =
            serde_json::from_value(payload["root"].clone()).map_err(|e| e.to_string())?;
        let plans = payload["plans"].as_array().ok_or("missing plans")?;
        let Some(plan) = plans
            .iter()
            .max_by_key(|p| p["successor"]["combat"]["entities"]["player"]["current_hp"].as_i64())
        else {
            cases.push(json!({"source":path,"skipped":"no supplied win"}));
            continue;
        };
        let inputs: Vec<ClientInput> =
            serde_json::from_value(plan["inputs"].clone()).map_err(|e| e.to_string())?;
        let states = replay(&root, &inputs)?;
        let expected: CombatPosition =
            serde_json::from_value(plan["successor"].clone()).map_err(|e| e.to_string())?;
        if states.last() != Some(&expected) {
            return Err("source witness does not replay exactly".into());
        }
        let original_outcome =
            materialize_replay_checked_combat_outcome_v1(&root, &inputs, &expected)?;
        let mut windows = Vec::new();
        let mut skipped = Vec::new();
        for start in 0..inputs.len() {
            if start > 0
                && discard_play(&states[start - 1], &inputs[start - 1], &states[start]).is_some()
            {
                continue;
            }
            let mut block_end = start;
            let mut plays = Vec::new();
            while block_end < inputs.len() {
                let Some(play) = discard_play(
                    &states[block_end],
                    &inputs[block_end],
                    &states[block_end + 1],
                ) else {
                    break;
                };
                plays.push(play);
                block_end += 1;
            }
            if plays.len() < 2 {
                continue;
            }
            if plays.len() > args.max_block as usize {
                skipped.push(json!({"start":start,"actions":plays.len(),"reason":"block exceeds configured max_block"}));
                continue;
            }
            let Some(end) = (block_end..inputs.len()).find(|&i| {
                states[i].combat.rng.shuffle_rng != states[i + 1].combat.rng.shuffle_rng
            }) else {
                skipped.push(
                    json!({"start":start,"reason":"no later shuffle in supplied trajectory"}),
                );
                continue;
            };
            let map = match mapping(&states[end], &inputs[end], &states[end + 1]) {
                Ok(map) => map,
                Err(reason) => {
                    skipped.push(json!({"start":start,"shuffle_action":end,"reason":reason}));
                    continue;
                }
            };
            let offset = states[start].combat.zones.discard_pile.len();
            let appended: Vec<_> = plays
                .iter()
                .flat_map(|p| p.discarded.iter().copied())
                .collect();
            if map.input.get(offset..offset + appended.len()) != Some(appended.as_slice()) {
                skipped.push(json!({"start":start,"reason":"played append segment did not survive to shuffle input"}));
                continue;
            }
            let cuts = draw_cuts(&map, &states, &inputs, end);
            let widths: Vec<_> = plays.iter().map(|p| p.discarded.len()).collect();
            let proposed = mapped_orders(&widths, offset, &map, &cuts);
            let mut all = proposed.clone();
            if args.exhaustive_reference {
                permutations(&mut (0..plays.len()).collect::<Vec<_>>(), 0, &mut all);
            }
            let mut candidates = Vec::new();
            let mut cache: Vec<(CombatPosition, Value)> = Vec::new();
            for order in all {
                let candidate_elapsed = Instant::now();
                let proposed = proposed.contains(&order);
                let (after_block, mut changed_inputs) =
                    match play_order(&states[start], &plays, &order) {
                        Ok(x) => x,
                        Err(e) => {
                            candidates
                                .push(json!({"order":order,"proposed":proposed,"rejected":e}));
                            continue;
                        }
                    };
                let changed_segment: Vec<_> = after_block
                    .combat
                    .zones
                    .discard_pile
                    .iter()
                    .skip(offset)
                    .map(|c| c.lineage)
                    .collect();
                let (before, after, following) =
                    match follow_to_shuffle(after_block, &states, &inputs, block_end, end) {
                        Ok(x) => x,
                        Err(e) => {
                            candidates
                                .push(json!({"order":order,"proposed":proposed,"rejected":e}));
                            continue;
                        }
                    };
                let actual_map = match mapping(&before, following.last().unwrap(), &after) {
                    Ok(map) => map,
                    Err(reason) => {
                        candidates
                            .push(json!({"order":order,"proposed":proposed,"rejected":reason}));
                        continue;
                    }
                };
                let mut predicted_input = map.input.clone();
                predicted_input[offset..offset + appended.len()].copy_from_slice(&changed_segment);
                if actual_map.input != predicted_input
                    || actual_map.draw_to_input != map.draw_to_input
                    || actual_map.drawn != map.drawn
                {
                    candidates.push(json!({"order":order,"proposed":proposed,"rejected":"mapping no longer applies"}));
                    continue;
                }
                let draw_changes = changed_draw_batches(&map, &actual_map, &cuts);
                changed_inputs.extend(following);
                let mut prefix = inputs[..start].to_vec();
                prefix.extend(changed_inputs);
                let reused_continuation = cache.iter().any(|(p, _)| p == &after);
                let mut continuation =
                    if let Some((_, value)) = cache.iter().find(|(p, _)| p == &after) {
                        value.clone()
                    } else {
                        let value = continuation(&after, args.work)?;
                        cache.push((after.clone(), value.clone()));
                        value
                    };
                continuation["search_best_hp"] = continuation["best_hp"].clone();
                // Equal small search budgets can miss a known suffix even after
                // swapping identical cards. Attempt it afresh for every candidate,
                // and retain only the newly executed result, not the old outcome.
                match replay_supplied_suffix(&after, &inputs[end + 1..]) {
                    Ok((suffix, successor)) => {
                        let outcome = materialize_replay_checked_combat_outcome_v1(
                            &after, &suffix, &successor,
                        )?;
                        continuation["supplied_suffix_check"] = json!({"verified":true,"hp":outcome.final_hp,"inputs_used":suffix.len()});
                        if Some(outcome.final_hp as i64) > continuation["best_hp"].as_i64() {
                            continuation["best_hp"] = json!(outcome.final_hp);
                            continuation["hp_selected_witness"] = json!({"source":"replayed_supplied_inputs",
                                "inputs":suffix,"outcome":outcome,"successor":successor});
                        }
                    }
                    Err(reason) => {
                        continuation["supplied_suffix_check"] =
                            json!({"verified":false,"reason":reason});
                    }
                }
                // Independently check the complete adopted-prefix + new suffix.
                if !continuation["hp_selected_witness"].is_null() {
                    let suffix: Vec<ClientInput> = serde_json::from_value(
                        continuation["hp_selected_witness"]["inputs"].clone(),
                    )
                    .map_err(|e| e.to_string())?;
                    let final_state: CombatPosition = serde_json::from_value(
                        continuation["hp_selected_witness"]["successor"].clone(),
                    )
                    .map_err(|e| e.to_string())?;
                    let mut complete = prefix.clone();
                    complete.extend(suffix);
                    if replay(&root, &complete)?.last() != Some(&final_state) {
                        return Err("repaired complete witness failed replay".into());
                    }
                    continuation["hp_selected_witness"]["complete_outcome"] =
                        serde_json::to_value(materialize_replay_checked_combat_outcome_v1(
                            &root,
                            &complete,
                            &final_state,
                        )?)
                        .map_err(|e| e.to_string())?;
                    continuation["hp_selected_witness"]["complete_inputs"] = json!(complete);
                    continuation["hp_selected_witness"]["complete_replay_verified"] = json!(true);
                }
                candidates.push(json!({"order":order,"proposed":proposed,"prefix":prefix,
                    "draw_batch_changes":draw_changes,
                    "before_shuffle":{"hp":before.combat.entities.player.current_hp,"block":before.combat.entities.player.block,
                        "monsters":before.combat.entities.monsters},
                    "next_hp":after.combat.entities.player.current_hp,"next_hand":after.combat.zones.hand,
                    "elapsed_ms":candidate_elapsed.elapsed().as_secs_f64()*1000.0,
                    "reused_continuation":reused_continuation,
                    "continuation":continuation}));
            }
            let base: Vec<_> = (0..plays.len()).collect();
            let base_hp = candidates
                .iter()
                .find(|c| c["order"] == json!(base))
                .and_then(|c| c["continuation"]["best_hp"].as_i64());
            let mapped_hp = candidates
                .iter()
                .filter(|c| c["proposed"] == true)
                .filter_map(|c| c["continuation"]["best_hp"].as_i64())
                .max();
            let all_hp = candidates
                .iter()
                .filter_map(|c| c["continuation"]["best_hp"].as_i64())
                .max();
            let hp_label =
                |hp: Option<i64>| hp.map_or_else(|| "no witness".to_string(), |v| v.to_string());
            println!(
                "{} T{}: {} proposed / {} evaluated; HP baseline {}, proposed {}, reference {}",
                path.display(),
                states[start].combat.turn.turn_count + 1,
                proposed.len(),
                candidates.len(),
                hp_label(base_hp),
                hp_label(mapped_hp),
                if args.exhaustive_reference {
                    hp_label(all_hp)
                } else {
                    "off".into()
                }
            );
            windows.push(json!({"start":start,"block_end":block_end,"end":end,"turn":states[start].combat.turn.turn_count+1,
                "mapping":{"discard_input":map.input,"draw_to_input":map.draw_to_input,"drawn":map.drawn,"draw_cuts":cuts,"play_offset":offset},
                "shuffle_receipt":map.receipt,
                "pre_action_discard":states[end].combat.zones.discard_pile,
                "pre_action_hand":states[end].combat.zones.hand,
                "plays":plays.iter().map(|p|json!({"lineage":p.lineage,"discarded":p.discarded})).collect::<Vec<_>>(),
                "proposed_count":proposed.len(),"continuation_runs":cache.len(),"candidates":candidates}));
        }
        cases.push(
            json!({"source":path,"label":artifact["case"]["label"],"root":root,
            "original_inputs":inputs,"original_hp":original_outcome.final_hp,"original_outcome":original_outcome,
                "elapsed_ms":case_elapsed.elapsed().as_secs_f64()*1000.0,
                "skipped_blocks":skipped,"windows":windows}),
        );
    }
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    let result = json!({"information":"full_state_oracle","proposal":"whole-action append segments moved across observed baseline draw batches",
        "work_per_continuation":args.work,"max_block":args.max_block,
        "exhaustive_reference":args.exhaustive_reference,"continuation_potion_uses":0,
        "elapsed_ms":elapsed.elapsed().as_secs_f64()*1000.0,
        "git_head":String::from_utf8_lossy(&head.stdout).trim(),
        "producer_source_sha256":source_hash(),"shuffle_owner_sources":shuffle_owner_sources(),"cases":cases});
    fs::write(
        &args.output,
        serde_json::to_vec(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn source_hash() -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(include_bytes!("shuffle_order_probe.rs"))
    )
}

fn shuffle_owner_sources() -> Value {
    use sha2::{Digest, Sha256};
    let sources: [(&str, &[u8]); 6] = [
        (
            "crates/sts_guided_search/src/shuffle_order.rs",
            include_bytes!("../src/shuffle_order.rs"),
        ),
        (
            "src/runtime/combat/oracle_shuffle_observation.rs",
            include_bytes!("../../../src/runtime/combat/oracle_shuffle_observation.rs"),
        ),
        (
            "src/runtime/combat/combat_methods.rs",
            include_bytes!("../../../src/runtime/combat/combat_methods.rs"),
        ),
        (
            "src/runtime/combat/state.rs",
            include_bytes!("../../../src/runtime/combat/state.rs"),
        ),
        (
            "src/runtime/combat/mod.rs",
            include_bytes!("../../../src/runtime/combat/mod.rs"),
        ),
        (
            "src/sim/combat.rs",
            include_bytes!("../../../src/sim/combat.rs"),
        ),
    ];
    sources
        .into_iter()
        .map(|(path, bytes)| {
            (
                path.to_owned(),
                json!(format!("{:x}", Sha256::digest(bytes))),
            )
        })
        .collect()
}
