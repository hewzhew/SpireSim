from __future__ import annotations

import json

import pytest

torch = pytest.importorskip("torch")

from learning.tests.semantic_fixtures import (  # noqa: E402
    semantic_batch_fixture,
    semantic_schema_fixture,
)
from sts_learning.combat_search_distillation_candidate import (  # noqa: E402
    COMBAT_SEARCH_DISTILLATION_CANDIDATE_FILENAME,
    CombatSearchDistillationCandidateError,
    publish_combat_search_distillation_candidate,
    recover_combat_search_distillation_candidate,
)
from sts_learning.combat_search_distillation_spike import (  # noqa: E402
    fit_combat_search_distillation_scorer,
)
from sts_learning.policy import BehaviorManifestId  # noqa: E402
from sts_learning.published_combat_behavior import (  # noqa: E402
    PublishedCombatBehaviorError,
    recover_compatible_combat_scorer,
)
from sts_learning.torch_combat_session_config import (  # noqa: E402
    CombatSessionBridge,
    CombatWinSessionLimits,
)
from sts_learning.torch_policy import (  # noqa: E402
    RaggedCandidateScorer,
    RaggedScorerConfig,
    ragged_cross_entropy,
)
from sts_learning.torch_provenance import (  # noqa: E402
    COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS,
    COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
)
from sts_learning.semantic_batch import select_semantic_decision_rows  # noqa: E402
from sts_learning.train_combat_search_candidate import (  # noqa: E402
    _parse_args as parse_candidate_training_args,
)
from sts_learning.evaluate_combat_search_candidate import (  # noqa: E402
    _parse_args as parse_candidate_evaluation_args,
)


def _bridge() -> CombatSessionBridge:
    return CombatSessionBridge(
        combat_roots_from_artifact=lambda *args, **kwargs: None,
        semantic_schema=semantic_schema_fixture(),
    )


def _scorer(bridge: CombatSessionBridge) -> RaggedCandidateScorer:
    torch.manual_seed(928)
    scorer = RaggedCandidateScorer.from_bridge_schema(
        bridge.semantic_schema,
        RaggedScorerConfig(hidden_dim=8, relation_layers=1),
    )
    scorer.eval()
    scorer.requires_grad_(False)
    return scorer


def test_candidate_round_trip_is_exact_and_not_a_production_publication(
    tmp_path,
) -> None:
    bridge = _bridge()
    limits = CombatWinSessionLimits()
    scorer = _scorer(bridge)
    batch = semantic_batch_fixture()
    with torch.inference_mode():
        expected = scorer(batch)
        expected_values = expected.values.detach().clone()
        expected_ordinals = expected.greedy_ordinals()

    root = tmp_path / "candidate"
    receipt = publish_combat_search_distillation_candidate(
        root,
        scorer,
        bridge,
        limits,
        source_manifest_id=BehaviorManifestId(bytes.fromhex("11" * 32)),
        training_corpus_sha256="22" * 32,
        training_root_count=39,
        training_proposal_count=15,
        epochs=16,
        learning_rate=3e-4,
        max_grad_norm=1.0,
    )
    recovered = recover_combat_search_distillation_candidate(root, bridge, limits)
    with torch.inference_mode():
        actual = recovered.scorer(batch)

    assert torch.equal(actual.values, expected_values)
    assert actual.greedy_ordinals() == expected_ordinals
    assert recovered.candidate_id == receipt["candidate_id"]
    assert recovered.manifest_id.digest.hex() == receipt["manifest_id"]
    assert recovered.checkpoint_id.digest.hex() == receipt["checkpoint_id"]
    assert recovered.training_root_count == 39
    assert recovered.training_proposal_count == 15
    assert recovered.loss == COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS
    assert not (root / "training.jsonl").exists()
    with pytest.raises(
        PublishedCombatBehaviorError,
        match="missing training.jsonl",
    ):
        recover_compatible_combat_scorer(root, bridge, limits)


