use super::{
    batch::{
        create_case, file_hash, observe, prepare_action, prepare_action_with_upgrade, put, read,
        record_end_turn, BatchManifest,
    },
    regret::{validate_input, CombustSetup, Preparation, RegretHand},
    replay_regret_mechanism_v1, COMBUST_MECHANISM_SCOPE_V1, REGRET_MECHANISM_SCOPE_V1,
    REGRET_ORDER_MECHANISM_SCOPE_V1, RUPTURE_MECHANISM_SCOPE_V1,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};
use sts_oracle_run_control::content::cards::CardId;
use sts_parity_lockstep::{ComparisonStatusV1, LockstepModeV1};
use sts_parity_lockstep_java::{
    OwnedJavaLockstepTransportV1, RecordedJavaActionV1, RecordedJavaBoundaryV1,
};

// Closed mechanism inputs share collection, replay and evidence checks.
struct Case {
    name: &'static str,
    preparation: Preparation,
    hand: RegretHand,
    block: i32,
    buffer: bool,
    tungsten: bool,
    swap_master_ingredients: bool,
}
struct Batch {
    schema: &'static str,
    scope: &'static str,
    cases: &'static [Case],
    require_mixed_order_pair: bool,
}
const BLOCK_BATCH: Batch = Batch {
    schema: "sts-regret-block-mechanism-batch-v1",
    scope: REGRET_MECHANISM_SCOPE_V1,
    cases: &[
        Case::padded("plain", false, false),
        Case::padded("buffer", true, false),
        Case::padded("tungsten", false, true),
        Case::padded("buffer-tungsten", true, true),
    ],
    require_mixed_order_pair: false,
};
const ORDER_BATCH: Batch = Batch {
    schema: "sts-regret-damage-order-mechanism-batch-v1",
    scope: REGRET_ORDER_MECHANISM_SCOPE_V1,
    cases: &[
        Case {
            name: "one-tungsten",
            preparation: Preparation::Barricade,
            hand: RegretHand::Alone,
            block: 5,
            buffer: false,
            tungsten: true,
            swap_master_ingredients: false,
        },
        Case {
            name: "one-buffer-tungsten",
            preparation: Preparation::Barricade,
            hand: RegretHand::Alone,
            block: 5,
            buffer: true,
            tungsten: true,
            swap_master_ingredients: false,
        },
        Case {
            name: "mixed-block",
            preparation: Preparation::Barricade,
            hand: RegretHand::Mixed,
            block: 5,
            buffer: false,
            tungsten: false,
            swap_master_ingredients: false,
        },
        Case {
            name: "mixed-buffer-a",
            preparation: Preparation::Barricade,
            hand: RegretHand::Mixed,
            block: 0,
            buffer: true,
            tungsten: false,
            swap_master_ingredients: false,
        },
        Case {
            name: "mixed-buffer-b",
            preparation: Preparation::Barricade,
            hand: RegretHand::Mixed,
            block: 0,
            buffer: true,
            tungsten: false,
            swap_master_ingredients: true,
        },
    ],
    require_mixed_order_pair: true,
};
const RUPTURE_BATCH: Batch = Batch {
    schema: "sts-rupture-hp-loss-mechanism-batch-v1",
    scope: RUPTURE_MECHANISM_SCOPE_V1,
    cases: &[
        Case::rupture("regret-one", RegretHand::Alone, false, false),
        Case::rupture("regret-three", RegretHand::Padded, false, false),
        Case::rupture("regret-buffer", RegretHand::Alone, true, false),
        Case::rupture("regret-tungsten", RegretHand::Alone, false, true),
        Case::rupture("regret-buffer-tungsten", RegretHand::Alone, true, true),
        Case::rupture("brutality-one", RegretHand::Empty, false, false),
        Case::rupture("brutality-buffer", RegretHand::Empty, true, false),
        Case::rupture("brutality-tungsten", RegretHand::Empty, false, true),
    ],
    require_mixed_order_pair: false,
};
const COMBUST_BATCH: Batch = Batch {
    schema: "sts-combust-rupture-mechanism-batch-v1",
    scope: COMBUST_MECHANISM_SCOPE_V1,
    cases: &[
        Case::combust(
            "single",
            CombustSetup::Base,
            RegretHand::Empty,
            false,
            false,
        ),
        Case::combust(
            "upgraded",
            CombustSetup::Upgraded,
            RegretHand::Empty,
            false,
            false,
        ),
        Case::combust(
            "stacked",
            CombustSetup::Stacked,
            RegretHand::Empty,
            false,
            false,
        ),
        Case::combust(
            "stacked-tungsten",
            CombustSetup::Stacked,
            RegretHand::Empty,
            false,
            true,
        ),
        Case::combust(
            "stacked-buffer",
            CombustSetup::Stacked,
            RegretHand::Empty,
            true,
            false,
        ),
        Case::combust(
            "regret",
            CombustSetup::Base,
            RegretHand::Alone,
            false,
            false,
        ),
        Case::combust(
            "regret-buffer",
            CombustSetup::Base,
            RegretHand::Alone,
            true,
            false,
        ),
        Case::combust(
            "regret-tungsten",
            CombustSetup::Base,
            RegretHand::Alone,
            false,
            true,
        ),
        Case::combust(
            "regret-buffer-tungsten",
            CombustSetup::Base,
            RegretHand::Alone,
            true,
            true,
        ),
        Case::combust(
            "regret-three-buffer",
            CombustSetup::Base,
            RegretHand::Padded,
            true,
            false,
        ),
    ],
    require_mixed_order_pair: false,
};
impl Case {
    const fn combust(
        name: &'static str,
        setup: CombustSetup,
        hand: RegretHand,
        buffer: bool,
        tungsten: bool,
    ) -> Self {
        Self {
            name,
            preparation: Preparation::RuptureCombust(setup),
            hand,
            block: 0,
            buffer,
            tungsten,
            swap_master_ingredients: false,
        }
    }
    const fn rupture(name: &'static str, hand: RegretHand, buffer: bool, tungsten: bool) -> Self {
        Self {
            name,
            preparation: if matches!(hand, RegretHand::Empty) {
                Preparation::RuptureBrutality
            } else {
                Preparation::Rupture
            },
            hand,
            block: 0,
            buffer,
            tungsten,
            swap_master_ingredients: false,
        }
    }
    const fn padded(name: &'static str, buffer: bool, tungsten: bool) -> Self {
        Self {
            name,
            preparation: Preparation::Barricade,
            hand: RegretHand::Padded,
            block: 5,
            buffer,
            tungsten,
            swap_master_ingredients: false,
        }
    }
    fn draft(&self) -> Value {
        let mut cards = Vec::new();
        if self.hand != RegretHand::Empty {
            cards.push(json!({"card_id":if self.swap_master_ingredients { "Burn" } else { "Regret" },"upgrades":0}));
        }
        cards.push(json!({"card_id":if self.preparation == Preparation::Barricade { "Barricade" } else { "Rupture" },"upgrades":1}));
        if self.preparation == Preparation::RuptureBrutality {
            cards.push(json!({"card_id":"Brutality","upgrades":0}));
        }
        if let Preparation::RuptureCombust(setup) = self.preparation {
            for upgrades in setup.upgrades() {
                cards.push(json!({"card_id":"Combust","upgrades":upgrades}));
            }
        }
        if self.block == 5 {
            cards.push(json!({"card_id":"Defend_R","upgrades":0}));
        }
        match self.hand {
            RegretHand::Empty | RegretHand::Alone => {},
            RegretHand::Padded => cards.extend([json!({"card_id":"Wound","upgrades":0}),json!({"card_id":"Wound","upgrades":0})]),
            RegretHand::Mixed => cards.extend([
                json!({"card_id":if self.swap_master_ingredients { "Regret" } else { "Burn" },"upgrades":0}),
                json!({"card_id":"Wound","upgrades":0}),
            ]),
        }
        let mut relics = Vec::new();
        if self.buffer {
            relics.push(json!({"relic_id":"FossilizedHelix"}));
        }
        if self.tungsten {
            relics.push(json!({"relic_id":"TungstenRod"}));
        }
        json!({
            "schema":"sts_java_play_scenario_draft_v3", "provenance":"constructed_validated", "case_id":format!("regret-{}",self.name),
            "run":{"character":"IRONCLAD","ascension":20,"seed":"20260907","content_profile":"all_unlocked"},
            "recipe":{"recipe_id":"fresh_neow_to_combat_root.v1","capabilities":[
                {"capability_id":"combat.encounter.exact.v1","payload":{"encounter_id":"Spire Growth"}},
                {"capability_id":"player.health.v1","payload":{"current_hp":80,"max_hp":80}},
                {"capability_id":"player.master_deck.exact.v1","payload":{"cards":cards}},
                {"capability_id":"player.relics.fresh_exact.v1","payload":{"relics":relics}}
            ]}
        })
    }
    fn validate(&self, before: &RecordedJavaBoundaryV1) -> Result<(), String> {
        let input = validate_input(before)?;
        if input.shape != self.hand
            || input.preparation != self.preparation
            || input.block != self.block
            || input.buffer != self.buffer
            || input.tungsten != self.tungsten
        {
            return Err("Regret batch input branch changed".into());
        }
        Ok(())
    }
}

