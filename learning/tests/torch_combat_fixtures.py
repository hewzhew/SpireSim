from __future__ import annotations

import json
from collections.abc import Sequence
from types import SimpleNamespace

import numpy as np

from learning.tests.semantic_fixtures import semantic_batch_fixture
from sts_learning import (
    BehaviorManifestId,
    CombatDecisionExperienceBatch,
    CombatDecisionProgress,
    CombatTerminalOutcome,
    CompletedCombatGroup,
    CompletedCombatGroupExperience,
    SelectionProbability,
    select_semantic_decision_rows,
)


ROOT_ID = "12" * 32
COMBAT_HASH = "ab" * 32


class OneRoundCombatGroup:
    """A real semantic policy round followed by fixed typed outcomes."""

    replicate_count = 2

    def __init__(
        self,
        root_id: str,
        exact_combat_state_hash: str,
        wins: tuple[bool, bool],
        *,
        final_hps: tuple[int, int] | None = None,
        enemy_final_hps: tuple[int, int] | None = None,
        terminal_kinds: tuple[int, int] | None = None,
        potion_slots: Sequence[int] | None = None,
    ) -> None:
        self.root_id = root_id
        self.exact_combat_state_hash = exact_combat_state_hash
        self.wins = wins
        self.final_hps = final_hps
        self.enemy_final_hps = enemy_final_hps
        self.terminal_kinds = terminal_kinds
        self.potion_slots = (
            None if potion_slots is None else tuple(potion_slots)
        )
        self.root_context = SimpleNamespace(
            act=1,
            floor=4,
            ascension_level=20,
            turn=1,
            is_boss_fight=False,
            is_elite_fight=False,
            monster_count=1,
            living_monster_count=1,
            potion_slot_count=2,
            filled_potion_count=2,
            usable_potion_count=2,
            master_deck_card_count=12,
            relic_count=1,
            hand_card_count=5,
            hp=80,
            max_hp=80,
        )
        self.root_gold = 99
        self.root_potion_ids = ("EntropicBrew", "GamblersBrew")
        self.terminal_count = 0
        self.ready = False
        self.choose_calls = 0

    def decision_batch(self, *, semantic: bool) -> dict[str, object]:
        if not semantic:
            raise AssertionError("combat generation requires semantic rows")
        batch = semantic_batch_fixture()
        batch["slot_indices"] = np.asarray([0, 1], dtype=np.uint64)
        return batch

    def decision_progress(self) -> list[SimpleNamespace]:
        return [
            SimpleNamespace(
                replicate_index=replicate_index,
                turn=1,
                player_hp=80,
                player_max_hp=80,
                enemy_hp=40,
                enemy_max_hp=40,
                potion_uuids=(101, 102),
                potion_ids=("EntropicBrew", "GamblersBrew"),
            )
            for replicate_index in range(self.replicate_count)
        ]

    def choose(self, ordinals: list[int]) -> None:
        if self.ready or len(ordinals) != self.replicate_count:
            raise AssertionError("combat group received an invalid choice")
        self.choose_calls += 1
        self.ready = True

    def ready_action_trace_json(self, replicate_index: int) -> str | None:
        if not self.ready:
            return None
        if not 0 <= replicate_index < self.replicate_count:
            raise AssertionError("combat trace replicate is out of range")
        return json.dumps(
            {
                "schema_name": "CombatLearningReadyActionTrace",
                "schema_version": 2,
                "replicate_index": replicate_index,
                "decision_ordinals": [0],
                "turn": 1,
                "energy": 3,
                "player_hp": 80,
                "player_max_hp": 80,
                "player_block": 0,
                "hand": [],
                "draw_count": 5,
                "discard_count": 0,
                "exhaust_count": 0,
                "potions": ["EntropicBrew", "GamblersBrew"],
                "monsters": [],
                "action": {"kind": "combat_input"},
            }
        )

    def step(self) -> dict[str, object]:
        if not self.ready or self.terminal_count != 0:
            raise AssertionError("combat group cannot step now")
        self.terminal_count = self.replicate_count
        final_hp = np.asarray(
            self.final_hps
            if self.final_hps is not None
            else [70 if won else 0 for won in self.wins],
            dtype=np.int32,
        )
        enemy_final_hp = np.asarray(
            self.enemy_final_hps
            if self.enemy_final_hps is not None
            else [0 if won else 20 for won in self.wins],
            dtype=np.int32,
        )
        terminal_kind = np.asarray(
            self.terminal_kinds
            if self.terminal_kinds is not None
            else [0 if won else 1 for won in self.wins],
            dtype=np.uint8,
        )
        used_slot = (
            0
            if self.potion_slots is None or 0 in self.potion_slots
            else (1 if 1 in self.potion_slots else None)
        )
        potion_use = (0, int(used_slot is not None))
        if used_slot == 0:
            final_potion_ids = (
                ("EntropicBrew", "GamblersBrew"),
                ("BlockPotion", "GamblersBrew"),
            )
        elif used_slot == 1:
            final_potion_ids = (
                ("EntropicBrew", "GamblersBrew"),
                ("EntropicBrew", "BlockPotion"),
            )
        else:
            final_potion_ids = (
                ("EntropicBrew", "GamblersBrew"),
                ("EntropicBrew", "GamblersBrew"),
            )
        return {
            "root_id": self.root_id,
            "exact_combat_state_hash": self.exact_combat_state_hash,
            "terminal_slot_indices": np.asarray([0, 1], dtype=np.uint64),
            "terminal_kind": terminal_kind,
            "terminal_won": np.asarray(self.wins, dtype=np.bool_),
            "terminal_start_hp": np.asarray([80, 80], dtype=np.int32),
            "terminal_final_hp": final_hp,
            "terminal_final_max_hp": np.asarray([80, 80], dtype=np.int32),
            "terminal_final_gold": np.asarray([99, 99], dtype=np.int32),
            "terminal_hp_loss": 80 - final_hp,
            "terminal_enemy_start_hp": np.asarray([40, 40], dtype=np.int32),
            "terminal_enemy_final_hp": enemy_final_hp,
            "terminal_turns": np.asarray([3, 5], dtype=np.uint32),
            "terminal_potions_used": np.asarray(potion_use, dtype=np.uint32),
            "terminal_potions_discarded": np.asarray([0, 0], dtype=np.uint32),
            "terminal_cards_played": np.asarray([8, 12], dtype=np.uint32),
            "terminal_potion_ids": final_potion_ids,
        }


