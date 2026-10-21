from __future__ import annotations

import hashlib
import json
from collections.abc import Sequence
from pathlib import Path
from types import SimpleNamespace

import pytest

pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.run_training_fixtures import published_behavior
from learning.tests.torch_combat_fixtures import OneRoundCombatGroup
from sts_learning.combat_evaluation import combat_observed_resource_frontier
from sts_learning.combat_outcomes import CombatTerminalOutcome
from sts_learning import RunDecisionScope
from sts_learning.evaluate_combat import (
    CombatEvaluationCommandConfig,
    CombatEvaluationCommandError,
    run_combat_evaluation,
)
from sts_learning.evaluate_combat_potions import (
    CombatPotionSweepCommandConfig,
    run_combat_potion_sweep,
)
from sts_learning.torch_behavior import FrozenDecisionRule
from sts_learning.torch_combat_session_config import CombatSessionBridge
from sts_learning.train_combat import (
    CombatTrainingCommandConfig,
    run_combat_training,
)
from sts_learning.train_run import RunTrainingCommandConfig, run_run_training


class _RootSource:
    def __init__(
        self,
        roots: tuple[
            tuple[str, str, tuple[bool, bool], tuple[int, int] | None],
            ...,
        ],
    ) -> None:
        self.roots = roots
        self.calls: list[tuple[int, tuple[int, ...] | None]] = []

    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        encounter_ids = ("Cultist", "JawWorm")
        monster_ids = (("Cultist",), ("JawWorm",))
        return [
            (
                slot_index,
                SimpleNamespace(
                    is_combat=True,
                    seed=10_000 + slot_index,
                    act=1,
                    floor=4,
                    hp=80,
                    max_hp=80,
                    gold=99,
                    potion_ids=("EntropicBrew", "GamblersBrew"),
                    encounter_id=encounter_ids[slot_index],
                    monster_ids=monster_ids[slot_index],
                ),
            )
            for slot_index in range(len(self.roots))
        ]

    def combat_root_audit(self, slot_index: int) -> SimpleNamespace:
        return SimpleNamespace(
            seed=10_000 + slot_index,
            act=1,
            floor=4,
            ascension_level=20,
            hp=80,
            max_hp=80,
            potion_ids=("EntropicBrew", "GamblersBrew"),
            encounter_id=("Cultist", "JawWorm")[slot_index],
            monster_ids=(("Cultist",), ("JawWorm",))[slot_index],
            is_elite_fight=False,
            is_boss_fight=False,
            master_deck_cards=[
                *[("Strike", 0)] * (5 - slot_index),
                *[("Defend", 0)] * 4,
                ("Bash", 0),
                ("AscendersBane", 0),
            ],
            relic_ids=["BurningBlood"],
        )

    def combat_group(
        self,
        slot_index: int,
        replicate_count: int,
        potion_slots: Sequence[int] | None = None,
    ):
        assert replicate_count == 2
        normalized_slots = (
            None if potion_slots is None else tuple(potion_slots)
        )
        self.calls.append((slot_index, normalized_slots))
        root_id, state_hash, wins, final_hps = self.roots[slot_index]
        return OneRoundCombatGroup(
            root_id,
            state_hash,
            wins,
            final_hps=final_hps,
            potion_slots=normalized_slots,
        )


