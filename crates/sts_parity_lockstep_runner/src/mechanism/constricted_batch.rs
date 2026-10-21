use super::{
    batch::{create_case, file_hash, observe, prepare_action, put, read, BatchManifest},
    replay_constricted_mechanism_v1, CONSTRICTED_MECHANISM_SCOPE_V1,
};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};
use sts_parity_lockstep::{ComparisonStatusV1, LockstepModeV1};
use sts_parity_lockstep_java::{
    project_monster_ai_mechanism_v1, project_player_mechanism_v1, OwnedJavaLockstepTransportV1,
    RecordedJavaActionV1, RecordedJavaBoundaryV1,
};

const SCHEMA: &str = "sts-constricted-low-block-mechanism-batch-v1";
const BLOCKS: [i32; 2] = [0, 5];
const PHASES: [&str; 2] = ["apply", "tick"];

/// Caller owns the isolated JVM. First failure retains the current case and
/// returns without resetting it or advancing the analyst-authored matrix.
pub fn record_constricted_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    mut progress: impl FnMut(&str),
) -> Result<(), String> {
    let initial = observe(oracle)?;
    if initial.frame["profile"] != "parity_scenario_control" {
        return Err("Constricted collection requires ScenarioControl".into());
    }
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    let mut manifest = BatchManifest {
        schema: SCHEMA.into(),
        scope: CONSTRICTED_MECHANISM_SCOPE_V1.into(),
        producer: initial.producer,
        collector_executable_sha256: file_hash(
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )?,
        files: BTreeMap::new(),
    };
    put(&destination.join("capture.json"), &manifest)?;
    let launch: serde_json::Value = read(oracle.runtime().launch_plan())?;
    put(&destination.join("launch-plan.json"), &launch)?;
    for block in BLOCKS {
        let name = format!("block-{block}");
        let directory = destination.join(&name);
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let result = (|| {
            let draft = json!({
                "schema":"sts_java_play_scenario_draft_v3", "provenance":"constructed_validated", "case_id":format!("constricted-{name}"),
                "run":{"character":"IRONCLAD","ascension":20,"seed":"20260907","content_profile":"all_unlocked"},
                "recipe":{"recipe_id":"fresh_neow_to_combat_root.v1","capabilities":[
                    {"capability_id":"combat.encounter.exact.v1","payload":{"encounter_id":"Spire Growth"}},
                    {"capability_id":"player.health.v1","payload":{"current_hp":80,"max_hp":80}},
                    {"capability_id":"player.master_deck.exact.v1","payload":{"cards":[
                        {"card_id":"Dark Shackles","upgrades":1},{"card_id":"PiercingWail","upgrades":1},
                        {"card_id":"Disarm","upgrades":1},{"card_id":"Berserk","upgrades":1},{"card_id":"Defend_R","upgrades":0}
                    ]}},
                    {"capability_id":"player.relics.fresh_exact.v1","payload":{"relics":[{"relic_id":"Runic Pyramid"}]}}
                ]}
            });
            create_case(oracle, &directory, &draft, block != 0, &manifest.producer)?;
            record_step(oracle, &directory, "apply", block)?;
            progress(&format!("{name} apply | Exact | no premature HP loss"));
            let mut ordinal = 0;
            for card in ["Dark Shackles", "PiercingWail", "Disarm", "Berserk"] {
                prepare_action(oracle, &directory, &mut ordinal, &[card])?;
            }
            if block != 0 {
                prepare_action(oracle, &directory, &mut ordinal, &["Defend_R"])?;
            }
            record_step(oracle, &directory, "tick", block)?;
            Ok::<(), String>(())
        })();
        if let Err(reason) = result {
            put(
                &destination.join("stopped.json"),
                &json!({"case":name,"reason":reason,"scope":CONSTRICTED_MECHANISM_SCOPE_V1}),
            )?;
            return Err(format!("{name}: {reason}"));
        }
        for phase in PHASES {
            for kind in ["before", "action", "after"] {
                let relative = format!("{name}/{phase}-{kind}.json");
                manifest
                    .files
                    .insert(relative.clone(), file_hash(&destination.join(relative))?);
            }
        }
        progress(&format!("{name} tick | Exact | {} HP loss", 12 - block));
    }
    put(&destination.join("complete.json"), &manifest)?;
    Ok(())
}

