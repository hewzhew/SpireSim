from __future__ import annotations

from dataclasses import replace

import numpy as np

from learning.tests.semantic_fixtures import semantic_batch_fixture
from sts_learning import (
    AttemptKey,
    BehaviorManifest,
    BehaviorManifestId,
    BehaviorManifestTemplate,
    BehaviorRuleBinding,
    CompletedAttemptExperience,
    DETERMINISTIC_SELECTION,
    DecisionRunProgress,
    DecisionExperienceBatch,
    DecisionLineage,
    ManifestArtifactId,
    ManifestArtifactKind,
    GREEDY_BEHAVIOR_RULE_V1,
    PublicAttemptTrajectoryV1,
    PublicDecisionSnapshot,
    SelectionProbability,
    TerminalAttemptOutcome,
    TerminalAttemptRecord,
    build_public_attempt_trajectory,
    select_semantic_decision_rows,
)


def behavior_manifest_template_fixture(
    *,
    semantic_schema_version: int = 2,
    behavior_rule: BehaviorRuleBinding = GREEDY_BEHAVIOR_RULE_V1,
    trainer_implementation: ManifestArtifactId | None = None,
) -> BehaviorManifestTemplate:
    def artifact(kind: ManifestArtifactKind) -> ManifestArtifactId:
        return ManifestArtifactId(kind, bytes([int(kind)]) * 32)

    return BehaviorManifestTemplate(
        model_definition=artifact(ManifestArtifactKind.MODEL_DEFINITION),
        model_config=artifact(ManifestArtifactKind.MODEL_CONFIG),
        behavior_rule=behavior_rule,
        semantic_schema=artifact(ManifestArtifactKind.SEMANTIC_SCHEMA),
        optimizer_config=artifact(ManifestArtifactKind.OPTIMIZER_CONFIG),
        trainer_implementation=(
            trainer_implementation
            if trainer_implementation is not None
            else artifact(ManifestArtifactKind.TRAINER_IMPLEMENTATION)
        ),
        semantic_schema_version=semantic_schema_version,
    )


def behavior_manifest_fixture(
    *,
    behavior_rule: BehaviorRuleBinding = GREEDY_BEHAVIOR_RULE_V1,
) -> BehaviorManifest:
    return behavior_manifest_template_fixture(behavior_rule=behavior_rule).bind(
        ManifestArtifactId(
            ManifestArtifactKind.MODEL_CHECKPOINT,
            bytes([int(ManifestArtifactKind.MODEL_CHECKPOINT)]) * 32,
        ),
        training_step=0,
    )


def decision_batch_fixture(
    *,
    slot: int,
    semantic_row: int,
    selected_ordinal: int,
    manifest_id: BehaviorManifestId,
    selection_probability: SelectionProbability = DETERMINISTIC_SELECTION,
) -> DecisionExperienceBatch:
    lineage = DecisionLineage(
        key=AttemptKey(
            slot_index=slot,
            episode_seed=100 + slot,
            episode_generation=0,
            attempt_index=1,
        ),
        recoveries_used=0,
    )
    payload = dict(
        select_semantic_decision_rows(
            semantic_batch_fixture(),
            [semantic_row],
        )
    )
    payload["slot_indices"] = np.array([slot], dtype=np.uint64)
    return DecisionExperienceBatch(
        payload=payload,
        lineages=(lineage,),
        selected_ordinals=(selected_ordinal,),
        selection_probabilities=(selection_probability,),
        behavior_manifest_id=manifest_id,
        decision_count=1,
        payload_bytes=1,
    )


def completed_attempt_fixture(
    *,
    slot: int,
    batches: tuple[DecisionExperienceBatch, ...],
    reward: int,
) -> CompletedAttemptExperience:
    lineage = batches[0].lineages[0]
    terminal = TerminalAttemptRecord(
        episode_seed=lineage.key.episode_seed,
        episode_generation=lineage.key.episode_generation,
        attempt_index=lineage.key.attempt_index,
        recoveries_used=lineage.recoveries_used,
        terminal=TerminalAttemptOutcome(
            slot_index=slot,
            terminal_reward=reward,
            terminal_act=3,
            terminal_floor=40,
            terminal_hp=20 if reward == 1 else 0,
            terminal_max_hp=80,
            terminal_gold=50,
        ),
    )
    return CompletedAttemptExperience(
        lineage=lineage,
        batches=batches,
        terminal=terminal,
        decision_count=sum(batch.decision_count for batch in batches),
        payload_bytes=sum(batch.payload_bytes for batch in batches),
    )


def public_snapshot_fixture(
    batch: DecisionExperienceBatch,
    *,
    is_combat: bool,
    identity_suffix: str = "fixture",
) -> PublicDecisionSnapshot:
    """Build one sanitized public identity aligned to a fixture payload row."""

    phase = int(np.asarray(batch.payload["phase"]).reshape(-1)[0])
    candidate_count = int(
        np.asarray(batch.payload["candidate_counts"]).reshape(-1)[0]
    )
    slot = batch.lineages[0].key.slot_index
    identity = f"slot-{slot}-{identity_suffix}"
    return PublicDecisionSnapshot(
        phase=phase,
        is_combat=is_combat,
        snapshot_id=f"snapshot-{identity}",
        observation_id=f"observation-{identity}",
        history_snapshot_id=f"history-{identity}",
        candidate_surface_id=f"surface-{identity}",
        candidate_ids=tuple(
            f"candidate-{identity}-{ordinal}"
            for ordinal in range(candidate_count)
        ),
    )


def with_run_progress_fixture(
    batch: DecisionExperienceBatch,
    *,
    act: int,
    floor: int,
    is_combat: bool,
    strategic_context_kind: int | None,
    identity_suffix: str = "fixture",
) -> DecisionExperienceBatch:
    """Attach typed progress and a matching public snapshot to one fixture row."""

    return replace(
        batch,
        run_progress=(
            DecisionRunProgress(
                episode_seed=batch.lineages[0].key.episode_seed,
                act=act,
                floor=floor,
                is_combat=is_combat,
                strategic_context_kind=strategic_context_kind,
                public_snapshot=public_snapshot_fixture(
                    batch,
                    is_combat=is_combat,
                    identity_suffix=identity_suffix,
                ),
            ),
        ),
    )


def public_attempt_trajectory_fixture(
    attempt: CompletedAttemptExperience,
) -> PublicAttemptTrajectoryV1:
    return build_public_attempt_trajectory(attempt)
