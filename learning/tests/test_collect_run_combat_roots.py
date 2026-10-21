from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace

import pytest

pytest.importorskip("torch")

from learning.tests.driver_fixtures import NumpyWinningBatchEnv  # noqa: E402
from learning.tests.run_training_fixtures import published_behavior  # noqa: E402
from sts_learning.combat_potion_lane import CombatPotionLane  # noqa: E402
from sts_learning.collect_run_combat_roots import (  # noqa: E402
    CombatFightClass,
    EncounterQuota,
    RequiredPotionSlot,
    RunCombatRootCollectionError,
    RunCombatRootCollectionConfig,
    _RootCaptureSink,
    _parser,
    run_run_combat_root_collection,
)
from sts_learning.evaluate_run import RunPotionLane  # noqa: E402
from sts_learning.seeds import SeedPartition, SeedPartitionSpec  # noqa: E402
from sts_learning.torch_behavior import FrozenDecisionRule  # noqa: E402


def test_curriculum_cli_defaults_combat_decisions_to_greedy(tmp_path: Path) -> None:
    arguments = _parser().parse_args(
        [
            "--behavior",
            str(tmp_path / "behavior"),
            "--output",
            str(tmp_path / "roots.bin"),
            "--roots",
            "1",
            "--ascension",
            "20",
        ]
    )

    assert arguments.combat_decision_rule == "greedy"
    assert arguments.seed_partition == "training"
    assert arguments.held_out_numerator == 1
    assert arguments.partition_denominator == 10
    assert arguments.max_floor is None
    assert arguments.required_prior_combats is None
    assert arguments.fight_class == "any"


class _RootCapturingWinningEnv(NumpyWinningBatchEnv):
    potion_ids = ("FearPotion", None, None)

    def __init__(self, seeds: list[int], ascension_level: int) -> None:
        assert ascension_level == 20
        super().__init__(seeds)

    @staticmethod
    def supported_potion_ids() -> list[str]:
        return ["FearPotion", "FirePotion"]

    @staticmethod
    def canonical_encounter_id(raw: str) -> str:
        if raw not in {"JawWorm", "Cultist"}:
            raise ValueError("unsupported encounter")
        return raw

    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        return [
            (
                slot,
                SimpleNamespace(
                    boundary_kind=2 if terminal else 1,
                    is_combat=not terminal,
                    is_terminal=terminal,
                    strategic_context_kind=None,
                    seed=self.seeds[slot],
                    act=1,
                    floor=3,
                    hp=20 if terminal else 70,
                    max_hp=80,
                    gold=50,
                    potion_ids=list(self.potion_ids),
                    encounter_id=None if terminal else "JawWorm",
                    monster_ids=[] if terminal else ["JawWorm"],
                ),
            )
            for slot, terminal in enumerate(self.terminal)
        ]

    def combat_root_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        return [
            (
                slot,
                SimpleNamespace(
                    act=1,
                    floor=3,
                    ascension_level=20,
                    hp=70,
                    max_hp=80,
                    filled_potion_count=sum(
                        potion is not None for potion in self.potion_ids
                    ),
                    usable_potion_count=sum(
                        potion is not None for potion in self.potion_ids
                    ),
                    master_deck_card_count=11,
                    relic_count=1,
                ),
            )
            for slot, terminal in enumerate(self.terminal)
            if not terminal
        ]

    def combat_root_audit(self, slot_index: int) -> SimpleNamespace:
        assert slot_index == 0
        return SimpleNamespace(
            seed=self.seeds[slot_index],
            act=1,
            floor=3,
            ascension_level=20,
            hp=70,
            max_hp=80,
            potion_ids=tuple(self.potion_ids),
            encounter_id="JawWorm",
            monster_ids=("JawWorm",),
            is_elite_fight=False,
            is_boss_fight=False,
            master_deck_cards=[
                *[("Strike", 0)] * 5,
                *[("Defend", 0)] * 4,
                ("Bash", 0),
                ("AscendersBane", 0),
            ],
            relic_ids=["BurningBlood"],
        )

    def strategic_decision_audit_json(self, slot_index: int) -> None:
        assert slot_index == 0
        return None

    def combat_root_artifact_bytes(
        self,
        slot_indices: list[int],
        *,
        max_bytes: int,
    ) -> bytes:
        assert slot_indices == [0]
        payload = b"opaque-root:" + self.seeds[0].to_bytes(8, "big")
        assert len(payload) <= max_bytes
        return payload


