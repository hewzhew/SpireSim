"""Bounded content-addressed provenance for behavior-policy checkpoints."""

from __future__ import annotations

import hashlib
import operator
import struct
from collections.abc import Sequence
from dataclasses import dataclass
from enum import IntEnum

from .policy import BehaviorManifestId


class BehaviorManifestError(ValueError):
    """A behavior manifest or registry operation is not exact and safe."""


class ManifestArtifactKind(IntEnum):
    """Typed external artifacts referenced by one behavior manifest."""

    MODEL_CHECKPOINT = 1
    MODEL_DEFINITION = 2
    MODEL_CONFIG = 3
    SEMANTIC_SCHEMA = 4
    OPTIMIZER_CONFIG = 5
    TRAINER_IMPLEMENTATION = 6
    BEHAVIOR_RULE = 7
    BEHAVIOR_RULE_CONFIG = 8


@dataclass(frozen=True, order=True)
class ManifestArtifactId:
    """SHA-256 content identity of an externally owned immutable artifact."""

    kind: ManifestArtifactKind
    digest: bytes

    def __post_init__(self) -> None:
        if not isinstance(self.kind, ManifestArtifactKind):
            raise BehaviorManifestError("manifest artifact kind must be typed")
        if not isinstance(self.digest, bytes):
            raise BehaviorManifestError("manifest artifact digest must be immutable bytes")
        if len(self.digest) != 32:
            raise BehaviorManifestError("manifest artifact digest must contain 32 bytes")

    @classmethod
    def from_content(
        cls,
        kind: ManifestArtifactKind,
        content: bytes,
    ) -> ManifestArtifactId:
        """Hash one immutable externally owned artifact without retaining it."""

        if not isinstance(content, bytes):
            raise BehaviorManifestError("manifest artifact content must be immutable bytes")
        return cls(kind, hashlib.sha256(content).digest())


@dataclass(frozen=True)
class BehaviorRuleBinding:
    """Exact implementation and configuration identities for choosing actions."""

    implementation: ManifestArtifactId
    configuration: ManifestArtifactId

    def __post_init__(self) -> None:
        for field_name, expected_kind in (
            ("implementation", ManifestArtifactKind.BEHAVIOR_RULE),
            ("configuration", ManifestArtifactKind.BEHAVIOR_RULE_CONFIG),
        ):
            artifact = getattr(self, field_name)
            if not isinstance(artifact, ManifestArtifactId):
                raise BehaviorManifestError(
                    f"behavior rule {field_name} must be a ManifestArtifactId"
                )
            if artifact.kind is not expected_kind:
                raise BehaviorManifestError(
                    f"behavior rule {field_name} must have kind {expected_kind.name}"
                )


GREEDY_BEHAVIOR_RULE_V1 = BehaviorRuleBinding(
    implementation=ManifestArtifactId.from_content(
        ManifestArtifactKind.BEHAVIOR_RULE,
        b"sts_learning.greedy_candidate_argmax\x00v1",
    ),
    configuration=ManifestArtifactId.from_content(
        ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
        b"sts_learning.greedy_candidate_argmax.config\x00v1",
    ),
)


_COMBAT_GREEDY_STRATEGIC_SAMPLED_RULE_V1 = ManifestArtifactId.from_content(
    ManifestArtifactKind.BEHAVIOR_RULE,
    b"sts_learning.combat_greedy_strategic_sampled\x00v1",
)

_COMBAT_ANCHORED_GREEDY_STRATEGIC_SAMPLED_RULE_V1 = (
    ManifestArtifactId.from_content(
        ManifestArtifactKind.BEHAVIOR_RULE,
        b"sts_learning.combat_anchored_greedy_strategic_sampled\x00v1",
    )
)


