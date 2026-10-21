"""Atomic content-addressed components and manifests for exact process resume."""

from __future__ import annotations

import hashlib
import operator
import os
import struct
from collections.abc import Mapping
from dataclasses import dataclass, field
from enum import IntEnum
from pathlib import Path

from ._content_store import (
    BoundedContentStore,
    ContentStoreError,
    ContentStoreLimits,
    PreparedContent,
)


class ResumeStoreError(RuntimeError):
    """A resume component, manifest, or publication is unsafe."""


class ResumeComponentKind(IntEnum):
    ENVIRONMENT = 1
    EPISODE_ROOT_BANK = 2
    SHADOW_MODEL = 3
    OPTIMIZER = 4
    CATEGORICAL_GENERATOR = 5
    GENERATION_METADATA = 6


_COMPONENT_MAGIC = b"STS-RESUME-COMPONENT\x00"
_COMPONENT_VERSION = 1
_MANIFEST_MAGIC = b"STS-RESUME-MANIFEST\x00"
_MANIFEST_VERSION = 1
_ALL_KINDS = tuple(ResumeComponentKind)


@dataclass(frozen=True, order=True)
class ResumeManifestId:
    digest: bytes

    def __post_init__(self) -> None:
        if not isinstance(self.digest, bytes) or len(self.digest) != 32:
            raise ResumeStoreError("resume manifest digest must be 32 immutable bytes")


@dataclass(frozen=True)
class ResumeComponent:
    kind: ResumeComponentKind
    payload: bytes = field(repr=False)

    def __post_init__(self) -> None:
        if not isinstance(self.kind, ResumeComponentKind):
            raise ResumeStoreError("resume component kind must be typed")
        if not isinstance(self.payload, bytes):
            raise ResumeStoreError("resume component payload must be immutable bytes")

    def to_bytes(self) -> bytes:
        return b"".join(
            (
                _COMPONENT_MAGIC,
                struct.pack(">IBQ", _COMPONENT_VERSION, int(self.kind), len(self.payload)),
                self.payload,
            )
        )

    @classmethod
    def from_bytes(cls, payload: bytes) -> ResumeComponent:
        if not isinstance(payload, bytes) or not payload.startswith(_COMPONENT_MAGIC):
            raise ResumeStoreError("resume component magic is invalid")
        header = len(_COMPONENT_MAGIC)
        if len(payload) < header + 13:
            raise ResumeStoreError("resume component header is truncated")
        version, kind_id, data_length = struct.unpack(">IBQ", payload[header : header + 13])
        if version != _COMPONENT_VERSION:
            raise ResumeStoreError("resume component version is unsupported")
        try:
            kind = ResumeComponentKind(kind_id)
        except ValueError as error:
            raise ResumeStoreError("resume component kind is unknown") from error
        data = payload[header + 13 :]
        if len(data) != data_length:
            raise ResumeStoreError("resume component payload length is inconsistent")
        component = cls(kind, data)
        if component.to_bytes() != payload:
            raise ResumeStoreError("resume component encoding is not canonical")
        return component


@dataclass(frozen=True)
class ResumeManifestEntry:
    kind: ResumeComponentKind
    digest: bytes
    stored_bytes: int

    def __post_init__(self) -> None:
        if not isinstance(self.kind, ResumeComponentKind):
            raise ResumeStoreError("resume manifest component kind must be typed")
        if not isinstance(self.digest, bytes) or len(self.digest) != 32:
            raise ResumeStoreError("resume component digest must be 32 immutable bytes")
        object.__setattr__(self, "stored_bytes", _positive(self.stored_bytes, "stored_bytes"))