class _PotionlessRootCapturingWinningEnv(_RootCapturingWinningEnv):
    potion_ids = (None, None, None)


class _AlternatingEncounterRootEnv(_PotionlessRootCapturingWinningEnv):
    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        rows = super().public_run_contexts()
        for slot, context in rows:
            if not self.terminal[slot]:
                context.encounter_id = (
                    "JawWorm" if self.seeds[slot] % 2 == 0 else "Cultist"
                )
                context.monster_ids = [
                    "JawWorm" if self.seeds[slot] % 2 == 0 else "Cultist"
                ]
        return rows


class _WrongAscensionRootEnv(_PotionlessRootCapturingWinningEnv):
    def combat_root_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        rows = super().combat_root_contexts()
        for _, context in rows:
            context.ascension_level = 0
        return rows


class _EliteRootEnv(_PotionlessRootCapturingWinningEnv):
    def combat_root_audit(self, slot_index: int) -> SimpleNamespace:
        audit = super().combat_root_audit(slot_index)
        audit.is_elite_fight = True
        return audit


class _DelayedStrategicAuditRootEnv(_PotionlessRootCapturingWinningEnv):
    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        rows = super().public_run_contexts()
        if self._round == 0:
            for _, context in rows:
                context.boundary_kind = 0
                context.is_combat = False
                context.strategic_context_kind = 2
                context.floor = 1
                context.hp = 75
                context.encounter_id = None
                context.monster_ids = []
        return rows

    def combat_root_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        if self._round == 0:
            return []
        return super().combat_root_contexts()

    def strategic_decision_audit_json(self, slot_index: int) -> str | None:
        assert slot_index == 0
        if self._round != 0:
            return None
        return json.dumps(
            {
                "schema": "sts-learning-strategic-decision-audit-v1",
                "decision_site": "card_reward",
                "candidates": [
                    {
                        "kind": "take_card",
                        "reward_item_index": 0,
                        "option_index": 0,
                        "card": "PommelStrike",
                        "upgrades": 0,
                    },
                    {"kind": "skip_card_reward", "reward_item_index": 0},
                ],
            }
        )


def test_collection_captures_one_potion_root_per_seed_and_merges_once(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(
        tmp_path,
        potion_lane=CombatPotionLane.NEVER,
    )
    run_bridge = replace(
        run_bridge,
        environment=_RootCapturingWinningEnv,
        environment_without_combat_potions=_RootCapturingWinningEnv,
        environment_from_checkpoint=(
            _RootCapturingWinningEnv.from_checkpoint_bytes
        ),
    )
    merge_calls: list[tuple[bytes, ...]] = []

    def merge(payloads: list[bytes], *, max_bytes: int) -> bytes:
        merge_calls.append(tuple(payloads))
        assert max_bytes == 1024
        return b"merged-root-artifact"

    output = tmp_path / "later-potion-roots.bin"
    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=2,
            max_batch_steps=2,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            min_floor=2,
            min_hp_percent=80,
            min_usable_potions=1,
            potion_lane=RunPotionLane.TRAINED,
            max_artifact_bytes=1024,
            required_potion=RequiredPotionSlot(0, "FearPotion"),
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=merge,
    )

    assert output.read_bytes() == b"merged-root-artifact"
    assert len(merge_calls) == 1
    assert len(merge_calls[0]) == 2
    assert merge_calls[0][0] != merge_calls[0][1]
    assert summary["root_count"] == 2
    assert summary["terminal_attempts"] == 2
    assert summary["required_potion_id"] == "FearPotion"
    assert summary["required_potion_slot"] == 0
    assert summary["requested_run_potion_lane"] == "trained"
    assert summary["run_potion_lane"] == "never"
    assert summary["schema"] == "sts-learning-run-combat-root-collection-v7"
    assert summary["seed_partition"] == "training"
    assert summary["seed_partition_held_out_numerator"] == 1
    assert summary["seed_partition_denominator"] == 10
    assert summary["max_floor"] is None
    assert summary["ascension_level"] == 20
    assert summary["combat_decision_rule"] == "greedy"
    assert summary["collection_manifest_id"] != summary["behavior_manifest_id"]
    assert summary["min_hp_percent"] == 80
    roots = summary["roots"]
    assert isinstance(roots, tuple)
    assert len({root["seed"] for root in roots}) == 2
    assert all(root["potion_ids"] == ("FearPotion", None, None) for root in roots)
    assert all(root["monster_ids"] == ("JawWorm",) for root in roots)
    assert all(root["is_elite_fight"] is False for root in roots)
    assert all(root["is_boss_fight"] is False for root in roots)
    assert all(root["ascension_level"] == 20 for root in roots)
    assert all(root["prior_combat_count"] == 0 for root in roots)
    assert all(root["relic_ids"] == ("BurningBlood",) for root in roots)
    assert roots[0]["deck"] == (
        {"card_id": "AscendersBane", "upgrades": 0, "count": 1},
        {"card_id": "Bash", "upgrades": 0, "count": 1},
        {"card_id": "Defend", "upgrades": 0, "count": 4},
        {"card_id": "Strike", "upgrades": 0, "count": 5},
    )
    assert all(root["prior_combats"] == () for root in roots)
    assert all(root["prior_strategic_decisions"] == () for root in roots)