def combat_greedy_strategic_sampled_rule_v1(
    sampled_rule: BehaviorRuleBinding,
) -> BehaviorRuleBinding:
    """Bind combat argmax plus the exact source rule for strategic rows."""

    if not isinstance(sampled_rule, BehaviorRuleBinding):
        raise BehaviorManifestError(
            "combat-scoped greedy behavior requires a sampled rule binding"
        )
    configuration = (
        b"sts_learning.combat_greedy_strategic_sampled.config\x00v1"
        + sampled_rule.implementation.digest
        + sampled_rule.configuration.digest
    )
    return BehaviorRuleBinding(
        implementation=_COMBAT_GREEDY_STRATEGIC_SAMPLED_RULE_V1,
        configuration=ManifestArtifactId.from_content(
            ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
            configuration,
        ),
    )


def combat_anchored_greedy_strategic_sampled_rule_v1(
    sampled_rule: BehaviorRuleBinding,
    combat_anchor_manifest_id: BehaviorManifestId,
) -> BehaviorRuleBinding:
    """Bind strategic sampling to one immutable combat scorer manifest."""

    if not isinstance(sampled_rule, BehaviorRuleBinding):
        raise BehaviorManifestError(
            "anchored combat behavior requires a sampled rule binding"
        )
    if not isinstance(combat_anchor_manifest_id, BehaviorManifestId):
        raise BehaviorManifestError(
            "anchored combat behavior requires a combat manifest identity"
        )
    configuration = (
        b"sts_learning.combat_anchored_greedy_strategic_sampled.config\x00v1"
        + sampled_rule.implementation.digest
        + sampled_rule.configuration.digest
        + combat_anchor_manifest_id.digest
    )
    return BehaviorRuleBinding(
        implementation=(
            _COMBAT_ANCHORED_GREEDY_STRATEGIC_SAMPLED_RULE_V1
        ),
        configuration=ManifestArtifactId.from_content(
            ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
            configuration,
        ),
    )