class ExactCombatRootSource:
    """Return fresh groups from a caller-declared sequence of exact roots."""

    def __init__(
        self,
        groups: tuple[tuple[str, str, tuple[bool, bool]], ...],
    ) -> None:
        self.specifications = groups
        self.groups: list[OneRoundCombatGroup] = []
        self.call_count = 0

    def combat_group(
        self,
        slot_index: int,
        replicate_count: int,
        potion_slots: Sequence[int] | None = None,
    ) -> OneRoundCombatGroup:
        if slot_index != 0 or replicate_count != 2:
            raise AssertionError("combat source received different generation bounds")
        try:
            specification = self.specifications[self.call_count]
        except IndexError as error:
            raise AssertionError("combat source was run too many times") from error
        self.call_count += 1
        group = OneRoundCombatGroup(*specification, potion_slots=potion_slots)
        self.groups.append(group)
        return group


def combat_group_experience_fixture(
    manifest_id: BehaviorManifestId,
    *,
    wins: tuple[bool, bool],
    first_probability: SelectionProbability | None = None,
    final_hps: tuple[int, int] | None = None,
    enemy_final_hps: tuple[int, int] | None = None,
    terminal_kinds: tuple[int, int] | None = None,
    potions_used: tuple[int, int] = (0, 1),
) -> CompletedCombatGroupExperience:
    first = CombatDecisionExperienceBatch(
        sequence_index=0,
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        replicate_indices=(0, 1),
        decision_progress=(
            _progress(0, turn=1, player_hp=80, enemy_hp=40),
            _progress(1, turn=1, player_hp=80, enemy_hp=40),
        ),
        payload=semantic_batch_fixture(),
        selected_ordinals=(0, 0),
        selection_probabilities=(
            first_probability
            if first_probability is not None
            else SelectionProbability.known(0.5),
            SelectionProbability.known(1.0 / 3.0),
        ),
        behavior_manifest_id=manifest_id,
        decision_count=2,
        payload_bytes=1,
    )
    second = CombatDecisionExperienceBatch(
        sequence_index=1,
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        replicate_indices=(1,),
        decision_progress=(
            _progress(1, turn=2, player_hp=75, enemy_hp=25),
        ),
        payload=select_semantic_decision_rows(semantic_batch_fixture(), [0]),
        selected_ordinals=(0,),
        selection_probabilities=(SelectionProbability.known(0.5),),
        behavior_manifest_id=manifest_id,
        decision_count=1,
        payload_bytes=1,
    )
    if final_hps is None:
        final_hps = (70 if wins[0] else 0, 10 if wins[1] else 0)
    if enemy_final_hps is None:
        enemy_final_hps = (0 if wins[0] else 20, 0 if wins[1] else 20)
    if terminal_kinds is None:
        terminal_kinds = (0 if wins[0] else 1, 0 if wins[1] else 1)
    outcomes = CompletedCombatGroup(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        outcomes=(
            _outcome(
                0,
                wins[0],
                terminal_kind=terminal_kinds[0],
                final_hp=final_hps[0],
                enemy_final_hp=enemy_final_hps[0],
                potions_used=potions_used[0],
            ),
            _outcome(
                1,
                wins[1],
                terminal_kind=terminal_kinds[1],
                final_hp=final_hps[1],
                enemy_final_hp=enemy_final_hps[1],
                potions_used=potions_used[1],
            ),
        ),
    )
    return CompletedCombatGroupExperience(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        behavior_manifest_id=manifest_id,
        batches=(first, second),
        outcomes=outcomes,
        decision_count=3,
        payload_bytes=2,
    )


def _progress(
    replicate_index: int,
    *,
    turn: int,
    player_hp: int,
    enemy_hp: int,
) -> CombatDecisionProgress:
    return CombatDecisionProgress(
        replicate_index=replicate_index,
        turn=turn,
        player_hp=player_hp,
        player_max_hp=80,
        enemy_hp=enemy_hp,
        enemy_max_hp=40,
        potion_uuids=(101, 102),
        potion_ids=("FearPotion", "GamblersBrew"),
    )


def _outcome(
    replicate_index: int,
    won: bool,
    *,
    terminal_kind: int,
    final_hp: int,
    enemy_final_hp: int,
    potions_used: int,
) -> CombatTerminalOutcome:
    return CombatTerminalOutcome(
        replicate_index=replicate_index,
        terminal_kind=terminal_kind,
        won=won,
        start_hp=80,
        final_hp=final_hp,
        final_max_hp=80,
        final_gold=99,
        hp_loss=80 - final_hp,
        enemy_start_hp=40,
        enemy_final_hp=enemy_final_hp,
        turns=3,
        potions_used=potions_used,
        potions_discarded=0,
        cards_played=8,
        final_potion_ids=(
            ("FearPotion", "GamblersBrew")
            if potions_used == 0
            else (None, "GamblersBrew")
        ),
    )