def test_scoped_collection_uses_a_distinct_verified_combat_anchor(
    tmp_path: Path,
) -> None:
    strategic_root = tmp_path / "strategic"
    anchor_root = tmp_path / "anchor"
    strategic_root.mkdir()
    anchor_root.mkdir()
    strategic, combat_bridge, run_bridge = published_behavior(
        strategic_root,
        potion_lane=CombatPotionLane.NEVER,
        model_seed=41,
    )
    anchor, _, _ = published_behavior(
        anchor_root,
        potion_lane=CombatPotionLane.NEVER,
        model_seed=42,
    )
    run_bridge = replace(
        run_bridge,
        environment=_PotionlessRootCapturingWinningEnv,
        environment_without_combat_potions=_PotionlessRootCapturingWinningEnv,
        environment_from_checkpoint=(
            _PotionlessRootCapturingWinningEnv.from_checkpoint_bytes
        ),
    )
    output = tmp_path / "scoped-roots.bin"

    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=anchor,
            strategic_behavior=strategic,
            output=output,
            root_count=1,
            max_batch_steps=1,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            min_floor=2,
            min_usable_potions=0,
            potion_lane=RunPotionLane.NEVER,
            max_artifact_bytes=1024,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda payloads, *, max_bytes: b"merged-scoped",
    )

    assert output.read_bytes() == b"merged-scoped"
    assert summary["schema"] == "sts-learning-run-combat-root-collection-v7"
    assert summary["collection_scope"] == (
        "combat_anchor_greedy_strategic_source_sampled"
    )
    assert summary["strategic_behavior"] == str(strategic.resolve())
    assert summary["strategic_source_manifest_id"] != summary[
        "combat_anchor_manifest_id"
    ]
    assert len(summary["strategic_source_checkpoint_id"]) == 64
    assert summary["behavior_manifest_id"] == summary["combat_anchor_manifest_id"]
    assert summary["collection_manifest_id"] not in {
        summary["strategic_source_manifest_id"],
        summary["combat_anchor_manifest_id"],
    }


def test_scoped_collection_rejects_sampled_combat_or_implicit_potion_lane(
    tmp_path: Path,
) -> None:
    behavior = tmp_path / "behavior"
    strategic = tmp_path / "strategic"
    behavior.mkdir()
    strategic.mkdir()
    base = dict(
        behavior=behavior,
        strategic_behavior=strategic,
        output=tmp_path / "roots.bin",
        root_count=1,
        max_batch_steps=1,
        wall_ms=10_000,
        behavior_seed=94,
        seed_start=100,
        ascension_level=20,
        min_floor=2,
        min_usable_potions=0,
        max_artifact_bytes=1024,
    )

    with pytest.raises(RunCombatRootCollectionError, match="requires greedy"):
        RunCombatRootCollectionConfig(
            **base,
            combat_decision_rule=FrozenDecisionRule.SAMPLED,
            potion_lane=RunPotionLane.NEVER,
        )
    with pytest.raises(RunCombatRootCollectionError, match="explicit whole-run"):
        RunCombatRootCollectionConfig(
            **base,
            potion_lane=RunPotionLane.TRAINED,
        )


