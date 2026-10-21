from __future__ import annotations

import json

import pytest

torch = pytest.importorskip("torch")

from sts_learning.combat_policy_divergence_collect import (  # noqa: E402
    _inspect_first_policy_divergence,
    _relevant_divergence_category,
)
from sts_learning.fixed_combat_policy_audit import FixedPolicyIdentity  # noqa: E402
from sts_learning.manifests import (  # noqa: E402
    ManifestArtifactId,
    ManifestArtifactKind,
)
from sts_learning.policy import BehaviorManifestId  # noqa: E402
from sts_learning.torch_policy import RaggedCandidateLogits  # noqa: E402


def _card(card_id: str, damage: int, target: int | None) -> dict[str, object]:
    return {
        "kind": "play_card",
        "hand_index": 0,
        "card": {
            "card_id": card_id,
            "upgrades": 0,
            "current_damage": damage,
            "damage_by_monster_order": [damage, damage],
        },
        "target": (
            None
            if target is None
            else {
                "monster_index": target,
                "slot": target,
                "enemy": {"enemy_id": "LouseNormal", "status": "known"},
            }
        ),
    }


class _Recovery:
    def __init__(self, state: int) -> None:
        self.source_root_id = "source-root"
        self.source_exact_combat_state_hash = "a" * 64
        self.root_id = f"decision-root-{state}"
        self.exact_combat_state_hash = f"{state + 2:02x}" * 32
        self.state = state

    def combat_root_artifact_bytes(self, *, max_bytes: int) -> bytes:
        payload = f"recovery-{self.state}".encode()
        assert len(payload) <= max_bytes
        return payload


class _Group:
    root_id = "source-root"
    exact_combat_state_hash = "a" * 64
    terminal_count = 0

    def __init__(self, decisions: list[list[dict[str, object]]]) -> None:
        self.decisions = decisions
        self.state = 0
        self.ready = False
        self.selected: list[int] = []

    def combat_decision_audit_json(self, replicate_index: int) -> str:
        assert replicate_index == 0
        return json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_root",
                "selection_prefix": [],
                "candidates": self.decisions[self.state],
            }
        )

    def decision_batch(self, *, semantic: bool) -> dict[str, object]:
        assert semantic
        return {
            "candidate_counts": [len(self.decisions[self.state])],
            "state": self.state,
            "semantic": {"schema_version": 7},
        }

    def capture_recovery_root(self, replicate_index: int) -> _Recovery:
        assert replicate_index == 0
        return _Recovery(self.state)

    def choose(self, ordinals: list[int]) -> None:
        assert len(ordinals) == 1
        self.selected.append(ordinals[0])
        self.ready = True

    def step(self) -> None:
        assert self.ready
        self.state += 1
        self.ready = False
        if self.state >= len(self.decisions):
            self.terminal_count = 1


class _SelectionGroup(_Group):
    def combat_decision_audit_json(self, replicate_index: int) -> str:
        assert replicate_index == 0
        return json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_selection" if self.state == 1 else "combat_root",
                "selection_prefix": [] if self.state != 1 else [0],
                "candidates": self.decisions[self.state],
            }
        )

    def capture_recovery_root(self, replicate_index: int) -> _Recovery:
        assert self.state != 1, "selection state is not an artifact root"
        return super().capture_recovery_root(replicate_index)

    def choose(self, ordinals: list[int]) -> None:
        assert len(ordinals) == 1
        self.selected.append(ordinals[0])
        if self.state == 0:
            self.state = 1
            self.ready = False
        else:
            self.ready = True


class _Policy:
    def __init__(
        self,
        manifest_id: BehaviorManifestId,
        logits_by_state: dict[int, tuple[float, ...]],
    ) -> None:
        self.behavior_manifest_id = manifest_id
        self.logits_by_state = logits_by_state

    def score(self, batch: object) -> RaggedCandidateLogits:
        assert isinstance(batch, dict)
        logits = self.logits_by_state[int(batch["state"])]
        return RaggedCandidateLogits(
            values=torch.tensor(logits, dtype=torch.float32),
            row_splits=torch.tensor([0, len(logits)], dtype=torch.long),
        )


def _identity(byte: int) -> FixedPolicyIdentity:
    return FixedPolicyIdentity(
        manifest_id=BehaviorManifestId(bytes([byte]) * 32),
        checkpoint_id=ManifestArtifactId.from_content(
            ManifestArtifactKind.MODEL_CHECKPOINT,
            bytes([byte]),
        ),
        training_step=byte,
        temperature=1.0,
    )