pub fn record_regret_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    progress: impl FnMut(&str),
) -> Result<(), String> {
    record_batch(&BLOCK_BATCH, oracle, destination, progress)
}
pub fn replay_regret_batch_v1(directory: &Path) -> Result<usize, String> {
    replay_batch(&BLOCK_BATCH, directory)
}
pub fn record_regret_order_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    progress: impl FnMut(&str),
) -> Result<(), String> {
    record_batch(&ORDER_BATCH, oracle, destination, progress)
}
pub fn replay_regret_order_batch_v1(directory: &Path) -> Result<usize, String> {
    replay_batch(&ORDER_BATCH, directory)
}
pub fn record_rupture_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    progress: impl FnMut(&str),
) -> Result<(), String> {
    record_batch(&RUPTURE_BATCH, oracle, destination, progress)
}
pub fn replay_rupture_batch_v1(directory: &Path) -> Result<usize, String> {
    replay_batch(&RUPTURE_BATCH, directory)
}
pub fn record_combust_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    progress: impl FnMut(&str),
) -> Result<(), String> {
    record_batch(&COMBUST_BATCH, oracle, destination, progress)
}
pub fn replay_combust_batch_v1(directory: &Path) -> Result<usize, String> {
    replay_batch(&COMBUST_BATCH, directory)
}