@dataclass(frozen=True)
class ResumeManifest:
    entries: tuple[ResumeManifestEntry, ...]

    def __post_init__(self) -> None:
        entries = tuple(self.entries)
        if tuple(entry.kind for entry in entries) != _ALL_KINDS:
            raise ResumeStoreError("resume manifest must bind every component exactly once")
        object.__setattr__(self, "entries", entries)

    @property
    def identity(self) -> ResumeManifestId:
        return ResumeManifestId(hashlib.sha256(self.to_bytes()).digest())

    def to_bytes(self) -> bytes:
        output = bytearray(_MANIFEST_MAGIC)
        output.extend(struct.pack(">IH", _MANIFEST_VERSION, len(self.entries)))
        for entry in self.entries:
            output.extend(struct.pack(">B", int(entry.kind)))
            output.extend(entry.digest)
            output.extend(struct.pack(">Q", entry.stored_bytes))
        return bytes(output)

    @classmethod
    def from_bytes(cls, payload: bytes) -> ResumeManifest:
        if not isinstance(payload, bytes) or not payload.startswith(_MANIFEST_MAGIC):
            raise ResumeStoreError("resume manifest magic is invalid")
        position = len(_MANIFEST_MAGIC)
        if len(payload) < position + 6:
            raise ResumeStoreError("resume manifest header is truncated")
        version, count = struct.unpack(">IH", payload[position : position + 6])
        position += 6
        if version != _MANIFEST_VERSION:
            raise ResumeStoreError("resume manifest version is unsupported")
        entries = []
        for _ in range(count):
            if position + 41 > len(payload):
                raise ResumeStoreError("resume manifest entry is truncated")
            kind_id = payload[position]
            digest = payload[position + 1 : position + 33]
            stored_bytes = struct.unpack(">Q", payload[position + 33 : position + 41])[0]
            position += 41
            try:
                kind = ResumeComponentKind(kind_id)
            except ValueError as error:
                raise ResumeStoreError("resume manifest component kind is unknown") from error
            entries.append(ResumeManifestEntry(kind, digest, stored_bytes))
        if position != len(payload):
            raise ResumeStoreError("resume manifest contains trailing bytes")
        manifest = cls(tuple(entries))
        if manifest.to_bytes() != payload:
            raise ResumeStoreError("resume manifest encoding is not canonical")
        return manifest


@dataclass(frozen=True)
class ResumeStoreLimits:
    max_components: int
    max_bytes_per_component: int
    max_total_component_bytes: int
    max_manifests: int
    max_bytes_per_manifest: int
    max_total_manifest_bytes: int

    def __post_init__(self) -> None:
        for name in (
            "max_components",
            "max_bytes_per_component",
            "max_total_component_bytes",
            "max_manifests",
            "max_bytes_per_manifest",
            "max_total_manifest_bytes",
        ):
            object.__setattr__(self, name, _positive(getattr(self, name), name))
        if self.max_bytes_per_component > self.max_total_component_bytes:
            raise ResumeStoreError("component byte limit exceeds component total")
        if self.max_bytes_per_manifest > self.max_total_manifest_bytes:
            raise ResumeStoreError("manifest byte limit exceeds manifest total")


@dataclass(frozen=True)
class PreparedResumePublication:
    manifest: ResumeManifest
    manifest_id: ResumeManifestId
    _components: tuple[PreparedContent, ...] = field(repr=False, compare=False)
    _manifest: PreparedContent = field(repr=False, compare=False)


@dataclass(frozen=True)
class ResumeStoreSnapshot:
    components: int
    component_bytes: int
    manifests: int
    manifest_bytes: int