def test_fight_class_selects_mechanical_category_without_naming_an_encounter(
    tmp_path: Path,
) -> None:
    behavior, _, _ = published_behavior(
        tmp_path,
        potion_lane=CombatPotionLane.NEVER,
    )
    config = RunCombatRootCollectionConfig(
        behavior=behavior,
        output=tmp_path / "ordinary-roots.bin",
        root_count=1,
        max_batch_steps=1,
        wall_ms=10_000,
        behavior_seed=94,
        seed_start=100,
        ascension_level=20,
        min_floor=2,
        min_usable_potions=0,
        potion_lane=RunPotionLane.NEVER,
        max_artifact_bytes=1024,
        fight_class=CombatFightClass.ORDINARY,
    )
    elite = _EliteRootEnv([100], 20)
    ordinary_only = _RootCaptureSink(config, None, (), lambda seed, act, floor: 0)

    ordinary_only.observe(elite)

    assert ordinary_only.roots == []


def test_collection_uses_the_declared_seed_partition(tmp_path: Path) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(
        tmp_path,
        potion_lane=CombatPotionLane.NEVER,
    )
    run_bridge = replace(
        run_bridge,
        environment=_PotionlessRootCapturingWinningEnv,
        environment_without_combat_potions=_PotionlessRootCapturingWinningEnv,
        environment_from_checkpoint=(
            _PotionlessRootCapturingWinningEnv.from_checkpoint_bytes
        ),
    )
    spec = SeedPartitionSpec(held_out_numerator=1, denominator=2)
    output = tmp_path / "held-out-roots.bin"
    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=2,
            max_batch_steps=2,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            seed_partition=SeedPartition.HELD_OUT,
            seed_partition_spec=spec,
            min_floor=2,
            min_usable_potions=0,
            potion_lane=RunPotionLane.TRAINED,
            max_artifact_bytes=1024,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda payloads, *, max_bytes: b"merged-held-out",
    )

    assert summary["seed_partition"] == "held_out"
    assert summary["seed_partition_held_out_numerator"] == 1
    assert summary["seed_partition_denominator"] == 2
    assert all(
        spec.classify(root["seed"]) is SeedPartition.HELD_OUT
        for root in summary["roots"]
    )


def test_prior_combat_selector_is_an_exact_typed_boundary(tmp_path: Path) -> None:
    behavior, _, _ = published_behavior(
        tmp_path,
        potion_lane=CombatPotionLane.NEVER,
    )
    config = RunCombatRootCollectionConfig(
        behavior=behavior,
        output=tmp_path / "second-combat-roots.bin",
        root_count=1,
        max_batch_steps=1,
        wall_ms=10_000,
        behavior_seed=94,
        seed_start=100,
        ascension_level=20,
        min_floor=2,
        required_prior_combat_count=1,
        min_usable_potions=0,
        potion_lane=RunPotionLane.TRAINED,
        max_artifact_bytes=1024,
    )
    env = _PotionlessRootCapturingWinningEnv([100], 20)
    blocked = _RootCaptureSink(config, None, (), lambda seed, act, floor: 0)
    blocked.observe(env)
    assert blocked.roots == []

    floor_blocked = _RootCaptureSink(
        replace(config, max_floor=2),
        None,
        (),
        lambda seed, act, floor: 1,
    )
    floor_blocked.observe(env)
    assert floor_blocked.roots == []

    admitted = _RootCaptureSink(config, None, (), lambda seed, act, floor: 1)
    admitted.observe(env)
    assert len(admitted.roots) == 1
    assert admitted.roots[0].prior_combat_count == 1


def test_collection_binds_prior_strategic_candidates_to_the_chosen_ordinal(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_DelayedStrategicAuditRootEnv,
        environment_without_combat_potions=_DelayedStrategicAuditRootEnv,
        environment_from_checkpoint=(
            _DelayedStrategicAuditRootEnv.from_checkpoint_bytes
        ),
    )
    output = tmp_path / "strategic-audit-root.bin"

    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=1,
            max_batch_steps=1,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            min_floor=2,
            min_usable_potions=0,
            max_artifact_bytes=1024,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda *_args, **_kwargs: b"merged-root-artifact",
    )

    root = summary["roots"][0]
    decisions = root["prior_strategic_decisions"]
    assert len(decisions) == 1
    decision = decisions[0]
    assert decision["decision_site"] == "card_reward"
    assert decision["act"] == 1
    assert decision["floor"] == 1
    assert decision["hp"] == 75
    assert len(decision["candidates"]) == 2
    assert decision["selected_action"] == decision["candidates"][
        decision["selected_ordinal"]
    ]