/// Analyst-authored inputs in one owned JVM. Stops at first failure,
/// retains all raw evidence, seals only after every comparison is Exact.
fn record_batch(
    batch: &Batch,
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    mut progress: impl FnMut(&str),
) -> Result<(), String> {
    let initial = observe(oracle)?;
    if initial.frame["profile"] != "parity_scenario_control" {
        return Err("Regret requires ScenarioControl".into());
    }
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    let mut manifest = BatchManifest {
        schema: batch.schema.into(),
        scope: batch.scope.into(),
        producer: initial.producer,
        collector_executable_sha256: file_hash(
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )?,
        files: BTreeMap::new(),
    };
    put(&destination.join("capture.json"), &manifest)?;
    let launch: serde_json::Value = read(oracle.runtime().launch_plan())?;
    put(&destination.join("launch-plan.json"), &launch)?;
    for (index, case) in batch.cases.iter().enumerate() {
        let name = case.name;
        let directory = destination.join(name);
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let result = (|| {
            create_case(
                oracle,
                &directory,
                &case.draft(),
                index > 0,
                &manifest.producer,
            )?;
            let mut ordinal = 0;
            let power_card = if case.preparation == Preparation::Barricade {
                "Barricade"
            } else {
                "Rupture"
            };
            prepare_action(oracle, &directory, &mut ordinal, &[power_card])?;
            if case.preparation == Preparation::RuptureBrutality {
                prepare_action(oracle, &directory, &mut ordinal, &["Brutality"])?;
            }
            if let Preparation::RuptureCombust(setup) = case.preparation {
                for &upgrade in setup.upgrades() {
                    prepare_action_with_upgrade(
                        oracle,
                        &directory,
                        &mut ordinal,
                        &["Combust"],
                        Some(upgrade),
                    )?;
                }
            }
            if case.block == 5 {
                prepare_action(oracle, &directory, &mut ordinal, &["Defend_R"])?;
            }
            record_end_turn(
                oracle,
                &directory,
                "",
                |before| case.validate(before),
                replay_regret_mechanism_v1,
            )
        })();
        if let Err(reason) = result {
            put(
                &destination.join("stopped.json"),
                &json!({"case":name,"reason":reason,"scope":batch.scope}),
            )?;
            return Err(format!("{name}: {reason}"));
        }
        for kind in ["before", "action", "after"] {
            let relative = format!("{name}/{kind}.json");
            manifest
                .files
                .insert(relative.clone(), file_hash(&destination.join(relative))?);
        }
        progress(&format!("{name} | Exact | declared EndTurn scope"));
    }
    if let Err(reason) = validate_coverage(batch, destination) {
        put(
            &destination.join("stopped.json"),
            &json!({"case":"coverage","reason":reason,"scope":batch.scope}),
        )?;
        return Err(reason);
    }
    put(&destination.join("complete.json"), &manifest)?;
    Ok(())
}

fn replay_batch(batch: &Batch, directory: &Path) -> Result<usize, String> {
    let manifest: BatchManifest = read(&directory.join("complete.json"))?;
    if manifest.schema != batch.schema
        || manifest.scope != batch.scope
        || manifest.files.len() != batch.cases.len() * 3
        || manifest.collector_executable_sha256.len() != 64
        || !manifest
            .collector_executable_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err("incomplete or unsupported Regret batch".into());
    }
    for recipe in batch.cases {
        let name = recipe.name;
        for kind in ["before", "action", "after"] {
            let relative = format!("{name}/{kind}.json");
            if manifest.files.get(&relative) != Some(&file_hash(&directory.join(&relative))?) {
                return Err(format!("{relative}: raw bytes changed or absent"));
            }
        }
        let case = directory.join(name);
        let before: RecordedJavaBoundaryV1 = read(&case.join("before.json"))?;
        let action: RecordedJavaActionV1 = read(&case.join("action.json"))?;
        let after: RecordedJavaBoundaryV1 = read(&case.join("after.json"))?;
        if before.observation.producer != manifest.producer
            || after.observation.producer != manifest.producer
        {
            return Err("Regret batch producer changed".into());
        }
        recipe.validate(&before)?;
        let report = replay_regret_mechanism_v1(&before, &action, &after)?;
        if report.status != ComparisonStatusV1::Exact {
            return Err(report
                .first_stop(LockstepModeV1::Audit)
                .unwrap_or_else(|| "Regret not Exact".into()));
        }
    }
    validate_coverage(batch, directory)?;
    Ok(batch.cases.len())
}