def test_evaluation_recovers_published_behavior_without_training_or_experience(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    # This contract must cross publication, durable recovery, and a distinct
    # artifact; a smaller fixture would no longer protect the command boundary.
    training_artifact = tmp_path / "training.bin"
    evaluation_artifact = tmp_path / "evaluation.bin"
    training_artifact.write_bytes(b"opaque-training-roots")
    evaluation_artifact.write_bytes(b"opaque-evaluation-roots")
    training_source = _RootSource(
        (
            ("12" * 32, "ab" * 32, (True, False), None),
            ("34" * 32, "cd" * 32, (True, True), (70, 50)),
        )
    )
    evaluation_source = _RootSource(
        (
            ("56" * 32, "ef" * 32, (False, True), (0, 61)),
            ("78" * 32, "01" * 32, (True, True), (72, 68)),
        )
    )

    def load(payload: bytes, **_: object) -> _RootSource:
        if payload == training_artifact.read_bytes():
            return training_source
        if payload == evaluation_artifact.read_bytes():
            return evaluation_source
        raise AssertionError("unexpected artifact")

    bridge = CombatSessionBridge(
        combat_roots_from_artifact=load,
        semantic_schema=semantic_schema_fixture(),
    )
    behavior = tmp_path / "behavior"
    run_combat_training(
        CombatTrainingCommandConfig(
            artifact=training_artifact,
            output=behavior,
            root_count=2,
            replicate_count=2,
            updates=1,
            model_seed=41,
            behavior_seed_base=92,
        ),
        bridge=bridge,
    )
    training_journal_before = (behavior / "training.jsonl").read_bytes()
    output = tmp_path / "held-out"

    with pytest.raises(
        CombatEvaluationCommandError,
        match="held-out evaluation artifact matches the training artifact",
    ):
        run_combat_evaluation(
            CombatEvaluationCommandConfig(
                artifact=training_artifact,
                behavior=behavior,
                output=tmp_path / "leaked-evaluation",
                root_count=2,
                replicate_count=2,
                behavior_seed_base=1_000,
            ),
            bridge=bridge,
        )
    assert not (tmp_path / "leaked-evaluation").exists()

    summary = run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=evaluation_artifact,
            behavior=behavior,
            output=output,
            root_count=2,
            replicate_count=2,
            behavior_seed_base=1_000,
        ),
        bridge=bridge,
    )

    assert summary["schema"] == "sts-learning-combat-held-out-evaluation-v16"
    assert summary["decision_rule"] == "sampled"
    assert summary["behavior_seed_scope"] == "root_replicate_independent"
    assert summary["behavior_seeds"] == ((1_000, 1_001), (1_002, 1_003))
    assert summary["evaluation_manifest_id"] == summary["behavior_manifest_id"]
    assert summary["behavior_training_kind"] == "combat"
    assert summary["potion_lane"] == "all"
    assert summary["potion_slots"] == ()
    assert summary["root_audits"][0]["encounter_id"] == "Cultist"
    assert summary["root_audits"][0]["deck"][-1] == {
        "card_id": "Strike",
        "upgrades": 0,
        "count": 5,
    }
    assert summary["root_audits"][1]["relic_ids"] == ("BurningBlood",)
    assert summary["decision_trace"] == {
        "file": None,
        "record_count": 0,
        "replicates_per_root": 0,
        "schema": "sts-learning-combat-action-trace-v2",
    }
    assert training_source.calls == [
        (0, None),
        (1, None),
        (0, None),
        (1, None),
    ]
    assert evaluation_source.calls == [(0, None), (1, None)]
    assert summary["wins"] == 3
    assert summary["losses"] == 1
    assert summary["behavior_training_step"] == 1
    assert summary["behavior_training_root_count"] == 2
    assert summary["behavior_training_all_loss_axis"] == "none"
    assert summary["behavior_training_artifact_sha256"] == hashlib.sha256(
        training_artifact.read_bytes()
    ).hexdigest()
    assert summary["behavior_training_potion_lane"] == "all"
    assert summary["behavior_training_potion_slots"] == ()
    assert summary["final_hp_sum"] == 201
    assert summary["enemy_final_hp_sum"] == 20
    assert summary["enemy_hp_progress_signal_roots"] == 1
    assert summary["enemy_hp_progress_signal_replicates"] == 2
    assert summary["gold_delta_sum"] == 0
    assert summary["lost_potion_ids"] == {"EntropicBrew": 2}
    assert summary["gained_potion_ids"] == {"BlockPotion": 2}
    assert summary["observed_resource_frontier_replicates"] == 3
    assert summary["observed_resource_dominated_replicates"] == 0
    assert summary["observed_resource_incomparable_winning_pairs"] == 1
    assert tuple(root["wins"] for root in summary["roots"]) == (1, 2)
    assert summary["roots"][0]["context"]["floor"] == 4
    assert summary["roots"][0]["context"]["seed"] == 10_000
    assert summary["roots"][0]["context"]["encounter_id"] == "Cultist"
    assert summary["roots"][0]["context"]["monster_ids"] == ("Cultist",)
    assert summary["roots"][0]["context"]["gold"] == 99
    assert summary["roots"][0]["context"]["potion_ids"] == (
        "EntropicBrew",
        "GamblersBrew",
    )
    assert tuple(
        outcome["final_hp"] for outcome in summary["roots"][0]["outcomes"]
    ) == (0, 61)
    assert summary["roots"][0]["enemy_start_hp"] == 40
    assert summary["roots"][0]["enemy_final_hp_sum"] == 20
    assert summary["roots"][0]["enemy_final_hp_min"] == 0
    assert summary["roots"][0]["enemy_final_hp_max"] == 20
    assert summary["roots"][0]["enemy_hp_progress_signal_replicates"] == 2
    assert tuple(
        outcome["enemy_final_hp"]
        for outcome in summary["roots"][0]["outcomes"]
    ) == (20, 0)
    assert summary["roots"][0]["outcomes"][1]["lost_potion_ids"] == (
        "EntropicBrew",
    )
    assert summary["roots"][0]["outcomes"][1]["gained_potion_ids"] == (
        "BlockPotion",
    )
    assert summary["roots"][1]["observed_resource_order"] == {
        "winning_replicate_indices": (0, 1),
        "frontier_replicate_indices": (0, 1),
        "dominated_replicate_indices": (),
        "dominators_by_replicate": ((), ()),
        "strict_order_pair_count": 0,
        "equivalent_pair_count": 0,
        "incomparable_pair_count": 1,
    }
    assert tuple(row["encounter_id"] for row in summary["encounters"]) == (
        "Cultist",
        "JawWorm",
    )
    assert summary["encounters"][0] == {
        "encounter_id": "Cultist",
        "root_count": 1,
        "replicate_count": 2,
        "wins": 1,
        "losses": 1,
        "final_hp_sum": 61,
        "hp_loss_sum": 99,
        "enemy_final_hp_sum": 20,
        "potions_used": 1,
        "potions_discarded": 0,
        "monster_lineups": (
            {"monster_ids": ("Cultist",), "root_count": 1},
        ),
    }
    assert (behavior / "training.jsonl").read_bytes() == training_journal_before
    assert tuple(path.name for path in output.iterdir()) == ("evaluation.json",)
    assert json.loads((output / "evaluation.json").read_text(encoding="utf-8"))[
        "behavior_manifest_id"
    ] == summary["behavior_manifest_id"]
    all_stdout = capsys.readouterr().out
    assert (
        "evaluation_complete=true wins=3 losses=1 decision_rule=sampled "
        "potion_lane=all "
        "potion_slots=all "
        "root_wins=1,2"
    ) in all_stdout
    assert "root_sites=A1F4,A1F4" in all_stdout
    assert (
        "root_resource_frontiers=1,2 root_resource_dominated=0,0" in all_stdout
    )
    assert "root_start_potions=EntropicBrew+GamblersBrew" in all_stdout
    assert (
        "lost_potions=EntropicBrew:2 gained_potions=BlockPotion:2" in all_stdout
    )

    trace_output = tmp_path / "held-out-with-trace"
    traced_summary = run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=evaluation_artifact,
            behavior=behavior,
            output=trace_output,
            root_count=2,
            replicate_count=2,
            behavior_seed_base=1_000,
            decision_rule=FrozenDecisionRule.GREEDY,
            trace_replicates_per_root=1,
        ),
        bridge=bridge,
        print_completion=False,
    )
    trace_rows = tuple(
        json.loads(line)
        for line in (trace_output / "combat-traces.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    )
    assert traced_summary["decision_trace"] == {
        "file": "combat-traces.jsonl",
        "record_count": 2,
        "replicates_per_root": 1,
        "schema": "sts-learning-combat-action-trace-v2",
    }
    assert traced_summary["decision_rule"] == "greedy"
    assert traced_summary["behavior_manifest_id"] == summary["behavior_manifest_id"]
    assert traced_summary["evaluation_manifest_id"] != summary["behavior_manifest_id"]
    assert set(path.name for path in trace_output.iterdir()) == {
        "combat-traces.jsonl",
        "evaluation.json",
    }
    assert tuple(row["root_slot_index"] for row in trace_rows) == (0, 1)
    assert all(
        row["schema"] == "sts-learning-combat-action-trace-v2"
        for row in trace_rows
    )
    assert all(row["decision"]["replicate_index"] == 0 for row in trace_rows)
    assert all(row["decision"]["model_round_index"] == 0 for row in trace_rows)
    assert all(row["decision"]["selected_ordinal"] == 0 for row in trace_rows)
    assert all(
        row["decision"]["selection_probability"] == 1.0
        for row in trace_rows
    )

    sweep_output = tmp_path / "held-out-potion-sweep"
    sweep = run_combat_potion_sweep(
        CombatPotionSweepCommandConfig(
            artifact=evaluation_artifact,
            behavior=behavior,
            output=sweep_output,
            root_count=2,
            replicate_count=2,
            behavior_seed_base=1_000,
        ),
        bridge=bridge,
    )

    assert tuple(lane["label"] for lane in sweep["lanes"]) == (
        "never",
        "root-slot-0",
        "root-slot-1",
        "all",
    )
    assert sweep["behavior_training_all_loss_axis"] == "none"
    assert tuple(lane["potions_used"] for lane in sweep["lanes"]) == (
        0,
        2,
        2,
        2,
    )
    assert sweep["lanes"][0]["lost_potion_ids"] == {}
    assert sweep["lanes"][1]["lost_potion_ids"] == {"EntropicBrew": 2}
    assert sweep["lanes"][2]["lost_potion_ids"] == {"GamblersBrew": 2}
    assert tuple(lane["label"] for lane in sweep["roots"][0]["lanes"]) == (
        "never",
        "root-slot-0",
        "root-slot-1",
        "all",
    )
    assert evaluation_source.calls == [
        (0, None),
        (1, None),
        (0, None),
        (1, None),
        (0, ()),
        (1, ()),
        (0, (0,)),
        (1, (0,)),
        (0, (1,)),
        (1, (1,)),
        (0, None),
        (1, None),
    ]
    assert set(path.name for path in sweep_output.iterdir()) == {
        "no-potions",
        "root-slot-0",
        "root-slot-1",
        "all-potions",
        "potion-sweep.json",
    }
    assert (behavior / "training.jsonl").read_bytes() == training_journal_before
    sweep_stdout = capsys.readouterr().out
    assert (
        "combat_potion_sweep_complete=true "
        "lanes=never,root-slot-0,root-slot-1,all"
    ) in sweep_stdout
    assert (
        "combat_potion_sweep_roots "
        "metrics=wins/final_hp_sum/potions_used/potions_discarded count=2"
    ) in sweep_stdout
    assert (
        "combat_potion_sweep_root slot=0 site=A1F4 hp=80/80 "
        "potions=EntropicBrew+GamblersBrew "
        "lanes=never:1/61/0/0,root-slot-0:1/61/1/0,"
        "root-slot-1:1/61/1/0,all:1/61/1/0"
    ) in sweep_stdout


def test_evaluation_accepts_a_verified_run_trained_behavior(tmp_path: Path) -> None:
    combat_behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_behavior = tmp_path / "run-behavior"
    run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=combat_behavior,
            output=run_behavior,
            slot_count=2,
            generations=1,
            attempts_per_update=2,
            max_batch_steps_per_generation=1,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=2,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    artifact = tmp_path / "run-held-out-roots.bin"
    artifact.write_bytes(b"distinct-run-held-out-roots")

    summary = run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=artifact,
            behavior=run_behavior,
            output=tmp_path / "run-held-out",
            root_count=2,
            replicate_count=2,
            behavior_seed_base=1_000,
        ),
        bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert summary["behavior_training_kind"] == "run"
    assert summary["behavior_training_root_count"] is None
    assert summary["behavior_training_artifact_sha256"] is None
    assert summary["behavior_run_sampling_mode"] == "independent-cohorts"
    assert summary["behavior_run_objective"]["attempts_per_update"] == 2
    assert summary["wins"] == 3
    assert summary["losses"] == 1


def test_evaluation_recovers_an_anchored_run_combat_policy(tmp_path: Path) -> None:
    combat_behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_behavior = tmp_path / "anchored-run-behavior"
    run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=combat_behavior,
            output=run_behavior,
            slot_count=1,
            generations=0,
            attempts_per_update=1,
            max_batch_steps_per_generation=1,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=1,
            evaluation_max_batch_steps=1,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
            decision_scope=RunDecisionScope.STRATEGIC,
            combat_decision_rule=FrozenDecisionRule.GREEDY,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    artifact = tmp_path / "anchored-run-held-out-roots.bin"
    artifact.write_bytes(b"distinct-anchored-run-held-out-roots")

    summary = run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=artifact,
            behavior=run_behavior,
            output=tmp_path / "anchored-run-held-out",
            root_count=2,
            replicate_count=2,
            behavior_seed_base=1_000,
        ),
        bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert summary["schema"] == "sts-learning-combat-held-out-evaluation-v16"
    assert summary["behavior_run_combat_decision_rule"] == "greedy"
    assert summary["behavior_run_combat_anchor_manifest_id"] is not None
    assert summary["behavior_run_combat_anchor_checkpoint_id"] is not None
    assert summary["behavior_run_combat_anchor_scorer"] == {
        "hidden_dim": 64,
        "relation_layers": 2,
        "value_head": False,
        "value_head_width": 1,
    }