def test_collection_rejects_a_root_that_changed_ascension(tmp_path: Path) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_WrongAscensionRootEnv,
        environment_without_combat_potions=_WrongAscensionRootEnv,
        environment_from_checkpoint=_WrongAscensionRootEnv.from_checkpoint_bytes,
    )
    output = tmp_path / "wrong-ascension-root.bin"

    with pytest.raises(
        RunCombatRootCollectionError,
        match="changed the requested ascension level",
    ):
        run_run_combat_root_collection(
            RunCombatRootCollectionConfig(
                behavior=behavior,
                output=output,
                root_count=1,
                max_batch_steps=1,
                wall_ms=10_000,
                behavior_seed=94,
                seed_start=100,
                ascension_level=20,
                min_floor=2,
                min_usable_potions=0,
                max_artifact_bytes=1024,
            ),
            combat_bridge=combat_bridge,
            run_bridge=run_bridge,
            artifact_merger=lambda *_args, **_kwargs: pytest.fail(
                "invalid ascension must not merge"
            ),
        )

    assert not output.exists()


def test_collection_can_make_only_combat_decisions_greedy(tmp_path: Path) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_PotionlessRootCapturingWinningEnv,
        environment_without_combat_potions=_PotionlessRootCapturingWinningEnv,
        environment_from_checkpoint=(
            _PotionlessRootCapturingWinningEnv.from_checkpoint_bytes
        ),
    )
    output = tmp_path / "combat-greedy-root.bin"

    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=1,
            max_batch_steps=1,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            combat_decision_rule=FrozenDecisionRule.GREEDY,
            min_floor=2,
            min_usable_potions=0,
            max_artifact_bytes=1024,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda payloads, *, max_bytes: payloads[0],
    )

    assert output.is_file()
    assert summary["combat_decision_rule"] == "greedy"
    assert summary["collection_manifest_id"] != summary["behavior_manifest_id"]


def test_collection_can_capture_a_potionless_combat_root(tmp_path: Path) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_PotionlessRootCapturingWinningEnv,
        environment_without_combat_potions=_PotionlessRootCapturingWinningEnv,
        environment_from_checkpoint=(
            _PotionlessRootCapturingWinningEnv.from_checkpoint_bytes
        ),
    )
    output = tmp_path / "potionless-root.bin"

    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=1,
            max_batch_steps=1,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            min_floor=2,
            min_usable_potions=0,
            max_artifact_bytes=1024,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda payloads, *, max_bytes: payloads[0],
    )

    assert output.is_file()
    assert summary["min_usable_potions"] == 0
    assert summary["roots"][0]["potion_ids"] == (None, None, None)
    assert summary["roots"][0]["usable_potion_count"] == 0


def test_collection_can_require_distinct_encounters(tmp_path: Path) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_AlternatingEncounterRootEnv,
        environment_without_combat_potions=_AlternatingEncounterRootEnv,
        environment_from_checkpoint=_AlternatingEncounterRootEnv.from_checkpoint_bytes,
    )
    output = tmp_path / "distinct-encounters.bin"

    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=2,
            max_batch_steps=2,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            min_floor=2,
            min_usable_potions=0,
            max_artifact_bytes=1024,
            distinct_encounters=True,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda payloads, *, max_bytes: b"merged",
    )

    assert summary["distinct_encounters"] is True
    assert {root["encounter_id"] for root in summary["roots"]} == {
        "Cultist",
        "JawWorm",
    }


def test_collection_can_require_one_exact_encounter(tmp_path: Path) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_RootCapturingWinningEnv,
        environment_without_combat_potions=_RootCapturingWinningEnv,
        environment_from_checkpoint=_RootCapturingWinningEnv.from_checkpoint_bytes,
    )
    output = tmp_path / "jaw-worm-root.bin"

    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=1,
            max_batch_steps=1,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            min_floor=2,
            min_usable_potions=0,
            max_artifact_bytes=1024,
            required_encounter_id="JawWorm",
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda payloads, *, max_bytes: payloads[0],
    )

    assert summary["required_encounter_id"] == "JawWorm"
    assert summary["roots"][0]["encounter_id"] == "JawWorm"


