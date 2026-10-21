from __future__ import annotations

import unittest

from sts_learning import (
    BehaviorManifest,
    BehaviorManifestError,
    BehaviorManifestId,
    BehaviorManifestRegistry,
    BehaviorManifestTemplate,
    BehaviorRuleBinding,
    ManifestArtifactId,
    ManifestArtifactKind,
)


def _artifact(kind: ManifestArtifactKind, marker: int) -> ManifestArtifactId:
    return ManifestArtifactId(kind, bytes([marker]) * 32)


def _manifest(
    *,
    checkpoint_marker: int = 1,
    config_marker: int = 3,
    behavior_config_marker: int = 8,
) -> BehaviorManifest:
    return BehaviorManifest(
        model_checkpoint=_artifact(
            ManifestArtifactKind.MODEL_CHECKPOINT,
            checkpoint_marker,
        ),
        model_definition=_artifact(ManifestArtifactKind.MODEL_DEFINITION, 2),
        model_config=_artifact(ManifestArtifactKind.MODEL_CONFIG, config_marker),
        behavior_rule=BehaviorRuleBinding(
            implementation=_artifact(ManifestArtifactKind.BEHAVIOR_RULE, 7),
            configuration=_artifact(
                ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
                behavior_config_marker,
            ),
        ),
        semantic_schema=_artifact(ManifestArtifactKind.SEMANTIC_SCHEMA, 4),
        optimizer_config=_artifact(ManifestArtifactKind.OPTIMIZER_CONFIG, 5),
        trainer_implementation=_artifact(
            ManifestArtifactKind.TRAINER_IMPLEMENTATION,
            6,
        ),
        semantic_schema_version=1,
        training_step=10,
    )


