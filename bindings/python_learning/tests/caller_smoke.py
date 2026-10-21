from __future__ import annotations

import pathlib
import sys
import hashlib

import sts_learning_bridge
from sts_learning import (
    BatchPolicyChoice, BehaviorManifestId, OnlineBatchDriver, RecoveryPlan,
    SeedPartition, SeedSchedule, initialize_population,
)
from sts_learning_bridge import LearningBatchEnv


class FirstLegalSmokePolicy:
    def choose(self, batch):
        return BatchPolicyChoice.deterministic(
            [0] * len(batch["slot_indices"]),
            BehaviorManifestId(hashlib.sha256(b"caller-smoke-first-legal-v1").digest()),
        )


class NoRecovery:
    def plan_recovery(self, accounting, snapshots):
        return RecoveryPlan()


def main() -> None:
    bridge_path = pathlib.Path(sts_learning_bridge.__file__).resolve()
    venv_root = pathlib.Path(sys.argv[1]).resolve()
    if venv_root not in bridge_path.parents:
        raise SystemExit(f"caller imported bridge outside isolated venv: {bridge_path}")

    population = initialize_population(
        lambda seeds: LearningBatchEnv(seeds, 20), slot_count=2,
        schedule=SeedSchedule(SeedPartition.TRAINING), max_recoveries_per_episode=0,
    )
    driver = OnlineBatchDriver(population, policy=FirstLegalSmokePolicy(), curriculum=NoRecovery())
    result = driver.run(batch_steps=4)
    if result.batch_steps != 4 or result.recoveries != 0:
        raise AssertionError("public caller did not complete its bounded batch steps")

    print(f"python_learning_caller_smoke isolated_bridge={bridge_path}")


if __name__ == "__main__":
    main()
