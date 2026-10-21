"""Shared bounded atomic store for immutable SHA-256-addressed payloads."""

from __future__ import annotations

import hashlib
import operator
import os
import re
import tempfile
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path


class ContentStoreError(RuntimeError):
    """A content store payload or filesystem state is unsafe."""


@dataclass(frozen=True)
class ContentStoreLimits:
    max_artifacts: int
    max_bytes_per_artifact: int
    max_total_bytes: int

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "max_artifacts",
            _positive(self.max_artifacts, "max_artifacts"),
        )
        object.__setattr__(
            self,
            "max_bytes_per_artifact",
            _positive(
                self.max_bytes_per_artifact,
                "max_bytes_per_artifact",
            ),
        )
        object.__setattr__(
            self,
            "max_total_bytes",
            _positive(self.max_total_bytes, "max_total_bytes"),
        )
        if self.max_bytes_per_artifact > self.max_total_bytes:
            raise ContentStoreError(
                "max_bytes_per_artifact cannot exceed max_total_bytes"
            )


@dataclass(frozen=True)
class PreparedContent:
    digest: bytes
    payload_bytes: int
    _payload: bytes = field(repr=False, compare=False)

    def __post_init__(self) -> None:
        if not isinstance(self.digest, bytes) or len(self.digest) != 32:
            raise ContentStoreError("prepared content digest must be 32 immutable bytes")
        if not isinstance(self._payload, bytes):
            raise ContentStoreError("prepared content payload must be immutable bytes")
        if self.payload_bytes != len(self._payload):
            raise ContentStoreError("prepared content byte count is incorrect")
        if hashlib.sha256(self._payload).digest() != self.digest:
            raise ContentStoreError("prepared content digest is incorrect")


@dataclass(frozen=True)
class ContentStoreSnapshot:
    artifacts: int
    total_bytes: int
    max_artifacts: int
    max_total_bytes: int


