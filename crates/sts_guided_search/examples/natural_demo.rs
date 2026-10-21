//! Explicit assistant demonstrations. JSON-lines transport; no acting policy.
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, Write};
use std::path::Path;
use sts_guided_search::natural::{NaturalAction, NaturalEpisode, SegmentRecipe};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    seeds: Vec<u64>,
    max_actions: usize,
    actor: String,
    information: String,
    purpose: String,
    #[serde(default)]
    segment: Option<SegmentRecipe>,
}
impl Plan {
    fn contract(&self) -> &'static str {
        if self.segment.is_some() {
            "supplied_segment_trial_v1"
        } else {
            "natural_assisted_demo_v1"
        }
    }
    fn episode(&self, seed: u64) -> Result<NaturalEpisode, String> {
        match &self.segment {
            Some(recipe) => NaturalEpisode::from_segment(seed, self.max_actions, recipe),
            None => NaturalEpisode::new(seed, self.max_actions),
        }
    }
    fn validate(&self) -> Result<(), String> {
        let purpose = if self.segment.is_some() {
            "resource_consequence_development"
        } else {
            "demonstration_development"
        };
        if self.seeds.is_empty()
            || self.seeds.len() > 128
            || !(1..=8192).contains(&self.max_actions)
            || self.actor != "codex_assisted"
            || self.information != "oracle_observation_no_search_no_sl"
            || self.purpose != purpose
        {
            return Err("unsupported demonstration/segment plan".into());
        }
        let mut unique = self.seeds.clone();
        unique.sort_unstable();
        unique.dedup();
        if unique.len() != self.seeds.len() {
            return Err("duplicate demonstration seed".into());
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Observe {
        episode: usize,
    },
    Choose {
        episode: usize,
        source: u64,
        action: NaturalAction,
        reason: String,
    },
    Finish,
}
fn digest(v: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(v).unwrap()))
}
fn settlement_record(e: &NaturalEpisode, results_before: usize) -> Value {
    if e.results().len() == results_before {
        return Value::Null;
    }
    // Snapshots include f32 fields widened to JSON's f64 Number. Compare the
    // recorded wire representation to the same serialize/parse of the freshly
    // computed state; decimal parsing need not preserve the incidental f64 bits.
    // Independent before/after digests still check the exact serialized state.
    serde_json::from_slice(&serde_json::to_vec(&e.snapshot()).unwrap()).unwrap()
}
fn write_row(file: &mut File, v: &Value) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *file, v)?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}
fn frame(e: &NaturalEpisode) -> Result<Value, String> {
    let r = e.run();
    let resources = e.resources();
    Ok(
        json!({"view": e.view()?, "source": e.source(), "step": e.steps(), "stop": e.stop(),
        "floor": r.floor_num, "hp": resources.hp, "max_hp": resources.max_hp, "gold": resources.gold,
        "engine": e.engine(), "deck": r.master_deck, "relics": resources.relics,
        "potions": resources.potions, "map": r.map,
        "options": e.options()?, "combat": e.combat_observation()?, "results": e.results()}),
    )
}
fn binary_hash() -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        "{:x}",
        Sha256::digest(fs::read(std::env::current_exe()?)?)
    ))
}
fn verify(dir: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    if dir.join("execution-failure.json").exists() {
        return Err("cannot verify a failed collection".into());
    }
    let producer: Value = serde_json::from_slice(&fs::read(dir.join("producer.json"))?)?;
    if producer["binary_sha256"] != binary_hash()? {
        return Err("producer binary mismatch".into());
    }
    let plan: Plan = serde_json::from_value(producer["plan"].clone())?;
    plan.validate()?;
    if producer["contract"] != plan.contract() {
        return Err("plan and collection contract differ".into());
    }
    let mut episodes = plan
        .seeds
        .iter()
        .map(|s| plan.episode(*s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut count = 0;
    for line in std::io::BufReader::new(File::open(dir.join("trajectory.jsonl"))?).lines() {
        let row: Value = serde_json::from_str(&line?)?;
        let e = &mut episodes[row["episode"].as_u64().ok_or("episode missing")? as usize];
        if digest(&e.snapshot()) != row["before"] {
            return Err(format!("replay source mismatch at {count}").into());
        }
        let encoded = e.combat_entities()?;
        // The numerical contract is f32, whereas serde_json::Value stores f64.
        // Read back the declared type before exact comparison: decimal parsing
        // can otherwise introduce an irrelevant f64 rounding difference.
        let recorded: Option<sts_guided_search::entities::EntityPosition> =
            serde_json::from_value(row["entities"].clone())?;
        if encoded != recorded {
            return Err(format!("replay numerical input mismatch at {count}").into());
        }
        let options = serde_json::to_value(e.options()?)?;
        if options != row["options"] {
            return Err(format!("replay choices mismatch at {count}").into());
        }
        let a: NaturalAction = serde_json::from_value(row["action"].clone())?;
        let results_before = e.results().len();
        let input = e.choose(e.source(), &a)?;
        if serde_json::to_value(input)? != row["input"] || digest(&e.snapshot()) != row["after"] {
            return Err(format!("replay successor mismatch at {count}").into());
        }
        if plan.segment.is_some() {
            let settlement = settlement_record(e, results_before);
            if settlement != row["settlement"] {
                return Err(format!("replay settlement mismatch at {count}").into());
            }
        }
        count += 1;
    }
    Ok(json!({"replay_verified": true, "choices": count,
        "trajectory_sha256": format!("{:x}", Sha256::digest(fs::read(dir.join("trajectory.jsonl"))?)),
        "producer_sha256": format!("{:x}", Sha256::digest(fs::read(dir.join("producer.json"))?)),
        "episodes": episodes.iter().enumerate().map(|(i,e)| json!({
        "episode": i, "seed": plan.seeds[i], "stop": e.stop(), "steps": e.steps(),
        "floor": e.run().floor_num, "hp": e.run().current_hp, "potions": e.run().potions,
        "combats": e.results(), "incomplete": e.stop().is_none(),
        "settled_or_stopped_state": plan.segment.as_ref().map(|_| e.snapshot())
    })).collect::<Vec<_>>()}))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("verify") && args.len() == 2 {
        println!("{}", verify(Path::new(&args[1]))?);
        return Ok(());
    }
    if args.len() != 2 {
        return Err(
            "usage: natural_demo <plan.json> <new-output-directory> | verify <directory>".into(),
        );
    }
    let plan_json: Value = serde_json::from_slice(&fs::read(&args[0])?)?;
    let plan: Plan = serde_json::from_value(plan_json.clone())?;
    plan.validate()?;
    let dir = Path::new(&args[1]);
    fs::create_dir(dir)?;
    fs::write(
        dir.join("producer.json"),
        serde_json::to_vec_pretty(&json!({
            "contract": plan.contract(), "plan": plan_json, "entity_schema": sts_guided_search::entities::SCHEMA,
            "binary_sha256": binary_hash()?, "game": {"class":"Ironclad", "ascension":20, "heart":true,"neow":if plan.segment.is_some() {"skipped_by_declared_construction"} else {"rich"}},
            "search_transitions":0, "sl_branches":0,
            "labels":if plan.segment.is_some() {"none_resource_experiment"} else {"chosen_actions_only"}
        }))?,
    )?;
    let mut journal = OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(dir.join("trajectory.jsonl"))?;
    let mut requests = OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(dir.join("requests.jsonl"))?;
    let mut episodes = plan
        .seeds
        .iter()
        .map(|s| plan.episode(*s))
        .collect::<Result<Vec<_>, _>>()?;
    println!("{}", json!({"ready":episodes.len()}));
    for line in std::io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        write_row(&mut requests, &value)?;
        let request: Request = serde_json::from_value(value)?;
        let response: Result<Value, String> = match request {
            Request::Observe { episode } => episodes
                .get(episode)
                .ok_or("unknown episode".to_string())
                .and_then(frame),
            Request::Choose {
                episode,
                source,
                action,
                reason,
            } => (|| {
                let e = episodes.get_mut(episode).ok_or("unknown episode")?;
                if reason.trim().is_empty() {
                    return Err("demonstration reason missing".into());
                }
                let reading = frame(e)?;
                let before = digest(&e.snapshot());
                let entities = e.combat_entities()?;
                let results_before = e.results().len();
                let options = e.options()?;
                let index = options
                    .iter()
                    .position(|o| o.action == action)
                    .ok_or("absent demonstrated action")?;
                let input = match e.choose(source, &action) {
                    Ok(input) => input,
                    Err(error) => {
                        if e.stop().is_some() {
                            fs::write(dir.join("execution-failure.json"), serde_json::to_vec_pretty(&json!({
                                "episode":episode, "error":error, "reading":reading, "action":action, "state":e.snapshot()
                            })).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                        }
                        return Err(error);
                    }
                };
                let after = digest(&e.snapshot());
                let settlement = if plan.segment.is_some() && results_before != e.results().len() {
                    Some(e.snapshot())
                } else {
                    None
                };
                write_row(&mut journal, &json!({"episode":episode, "source":source, "before":before,
                    "entities":entities, "options":options, "selected":index, "action":action, "input":input,
                    "reading":reading, "reason":reason, "after":after, "stop":e.stop(), "settlement":settlement})).map_err(|e| e.to_string())?;
                frame(e)
            })(),
            Request::Finish => break,
        };
        println!(
            "{}",
            match response {
                Ok(v) => json!({"reading":v}),
                Err(e) => json!({"error":e}),
            }
        );
        std::io::stdout().flush()?;
    }
    if dir.join("execution-failure.json").exists() {
        return Err("collection has an execution failure; preserve the failed attempt".into());
    }
    let report = verify(dir)?;
    fs::write(
        dir.join("verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{cards::CardId, monsters::factory::EncounterId};
    use sts_core::state::{core::ClientInput, map::RoomType};
    use sts_guided_search::natural::{NaturalStop, SegmentRoom};
    use sts_guided_search::observation::ModelChoice;

    fn plan() -> Plan {
        Plan {
            seeds: vec![811],
            max_actions: 50,
            actor: "codex_assisted".into(),
            information: "oracle_observation_no_search_no_sl".into(),
            purpose: "resource_consequence_development".into(),
            segment: Some(SegmentRecipe {
                hp: 60,
                deck: vec![(CardId::SearingBlow, 10); 5],
                relics: vec![],
                potions: vec![],
                route: vec![
                    SegmentRoom {
                        encounter: EncounterId::JawWorm,
                        room: RoomType::MonsterRoom
                    };
                    2
                ],
            }),
        }
    }

    #[test]
    fn supplied_plan_cannot_claim_natural_demonstration_provenance() {
        let mut p = plan();
        p.validate().unwrap();
        assert_eq!(p.contract(), "supplied_segment_trial_v1");
        p.purpose = "demonstration_development".into();
        assert!(p.validate().is_err());
        p.segment = None;
        p.validate().unwrap();
        assert_eq!(p.contract(), "natural_assisted_demo_v1");
        p.seeds.push(811);
        assert!(p.validate().is_err());
    }

    #[test]
    fn settlement_wire_comparison_accepts_roundtrip_but_rejects_resource_tampering() {
        let p = plan();
        let mut e = p.episode(811).unwrap();
        for completed in 0..2 {
            let a = NaturalAction::Run(ClientInput::SelectMapNode(0));
            e.choose(e.source(), &a).unwrap();
            assert_eq!(settlement_record(&e, completed), Value::Null);
            let a = e
                .options()
                .unwrap()
                .into_iter()
                .find(|o| matches!(o.combat_choice, Some(ModelChoice::Play { .. })))
                .unwrap()
                .action;
            e.choose(e.source(), &a).unwrap();
            let mut recorded: Value =
                serde_json::from_slice(&serde_json::to_vec(&e.snapshot()).unwrap()).unwrap();
            assert_eq!(settlement_record(&e, completed), recorded);
            recorded["results"][completed]["exit_hp"] = json!(1);
            assert_ne!(settlement_record(&e, completed), recorded);
            if completed == 0 {
                e.choose(e.source(), &NaturalAction::Run(ClientInput::Proceed))
                    .unwrap();
            }
        }
        assert_eq!(e.stop(), Some(&NaturalStop::SegmentCompleted));
    }
}