def test_candidate_receipt_cannot_claim_production_authority(tmp_path) -> None:
    bridge = _bridge()
    limits = CombatWinSessionLimits()
    root = tmp_path / "candidate"
    publish_combat_search_distillation_candidate(
        root,
        _scorer(bridge),
        bridge,
        limits,
        source_manifest_id=BehaviorManifestId(bytes.fromhex("33" * 32)),
        training_corpus_sha256="44" * 32,
        training_root_count=4,
        training_proposal_count=1,
        epochs=2,
        learning_rate=1e-3,
        max_grad_norm=0.5,
    )
    path = root / COMBAT_SEARCH_DISTILLATION_CANDIDATE_FILENAME
    payload = json.loads(path.read_text(encoding="utf-8"))
    payload["production_eligible"] = True
    path.write_text(
        json.dumps(payload, separators=(",", ":"), sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )

    with pytest.raises(
        CombatSearchDistillationCandidateError,
        match="unsupported authority",
    ):
        recover_combat_search_distillation_candidate(root, bridge, limits)


def test_distillation_fit_is_one_bounded_frozen_update() -> None:
    bridge = _bridge()
    limits = CombatWinSessionLimits()
    anchor = _scorer(bridge)
    fixture = semantic_batch_fixture()
    first = select_semantic_decision_rows(fixture, [0])
    second = select_semantic_decision_rows(fixture, [1])
    records = (
        {
            "batch": first,
            "baseline_ordinal": 0,
            "proposal_ordinal": 1,
        },
        {
            "batch": second,
            "baseline_ordinal": 0,
            "proposal_ordinal": 2,
        },
    )
    training = {"records": records, "proposal_records": records}
    with torch.inference_mode():
        before = anchor(fixture).values.detach().clone()

    fitted, optimizer, losses, gradient_norms, loss_components = (
        fit_combat_search_distillation_scorer(
            anchor,
            training,
            limits,
            epochs=1,
            learning_rate=3e-4,
            max_grad_norm=1.0,
        )
    )
    with torch.inference_mode():
        after = fitted(fixture).values

    assert optimizer.learning_rate == 3e-4
    assert len(losses) == len(gradient_norms) == 1
    assert len(loss_components) == 1
    assert torch.isfinite(torch.tensor(losses + gradient_norms)).all()
    assert not torch.equal(after, before)
    assert not fitted.training
    assert not any(parameter.requires_grad for parameter in fitted.parameters())
    assert not anchor.training
    assert not any(parameter.requires_grad for parameter in anchor.parameters())


def test_legacy_candidate_loss_identity_still_round_trips(tmp_path) -> None:
    bridge = _bridge()
    limits = CombatWinSessionLimits()
    root = tmp_path / "legacy-candidate"

    publish_combat_search_distillation_candidate(
        root,
        _scorer(bridge),
        bridge,
        limits,
        source_manifest_id=BehaviorManifestId(bytes.fromhex("55" * 32)),
        training_corpus_sha256="66" * 32,
        training_root_count=4,
        training_proposal_count=1,
        epochs=1,
        learning_rate=3e-4,
        max_grad_norm=1.0,
        loss=COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS,
    )

    recovered = recover_combat_search_distillation_candidate(root, bridge, limits)

    assert recovered.loss == COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS


def test_sparse_proposals_are_not_diluted_by_retained_baseline_rows() -> None:
    bridge = _bridge()
    limits = CombatWinSessionLimits()
    anchor = _scorer(bridge)
    fixture = semantic_batch_fixture()
    proposal_batch = select_semantic_decision_rows(fixture, [0])
    retained_batch = select_semantic_decision_rows(fixture, [1])
    with torch.inference_mode():
        proposal_greedy = anchor(proposal_batch).greedy_ordinals()[0]
        retained_greedy = anchor(retained_batch).greedy_ordinals()[0]
    proposal_target = 1 if proposal_greedy != 1 else 0
    proposal_record = {
        "batch": proposal_batch,
        "baseline_ordinal": proposal_greedy,
        "proposal_ordinal": proposal_target,
    }
    retained_record = {
        "batch": retained_batch,
        "baseline_ordinal": retained_greedy,
        "proposal_ordinal": None,
    }
    training = {
        "records": (proposal_record,) + (retained_record,) * 6,
        "proposal_records": (proposal_record,),
    }
    with torch.inference_mode():
        before = ragged_cross_entropy(
            anchor(proposal_batch),
            (proposal_target,),
        ).item()

    fitted, _, _, _, components = fit_combat_search_distillation_scorer(
        anchor,
        training,
        limits,
        epochs=1,
        learning_rate=3e-4,
        max_grad_norm=1.0,
    )
    with torch.inference_mode():
        after = ragged_cross_entropy(
            fitted(proposal_batch),
            (proposal_target,),
        ).item()

    assert after < before
    assert abs(components[0]["retained_forward_kl_before_step"]) < 1e-7
    assert components[0]["retained_forward_kl_after_step"] > 0.0


def test_candidate_training_defaults_to_one_step_without_held_out_inputs() -> None:
    arguments = parse_candidate_training_args(
        [
            "--training-artifact",
            "train.bin",
            "--training-search",
            "search/manifest.json",
            "--behavior",
            "baseline",
            "--candidate-output",
            "candidate",
            "--output",
            "result.json",
        ]
    )

    assert arguments.epochs == 1
    assert arguments.learning_rate == 3e-4
    assert not hasattr(arguments, "held_out_artifact")


def test_candidate_evaluation_payload_bound_is_explicit() -> None:
    default = parse_candidate_evaluation_args(
        [
            "--artifact",
            "roots.bin",
            "--roots",
            "2",
            "--baseline-behavior",
            "baseline",
            "--candidate",
            "candidate",
            "--output",
            "result.json",
        ]
    )
    overridden = parse_candidate_evaluation_args(
        [
            "--artifact",
            "roots.bin",
            "--roots",
            "2",
            "--baseline-behavior",
            "baseline",
            "--candidate",
            "candidate",
            "--output",
            "result.json",
            "--max-experience-payload-bytes",
            "268435456",
        ]
    )

    assert default.max_experience_payload_bytes == 64 * 1024 * 1024
    assert overridden.max_experience_payload_bytes == 256 * 1024 * 1024
