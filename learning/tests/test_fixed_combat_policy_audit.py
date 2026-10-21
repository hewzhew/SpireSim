from __future__ import annotations

import json

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from sts_learning.fixed_combat_policy_audit import (  # noqa: E402
    FixedCombatPolicyAuditError,
    FixedPolicyIdentity,
    _replay_decision_prefix,
    compare_fixed_combat_decision,
)
from sts_learning.manifests import (  # noqa: E402
    ManifestArtifactId,
    ManifestArtifactKind,
)
from sts_learning.policy import BehaviorManifestId  # noqa: E402
from sts_learning.combat_potion_lane import CombatPotionLane  # noqa: E402
from sts_learning.torch_policy import RaggedCandidateLogits  # noqa: E402


class _Group:
    root_id = "root-1"
    exact_combat_state_hash = "b" * 64

    def __init__(self, *, candidate_count: int = 2) -> None:
        self.candidate_count = candidate_count
        self.audit_calls = 0
        self.batch_calls = 0

    def combat_decision_audit_json(self, replicate_index: int) -> str:
        assert replicate_index == 0
        self.audit_calls += 1
        return json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_root",
                "selection_prefix": [],
                "candidates": [
                    {
                        "kind": "play_card",
                        "hand_index": 0,
                        "target_monster_index": 1,
                    },
                    {"kind": "end_turn"},
                ],
            }
        )

    def decision_batch(self, *, semantic: bool) -> dict[str, object]:
        assert semantic
        self.batch_calls += 1
        return {
            "candidate_counts": np.asarray([self.candidate_count], dtype=np.uint64),
            "semantic": {"schema_version": 7},
        }


class _Policy:
    def __init__(self, manifest_id: BehaviorManifestId, logits: tuple[float, ...]) -> None:
        self.behavior_manifest_id = manifest_id
        self.logits = logits
        self.score_calls = 0

    def score(self, batch: object) -> RaggedCandidateLogits:
        assert isinstance(batch, dict)
        self.score_calls += 1
        return RaggedCandidateLogits(
            values=torch.tensor(self.logits, dtype=torch.float32),
            row_splits=torch.tensor([0, len(self.logits)], dtype=torch.long),
        )


class _ReplayGroup:
    ready = False
    terminal_count = 0

    def __init__(self) -> None:
        self.state = 0
        self.selected: list[int] = []

    def combat_decision_audit_json(self, replicate_index: int) -> str:
        assert replicate_index == 0
        return json.dumps(
            {
                "schema": "sts-learning-combat-decision-audit-v1",
                "phase": "combat_root",
                "selection_prefix": [],
                "candidates": [
                    {"kind": "end_turn"},
                    {"kind": "play_card", "hand_index": self.state},
                ],
            }
        )

    def choose(self, ordinals: list[int]) -> None:
        assert len(ordinals) == 1
        self.selected.append(ordinals[0])
        self.ready = True

    def step(self) -> None:
        assert self.ready
        self.state += 1
        self.ready = False


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


def test_fixed_combat_policy_audit_compares_every_candidate_without_choice() -> None:
    group = _Group()
    baseline_identity = _identity(1)
    candidate_identity = _identity(2)
    baseline = _Policy(baseline_identity.manifest_id, (2.0, 0.0))
    candidate = _Policy(candidate_identity.manifest_id, (0.0, 2.0))

    result = compare_fixed_combat_decision(
        group,
        baseline,
        candidate,
        baseline_identity=baseline_identity,
        candidate_identity=candidate_identity,
        artifact_sha256="a" * 64,
        expected_roots=3,
        root_slot=1,
        root_audit={"encounter_id": "ThreeSentries", "ascension_level": 20},
        potion_lane=CombatPotionLane.NEVER,
    )

    assert result.baseline.top_ordinal == 0
    assert result.candidate.top_ordinal == 1
    assert result.candidates[0].baseline_rank == 1
    assert result.candidates[0].candidate_rank == 2
    assert result.candidates[0].candidate_probability < 0.5
    assert result.candidates[1].candidate_probability > 0.5
    assert result.candidates[0].candidate_id != result.candidates[1].candidate_id
    assert len(result.decision_id) == 64
    assert len(result.audit_id) == 64
    assert result.source_root_id == "root-1"
    assert result.decision_root_id == "root-1"
    assert result.replay_prefix == ()
    assert group.audit_calls == 1
    assert group.batch_calls == 1
    assert baseline.score_calls == 1
    assert candidate.score_calls == 1

    mapping = result.as_mapping()
    assert mapping["schema"] == "sts-learning-fixed-combat-policy-audit-v1"
    assert mapping["root_slot"] == 1
    assert mapping["decision_root_id"] == "root-1"
    assert mapping["potion_lane"] == "never"
    assert mapping["candidates"][0]["probability_delta"] < 0.0
    assert mapping["candidates"][1]["probability_delta"] > 0.0


def test_fixed_combat_policy_audit_rejects_candidate_surface_misalignment() -> None:
    baseline_identity = _identity(1)
    candidate_identity = _identity(2)

    with pytest.raises(
        FixedCombatPolicyAuditError,
        match="audit candidates disagree",
    ):
        compare_fixed_combat_decision(
            _Group(candidate_count=1),
            _Policy(baseline_identity.manifest_id, (2.0, 0.0)),
            _Policy(candidate_identity.manifest_id, (0.0, 2.0)),
            baseline_identity=baseline_identity,
            candidate_identity=candidate_identity,
            artifact_sha256="a" * 64,
            expected_roots=1,
            root_slot=0,
            root_audit={},
            potion_lane=CombatPotionLane.NEVER,
        )


def test_fixed_combat_policy_audit_rejects_same_manifest() -> None:
    identity = _identity(1)
    with pytest.raises(
        FixedCombatPolicyAuditError,
        match="distinct behavior manifests",
    ):
        compare_fixed_combat_decision(
            _Group(),
            _Policy(identity.manifest_id, (1.0, 0.0)),
            _Policy(identity.manifest_id, (0.0, 1.0)),
            baseline_identity=identity,
            candidate_identity=identity,
            artifact_sha256="a" * 64,
            expected_roots=1,
            root_slot=0,
            root_audit={},
            potion_lane=CombatPotionLane.NEVER,
        )


def test_fixed_combat_policy_audit_replays_an_explicit_decision_prefix() -> None:
    group = _ReplayGroup()

    trace = _replay_decision_prefix(group, (1, 0))

    assert group.state == 2
    assert group.selected == [1, 0]
    assert [step["selected_ordinal"] for step in trace] == [1, 0]
    assert trace[0]["selected_candidate"] == {
        "kind": "play_card",
        "hand_index": 0,
    }
    assert trace[1]["selected_candidate"] == {"kind": "end_turn"}
