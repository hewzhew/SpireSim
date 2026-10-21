from types import SimpleNamespace

from sts_learning.combat_public_history_chance import (
    PublicCombatEntryHistoryV1,
    PublicHistoryRunSeedScanConfig,
    merge_retained_run_seed_particles_v1,
    scan_public_history_run_seed_population_v1,
)
from sts_learning.public_decision_prefix import PublicDecisionPrefixStepV1
from sts_learning.seeds import SeedPartitionSpec


class _ReplayEnvironment:
    def __init__(self, seeds: list[int], ascension_level: int) -> None:
        assert ascension_level == 20
        self.slot_count = len(seeds)
        self._seeds = dict(enumerate(seeds))
        self._states = {
            slot: "wrong-prefix" if seed == 11 else "prefix"
            for slot, seed in self._seeds.items()
        }
        self._last_rows: list[int] = []

    def decision_batch(self):
        self._last_rows = [
            slot for slot, state in self._states.items() if state != "terminated"
        ]
        return {
            "slot_indices": self._last_rows,
            "candidate_counts": [2] * len(self._last_rows),
        }

    def public_run_contexts(self):
        return [
            (
                slot,
                SimpleNamespace(
                    seed=self._seeds[slot],
                    act=1,
                    floor=1,
                    is_combat=self._states[slot].startswith("combat"),
                    strategic_context_kind=(
                        None
                        if self._states[slot].startswith("combat")
                        else 1
                    ),
                ),
            )
            for slot in self._last_rows
        ]

    def public_information_snapshots(self):
        return [
            (slot, self._snapshot(self._states[slot])) for slot in self._last_rows
        ]

    def choose(self, ordinals: list[int]) -> None:
        assert len(ordinals) == len(self._last_rows)
        for row, slot in enumerate(self._last_rows):
            if self._states[slot] == "prefix":
                assert ordinals[row] == 1

    def step(self):
        for slot, state in tuple(self._states.items()):
            if state not in {"prefix", "wrong-prefix"}:
                continue
            seed = self._seeds[slot]
            self._states[slot] = {
                10: "combat-target",
                11: "combat-other",
                12: "combat-other",
                13: "terminated",
            }[seed]
        return {}

    def combat_root_artifact_bytes(self, slot_indices, *, max_bytes):
        assert len(slot_indices) == 1
        payload = f"root:{self._seeds[slot_indices[0]]}".encode()
        assert len(payload) <= max_bytes
        return payload

    @staticmethod
    def _snapshot(state: str):
        is_combat = state.startswith("combat")
        snapshot_id = {
            "prefix": "prefix-snapshot",
            "wrong-prefix": "different-prefix",
            "combat-target": "target-combat",
            "combat-other": "different-combat",
        }[state]
        return SimpleNamespace(
            phase=1 if is_combat else 0,
            is_combat=is_combat,
            snapshot_id=snapshot_id,
            observation_id=f"observation:{snapshot_id}",
            history_snapshot_id=f"history:{snapshot_id}",
            candidate_surface_id=f"surface:{snapshot_id}",
            candidate_ids=("stay", "go") if not is_combat else ("end", "play"),
        )


def test_run_seed_scan_replays_the_exact_public_prefix_and_rejects_divergence():
    target = PublicCombatEntryHistoryV1(
        source_seed=10,
        ascension_level=20,
        public_decision_prefix_id="target-prefix",
        previous_decisions=(
            PublicDecisionPrefixStepV1(
                snapshot_id="prefix-snapshot",
                selected_candidate_id="go",
            ),
        ),
        current_snapshot_id="target-combat",
    )
    result = scan_public_history_run_seed_population_v1(
        PublicHistoryRunSeedScanConfig(
            target=target,
            candidate_seed_start=10,
            candidate_seed_count=4,
            partition_spec=SeedPartitionSpec(held_out_numerator=0, denominator=1),
            slot_count=4,
            retained_particle_count=2,
            sampling_seed=99,
        ),
        environment_constructor=_ReplayEnvironment,
    )

    assert result.complete
    assert result.scanned_candidate_count == 4
    assert result.accepted_candidate_count == 1
    assert tuple(particle.run_seed for particle in result.retained_particles) == (10,)
    assert result.source_seed_in_frame
    assert result.source_seed_reconstructed
    assert dict(result.rejection_counts) == {
        "combat_snapshot_mismatch": 1,
        "prefix_snapshot_mismatch_0": 1,
        "terminated_before_target": 1,
    }
    assert merge_retained_run_seed_particles_v1(
        result,
        artifact_merger=lambda payloads, *, max_bytes: b"|".join(payloads),
        max_bytes=1024,
    ) == b"root:10"
