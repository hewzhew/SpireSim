"""Durable bounded catalog for canonical behavior manifest payloads."""

from __future__ import annotations

import operator
import os
from dataclasses import dataclass, field

from ._content_store import (
    BoundedContentStore,
    ContentStoreError,
    ContentStoreLimits,
    PreparedContent,
)
from .manifests import BehaviorManifest, BehaviorManifestRegistry
from .policy import BehaviorManifestId


class BehaviorManifestCatalogError(RuntimeError):
    """A durable manifest catalog operation is unsafe or inconsistent."""


@dataclass(frozen=True)
class BehaviorManifestCatalogLimits:
    max_manifests: int
    max_bytes_per_manifest: int
    max_total_bytes: int

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "max_manifests",
            _positive(self.max_manifests, "max_manifests"),
        )
        object.__setattr__(
            self,
            "max_bytes_per_manifest",
            _positive(
                self.max_bytes_per_manifest,
                "max_bytes_per_manifest",
            ),
        )
        object.__setattr__(
            self,
            "max_total_bytes",
            _positive(self.max_total_bytes, "max_total_bytes"),
        )
        if self.max_bytes_per_manifest > self.max_total_bytes:
            raise BehaviorManifestCatalogError(
                "max_bytes_per_manifest cannot exceed max_total_bytes"
            )

    def _content_limits(self) -> ContentStoreLimits:
        return ContentStoreLimits(
            max_artifacts=self.max_manifests,
            max_bytes_per_artifact=self.max_bytes_per_manifest,
            max_total_bytes=self.max_total_bytes,
        )


@dataclass(frozen=True)
class PreparedBehaviorManifest:
    manifest_id: BehaviorManifestId
    manifest: BehaviorManifest
    payload_bytes: int
    _content: PreparedContent = field(repr=False, compare=False)

    def __post_init__(self) -> None:
        if not isinstance(self.manifest_id, BehaviorManifestId):
            raise BehaviorManifestCatalogError("prepared manifest id must be typed")
        if not isinstance(self.manifest, BehaviorManifest):
            raise BehaviorManifestCatalogError("prepared manifest content must be typed")
        if not isinstance(self._content, PreparedContent):
            raise BehaviorManifestCatalogError("prepared manifest payload must be typed")
        if self.manifest.identity != self.manifest_id:
            raise BehaviorManifestCatalogError(
                "prepared manifest id conflicts with content"
            )
        if self.manifest_id.digest != self._content.digest:
            raise BehaviorManifestCatalogError(
                "prepared manifest payload digest is incorrect"
            )
        if self.payload_bytes != self._content.payload_bytes:
            raise BehaviorManifestCatalogError(
                "prepared manifest payload byte count is incorrect"
            )


@dataclass(frozen=True)
class BehaviorManifestCatalogSnapshot:
    manifests: int
    total_bytes: int
    max_manifests: int
    max_total_bytes: int


class BoundedBehaviorManifestCatalog:
    """No-eviction durable owner for exact behavior manifests."""

    def __init__(
        self,
        root: str | os.PathLike[str],
        limits: BehaviorManifestCatalogLimits,
    ) -> None:
        if not isinstance(limits, BehaviorManifestCatalogLimits):
            raise BehaviorManifestCatalogError("manifest catalog limits must be typed")
        self.limits = limits
        try:
            self._store = BoundedContentStore(
                root,
                suffix=".stsmanifest",
                limits=limits._content_limits(),
                validate_payload=BehaviorManifest.from_bytes,
            )
        except ContentStoreError as error:
            raise BehaviorManifestCatalogError(str(error)) from error
        self.root = self._store.root

    @property
    def snapshot(self) -> BehaviorManifestCatalogSnapshot:
        snapshot = self._store.snapshot
        return BehaviorManifestCatalogSnapshot(
            manifests=snapshot.artifacts,
            total_bytes=snapshot.total_bytes,
            max_manifests=snapshot.max_artifacts,
            max_total_bytes=snapshot.max_total_bytes,
        )

    @property
    def manifest_ids(self) -> tuple[BehaviorManifestId, ...]:
        return tuple(BehaviorManifestId(digest) for digest in self._store.digests)

    def prepare(self, manifest: BehaviorManifest) -> PreparedBehaviorManifest:
        if not isinstance(manifest, BehaviorManifest):
            raise BehaviorManifestCatalogError(
                "manifest catalog accepts only BehaviorManifest values"
            )
        try:
            content = self._store.prepare(manifest.to_bytes())
        except ContentStoreError as error:
            raise BehaviorManifestCatalogError(str(error)) from error
        return PreparedBehaviorManifest(
            manifest_id=manifest.identity,
            manifest=manifest,
            payload_bytes=content.payload_bytes,
            _content=content,
        )

    def preview_commit(
        self,
        prepared: PreparedBehaviorManifest,
    ) -> BehaviorManifestId:
        if not isinstance(prepared, PreparedBehaviorManifest):
            raise BehaviorManifestCatalogError("manifest catalog commit must be prepared")
        try:
            digest = self._store.preview_commit(prepared._content)
        except ContentStoreError as error:
            raise BehaviorManifestCatalogError(str(error)) from error
        if digest != prepared.manifest_id.digest:
            raise BehaviorManifestCatalogError(
                "manifest catalog preview returned a different identity"
            )
        return prepared.manifest_id

    def preview_novel_commit(self, prepared: PreparedBehaviorManifest) -> None:
        """Require capacity for one additional manifest of this exact size."""

        if not isinstance(prepared, PreparedBehaviorManifest):
            raise BehaviorManifestCatalogError("manifest commit must be prepared")
        try:
            self._store.preview_novel_commit(prepared._content)
        except ContentStoreError as error:
            raise BehaviorManifestCatalogError(str(error)) from error

    def commit(self, prepared: PreparedBehaviorManifest) -> BehaviorManifestId:
        self.preview_commit(prepared)
        try:
            digest = self._store.commit(prepared._content)
        except ContentStoreError as error:
            raise BehaviorManifestCatalogError(str(error)) from error
        if digest != prepared.manifest_id.digest:
            raise BehaviorManifestCatalogError(
                "manifest catalog committed a different identity"
            )
        return prepared.manifest_id

    def resolve(self, manifest_id: BehaviorManifestId) -> BehaviorManifest:
        if not isinstance(manifest_id, BehaviorManifestId):
            raise BehaviorManifestCatalogError("manifest catalog lookup id must be typed")
        try:
            payload = self._store.read(manifest_id.digest)
        except ContentStoreError as error:
            if "unknown content identity" in str(error):
                raise BehaviorManifestCatalogError(
                    "unknown durable behavior manifest identity"
                ) from error
            raise BehaviorManifestCatalogError(str(error)) from error
        manifest = BehaviorManifest.from_bytes(payload)
        if manifest.identity != manifest_id:
            raise BehaviorManifestCatalogError(
                "durable behavior manifest does not reproduce its identity"
            )
        return manifest

    def hydrate_registry(
        self,
        registry: BehaviorManifestRegistry,
    ) -> tuple[BehaviorManifestId, ...]:
        if not isinstance(registry, BehaviorManifestRegistry):
            raise BehaviorManifestCatalogError("catalog hydration requires a registry")
        entries = tuple(
            (manifest_id, self.resolve(manifest_id))
            for manifest_id in self.manifest_ids
        )
        return registry.register_many(entries)


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise BehaviorManifestCatalogError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise BehaviorManifestCatalogError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise BehaviorManifestCatalogError(f"{name} must be positive")
    return normalized
