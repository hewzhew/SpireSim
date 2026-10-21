from __future__ import annotations

import json
from pathlib import Path

from sts_learning import (
    CompleteTurnCapturedCombat,
    CompleteTurnRunCapture,
    ReplayVerifiedCompleteTurn,
    aggregate_complete_turn_teacher_carriage,
)


class _FakeCompleteTurnEnv:
    def __init__(self) -> None:
        self.depth = 0
        self.lost_teacher = False
        self.endpoint_reserved: list[int] = []
        self.is_terminal = False
        self.terminal = "unresolved"

    def prefix_layer_batch(self) -> dict[str, object] | None:
        if self.depth >= 3:
            return None
        return {
            "atomic_depth": self.depth + 1,
            "max_retained_states": 1,
            "candidate_counts": [3],
        }

    def matching_prefix_successor_ordinal(self, inputs: bytes) -> int | None:
        assert inputs in (b"[1]", b"[1,2]")
        if self.lost_teacher:
            return None
        return (1, 0)[self.depth]

    def retain_prefix_layer_with_default_fill(self, ordinals: list[int]) -> None:
        expected = (1, 0, None)[self.depth]
        if expected is not None and expected not in ordinals:
            self.lost_teacher = True
        self.depth += 1

    def prefix_layer_public_view_json(self) -> bytes:
        return json.dumps(
            {
                "schema": "complete_turn_prefix_layer_view_v1",
                "atomic_depth": self.depth + 1,
                "candidates": [{"ordinal": ordinal} for ordinal in range(3)],
            }
        ).encode()

    def completed_endpoint_batch(self) -> dict[str, object]:
        return {"max_retained_states": 2, "candidate_counts": [4]}

    def matching_completed_endpoint_successor_ordinal(self, inputs: bytes) -> int:
        assert inputs == b"[1,2,3]"
        return 2

    def completed_endpoint_public_view_json(self) -> bytes:
        return json.dumps(
            {
                "schema": "complete_turn_completed_endpoint_layer_view_v1",
                "candidates": [{"ordinal": ordinal} for ordinal in range(4)],
            }
        ).encode()

    def retain_completed_endpoints_with_default_fill(self, ordinals: list[int]) -> None:
        self.endpoint_reserved = ordinals

    def decision_batch(self) -> dict[str, object]:
        return {"candidate_counts": [2]}

    def matching_successor_ordinal(self, inputs: bytes) -> int | None:
        assert inputs == b"[1,2,3]"
        return 0 if 2 in self.endpoint_reserved else None

    def choose(self, ordinal: int) -> None:
        assert ordinal == 0
        self.is_terminal = True
        self.terminal = "win"


def _capture() -> CompleteTurnRunCapture:
    turn = ReplayVerifiedCompleteTurn(
        from_turn=1,
        to_turn=1,
        boundary="terminal_win",
        inputs_json=b"[1,2,3]",
        input_prefixes_json=(b"[1]", b"[1,2]", b"[1,2,3]"),
    )
    combat = CompleteTurnCapturedCombat(
        ordinal=0,
        act=1,
        floor=1,
        encounter_id="Cultist",
        public_snapshot_id="snapshot",
        root_exact_state_hash="root",
        run_session_fingerprint="session",
        case_path=Path("combat.case.json"),
        actions_path=Path("combat.actions.json"),
        case_payload=b"case",
        actions_payload=b"actions",
        action_count=3,
        turns=(turn,),
    )
    return CompleteTurnRunCapture(
        manifest_path=Path("manifest.json"),
        source_receipt_path=Path("receipt.json"),
        seed=7,
        episode_outcome={"status": "success"},
        curriculum_eligible=True,
        curriculum_ineligibility=None,
        captured_combat_entries=1,
        committed_combat_actions=3,
        actor_installed_plans=1,
        actor_plan_actions=3,
        combats=(combat,),
    )


def _factory(payload: bytes, **_: object) -> _FakeCompleteTurnEnv:
    assert payload == b"case"
    return _FakeCompleteTurnEnv()


def test_teacher_reservations_carry_the_exact_win_without_negative_labels() -> None:
    result = aggregate_complete_turn_teacher_carriage(
        _capture(), environment_factory=_factory
    )

    assert result.all_combats_closed
    assert result.completed_combat_ordinals == (0,)
    assert [row.stage for row in result.rows] == [
        "prefix",
        "prefix",
        "prefix",
        "completed_endpoint",
    ]
    assert [row.positive_was_reserved for row in result.rows] == [
        True,
        True,
        None,
        True,
    ]


def test_learner_induced_prefix_loss_retains_the_positive_failure_row() -> None:
    result = aggregate_complete_turn_teacher_carriage(
        _capture(),
        environment_factory=_factory,
        reservation_policy=lambda _stage, _batch: [],
    )

    assert result.learner_controlled
    assert result.completed_combat_ordinals == ()
    assert len(result.rows) == 1
    assert result.rows[0].positive_ordinal == 1
    assert result.rows[0].positive_was_reserved is False
    assert len(result.failures) == 1
    assert result.failures[0].stage == "prefix"
    assert result.failures[0].atomic_depth == 2


def test_public_views_are_opt_in_and_align_with_positive_rows() -> None:
    result = aggregate_complete_turn_teacher_carriage(
        _capture(),
        environment_factory=_factory,
        include_public_views=True,
    )

    positive_rows = [row for row in result.rows if row.positive_ordinal is not None]
    assert all(row.public_view is not None for row in positive_rows)
    assert positive_rows[0].public_view == {
        "schema": "complete_turn_prefix_layer_view_v1",
        "atomic_depth": 1,
        "candidates": [{"ordinal": 0}, {"ordinal": 1}, {"ordinal": 2}],
    }