class BoundedContentStore:
    """No-eviction atomic file store over one validated payload kind."""

    def __init__(
        self,
        root: str | os.PathLike[str],
        *,
        suffix: str,
        limits: ContentStoreLimits,
        validate_payload: Callable[[bytes], object],
    ) -> None:
        if not isinstance(limits, ContentStoreLimits):
            raise ContentStoreError("content store limits must be typed")
        if not isinstance(suffix, str) or re.fullmatch(r"\.[a-z][a-z0-9]*", suffix) is None:
            raise ContentStoreError("content store suffix is invalid")
        if not callable(validate_payload):
            raise ContentStoreError("content store payload validator must be callable")
        self.root = Path(root).resolve()
        self.suffix = suffix
        self.limits = limits
        self._validate_payload = validate_payload
        self._name = re.compile(rf"([0-9a-f]{{64}}){re.escape(suffix)}")
        if self.root.exists() and not self.root.is_dir():
            raise ContentStoreError("content store root is not a directory")
        self.root.mkdir(exist_ok=True)
        self._entries: dict[bytes, tuple[Path, int]] = {}
        self._load_existing()

    @property
    def snapshot(self) -> ContentStoreSnapshot:
        return ContentStoreSnapshot(
            artifacts=len(self._entries),
            total_bytes=sum(size for _, size in self._entries.values()),
            max_artifacts=self.limits.max_artifacts,
            max_total_bytes=self.limits.max_total_bytes,
        )

    @property
    def digests(self) -> tuple[bytes, ...]:
        return tuple(sorted(self._entries))

    def prepare(self, payload: bytes) -> PreparedContent:
        if not isinstance(payload, bytes):
            raise ContentStoreError("content store payload must be immutable bytes")
        if len(payload) > self.limits.max_bytes_per_artifact:
            raise ContentStoreError("content exceeds its per-artifact byte limit")
        self._validate_payload(payload)
        return PreparedContent(hashlib.sha256(payload).digest(), len(payload), payload)

    def preview_commit(self, prepared: PreparedContent) -> bytes:
        if not isinstance(prepared, PreparedContent):
            raise ContentStoreError("content commit must be prepared")
        if prepared.payload_bytes > self.limits.max_bytes_per_artifact:
            raise ContentStoreError("content exceeds its per-artifact byte limit")
        existing = self._entries.get(prepared.digest)
        if existing is not None:
            if self.read(prepared.digest) != prepared._payload:
                raise ContentStoreError("content digest conflicts with stored payload")
            return prepared.digest
        if len(self._entries) >= self.limits.max_artifacts:
            raise ContentStoreError("content store capacity exceeded")
        if self.snapshot.total_bytes + prepared.payload_bytes > self.limits.max_total_bytes:
            raise ContentStoreError("content store total byte limit exceeded")
        return prepared.digest

    def preview_novel_commit(self, prepared: PreparedContent) -> None:
        """Require room for one new payload even if this digest already exists."""

        if not isinstance(prepared, PreparedContent):
            raise ContentStoreError("content commit must be prepared")
        if prepared.payload_bytes > self.limits.max_bytes_per_artifact:
            raise ContentStoreError("content exceeds its per-artifact byte limit")
        if len(self._entries) >= self.limits.max_artifacts:
            raise ContentStoreError("content store capacity exceeded")
        if self.snapshot.total_bytes + prepared.payload_bytes > self.limits.max_total_bytes:
            raise ContentStoreError("content store total byte limit exceeded")

    def commit(self, prepared: PreparedContent) -> bytes:
        digest = self.preview_commit(prepared)
        if digest in self._entries:
            return digest

        target = self._path(digest)
        temporary: Path | None = None
        published = False
        try:
            with tempfile.NamedTemporaryFile(
                mode="wb",
                prefix=".pending-",
                suffix=".tmp",
                dir=self.root,
                delete=False,
            ) as output:
                temporary = Path(output.name)
                output.write(prepared._payload)
                output.flush()
                os.fsync(output.fileno())
            try:
                os.link(temporary, target)
                published = True
            except FileExistsError:
                if (
                    target.stat().st_size != prepared.payload_bytes
                    or target.read_bytes() != prepared._payload
                ):
                    raise ContentStoreError(
                        "content target conflicts with prepared payload"
                    )
        finally:
            if temporary is not None and temporary.exists():
                temporary.unlink()

        if published or digest not in self._entries:
            self._entries[digest] = (target, prepared.payload_bytes)
        return digest

    def read(self, digest: bytes) -> bytes:
        if not isinstance(digest, bytes) or len(digest) != 32:
            raise ContentStoreError("content lookup digest must be 32 immutable bytes")
        try:
            path, expected_size = self._entries[digest]
        except KeyError as error:
            raise ContentStoreError("unknown content identity") from error
        payload = path.read_bytes()
        if len(payload) != expected_size:
            raise ContentStoreError("stored content size changed")
        if hashlib.sha256(payload).digest() != digest:
            raise ContentStoreError("stored content digest changed")
        self._validate_payload(payload)
        return payload

    def _load_existing(self) -> None:
        total_bytes = 0
        entries: dict[bytes, tuple[Path, int]] = {}
        with os.scandir(self.root) as directory:
            for entry in directory:
                if not entry.is_file(follow_symlinks=False):
                    raise ContentStoreError(
                        f"content store contains unexpected entry {entry.name!r}"
                    )
                match = self._name.fullmatch(entry.name)
                if match is None:
                    raise ContentStoreError(
                        f"content store contains unexpected file {entry.name!r}"
                    )
                size = entry.stat(follow_symlinks=False).st_size
                if size > self.limits.max_bytes_per_artifact:
                    raise ContentStoreError("existing content exceeds its byte limit")
                total_bytes += size
                if total_bytes > self.limits.max_total_bytes:
                    raise ContentStoreError("existing content exceeds total byte limit")
                if len(entries) >= self.limits.max_artifacts:
                    raise ContentStoreError("existing content exceeds store capacity")
                digest = bytes.fromhex(match.group(1))
                path = Path(entry.path)
                payload = path.read_bytes()
                if len(payload) != size or hashlib.sha256(payload).digest() != digest:
                    raise ContentStoreError("existing content digest is corrupt")
                self._validate_payload(payload)
                entries[digest] = (path, size)
        self._entries = entries

    def _path(self, digest: bytes) -> Path:
        return self.root / f"{digest.hex()}{self.suffix}"


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise ContentStoreError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise ContentStoreError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise ContentStoreError(f"{name} must be positive")
    return normalized