def test_collection_fulfills_each_encounter_quota_from_distinct_seeds(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_AlternatingEncounterRootEnv,
        environment_without_combat_potions=_AlternatingEncounterRootEnv,
        environment_from_checkpoint=_AlternatingEncounterRootEnv.from_checkpoint_bytes,
    )
    output = tmp_path / "encounter-quotas.bin"

    summary = run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=behavior,
            output=output,
            root_count=3,
            max_batch_steps=4,
            wall_ms=10_000,
            behavior_seed=94,
            seed_start=100,
            ascension_level=20,
            min_floor=2,
            min_usable_potions=0,
            max_artifact_bytes=1024,
            encounter_quotas=(
                EncounterQuota("JawWorm", 1),
                EncounterQuota("Cultist", 2),
            ),
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
        artifact_merger=lambda payloads, *, max_bytes: b"merged",
    )

    assert output.is_file()
    assert [root["seed"] for root in summary["roots"]] == [100, 101, 103]
    assert summary["encounter_quotas"] == (
        {
            "encounter_id": "JawWorm",
            "requested_roots": 1,
            "captured_roots": 1,
        },
        {
            "encounter_id": "Cultist",
            "requested_roots": 2,
            "captured_roots": 2,
        },
    )


def test_incomplete_encounter_quota_publishes_no_artifact(tmp_path: Path) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_AlternatingEncounterRootEnv,
        environment_without_combat_potions=_AlternatingEncounterRootEnv,
        environment_from_checkpoint=_AlternatingEncounterRootEnv.from_checkpoint_bytes,
    )
    output = tmp_path / "incomplete-encounter-quotas.bin"

    with pytest.raises(
        RunCombatRootCollectionError,
        match=r"Cultist=1/2",
    ):
        run_run_combat_root_collection(
            RunCombatRootCollectionConfig(
                behavior=behavior,
                output=output,
                root_count=3,
                max_batch_steps=3,
                wall_ms=10_000,
                behavior_seed=94,
                seed_start=100,
                ascension_level=20,
                min_floor=2,
                min_usable_potions=0,
                max_artifact_bytes=1024,
                encounter_quotas=(
                    EncounterQuota("JawWorm", 1),
                    EncounterQuota("Cultist", 2),
                ),
            ),
            combat_bridge=combat_bridge,
            run_bridge=run_bridge,
            artifact_merger=lambda *_args, **_kwargs: pytest.fail(
                "incomplete encounter quotas must not merge"
            ),
        )

    assert not output.exists()


def test_required_encounter_rejects_unsupported_identity_before_collection(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_RootCapturingWinningEnv,
        environment_without_combat_potions=_RootCapturingWinningEnv,
        environment_from_checkpoint=_RootCapturingWinningEnv.from_checkpoint_bytes,
    )
    output = tmp_path / "unsupported-monster.bin"

    with pytest.raises(
        RunCombatRootCollectionError,
        match="unsupported by the installed bridge",
    ):
        run_run_combat_root_collection(
            RunCombatRootCollectionConfig(
                behavior=behavior,
                output=output,
                root_count=1,
                max_batch_steps=1,
                wall_ms=10_000,
                behavior_seed=94,
                seed_start=100,
                ascension_level=20,
                min_floor=2,
                min_usable_potions=0,
                max_artifact_bytes=1024,
                required_encounter_id="NotAMonster",
            ),
            combat_bridge=combat_bridge,
            run_bridge=run_bridge,
        )

    assert not output.exists()


def test_bounded_unmatched_potion_collection_publishes_no_artifact(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    run_bridge = replace(
        run_bridge,
        environment=_RootCapturingWinningEnv,
        environment_without_combat_potions=_RootCapturingWinningEnv,
        environment_from_checkpoint=(
            _RootCapturingWinningEnv.from_checkpoint_bytes
        ),
    )
    output = tmp_path / "incomplete-roots.bin"

    with pytest.raises(RunCombatRootCollectionError, match="did not reach"):
        run_run_combat_root_collection(
            RunCombatRootCollectionConfig(
                behavior=behavior,
                output=output,
                root_count=1,
                max_batch_steps=1,
                wall_ms=10_000,
                behavior_seed=95,
                seed_start=200,
                ascension_level=20,
                min_floor=2,
                min_usable_potions=1,
                potion_lane=RunPotionLane.NEVER,
                max_artifact_bytes=1024,
                required_potion=RequiredPotionSlot(0, "FirePotion"),
            ),
            combat_bridge=combat_bridge,
            run_bridge=run_bridge,
            artifact_merger=lambda *_args, **_kwargs: pytest.fail(
                "incomplete collection must not merge"
            ),
        )

    assert not output.exists()