def test_collector_follows_only_shared_actions_then_captures_target_divergence() -> None:
    decisions = [
        [{"kind": "end_turn"}, _card("Defend", 0, None)],
        [_card("Strike", 6, 0), _card("Strike", 6, 1)],
    ]
    group = _Group(decisions)
    baseline_identity = _identity(1)
    candidate_identity = _identity(2)

    row, artifact = _inspect_first_policy_divergence(
        group,
        _Policy(baseline_identity.manifest_id, {0: (2.0, 0.0), 1: (2.0, 0.0)}),
        _Policy(candidate_identity.manifest_id, {0: (3.0, 0.0), 1: (0.0, 2.0)}),
        baseline_identity=baseline_identity,
        candidate_identity=candidate_identity,
        artifact_sha256="b" * 64,
        expected_roots=1,
        root_slot=0,
        root_audit={"encounter_id": "TwoLouse", "ascension_level": 20},
        max_decisions=8,
        max_artifact_bytes=1_024,
    )

    assert artifact == b"recovery-1"
    assert group.selected == [0]
    assert row["status"] == "captured_first_divergence"
    assert row["divergence_category"] == "same_card_profile_different_target"
    assert row["shared_decision_count"] == 1
    audit = row["audit"]
    assert audit["decision_root_id"] == "decision-root-1"
    assert audit["baseline"]["top_ordinal"] == 0
    assert audit["candidate"]["top_ordinal"] == 1
    assert len(audit["replay_prefix"]) == 1
    assert audit["candidates"][0]["semantics"]["target"]["monster_index"] == 0
    assert audit["candidates"][1]["semantics"]["target"]["monster_index"] == 1


def test_collector_rejects_the_root_when_its_first_divergence_is_not_attack_like() -> None:
    group = _Group([[{"kind": "end_turn"}, _card("Defend", 0, None)]])
    baseline_identity = _identity(1)
    candidate_identity = _identity(2)

    row, artifact = _inspect_first_policy_divergence(
        group,
        _Policy(baseline_identity.manifest_id, {0: (2.0, 0.0)}),
        _Policy(candidate_identity.manifest_id, {0: (0.0, 2.0)}),
        baseline_identity=baseline_identity,
        candidate_identity=candidate_identity,
        artifact_sha256="b" * 64,
        expected_roots=1,
        root_slot=0,
        root_audit={"encounter_id": "TwoLouse", "ascension_level": 20},
        max_decisions=8,
        max_artifact_bytes=1_024,
    )

    assert artifact is None
    assert group.selected == []
    assert row["status"] == "rejected_first_divergence"
    assert row["divergence_category"] is None
    assert row["shared_decision_count"] == 0


def test_collector_crosses_a_shared_selection_without_exporting_it_as_a_root() -> None:
    group = _SelectionGroup(
        [
            [{"kind": "selection_family"}, {"kind": "end_turn"}],
            [{"kind": "selection_submit"}, {"kind": "selection_append"}],
            [_card("Strike", 6, 0), _card("Strike", 6, 1)],
        ]
    )
    baseline_identity = _identity(1)
    candidate_identity = _identity(2)

    row, artifact = _inspect_first_policy_divergence(
        group,
        _Policy(
            baseline_identity.manifest_id,
            {0: (2.0, 0.0), 1: (2.0, 0.0), 2: (2.0, 0.0)},
        ),
        _Policy(
            candidate_identity.manifest_id,
            {0: (3.0, 0.0), 1: (3.0, 0.0), 2: (0.0, 2.0)},
        ),
        baseline_identity=baseline_identity,
        candidate_identity=candidate_identity,
        artifact_sha256="b" * 64,
        expected_roots=1,
        root_slot=0,
        root_audit={"encounter_id": "TwoLouse", "ascension_level": 20},
        max_decisions=8,
        max_artifact_bytes=1_024,
    )

    assert artifact == b"recovery-2"
    assert group.selected == [0, 0]
    assert row["status"] == "captured_first_divergence"
    assert row["decision_state_identity_kind"] == "exact_combat_root"
    assert row["shared_decision_count"] == 2
    assert row["audit"]["replay_prefix"][1]["phase"] == "combat_selection"


def test_different_damaging_cards_are_a_relevant_attack_order_divergence() -> None:
    assert (
        _relevant_divergence_category(
            _card("Strike", 6, 0),
            _card("Bash", 8, 0),
        )
        == "damaging_card_vs_damaging_card"
    )
    assert _relevant_divergence_category(_card("Defend", 0, None), {"kind": "end_turn"}) is None