@dataclass(frozen=True)
class BehaviorManifest:
    """Exact external identities needed to reproduce one behavior policy."""

    model_checkpoint: ManifestArtifactId
    model_definition: ManifestArtifactId
    model_config: ManifestArtifactId
    behavior_rule: BehaviorRuleBinding
    semantic_schema: ManifestArtifactId
    optimizer_config: ManifestArtifactId
    trainer_implementation: ManifestArtifactId
    semantic_schema_version: int
    training_step: int

    def __post_init__(self) -> None:
        if not isinstance(self.behavior_rule, BehaviorRuleBinding):
            raise BehaviorManifestError(
                "behavior_rule must be a BehaviorRuleBinding"
            )
        for field_name, expected_kind in (
            ("model_checkpoint", ManifestArtifactKind.MODEL_CHECKPOINT),
            ("model_definition", ManifestArtifactKind.MODEL_DEFINITION),
            ("model_config", ManifestArtifactKind.MODEL_CONFIG),
            ("semantic_schema", ManifestArtifactKind.SEMANTIC_SCHEMA),
            ("optimizer_config", ManifestArtifactKind.OPTIMIZER_CONFIG),
            ("trainer_implementation", ManifestArtifactKind.TRAINER_IMPLEMENTATION),
        ):
            artifact = getattr(self, field_name)
            if not isinstance(artifact, ManifestArtifactId):
                raise BehaviorManifestError(f"{field_name} must be a ManifestArtifactId")
            if artifact.kind is not expected_kind:
                raise BehaviorManifestError(
                    f"{field_name} must have kind {expected_kind.name}"
                )
        object.__setattr__(
            self,
            "semantic_schema_version",
            _non_negative_integer(
                self.semantic_schema_version,
                "semantic_schema_version",
            ),
        )
        object.__setattr__(
            self,
            "training_step",
            _non_negative_integer(self.training_step, "training_step"),
        )

    @property
    def identity(self) -> BehaviorManifestId:
        """Return the canonical identity without retaining any artifact payload."""

        digest = hashlib.sha256(self.to_bytes()).digest()
        return BehaviorManifestId(digest)

    def to_bytes(self) -> bytes:
        """Encode the complete manifest in its versioned canonical format."""

        payload = bytearray(_BEHAVIOR_MANIFEST_MAGIC)
        payload.extend(
            struct.pack(
                ">IQQ",
                _BEHAVIOR_MANIFEST_VERSION,
                self.semantic_schema_version,
                self.training_step,
            )
        )
        for artifact in (
            self.model_checkpoint,
            self.model_definition,
            self.model_config,
            self.behavior_rule.implementation,
            self.behavior_rule.configuration,
            self.semantic_schema,
            self.optimizer_config,
            self.trainer_implementation,
        ):
            payload.append(int(artifact.kind))
            payload.extend(artifact.digest)
        return bytes(payload)

    @classmethod
    def from_bytes(cls, payload: bytes) -> BehaviorManifest:
        """Decode only the exact maintained canonical format."""

        if not isinstance(payload, bytes):
            raise BehaviorManifestError("behavior manifest payload must be immutable bytes")
        if not payload.startswith(_BEHAVIOR_MANIFEST_MAGIC):
            raise BehaviorManifestError("behavior manifest magic is invalid")
        position = len(_BEHAVIOR_MANIFEST_MAGIC)
        header_end = position + struct.calcsize(">IQQ")
        if header_end > len(payload):
            raise BehaviorManifestError("behavior manifest header is truncated")
        version, schema_version, training_step = struct.unpack(
            ">IQQ",
            payload[position:header_end],
        )
        position = header_end
        if version != _BEHAVIOR_MANIFEST_VERSION:
            raise BehaviorManifestError("behavior manifest version is unsupported")

        artifacts: list[ManifestArtifactId] = []
        for expected_kind in (
            ManifestArtifactKind.MODEL_CHECKPOINT,
            ManifestArtifactKind.MODEL_DEFINITION,
            ManifestArtifactKind.MODEL_CONFIG,
            ManifestArtifactKind.BEHAVIOR_RULE,
            ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
            ManifestArtifactKind.SEMANTIC_SCHEMA,
            ManifestArtifactKind.OPTIMIZER_CONFIG,
            ManifestArtifactKind.TRAINER_IMPLEMENTATION,
        ):
            end = position + 33
            if end > len(payload):
                raise BehaviorManifestError("behavior manifest artifact is truncated")
            try:
                kind = ManifestArtifactKind(payload[position])
            except ValueError as error:
                raise BehaviorManifestError(
                    "behavior manifest artifact kind is unknown"
                ) from error
            if kind is not expected_kind:
                raise BehaviorManifestError(
                    f"behavior manifest expected artifact kind {expected_kind.name}"
                )
            artifacts.append(ManifestArtifactId(kind, payload[position + 1 : end]))
            position = end
        if position != len(payload):
            raise BehaviorManifestError("behavior manifest contains trailing bytes")
        return cls(
            model_checkpoint=artifacts[0],
            model_definition=artifacts[1],
            model_config=artifacts[2],
            behavior_rule=BehaviorRuleBinding(
                implementation=artifacts[3],
                configuration=artifacts[4],
            ),
            semantic_schema=artifacts[5],
            optimizer_config=artifacts[6],
            trainer_implementation=artifacts[7],
            semantic_schema_version=schema_version,
            training_step=training_step,
        )