class BoundedResumeStore:
    """Commit all immutable components before publishing one small manifest."""

    def __init__(self, root: str | os.PathLike[str], limits: ResumeStoreLimits) -> None:
        if not isinstance(limits, ResumeStoreLimits):
            raise ResumeStoreError("resume store limits must be typed")
        self.root = Path(root).resolve()
        if self.root.exists() and not self.root.is_dir():
            raise ResumeStoreError("resume store root is not a directory")
        self.root.mkdir(exist_ok=True)
        self.limits = limits
        try:
            self._components = BoundedContentStore(
                self.root / "components",
                suffix=".stsresume",
                limits=ContentStoreLimits(
                    max_artifacts=limits.max_components,
                    max_bytes_per_artifact=limits.max_bytes_per_component,
                    max_total_bytes=limits.max_total_component_bytes,
                ),
                validate_payload=ResumeComponent.from_bytes,
            )
            self._manifests = BoundedContentStore(
                self.root / "manifests",
                suffix=".stsresumemanifest",
                limits=ContentStoreLimits(
                    max_artifacts=limits.max_manifests,
                    max_bytes_per_artifact=limits.max_bytes_per_manifest,
                    max_total_bytes=limits.max_total_manifest_bytes,
                ),
                validate_payload=ResumeManifest.from_bytes,
            )
        except ContentStoreError as error:
            raise ResumeStoreError(str(error)) from error

    @property
    def snapshot(self) -> ResumeStoreSnapshot:
        components = self._components.snapshot
        manifests = self._manifests.snapshot
        return ResumeStoreSnapshot(
            components=components.artifacts,
            component_bytes=components.total_bytes,
            manifests=manifests.artifacts,
            manifest_bytes=manifests.total_bytes,
        )

    @property
    def manifest_ids(self) -> tuple[ResumeManifestId, ...]:
        return tuple(ResumeManifestId(digest) for digest in self._manifests.digests)

    def prepare(
        self,
        payloads: Mapping[ResumeComponentKind, bytes],
    ) -> PreparedResumePublication:
        if not isinstance(payloads, Mapping) or set(payloads) != set(_ALL_KINDS):
            raise ResumeStoreError("resume publication must contain every component")
        prepared_components = []
        entries = []
        try:
            for kind in _ALL_KINDS:
                component = ResumeComponent(kind, payloads[kind])
                prepared = self._components.prepare(component.to_bytes())
                prepared_components.append(prepared)
                entries.append(
                    ResumeManifestEntry(kind, prepared.digest, prepared.payload_bytes)
                )
            manifest = ResumeManifest(tuple(entries))
            prepared_manifest = self._manifests.prepare(manifest.to_bytes())
        except (ContentStoreError, KeyError) as error:
            raise ResumeStoreError(str(error)) from error
        if prepared_manifest.digest != manifest.identity.digest:
            raise ResumeStoreError("prepared resume manifest identity changed")
        return PreparedResumePublication(
            manifest=manifest,
            manifest_id=manifest.identity,
            _components=tuple(prepared_components),
            _manifest=prepared_manifest,
        )

    def preview_commit(self, prepared: PreparedResumePublication) -> ResumeManifestId:
        if not isinstance(prepared, PreparedResumePublication):
            raise ResumeStoreError("resume publication must be prepared")
        try:
            for component in prepared._components:
                self._components.preview_commit(component)
            existing = set(self._components.digests)
            novel = {
                component.digest: component.payload_bytes
                for component in prepared._components
                if component.digest not in existing
            }
            component_snapshot = self._components.snapshot
            if (
                component_snapshot.artifacts + len(novel)
                > self.limits.max_components
            ):
                raise ResumeStoreError("resume component store capacity exceeded")
            if (
                component_snapshot.total_bytes + sum(novel.values())
                > self.limits.max_total_component_bytes
            ):
                raise ResumeStoreError("resume component store total byte limit exceeded")
            digest = self._manifests.preview_commit(prepared._manifest)
        except ContentStoreError as error:
            raise ResumeStoreError(str(error)) from error
        if digest != prepared.manifest_id.digest:
            raise ResumeStoreError("resume manifest preview returned another identity")
        return prepared.manifest_id

    def commit(self, prepared: PreparedResumePublication) -> ResumeManifestId:
        self.preview_commit(prepared)
        try:
            for component in prepared._components:
                self._components.commit(component)
            digest = self._manifests.commit(prepared._manifest)
        except ContentStoreError as error:
            raise ResumeStoreError(str(error)) from error
        if digest != prepared.manifest_id.digest:
            raise ResumeStoreError("resume store committed another manifest identity")
        return prepared.manifest_id

    def resolve(
        self,
        manifest_id: ResumeManifestId,
    ) -> dict[ResumeComponentKind, bytes]:
        if not isinstance(manifest_id, ResumeManifestId):
            raise ResumeStoreError("resume lookup id must be typed")
        try:
            manifest_payload = self._manifests.read(manifest_id.digest)
            manifest = ResumeManifest.from_bytes(manifest_payload)
            if manifest.identity != manifest_id:
                raise ResumeStoreError("resume manifest does not reproduce its identity")
            resolved = {}
            for entry in manifest.entries:
                stored = self._components.read(entry.digest)
                if len(stored) != entry.stored_bytes:
                    raise ResumeStoreError("resume component stored byte count changed")
                component = ResumeComponent.from_bytes(stored)
                if component.kind is not entry.kind:
                    raise ResumeStoreError("resume component kind does not match manifest")
                resolved[entry.kind] = component.payload
            return resolved
        except ContentStoreError as error:
            raise ResumeStoreError(str(error)) from error


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise ResumeStoreError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise ResumeStoreError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise ResumeStoreError(f"{name} must be positive")
    return normalized