pub fn replay_constricted_batch_v1(directory: &Path) -> Result<usize, String> {
    let manifest: BatchManifest = read(&directory.join("complete.json"))?;
    if manifest.schema != SCHEMA
        || manifest.scope != CONSTRICTED_MECHANISM_SCOPE_V1
        || manifest.files.len() != 12
        || manifest.collector_executable_sha256.len() != 64
        || !manifest
            .collector_executable_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err("incomplete or unsupported Constricted batch".into());
    }
    for block in BLOCKS {
        for phase in PHASES {
            let name = format!("block-{block}/{phase}");
            for kind in ["before", "action", "after"] {
                let relative = format!("{name}-{kind}.json");
                if manifest.files.get(&relative) != Some(&file_hash(&directory.join(&relative))?) {
                    return Err(format!("{relative}: raw bytes changed or absent"));
                }
            }
            let before: RecordedJavaBoundaryV1 =
                read(&directory.join(format!("{name}-before.json")))?;
            let action: RecordedJavaActionV1 =
                read(&directory.join(format!("{name}-action.json")))?;
            let after: RecordedJavaBoundaryV1 =
                read(&directory.join(format!("{name}-after.json")))?;
            if before.observation.producer != manifest.producer
                || after.observation.producer != manifest.producer
            {
                return Err("Constricted batch producer changed".into());
            }
            validate_input(&before, phase, block)?;
            let report = replay_constricted_mechanism_v1(&before, &action, &after)?;
            if report.status != ComparisonStatusV1::Exact {
                return Err(report
                    .first_stop(LockstepModeV1::Audit)
                    .unwrap_or_else(|| "Constricted not Exact".into()));
            }
        }
    }
    Ok(4)
}

fn record_step(
    oracle: &mut OwnedJavaLockstepTransportV1,
    directory: &Path,
    phase: &str,
    block: i32,
) -> Result<(), String> {
    super::batch::record_end_turn(
        oracle,
        directory,
        &format!("{phase}-"),
        |before| validate_input(before, phase, block),
        replay_constricted_mechanism_v1,
    )
}

fn validate_input(before: &RecordedJavaBoundaryV1, phase: &str, block: i32) -> Result<(), String> {
    let raw = &before.observation;
    let (player, powers) = project_player_mechanism_v1(raw)?;
    let (monsters, _) = project_monster_ai_mechanism_v1(raw)?;
    let [monster] = monsters.as_slice() else {
        return Err("Constricted requires one monster".into());
    };
    let amount = |id: &str| powers.iter().find(|p| p.power_id == id).map(|p| p.amount);
    let enemy_amount = |id: &str| {
        monster
            .powers
            .iter()
            .find(|p| p.power_id == id)
            .map(|p| p.amount)
    };
    let valid = raw.frame["scenario"]["provenance"] == "constructed_validated"
        && raw.frame["public_base"]["run"]["ascension"] == 20
        && player.current_hp == 80
        && player.max_hp == 80
        && monster.enemy_id == "spiregrowth"
        && if phase == "apply" {
            player.block == 0
                && powers.is_empty()
                && monster.next_move_id == 2
                && monster.powers.is_empty()
        } else {
            player.block == block
                && powers.len() == 3
                && amount("constricted") == Some(12)
                && amount("vulnerable") == Some(1)
                && amount("berserk") == Some(1)
                && monster.powers.len() == 2
                && enemy_amount("strength") == Some(-26)
                && enemy_amount("shackled") == Some(23)
                && [1, 3].contains(&monster.next_move_id)
        };
    if !valid {
        return Err(format!(
            "{phase}: preparation did not reach the declared low-block recipe"
        ));
    }
    Ok(())
}