@dataclass(frozen=True)
class BehaviorManifestTemplate:
    """Fixed non-checkpoint provenance used to bind a published model state."""

    model_definition: ManifestArtifactId
    model_config: ManifestArtifactId
    behavior_rule: BehaviorRuleBinding
    semantic_schema: ManifestArtifactId
    optimizer_config: ManifestArtifactId
    trainer_implementation: ManifestArtifactId
    semantic_schema_version: int

    def __post_init__(self) -> None:
        if not isinstance(self.behavior_rule, BehaviorRuleBinding):
            raise BehaviorManifestError(
                "behavior_rule must be a BehaviorRuleBinding"
            )
        for field_name, expected_kind in (
            ("model_definition", ManifestArtifactKind.MODEL_DEFINITION),
            ("model_config", ManifestArtifactKind.MODEL_CONFIG),
            ("semantic_schema", ManifestArtifactKind.SEMANTIC_SCHEMA),
            ("optimizer_config", ManifestArtifactKind.OPTIMIZER_CONFIG),
            ("trainer_implementation", ManifestArtifactKind.TRAINER_IMPLEMENTATION),
        ):
            artifact = getattr(self, field_name)
            if not isinstance(artifact, ManifestArtifactId):
                raise BehaviorManifestError(f"{field_name} must be a ManifestArtifactId")
            if artifact.kind is not expected_kind:
                raise BehaviorManifestError(
                    f"{field_name} must have kind {expected_kind.name}"
                )
        object.__setattr__(
            self,
            "semantic_schema_version",
            _non_negative_integer(
                self.semantic_schema_version,
                "semantic_schema_version",
            ),
        )

    def bind(
        self,
        model_checkpoint: ManifestArtifactId,
        *,
        training_step: int,
    ) -> BehaviorManifest:
        """Create one exact manifest; the returned identity includes the step."""

        return BehaviorManifest(
            model_checkpoint=model_checkpoint,
            model_definition=self.model_definition,
            model_config=self.model_config,
            behavior_rule=self.behavior_rule,
            semantic_schema=self.semantic_schema,
            optimizer_config=self.optimizer_config,
            trainer_implementation=self.trainer_implementation,
            semantic_schema_version=self.semantic_schema_version,
            training_step=training_step,
        )


@dataclass(frozen=True)
class BehaviorManifestRegistrySnapshot:
    capacity: int
    registered_manifests: int


