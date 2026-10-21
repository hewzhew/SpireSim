use std::fs;
use std::path::{Path, PathBuf};

// FNV-1a is used only as a deterministic change detector, not for
// adversarial integrity.
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Source owners that define the imitation artifact's feature vocabulary,
/// legal action language, and action identity.
///
/// Keep this list deliberately narrower than the whole simulator. Runtime
/// combat semantics are validated by the simulator/runtime contracts; this
/// identity invalidates a learned artifact when the inputs interpreted by the
/// model or the actions ranked by it change.
const ACTION_CONTRACT_PATHS: &[&str] = &[
    "src/eval/combat_action_imitation.rs",
    "src/sim/combat_action.rs",
    "src/sim/combat_action_surface.rs",
    "src/sim/combat_legal_actions.rs",
    "src/ai/combat_search_v2/pending_choice_action_prefix.rs",
    "src/ai/combat_search_v2/oracle_action_policy",
    "src/ai/analysis/card_semantics.rs",
    "src/content/identity.rs",
    "src/content/cards/mod.rs",
    "src/content/monsters/mod.rs",
    "src/content/potions/mod.rs",
    "src/content/powers/mod.rs",
    "src/state/core/input.rs",
    "src/state/core/pending_choice.rs",
    "src/state/selection.rs",
];

const GUIDANCE_CONTRACT_PATHS: &[&str] = &[
    "src/eval/combat_guidance_bundle.rs",
    "crates/sts_combat_tactics/src",
    "crates/sts_combat_knowledge/src",
    "crates/sts_combat_atomic/src/tactical_policy.rs",
    "src/ai/analysis/card_semantics.rs",
    "src/ai/combat_search_v2/mod.rs",
    "src/ai/combat_search_v2/action_ordering",
    "src/ai/combat_search_v2/action_priority.rs",
    "src/ai/combat_search_v2/action_priority",
    "src/ai/combat_search_v2/pending_choice_ordering.rs",
    "src/ai/combat_search_v2/pending_choice_ordering",
    "src/ai/combat_search_v2/potions",
    "src/ai/combat_search_v2/value.rs",
    "src/ai/combat_search_v2/value",
    "src/ai/combat_search_v2/oracle_action_policy",
];

fn collect_contract_files(repository_root: &Path, relative_path: &Path, files: &mut Vec<PathBuf>) {
    let path = repository_root.join(relative_path);
    if path.is_file() {
        files.push(relative_path.to_path_buf());
        return;
    }
    let entries = fs::read_dir(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read combat runtime contract path {}: {error}",
            path.display()
        )
    });
    let mut children = entries
        .map(|entry| {
            entry.unwrap_or_else(|error| {
                panic!(
                    "failed to enumerate combat runtime contract path {}: {error}",
                    path.display()
                )
            })
        })
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let child_relative_path = relative_path.join(child.file_name());
        if child.path().is_dir() {
            if child.file_name() == "tests" {
                continue;
            }
            collect_contract_files(repository_root, &child_relative_path, files);
        } else if child
            .path()
            .extension()
            .is_some_and(|extension| extension == "rs")
            && child.file_name() != "tests.rs"
        {
            files.push(child_relative_path);
        }
    }
}

fn contract_hash(repository_root: &Path, contract_paths: &[&str], mut hash: u64) -> u64 {
    let mut relative_files = Vec::new();
    for contract_path in contract_paths {
        let relative_path = Path::new(contract_path);
        collect_contract_files(repository_root, relative_path, &mut relative_files);
    }
    relative_files.sort();
    relative_files.dedup();
    for relative_path in relative_files {
        let path = repository_root.join(&relative_path);
        let stable_relative_path = relative_path.to_string_lossy().replace('\\', "/");
        for byte in stable_relative_path
            .as_bytes()
            .iter()
            .copied()
            .chain([0])
            .chain(
                fs::read(&path)
                    .unwrap_or_else(|error| {
                        panic!(
                            "failed to read combat runtime contract file {}: {error}",
                            path.display()
                        )
                    })
                    .into_iter(),
            )
            .chain([0xff])
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

/// Emits one compatibility identity for every crate that compiles the shared
/// combat action-imitation implementation.
pub fn emit(repository_root: &Path) {
    for contract_path in ACTION_CONTRACT_PATHS.iter().chain(GUIDANCE_CONTRACT_PATHS) {
        println!(
            "cargo:rerun-if-changed={}",
            repository_root.join(contract_path).display()
        );
    }

    let action_hash = contract_hash(repository_root, ACTION_CONTRACT_PATHS, FNV_OFFSET_BASIS);
    println!("cargo:rustc-env=STS_COMBAT_ACTION_IMITATION_RUNTIME_ID=fnv1a64:{action_hash:016x}");

    let guidance_hash = contract_hash(repository_root, GUIDANCE_CONTRACT_PATHS, FNV_OFFSET_BASIS);
    println!("cargo:rustc-env=STS_COMBAT_GUIDANCE_RUNTIME_ID=fnv1a64:{guidance_hash:016x}");
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUIRED_ACTION_OWNERS: &[&str] = &[
        "src/sim/combat_legal_actions.rs",
        "src/ai/combat_search_v2/oracle_action_policy",
        "src/ai/analysis/card_semantics.rs",
        "src/content/identity.rs",
        "src/content/cards/mod.rs",
        "src/content/monsters/mod.rs",
        "src/content/potions/mod.rs",
        "src/content/powers/mod.rs",
        "src/state/core/input.rs",
        "src/state/core/pending_choice.rs",
        "src/state/selection.rs",
    ];

    #[test]
    fn action_runtime_contract_covers_every_direct_vocabulary_owner() {
        let repository_root = Path::new(env!("STS_REPOSITORY_ROOT"));
        let complete_hash = contract_hash(repository_root, ACTION_CONTRACT_PATHS, FNV_OFFSET_BASIS);

        for required_owner in REQUIRED_ACTION_OWNERS {
            assert!(
                ACTION_CONTRACT_PATHS.contains(required_owner),
                "missing action imitation vocabulary owner {required_owner}"
            );
            let without_owner = ACTION_CONTRACT_PATHS
                .iter()
                .copied()
                .filter(|path| path != required_owner)
                .collect::<Vec<_>>();
            assert_ne!(
                complete_hash,
                contract_hash(repository_root, &without_owner, FNV_OFFSET_BASIS),
                "action runtime ID did not depend on {required_owner}"
            );
        }
    }
}
