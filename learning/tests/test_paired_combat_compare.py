from __future__ import annotations

import copy

import pytest

pytest.importorskip("torch")

from sts_learning.paired_combat_compare import (
    PairedCombatComparisonError,
    compare_completed_combat_evaluations,
)


def _outcome(replicate: int, *, won: bool, final_hp: int, enemy_hp: int) -> dict[str, object]:
    return {
        "replicate_index": replicate,
        "won": won,
        "terminal_kind": 0 if won else 1,
        "final_hp": final_hp,
        "hp_loss": 75 - final_hp,
        "enemy_final_hp": enemy_hp,
        "final_gold": 50,
        "gold_delta": 0,
        "potions_used": 0,
        "potions_discarded": 0,
        "turns": 4,
        "cards_played": 10,
        "final_potion_ids": [None, None],
        "lost_potion_ids": [],
        "gained_potion_ids": [],
    }


def _root(
    slot: int,
    outcomes: list[dict[str, object]],
) -> dict[str, object]:
    return {
        "slot_index": slot,
        "root_id": f"{slot + 1:064x}",
        "exact_combat_state_hash": f"{slot + 11:064x}",
        "context": {
            "seed": 100 + slot,
            "encounter_id": "ThreeSentries" if slot == 0 else "GremlinNob",
        },
        "wins": sum(bool(outcome["won"]) for outcome in outcomes),
        "final_hp_sum": sum(int(outcome["final_hp"]) for outcome in outcomes),
        "enemy_final_hp_sum": sum(
            int(outcome["enemy_final_hp"]) for outcome in outcomes
        ),
        "outcomes": outcomes,
    }


def _summary(
    manifest_byte: str,
    roots: list[dict[str, object]],
) -> dict[str, object]:
    outcomes = [outcome for root in roots for outcome in root["outcomes"]]
    return {
        "schema": "sts-learning-combat-held-out-evaluation-v16",
        "kind": "completed",
        "artifact_sha256": "a" * 64,
        "root_count": len(roots),
        "replicate_count": 2,
        "decision_rule": "greedy",
        "potion_lane": "never",
        "potion_slots": (),
        "behavior_seed_scope": "root_replicate_independent",
        "behavior_seeds": ((1000, 1001), (1002, 1003)),
        "behavior_manifest_id": manifest_byte * 64,
        "behavior_checkpoint_id": manifest_byte.upper() * 64,
        "evaluation_manifest_id": manifest_byte * 64,
        "wins": sum(bool(outcome["won"]) for outcome in outcomes),
        "final_hp_sum": sum(int(outcome["final_hp"]) for outcome in outcomes),
        "enemy_final_hp_sum": sum(
            int(outcome["enemy_final_hp"]) for outcome in outcomes
        ),
        "roots": roots,
    }


def test_paired_combat_comparison_preserves_root_and_replicate_differences() -> None:
    baseline_roots = [
        _root(
            0,
            [
                _outcome(0, won=False, final_hp=0, enemy_hp=20),
                _outcome(1, won=False, final_hp=0, enemy_hp=18),
            ],
        ),
        _root(
            1,
            [
                _outcome(0, won=True, final_hp=30, enemy_hp=0),
                _outcome(1, won=True, final_hp=28, enemy_hp=0),
            ],
        ),
    ]
    candidate_roots = [
        _root(
            0,
            [
                _outcome(0, won=True, final_hp=20, enemy_hp=0),
                _outcome(1, won=True, final_hp=19, enemy_hp=0),
            ],
        ),
        _root(
            1,
            [
                _outcome(0, won=False, final_hp=0, enemy_hp=12),
                _outcome(1, won=False, final_hp=0, enemy_hp=15),
            ],
        ),
    ]

    comparison = compare_completed_combat_evaluations(
        _summary("1", baseline_roots),
        _summary("2", candidate_roots),
    )

    assert comparison["kind"] == "complete"
    assert comparison["contract"]["seed_scope"] == (
        "root_replicate_declared_greedy_policy_consumes_no_rng"
    )
    assert comparison["aggregate"]["win_delta"] == 0
    assert comparison["aggregate"]["improved_replicates"] == 2
    assert comparison["aggregate"]["regressed_replicates"] == 2
    assert comparison["aggregate"]["improved_roots"] == 1
    assert comparison["aggregate"]["regressed_roots"] == 1
    assert comparison["roots"][0]["win_delta"] == 2
    assert comparison["roots"][1]["win_delta"] == -2
    assert comparison["roots"][0]["outcomes"][0]["win_delta"] == 1
    assert comparison["roots"][1]["outcomes"][0]["win_delta"] == -1
    assert "better" not in comparison
    assert len(comparison["contract_id"]) == 64
    assert len(comparison["comparison_id"]) == 64


def test_paired_combat_comparison_rejects_root_identity_drift() -> None:
    roots = [
        _root(
            0,
            [
                _outcome(0, won=True, final_hp=20, enemy_hp=0),
                _outcome(1, won=True, final_hp=20, enemy_hp=0),
            ],
        ),
        _root(
            1,
            [
                _outcome(0, won=True, final_hp=20, enemy_hp=0),
                _outcome(1, won=True, final_hp=20, enemy_hp=0),
            ],
        ),
    ]
    drifted = copy.deepcopy(roots)
    drifted[1]["context"]["seed"] = 999

    with pytest.raises(PairedCombatComparisonError, match="disagrees on context"):
        compare_completed_combat_evaluations(
            _summary("1", roots),
            _summary("2", drifted),
        )


def test_paired_combat_comparison_accepts_sampled_root_replicate_streams() -> None:
    roots = [
        _root(
            0,
            [
                _outcome(0, won=True, final_hp=20, enemy_hp=0),
                _outcome(1, won=True, final_hp=20, enemy_hp=0),
            ],
        ),
        _root(
            1,
            [
                _outcome(0, won=True, final_hp=20, enemy_hp=0),
                _outcome(1, won=True, final_hp=20, enemy_hp=0),
            ],
        ),
    ]
    baseline = _summary("1", roots)
    candidate = _summary("2", roots)
    baseline["decision_rule"] = "sampled"
    candidate["decision_rule"] = "sampled"

    comparison = compare_completed_combat_evaluations(baseline, candidate)

    assert comparison["contract"]["decision_rule"] == "sampled"
    assert comparison["contract"]["seed_scope"] == (
        "root_replicate_independent_policy_rng"
    )


def test_paired_combat_comparison_rejects_reused_replicate_rng_seed() -> None:
    roots = [
        _root(
            slot,
            [
                _outcome(0, won=True, final_hp=20, enemy_hp=0),
                _outcome(1, won=True, final_hp=20, enemy_hp=0),
            ],
        )
        for slot in range(2)
    ]
    baseline = _summary("1", roots)
    candidate = _summary("2", roots)
    baseline["behavior_seeds"] = ((1000, 1001), (1001, 1003))
    candidate["behavior_seeds"] = ((1000, 1001), (1001, 1003))

    with pytest.raises(PairedCombatComparisonError, match="must be distinct"):
        compare_completed_combat_evaluations(baseline, candidate)