def test_observed_resource_frontier_keeps_hp_potion_tradeoffs_incomparable() -> None:
    def outcome(
        replicate_index: int,
        *,
        won: bool,
        hp: int,
        potions: tuple[str | None, ...],
    ) -> CombatTerminalOutcome:
        return CombatTerminalOutcome(
            replicate_index=replicate_index,
            terminal_kind=0 if won else 1,
            won=won,
            start_hp=80,
            final_hp=hp,
            final_max_hp=80,
            final_gold=99,
            hp_loss=80 - hp,
            enemy_start_hp=40,
            enemy_final_hp=0 if won else 20,
            turns=3,
            potions_used=0,
            potions_discarded=0,
            cards_played=8,
            final_potion_ids=potions,
        )

    frontier = combat_observed_resource_frontier(
        (
            outcome(0, won=True, hp=60, potions=("BlockPotion", "SkillPotion")),
            outcome(1, won=True, hp=70, potions=("BlockPotion", None)),
            outcome(2, won=True, hp=50, potions=("BlockPotion", None)),
            outcome(3, won=False, hp=0, potions=("BlockPotion", "SkillPotion")),
        )
    )

    assert frontier.winning_replicate_indices == (0, 1, 2)
    assert frontier.frontier_replicate_indices == (0, 1)
    assert frontier.dominated_replicate_indices == (2,)
    assert frontier.dominators_by_replicate == ((), (), (0, 1), ())
    assert frontier.strict_order_pair_count == 2
    assert frontier.equivalent_pair_count == 0
    assert frontier.incomparable_pair_count == 1
