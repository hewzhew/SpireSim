from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path
from types import SimpleNamespace

from learning.tests.driver_fixtures import (
    FakeCheckpointBatch,
    NumpyWinningBatchEnv,
)
from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.torch_combat_fixtures import OneRoundCombatGroup
from sts_learning.combat_potion_lane import CombatPotionLane
from sts_learning.torch_combat_session_config import CombatSessionBridge
from sts_learning.torch_session_config import CategoricalSessionBridge
from sts_learning.train_combat import (
    CombatTrainingCommandConfig,
    run_combat_training,
)


class CombatRootSource:
    def combat_root_audit(self, slot_index: int) -> SimpleNamespace:
        return SimpleNamespace(
            seed=20_000 + slot_index,
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

    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        return [
            (
                slot_index,
                SimpleNamespace(
                    is_combat=True,
                    seed=20_000 + slot_index,
                    act=1,
                    floor=4,
                    hp=80,
                    max_hp=80,
                    gold=99,
                    potion_ids=("EntropicBrew", "GamblersBrew"),
                    encounter_id=("Cultist", "JawWorm")[slot_index],
                    monster_ids=(("Cultist",), ("JawWorm",))[slot_index],
                ),
            )
            for slot_index in range(2)
        ]

    def combat_group(
        self,
        slot_index: int,
        replicate_count: int,
        potion_slots: Sequence[int] | None = None,
    ) -> OneRoundCombatGroup:
        assert replicate_count == 2
        assert potion_slots in (None, ())
        return OneRoundCombatGroup(
            f"{slot_index + 1:02x}" * 32,
            f"{slot_index + 17:02x}" * 32,
            (True, False) if slot_index == 0 else (True, True),
            potion_slots=None if potion_slots is None else tuple(potion_slots),
        )


def published_behavior(
    root: Path,
    *,
    potion_lane: CombatPotionLane = CombatPotionLane.ALL,
    model_seed: int = 41,
) -> tuple[Path, CombatSessionBridge, CategoricalSessionBridge]:
    artifact = root / "combat-roots.bin"
    artifact.write_bytes(b"opaque-combat-roots")
    schema = semantic_schema_fixture()
    combat_bridge = CombatSessionBridge(
        combat_roots_from_artifact=lambda payload, **_: CombatRootSource(),
        semantic_schema=schema,
    )
    behavior = root / "behavior"
    run_combat_training(
        CombatTrainingCommandConfig(
            artifact=artifact,
            output=behavior,
            root_count=2,
            replicate_count=2,
            updates=1,
            model_seed=model_seed,
            behavior_seed_base=92,
            potion_lane=potion_lane,
        ),
        bridge=combat_bridge,
    )
    run_bridge = CategoricalSessionBridge(
        environment=lambda seeds, ascension_level: NumpyWinningBatchEnv(seeds),
        environment_without_combat_potions=(
            lambda seeds, ascension_level: NumpyWinningBatchEnv(seeds)
        ),
        environment_from_checkpoint=NumpyWinningBatchEnv.from_checkpoint_bytes,
        checkpoint_bank_from_checkpoint=(
            FakeCheckpointBatch.from_checkpoint_bytes
        ),
        semantic_schema=schema,
    )
    return behavior, combat_bridge, run_bridge
