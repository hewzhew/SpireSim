//! Explicit developer experiment; ordinary tests never launch Java.
use std::{path::PathBuf, time::Duration};
use sts_java_parity_runtime::{
    launch_java_parity_runtime_v1, JavaParityInitialBoundaryV1, JavaParityLaunchProfileV1,
    JavaParityLaunchRequestV1, JavaParityStorageSeedV1,
};
use sts_parity_lockstep_java::OwnedJavaLockstepTransportV1;
use sts_parity_lockstep_runner::mechanism::{
    record_combust_batch_v1, record_constricted_batch_v1, record_hp_draw_batch_v1,
    record_hp_draw_powers_batch_v1, record_hp_draw_prevention_batch_v1, record_regret_batch_v1,
    record_regret_order_batch_v1, record_rupture_batch_v1, record_shackled_haste_batch_v1,
    replay_combust_batch_v1, replay_constricted_batch_v1, replay_hp_draw_batch_v1,
    replay_regret_batch_v1, replay_regret_order_batch_v1, replay_rupture_batch_v1,
    replay_shackled_haste_batch_v1,
};
use sts_workbench_supervisor::{
    OwnedStorageRegistryV1, RepositoryLifecycleOwnerV1, RepositoryLifecycleRoleV1,
};

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_shackled_haste() -> Result<(), String> {
    collect_batch(Mechanism::Haste)
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_constricted() -> Result<(), String> {
    collect_batch(Mechanism::Constricted)
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_regret() -> Result<(), String> {
    collect_batch(Mechanism::Regret)
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_regret_order() -> Result<(), String> {
    collect_batch(Mechanism::RegretOrder)
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_rupture() -> Result<(), String> {
    collect_batch(Mechanism::Rupture)
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_combust() -> Result<(), String> {
    collect_batch(Mechanism::Combust)
}

enum Mechanism {
    HpDrawPowers,
    HpDrawPrevention,
    HpDraw,
    Haste,
    Constricted,
    Regret,
    RegretOrder,
    Rupture,
    Combust,
}

fn collect_batch(mechanism: Mechanism) -> Result<(), String> {
    let destination = required_path("STS_MECHANISM_OUTPUT")?;
    if !destination.is_absolute()
        || destination.exists()
        || !destination.parent().is_some_and(|parent| parent.is_dir())
    {
        return Err("STS_MECHANISM_OUTPUT must be an unused absolute directory path".to_owned());
    }
    let game = required_path("STS_MECHANISM_GAME_DIRECTORY")?;
    if !game.is_absolute() || !game.is_dir() {
        return Err(
            "STS_MECHANISM_GAME_DIRECTORY must be an existing absolute directory".to_owned(),
        );
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let lifecycle =
        RepositoryLifecycleOwnerV1::acquire_for(&repo, RepositoryLifecycleRoleV1::ParityOracleHost)
            .map_err(|e| e.to_string())?;
    let registry = OwnedStorageRegistryV1::new(&lifecycle, &game);
    let (prepared, startup) = launch_java_parity_runtime_v1(JavaParityLaunchRequestV1 {
        storage_registry: &registry,
        storage_seed: JavaParityStorageSeedV1::Fresh,
        profile: JavaParityLaunchProfileV1::ScenarioControl,
        initial_boundary: JavaParityInitialBoundaryV1::FreshStart,
    })?;
    println!(
        "constructed oracle ready | {:.1}s startup",
        startup.as_secs_f64()
    );
    let mut oracle = OwnedJavaLockstepTransportV1::new(
        prepared.into_owner().map_err(|e| e.to_string())?,
        Duration::from_secs(30),
    );
    let progress = |message: &str| println!("{message}");
    let result = match mechanism {
        Mechanism::HpDrawPowers => {
            record_hp_draw_powers_batch_v1(&mut oracle, &destination, progress)
        }
        Mechanism::HpDrawPrevention => {
            record_hp_draw_prevention_batch_v1(&mut oracle, &destination, progress)
        }
        Mechanism::HpDraw => record_hp_draw_batch_v1(&mut oracle, &destination, progress),
        Mechanism::Haste => record_shackled_haste_batch_v1(&mut oracle, &destination, progress),
        Mechanism::Constricted => record_constricted_batch_v1(&mut oracle, &destination, progress),
        Mechanism::Regret => record_regret_batch_v1(&mut oracle, &destination, progress),
        Mechanism::RegretOrder => record_regret_order_batch_v1(&mut oracle, &destination, progress),
        Mechanism::Rupture => record_rupture_batch_v1(&mut oracle, &destination, progress),
        Mechanism::Combust => record_combust_batch_v1(&mut oracle, &destination, progress),
    };
    // The library retains the owner on failure. This explicit test caller
    // chooses to close its isolated JVM after preserving the failed case.
    let shutdown = oracle
        .runtime_mut()
        .shutdown()
        .map_err(|error| error.to_string());
    result?;
    shutdown?;
    let cases = match mechanism {
        Mechanism::HpDrawPowers => replay_hp_draw_batch_v1(&destination)?,
        Mechanism::HpDrawPrevention => replay_hp_draw_batch_v1(&destination)?,
        Mechanism::HpDraw => replay_hp_draw_batch_v1(&destination)?,
        Mechanism::Haste => replay_shackled_haste_batch_v1(&destination)?,
        Mechanism::Constricted => replay_constricted_batch_v1(&destination)?,
        Mechanism::Regret => replay_regret_batch_v1(&destination)?,
        Mechanism::RegretOrder => replay_regret_order_batch_v1(&destination)?,
        Mechanism::Rupture => replay_rupture_batch_v1(&destination)?,
        Mechanism::Combust => replay_combust_batch_v1(&destination)?,
    };
    println!("mechanism batch | {cases} live + offline Exact | declared scope only");
    Ok(())
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_hp_draw() -> Result<(), String> {
    collect_batch(Mechanism::HpDraw)
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_hp_draw_prevention() -> Result<(), String> {
    collect_batch(Mechanism::HpDrawPrevention)
}

#[test]
#[ignore = "explicit read-only replay; requires STS_MECHANISM_OUTPUT"]
fn replay_hp_draw() -> Result<(), String> {
    let cases = replay_hp_draw_batch_v1(&required_path("STS_MECHANISM_OUTPUT")?)?;
    println!("HP draw batch | {cases} Exact | offline, bound cards/relics/creatures + RNG");
    Ok(())
}

#[test]
#[ignore = "explicit read-only replay; requires STS_MECHANISM_OUTPUT"]
fn replay_shackled_haste() -> Result<(), String> {
    let cases = replay_shackled_haste_batch_v1(&required_path("STS_MECHANISM_OUTPUT")?)?;
    println!("mechanism batch | {cases} Exact | offline, monster + floor.ai only");
    Ok(())
}

#[test]
#[ignore = "explicit read-only replay; requires STS_MECHANISM_OUTPUT"]
fn replay_constricted() -> Result<(), String> {
    let cases = replay_constricted_batch_v1(&required_path("STS_MECHANISM_OUTPUT")?)?;
    println!("Constricted batch | {cases} Exact | offline, creatures + AI/shuffle");
    Ok(())
}

#[test]
#[ignore = "explicit read-only replay; requires STS_MECHANISM_OUTPUT"]
fn replay_regret() -> Result<(), String> {
    let cases = replay_regret_batch_v1(&required_path("STS_MECHANISM_OUTPUT")?)?;
    println!("Regret batch | {cases} Exact | offline, creatures + AI/shuffle");
    Ok(())
}

#[test]
#[ignore = "explicit read-only replay; requires STS_MECHANISM_OUTPUT"]
fn replay_regret_order() -> Result<(), String> {
    let cases = replay_regret_order_batch_v1(&required_path("STS_MECHANISM_OUTPUT")?)?;
    println!("Regret order batch | {cases} Exact | offline, creatures + AI/shuffle");
    Ok(())
}

#[test]
#[ignore = "explicit read-only replay; requires STS_MECHANISM_OUTPUT"]
fn replay_rupture() -> Result<(), String> {
    let cases = replay_rupture_batch_v1(&required_path("STS_MECHANISM_OUTPUT")?)?;
    println!("Rupture batch | {cases} Exact | offline, creatures + AI/shuffle");
    Ok(())
}

fn required_path(name: &str) -> Result<PathBuf, String> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("requires explicit {name}"))
}

#[test]
#[ignore = "explicit read-only replay; requires STS_MECHANISM_OUTPUT"]
fn replay_combust() -> Result<(), String> {
    let cases = replay_combust_batch_v1(&required_path("STS_MECHANISM_OUTPUT")?)?;
    println!("Combust batch | {cases} Exact | offline, creatures + AI/shuffle");
    Ok(())
}

#[test]
#[ignore = "explicit bounded Java collection; requires STS_MECHANISM_OUTPUT and STS_MECHANISM_GAME_DIRECTORY"]
fn collect_hp_draw_powers() -> Result<(), String> {
    collect_batch(Mechanism::HpDrawPowers)
}