fn validate_coverage(batch: &Batch, directory: &Path) -> Result<(), String> {
    if batch.require_mixed_order_pair {
        let order = |case: &str| -> Result<bool, String> {
            let before = read(&directory.join(case).join("before.json"))?;
            let hand = validate_input(&before)?.hand;
            let burn = hand
                .iter()
                .position(|id| *id == CardId::Burn)
                .ok_or("Burn missing from order pair")?;
            let regret = hand
                .iter()
                .position(|id| *id == CardId::Regret)
                .ok_or("Regret missing from order pair")?;
            Ok(burn < regret)
        };
        if order("mixed-buffer-a")? == order("mixed-buffer-b")? {
            return Err("Regret matrix did not observe both Burn/Regret hand orders".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regret_order_batch_requires_sealing_and_both_observed_hand_orders() {
        let directory = std::env::temp_dir().join(format!(
            "regret-order-envelope-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        let result = (|| -> Result<(), String> {
            let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../sts_parity_lockstep_java/tests/fixtures/regret_order");
            let sample: RecordedJavaBoundaryV1 = read(&fixtures.join("one-tungsten-before.json"))?;
            let mut manifest = BatchManifest {
                schema: ORDER_BATCH.schema.into(),
                scope: ORDER_BATCH.scope.into(),
                producer: sample.observation.producer,
                collector_executable_sha256: "0".repeat(64),
                files: BTreeMap::new(),
            };
            // Synthetic test envelope over immutable oracle bytes; no live
            // collection or new provenance is claimed by this temporary batch.
            for case in ORDER_BATCH.cases {
                fs::create_dir(directory.join(case.name)).map_err(|e| e.to_string())?;
                for kind in ["before", "action", "after"] {
                    let relative = format!("{}/{kind}.json", case.name);
                    let target = directory.join(&relative);
                    fs::copy(fixtures.join(format!("{}-{kind}.json", case.name)), &target)
                        .map_err(|e| e.to_string())?;
                    manifest.files.insert(relative, file_hash(&target)?);
                }
            }
            assert!(replay_regret_order_batch_v1(&directory).is_err());
            put(&directory.join("complete.json"), &manifest)?;
            assert_eq!(replay_regret_order_batch_v1(&directory)?, 5);
            // A complete, individually Exact matrix must still fail if its
            // supposed order contrast repeats the same observed hand order.
            for kind in ["before", "action", "after"] {
                let relative = format!("mixed-buffer-b/{kind}.json");
                let target = directory.join(&relative);
                fs::copy(
                    fixtures.join(format!("mixed-buffer-a-{kind}.json")),
                    &target,
                )
                .map_err(|e| e.to_string())?;
                manifest.files.insert(relative, file_hash(&target)?);
            }
            fs::write(
                directory.join("complete.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .map_err(|e| e.to_string())?;
            assert!(replay_regret_order_batch_v1(&directory)
                .unwrap_err()
                .contains("both Burn/Regret hand orders"));
            Ok(())
        })();
        // Only this test's unique temporary directory is removed.
        fs::remove_dir_all(&directory).unwrap();
        result.unwrap();
    }

    #[test]
    fn regret_recipes_compile_before_oracle_launch() {
        let directory =
            std::env::temp_dir().join(format!("regret-recipe-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let result = (|| -> Result<(), String> {
            for batch in [&BLOCK_BATCH, &ORDER_BATCH, &RUPTURE_BATCH, &COMBUST_BATCH] {
                let scope = directory.join(batch.schema);
                fs::create_dir(&scope).map_err(|e| e.to_string())?;
                for case in batch.cases {
                    let draft = scope.join(format!("{}.json", case.name));
                    put(&draft, &case.draft())?;
                    sts_java_parity_live::compile_scenario_plan(
                        &draft,
                        &scope.join(format!("{}-plan.json", case.name)),
                    )?;
                }
            }
            Ok(())
        })();
        // Only this test's unique temporary directory is removed.
        fs::remove_dir_all(&directory).unwrap();
        result.unwrap();
    }
}
