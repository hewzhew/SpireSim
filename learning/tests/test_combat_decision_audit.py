from __future__ import annotations

import json

import pytest

from sts_learning.combat_decision_audit import (
    CombatDecisionAuditError,
    read_combat_decision_audit,
)


class _Env:
    def __init__(self, payload: object) -> None:
        self.payload = payload
        self.calls: list[int] = []

    def combat_decision_audit_json(self, replicate_index: int) -> object:
        self.calls.append(replicate_index)
        return self.payload


def test_combat_decision_audit_preserves_typed_candidate_order() -> None:
    env = _Env(
        json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_root",
                "selection_prefix": [],
                "candidates": [
                    {
                        "kind": "play_card",
                        "hand_index": 2,
                        "target_monster_index": 1,
                    },
                    {"kind": "end_turn"},
                ],
            }
        )
    )

    audit = read_combat_decision_audit(env, 3)

    assert audit is not None
    assert audit.phase == "combat_root"
    assert audit.selection_prefix == ()
    assert audit.candidates == (
        {
            "kind": "play_card",
            "hand_index": 2,
            "target_monster_index": 1,
        },
        {"kind": "end_turn"},
    )
    assert env.calls == [3]


def test_combat_decision_audit_accepts_selection_prefix_and_null() -> None:
    selection = _Env(
        json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_selection",
                "selection_prefix": [4],
                "candidates": [
                    {"kind": "selection_submit"},
                    {
                        "kind": "selection_append",
                        "domain_index": 2,
                        "domain": {"kind": "card", "ordinal": 7},
                    },
                ],
            }
        )
    )
    audit = read_combat_decision_audit(selection, 0)
    assert audit is not None
    assert audit.phase == "combat_selection"
    assert audit.selection_prefix == (4,)
    assert read_combat_decision_audit(_Env(None), 0) is None


@pytest.mark.parametrize(
    "payload",
    (
        "",
        "not-json",
        json.dumps({"schema": "unknown"}),
        json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_root",
                "selection_prefix": [1],
                "candidates": [{"kind": "end_turn"}],
            }
        ),
        json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_root",
                "selection_prefix": [],
                "candidates": [],
            }
        ),
    ),
)
def test_combat_decision_audit_rejects_malformed_payloads(payload: str) -> None:
    with pytest.raises(CombatDecisionAuditError):
        read_combat_decision_audit(_Env(payload), 0)