class BehaviorManifestRegistry:
    """Fixed-capacity exact lookup; model objects and checkpoints stay external."""

    def __init__(self, capacity: int) -> None:
        self.capacity = _positive_integer(capacity, "capacity")
        self._entries: dict[BehaviorManifestId, BehaviorManifest] = {}

    @property
    def snapshot(self) -> BehaviorManifestRegistrySnapshot:
        return BehaviorManifestRegistrySnapshot(
            capacity=self.capacity,
            registered_manifests=len(self._entries),
        )

    def register(
        self,
        manifest: BehaviorManifest,
        *,
        claimed_id: BehaviorManifestId | None = None,
    ) -> BehaviorManifestId:
        """Register one manifest after checking any persisted claimed identity."""

        identity = self.preview_registration(manifest, claimed_id=claimed_id)
        if identity in self._entries:
            return identity
        self._entries[identity] = manifest
        return identity

    def preview_registration(
        self,
        manifest: BehaviorManifest,
        *,
        claimed_id: BehaviorManifestId | None = None,
    ) -> BehaviorManifestId:
        """Validate a registration without consuming registry capacity."""

        if not isinstance(manifest, BehaviorManifest):
            raise BehaviorManifestError("registry accepts only BehaviorManifest values")
        identity = manifest.identity
        if claimed_id is not None:
            if not isinstance(claimed_id, BehaviorManifestId):
                raise BehaviorManifestError("claimed manifest id must be typed")
            if claimed_id != identity:
                raise BehaviorManifestError(
                    "claimed behavior manifest id conflicts with manifest content"
                )
        existing = self._entries.get(identity)
        if existing is not None:
            if existing != manifest:
                raise BehaviorManifestError(
                    "behavior manifest identity conflicts with registered content"
                )
            return identity
        if len(self._entries) >= self.capacity:
            raise BehaviorManifestError("behavior manifest registry capacity exceeded")
        return identity

    def preview_novel_registration(self) -> None:
        """Require one free row without treating an existing identity as capacity."""

        if len(self._entries) >= self.capacity:
            raise BehaviorManifestError("behavior manifest registry capacity exceeded")

    def resolve(self, identity: BehaviorManifestId) -> BehaviorManifest:
        """Resolve only known typed identities."""

        if not isinstance(identity, BehaviorManifestId):
            raise BehaviorManifestError("behavior manifest lookup id must be typed")
        try:
            return self._entries[identity]
        except KeyError as error:
            raise BehaviorManifestError("unknown behavior manifest identity") from error

    def register_many(
        self,
        entries: Sequence[tuple[BehaviorManifestId, BehaviorManifest]],
    ) -> tuple[BehaviorManifestId, ...]:
        """Atomically hydrate multiple exact bindings without partial capacity use."""

        try:
            normalized = tuple(entries)
        except TypeError as error:
            raise BehaviorManifestError("manifest entries must be a sequence") from error
        tentative = dict(self._entries)
        identities: list[BehaviorManifestId] = []
        for entry in normalized:
            if not isinstance(entry, tuple) or len(entry) != 2:
                raise BehaviorManifestError("manifest entry must be an id/manifest pair")
            claimed_id, manifest = entry
            if not isinstance(claimed_id, BehaviorManifestId):
                raise BehaviorManifestError("manifest entry id must be typed")
            if not isinstance(manifest, BehaviorManifest):
                raise BehaviorManifestError("manifest entry content must be typed")
            if manifest.identity != claimed_id:
                raise BehaviorManifestError(
                    "manifest entry id conflicts with manifest content"
                )
            existing = tentative.get(claimed_id)
            if existing is not None and existing != manifest:
                raise BehaviorManifestError(
                    "manifest entry conflicts with registered content"
                )
            if existing is None:
                if len(tentative) >= self.capacity:
                    raise BehaviorManifestError(
                        "behavior manifest registry capacity exceeded"
                    )
                tentative[claimed_id] = manifest
            identities.append(claimed_id)
        self._entries = tentative
        return tuple(identities)

    def replace_active(
        self,
        previous_identity: BehaviorManifestId | None,
        manifest: BehaviorManifest,
    ) -> BehaviorManifestId:
        """Atomically rotate the one live behavior binding.

        Durable catalogs retain history.  The live training registry does not:
        once the caller has consumed every sample from the previous behavior,
        it may replace that exact row without growing registry capacity.
        """

        if not isinstance(manifest, BehaviorManifest):
            raise BehaviorManifestError(
                "active registry replacement requires a BehaviorManifest"
            )
        if previous_identity is None:
            if self._entries:
                raise BehaviorManifestError(
                    "initial active registry replacement requires an empty registry"
                )
            tentative: dict[BehaviorManifestId, BehaviorManifest] = {}
        else:
            if not isinstance(previous_identity, BehaviorManifestId):
                raise BehaviorManifestError(
                    "previous active manifest id must be typed"
                )
            try:
                previous = self._entries[previous_identity]
            except KeyError as error:
                raise BehaviorManifestError(
                    "previous active behavior is not registered"
                ) from error
            if previous.identity != previous_identity:
                raise BehaviorManifestError(
                    "previous active behavior identity conflicts with content"
                )
            tentative = dict(self._entries)
            del tentative[previous_identity]

        identity = manifest.identity
        existing = tentative.get(identity)
        if existing is not None and existing != manifest:
            raise BehaviorManifestError(
                "replacement behavior identity conflicts with registered content"
            )
        if existing is None:
            if len(tentative) >= self.capacity:
                raise BehaviorManifestError(
                    "behavior manifest registry capacity exceeded"
                )
            tentative[identity] = manifest
        self._entries = tentative
        return identity

    def require_exact(
        self,
        identity: BehaviorManifestId,
        expected: BehaviorManifest,
    ) -> BehaviorManifest:
        """Resolve an id and reject any checkpoint, config, or schema mismatch."""

        if not isinstance(expected, BehaviorManifest):
            raise BehaviorManifestError("expected binding must be a BehaviorManifest")
        registered = self.resolve(identity)
        if registered != expected:
            raise BehaviorManifestError(
                "registered behavior manifest does not match the expected exact binding"
            )
        return registered


def _positive_integer(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized <= 0:
        raise BehaviorManifestError(f"{name} must be positive")
    return normalized


def _non_negative_integer(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized < 0:
        raise BehaviorManifestError(f"{name} must be non-negative")
    return normalized


def _integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise BehaviorManifestError(f"{name} must be an integer")
    try:
        return operator.index(value)
    except TypeError as error:
        raise BehaviorManifestError(f"{name} must be an integer") from error


_BEHAVIOR_MANIFEST_MAGIC = b"sts-behavior-manifest\x00"
_BEHAVIOR_MANIFEST_VERSION = 2
