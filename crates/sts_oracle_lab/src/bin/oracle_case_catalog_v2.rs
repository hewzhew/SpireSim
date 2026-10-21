//! Append-only typed catalog for combat cases admitted to the V2 laboratory.
//!
//! Old case directories are never scanned implicitly. A case enters this
//! catalog only when a caller imports it or runs a V2 contract against it.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_case::load_combat_case;

use super::print_json;
pub(super) use sts_oracle_v2_cli::{CaseCommand, CaseCommandArgs};

const CASE_CATALOG_SCHEMA: &str = "OracleCombatCaseCatalogEntryV2";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct CombatCaseCatalogEntryV2 {
    schema_name: String,
    schema_version: u32,
    pub(super) id: String,
    pub(super) path: PathBuf,
    seed: u64,
    ascension: u8,
    act: u8,
    floor: i32,
    hp: i32,
    max_hp: i32,
    gold: i32,
    enemies: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CaseListResultV2 {
    schema_name: &'static str,
    schema_version: u32,
    matches: Vec<CombatCaseCatalogEntryV2>,
}

pub(super) fn run(args: CaseCommandArgs) -> Result<(), String> {
    match args.command {
        CaseCommand::Import(args) => print_json(&register_case(&args.case)?),
        CaseCommand::List(args) => {
            let enemy = args.enemy.as_deref().map(str::to_ascii_lowercase);
            let matches = read_catalog()?
                .into_values()
                .filter(|entry| args.seed.is_none_or(|seed| entry.seed == seed))
                .filter(|entry| args.act.is_none_or(|act| entry.act == act))
                .filter(|entry| args.floor.is_none_or(|floor| entry.floor == floor))
                .filter(|entry| {
                    enemy.as_ref().is_none_or(|needle| {
                        entry
                            .enemies
                            .iter()
                            .any(|enemy| enemy.to_ascii_lowercase().contains(needle))
                    })
                })
                .take(args.limit)
                .collect();
            print_json(&CaseListResultV2 {
                schema_name: "OracleCombatCaseListV2",
                schema_version: 2,
                matches,
            })
        }
    }
}

pub(super) fn register_case(path: &Path) -> Result<CombatCaseCatalogEntryV2, String> {
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        format!(
            "failed to resolve combat case '{}': {error}",
            path.display()
        )
    })?;
    let case = load_combat_case(&canonical_path)?;
    let id = combat_exact_state_hash_v2(&case.core.position.engine, &case.core.position.combat);
    let entry = CombatCaseCatalogEntryV2 {
        schema_name: CASE_CATALOG_SCHEMA.to_owned(),
        schema_version: 2,
        id: id.clone(),
        path: canonical_path,
        seed: case.core.source.seed,
        ascension: case.core.source.ascension,
        act: case.core.run.act,
        floor: case.core.run.floor,
        hp: case.core.run.hp,
        max_hp: case.core.run.max_hp,
        gold: case.core.run.gold,
        enemies: case.core.combat.enemies,
    };
    let existing = read_catalog()?;
    if existing.get(&id) == Some(&entry) {
        return Ok(entry);
    }

    let catalog_path = catalog_path();
    if let Some(parent) = catalog_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create V2 case catalog directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&catalog_path)
        .map_err(|error| {
            format!(
                "failed to open V2 case catalog '{}': {error}",
                catalog_path.display()
            )
        })?;
    serde_json::to_writer(&mut file, &entry)
        .map_err(|error| format!("failed to encode V2 case catalog entry: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("failed to finish V2 case catalog entry: {error}"))?;
    Ok(entry)
}

pub(super) fn resolve_case(
    path: Option<&Path>,
    id_prefix: Option<&str>,
) -> Result<CombatCaseCatalogEntryV2, String> {
    match (path, id_prefix) {
        (Some(path), None) => register_case(path),
        (None, Some(prefix)) => {
            let matches = read_catalog()?
                .into_values()
                .filter(|entry| entry.id.starts_with(prefix))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [] => Err(format!(
                    "V2 case catalog has no exact-root id beginning with '{prefix}'"
                )),
                [entry] => Ok(entry.clone()),
                _ => Err(format!(
                    "V2 case id prefix '{prefix}' is ambiguous across {} entries",
                    matches.len()
                )),
            }
        }
        _ => Err("provide exactly one of --case or --case-id".to_owned()),
    }
}

fn read_catalog() -> Result<BTreeMap<String, CombatCaseCatalogEntryV2>, String> {
    let path = catalog_path();
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let file = fs::File::open(&path).map_err(|error| {
        format!(
            "failed to open V2 case catalog '{}': {error}",
            path.display()
        )
    })?;
    let mut entries = BTreeMap::new();
    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| {
            format!(
                "failed reading V2 case catalog '{}' at line {}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: CombatCaseCatalogEntryV2 = serde_json::from_str(&line).map_err(|error| {
            format!(
                "invalid V2 case catalog '{}' at line {}: {error}",
                path.display(),
                line_index + 1
            )
        })?;
        if entry.schema_name != CASE_CATALOG_SCHEMA || entry.schema_version != 2 {
            return Err(format!(
                "unsupported V2 case catalog entry at line {}",
                line_index + 1
            ));
        }
        entries.insert(entry.id.clone(), entry);
    }
    Ok(entries)
}

fn catalog_path() -> PathBuf {
    PathBuf::from(env!("STS_REPOSITORY_ROOT"))
        .join(".oracle-lab")
        .join("v2")
        .join("case-index.jsonl")
}
