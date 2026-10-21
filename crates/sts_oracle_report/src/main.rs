use std::path::PathBuf;

use sts_oracle_report::{
    inspect_public_actor_census_v9, load_public_actor_census_v9, load_resident_corridor_audit_v1,
    render_public_actor_census_inspection_v1, render_resident_corridor_audit_v1,
    PublicActorCensusInspectFilterV1,
};

const HELP: &str = r#"Usage:
  oracle_report census <report.json> [--episode N] [--attempt N]
                        [--encounter ID] [--floor N] [--json]
  oracle_report resident-corridor <report.json> [--json]

Reads and validates existing durable evidence only. It never restores game
state, runs search, or modifies the source report.
"#;

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print!("{HELP}");
        return Ok(());
    };
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if command == "resident-corridor" {
        let Some(path) = args.next() else {
            return Err(format!(
                "resident corridor report path is required\n\n{HELP}"
            ));
        };
        let mut json = false;
        for argument in args.by_ref() {
            match argument.as_str() {
                "--json" => json = true,
                "--help" | "-h" => {
                    print!("{HELP}");
                    return Ok(());
                }
                _ => {
                    return Err(format!(
                        "unknown resident-corridor argument {argument:?}\n\n{HELP}"
                    ))
                }
            }
        }
        let report = load_resident_corridor_audit_v1(&PathBuf::from(path))?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| format!("failed to serialize resident audit: {error}"))?
            );
        } else {
            print!("{}", render_resident_corridor_audit_v1(&report));
        }
        return Ok(());
    }
    if command != "census" {
        return Err(format!("unknown report command {command:?}\n\n{HELP}"));
    }
    let Some(path) = args.next() else {
        return Err(format!("census report path is required\n\n{HELP}"));
    };
    let mut filter = PublicActorCensusInspectFilterV1::default();
    let mut json = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--episode" => filter.episode = Some(parse_usize("--episode", args.next())?),
            "--attempt" => filter.attempt = Some(parse_usize("--attempt", args.next())?),
            "--encounter" => filter.encounter = Some(required_value("--encounter", args.next())?),
            "--floor" => filter.floor = Some(parse_u32("--floor", args.next())?),
            "--json" => json = true,
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(());
            }
            _ => return Err(format!("unknown census argument {argument:?}\n\n{HELP}")),
        }
    }

    let report = load_public_actor_census_v9(&PathBuf::from(path))?;
    let inspection = inspect_public_actor_census_v9(&report, &filter)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&inspection)
                .map_err(|error| format!("failed to serialize inspection: {error}"))?
        );
    } else {
        print!("{}", render_public_actor_census_inspection_v1(&inspection));
    }
    Ok(())
}

fn required_value(flag: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_usize(flag: &str, value: Option<String>) -> Result<usize, String> {
    let value = required_value(flag, value)?;
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid {flag} value {value:?}: {error}"))
}

fn parse_u32(flag: &str, value: Option<String>) -> Result<u32, String> {
    let value = required_value(flag, value)?;
    value
        .parse::<u32>()
        .map_err(|error| format!("invalid {flag} value {value:?}: {error}"))
}
