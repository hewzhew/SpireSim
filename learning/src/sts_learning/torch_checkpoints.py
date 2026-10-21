"""Deterministic tensor-only PyTorch checkpoints with a bounded file store."""

from __future__ import annotations

import operator
import os
from collections.abc import Callable
from dataclasses import dataclass, field

from torch import nn

from ._content_store import (
    BoundedContentStore,
    ContentStoreError,
    ContentStoreLimits,
    PreparedContent,
)
from ._torch_checkpoint_codec import (
    TorchCheckpointError,
    decode_state_dict,
    encode_state_dict,
    validate_compatible_state,
)
from .manifests import ManifestArtifactId, ManifestArtifactKind


@dataclass(frozen=True)
class TorchCheckpointLimits:
    max_checkpoints: int
    max_bytes_per_checkpoint: int
    max_total_bytes: int

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "max_checkpoints",
            _positive_integer(self.max_checkpoints, "max_checkpoints"),
        )
        object.__setattr__(
            self,
            "max_bytes_per_checkpoint",
            _positive_integer(
                self.max_bytes_per_checkpoint,
                "max_bytes_per_checkpoint",
            ),
        )
        object.__setattr__(
            self,
            "max_total_bytes",
            _positive_integer(self.max_total_bytes, "max_total_bytes"),
        )
        if self.max_bytes_per_checkpoint > self.max_total_bytes:
            raise TorchCheckpointError(
                "max_bytes_per_checkpoint cannot exceed max_total_bytes"
            )

    def _content_limits(self) -> ContentStoreLimits:
        return ContentStoreLimits(
            max_artifacts=self.max_checkpoints,
            max_bytes_per_artifact=self.max_bytes_per_checkpoint,
            max_total_bytes=self.max_total_bytes,
        )


@dataclass(frozen=True)
class PreparedTorchCheckpoint:
    artifact_id: ManifestArtifactId
    payload_bytes: int
    _content: PreparedContent = field(repr=False, compare=False)

    def __post_init__(self) -> None:
        if (
            not isinstance(self.artifact_id, ManifestArtifactId)
            or self.artifact_id.kind is not ManifestArtifactKind.MODEL_CHECKPOINT
        ):
            raise TorchCheckpointError(
                "prepared checkpoint requires a MODEL_CHECKPOINT artifact id"
            )
        if not isinstance(self._content, PreparedContent):
            raise TorchCheckpointError("prepared checkpoint content must be typed")
        if self.payload_bytes != self._content.payload_bytes:
            raise TorchCheckpointError("prepared checkpoint byte count is incorrect")
        if self.artifact_id.digest != self._content.digest:
            raise TorchCheckpointError("prepared checkpoint digest is incorrect")


@dataclass(frozen=True)
class TorchCheckpointStoreSnapshot:
    checkpoints: int
    total_bytes: int
    max_checkpoints: int
    max_total_bytes: int


class BoundedTorchCheckpointStore:
    """No-eviction content store for exact, tensor-only model checkpoints."""

    def __init__(self, root: str | os.PathLike[str], limits: TorchCheckpointLimits):
        if not isinstance(limits, TorchCheckpointLimits):
            raise TorchCheckpointError("checkpoint store limits must be typed")
        self.limits = limits
        try:
            self._store = BoundedContentStore(
                root,
                suffix=".ststorch",
                limits=limits._content_limits(),
                validate_payload=decode_state_dict,
            )
        except ContentStoreError as error:
            raise TorchCheckpointError(str(error)) from error
        self.root = self._store.root

    @property
    def snapshot(self) -> TorchCheckpointStoreSnapshot:
        snapshot = self._store.snapshot
        return TorchCheckpointStoreSnapshot(
            checkpoints=snapshot.artifacts,
            total_bytes=snapshot.total_bytes,
            max_checkpoints=snapshot.max_artifacts,
            max_total_bytes=snapshot.max_total_bytes,
        )

    def prepare(self, model: nn.Module) -> PreparedTorchCheckpoint:
        if not isinstance(model, nn.Module):
            raise TorchCheckpointError("checkpoint source must be a torch Module")
        payload = encode_state_dict(
            model.state_dict(),
            max_bytes=self.limits.max_bytes_per_checkpoint,
        )
        try:
            content = self._store.prepare(payload)
        except ContentStoreError as error:
            raise TorchCheckpointError(str(error)) from error
        artifact_id = ManifestArtifactId(
            ManifestArtifactKind.MODEL_CHECKPOINT,
            content.digest,
        )
        return PreparedTorchCheckpoint(artifact_id, content.payload_bytes, content)

    def preview_commit(
        self,
        prepared: PreparedTorchCheckpoint,
    ) -> ManifestArtifactId:
        if not isinstance(prepared, PreparedTorchCheckpoint):
            raise TorchCheckpointError("checkpoint commit must be prepared")
        try:
            digest = self._store.preview_commit(prepared._content)
        except ContentStoreError as error:
            raise TorchCheckpointError(str(error)) from error
        if digest != prepared.artifact_id.digest:
            raise TorchCheckpointError("checkpoint preview returned a different identity")
        return prepared.artifact_id

    def preview_novel_commit(self, prepared: PreparedTorchCheckpoint) -> None:
        """Require capacity for one additional checkpoint of this exact size."""

        if not isinstance(prepared, PreparedTorchCheckpoint):
            raise TorchCheckpointError("checkpoint commit must be prepared")
        try:
            self._store.preview_novel_commit(prepared._content)
        except ContentStoreError as error:
            raise TorchCheckpointError(str(error)) from error

    def commit(self, prepared: PreparedTorchCheckpoint) -> ManifestArtifactId:
        self.preview_commit(prepared)
        try:
            digest = self._store.commit(prepared._content)
        except ContentStoreError as error:
            raise TorchCheckpointError(str(error)) from error
        if digest != prepared.artifact_id.digest:
            raise TorchCheckpointError("checkpoint store committed a different identity")
        return prepared.artifact_id

    def materialize(
        self,
        artifact_id: ManifestArtifactId,
        factory: Callable[[], nn.Module],
    ) -> nn.Module:
        """Restore into a fresh model so an existing scorer cannot be half-mutated."""

        if not callable(factory):
            raise TorchCheckpointError("checkpoint model factory must be callable")
        payload = self._read_verified(artifact_id)
        state = decode_state_dict(payload)
        model = factory()
        if not isinstance(model, nn.Module):
            raise TorchCheckpointError("checkpoint factory did not return a torch Module")
        validate_compatible_state(model.state_dict(), state)
        model.load_state_dict(state, strict=True)
        restored = self.prepare(model)
        if restored.artifact_id != artifact_id:
            raise TorchCheckpointError("restored model does not reproduce checkpoint digest")
        return model

    def _read_verified(self, artifact_id: ManifestArtifactId) -> bytes:
        if (
            not isinstance(artifact_id, ManifestArtifactId)
            or artifact_id.kind is not ManifestArtifactKind.MODEL_CHECKPOINT
        ):
            raise TorchCheckpointError("checkpoint lookup id must be typed")
        try:
            return self._store.read(artifact_id.digest)
        except ContentStoreError as error:
            if "unknown content identity" in str(error):
                raise TorchCheckpointError("unknown model checkpoint identity") from error
            raise TorchCheckpointError(str(error)) from error


def _positive_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchCheckpointError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchCheckpointError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise TorchCheckpointError(f"{name} must be positive")
    return normalized