class BehaviorManifestTests(unittest.TestCase):
    def test_canonical_manifest_round_trips_and_rejects_trailing_bytes(self) -> None:
        manifest = _manifest()
        payload = manifest.to_bytes()

        self.assertEqual(BehaviorManifest.from_bytes(payload), manifest)
        self.assertEqual(BehaviorManifest.from_bytes(payload).identity, manifest.identity)
        with self.assertRaisesRegex(BehaviorManifestError, "trailing bytes"):
            BehaviorManifest.from_bytes(payload + b"extra")
        with self.assertRaisesRegex(BehaviorManifestError, "magic"):
            BehaviorManifest.from_bytes(b"invalid")

    def test_artifact_content_hash_does_not_retain_or_accept_mutable_bytes(self) -> None:
        artifact = ManifestArtifactId.from_content(
            ManifestArtifactKind.MODEL_CONFIG,
            b"exact-config",
        )

        self.assertEqual(
            artifact,
            ManifestArtifactId.from_content(
                ManifestArtifactKind.MODEL_CONFIG,
                b"exact-config",
            ),
        )
        with self.assertRaisesRegex(BehaviorManifestError, "immutable bytes"):
            ManifestArtifactId.from_content(
                ManifestArtifactKind.MODEL_CONFIG,
                bytearray(b"mutable"),  # type: ignore[arg-type]
            )

    def test_identity_changes_with_checkpoint_or_behavior_rule(self) -> None:
        first = _manifest()
        same = _manifest()
        updated = _manifest(checkpoint_marker=7)
        changed_rule = _manifest(behavior_config_marker=9)

        self.assertEqual(first.identity, same.identity)
        self.assertNotEqual(first.identity, updated.identity)
        self.assertNotEqual(first.identity, changed_rule.identity)

    def test_artifact_kind_cannot_be_swapped_between_manifest_fields(self) -> None:
        with self.assertRaisesRegex(BehaviorManifestError, "MODEL_CHECKPOINT"):
            BehaviorManifest(
                model_checkpoint=_artifact(ManifestArtifactKind.MODEL_CONFIG, 1),
                model_definition=_artifact(ManifestArtifactKind.MODEL_DEFINITION, 2),
                model_config=_artifact(ManifestArtifactKind.MODEL_CONFIG, 3),
                behavior_rule=BehaviorRuleBinding(
                    implementation=_artifact(ManifestArtifactKind.BEHAVIOR_RULE, 7),
                    configuration=_artifact(
                        ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
                        8,
                    ),
                ),
                semantic_schema=_artifact(ManifestArtifactKind.SEMANTIC_SCHEMA, 4),
                optimizer_config=_artifact(ManifestArtifactKind.OPTIMIZER_CONFIG, 5),
                trainer_implementation=_artifact(
                    ManifestArtifactKind.TRAINER_IMPLEMENTATION,
                    6,
                ),
                semantic_schema_version=1,
                training_step=10,
            )

    def test_registry_rejects_unknown_and_conflicting_claimed_id(self) -> None:
        registry = BehaviorManifestRegistry(capacity=2)
        manifest = _manifest()

        with self.assertRaisesRegex(BehaviorManifestError, "unknown"):
            registry.resolve(BehaviorManifestId(b"\xff" * 32))
        with self.assertRaisesRegex(BehaviorManifestError, "conflicts"):
            registry.register(
                manifest,
                claimed_id=BehaviorManifestId(b"\x00" * 32),
            )

        self.assertEqual(registry.snapshot.registered_manifests, 0)

    def test_exact_binding_rejects_checkpoint_and_config_mismatch(self) -> None:
        registry = BehaviorManifestRegistry(capacity=2)
        manifest = _manifest()
        identity = registry.register(manifest)

        self.assertIs(registry.require_exact(identity, manifest), manifest)
        for mismatched in (
            _manifest(checkpoint_marker=7),
            _manifest(config_marker=8),
        ):
            with self.assertRaisesRegex(BehaviorManifestError, "exact binding"):
                registry.require_exact(identity, mismatched)

    def test_registry_is_idempotent_but_never_evicts_past_capacity(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        manifest = _manifest()
        identity = registry.register(manifest)

        self.assertEqual(registry.register(manifest), identity)
        self.assertEqual(registry.preview_registration(manifest), identity)
        with self.assertRaisesRegex(BehaviorManifestError, "capacity"):
            registry.preview_novel_registration()
        with self.assertRaisesRegex(BehaviorManifestError, "capacity"):
            registry.register(_manifest(checkpoint_marker=9))
        self.assertEqual(registry.snapshot.registered_manifests, 1)

    def test_registration_preview_consumes_no_capacity(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        manifest = _manifest()

        self.assertEqual(registry.preview_registration(manifest), manifest.identity)
        self.assertEqual(registry.snapshot.registered_manifests, 0)
        self.assertEqual(registry.register(manifest), manifest.identity)

    def test_explicit_active_replacement_is_atomic_at_capacity_one(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        first = _manifest(checkpoint_marker=1)
        second = _manifest(checkpoint_marker=2)

        first_id = registry.replace_active(None, first)
        second_id = registry.replace_active(first_id, second)

        self.assertEqual(registry.resolve(second_id), second)
        self.assertEqual(registry.snapshot.registered_manifests, 1)
        with self.assertRaisesRegex(BehaviorManifestError, "unknown"):
            registry.resolve(first_id)
        with self.assertRaisesRegex(BehaviorManifestError, "previous active"):
            registry.replace_active(first_id, first)
        self.assertEqual(registry.resolve(second_id), second)
        with self.assertRaisesRegex(BehaviorManifestError, "empty registry"):
            registry.replace_active(None, first)
        self.assertEqual(registry.resolve(second_id), second)

    def test_template_binds_one_checkpoint_and_training_step(self) -> None:
        manifest = _manifest()
        template = BehaviorManifestTemplate(
            model_definition=manifest.model_definition,
            model_config=manifest.model_config,
            behavior_rule=manifest.behavior_rule,
            semantic_schema=manifest.semantic_schema,
            optimizer_config=manifest.optimizer_config,
            trainer_implementation=manifest.trainer_implementation,
            semantic_schema_version=manifest.semantic_schema_version,
        )

        self.assertEqual(
            template.bind(manifest.model_checkpoint, training_step=10),
            manifest,
        )


if __name__ == "__main__":
    unittest.main()
