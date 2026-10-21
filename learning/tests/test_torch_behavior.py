from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

from learning.tests.semantic_fixtures import (
    semantic_batch_fixture,
    semantic_schema_fixture,
)
from learning.tests.torch_outcome_fixtures import (
    behavior_manifest_fixture,
    behavior_manifest_template_fixture,
)
from sts_learning import (
    BehaviorManifestCatalogLimits,
    BehaviorManifestRegistry,
    BehaviorRuleBinding,
    BoundedBehaviorManifestCatalog,
    DecisionRunProgress,
    ManifestArtifactId,
    ManifestArtifactKind,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_behavior import (
        CategoricalTorchBehaviorController,
        CheckpointedCategoricalTorchPolicy,
        CheckpointedGreedyTorchPolicy,
        FrozenCombatAnchor,
        FrozenCombatGreedyTorchPolicy,
        FrozenDecisionRule,
        FrozenGreedyTorchPolicy,
        TorchBehaviorError,
        TorchBehaviorPublisher,
    )
    from sts_learning.manifests import (
        combat_anchored_greedy_strategic_sampled_rule_v1,
        combat_greedy_strategic_sampled_rule_v1,
    )
    from sts_learning.torch_checkpoints import (
        BoundedTorchCheckpointStore,
        TorchCheckpointError,
        TorchCheckpointLimits,
    )
    from sts_learning.torch_policy import (
        RaggedCandidateScorer,
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )


def _scorer(*, schema=None):
    return RaggedCandidateScorer.from_bridge_schema(
        schema or semantic_schema_fixture(),
        RaggedScorerConfig(hidden_dim=8, relation_layers=1),
    )


def _store(root, *, checkpoints: int = 3):
    return BoundedTorchCheckpointStore(
        root,
        TorchCheckpointLimits(
            max_checkpoints=checkpoints,
            max_bytes_per_checkpoint=2 * 1024 * 1024,
            max_total_bytes=checkpoints * 2 * 1024 * 1024,
        ),
    )


def _catalog(root, *, manifests: int = 3):
    return BoundedBehaviorManifestCatalog(
        root,
        BehaviorManifestCatalogLimits(
            max_manifests=manifests,
            max_bytes_per_manifest=1024,
            max_total_bytes=manifests * 1024,
        ),
    )


def _alternative_behavior_rule() -> BehaviorRuleBinding:
    return BehaviorRuleBinding(
        implementation=ManifestArtifactId.from_content(
            ManifestArtifactKind.BEHAVIOR_RULE,
            b"test.alternative_behavior_rule\x00v1",
        ),
        configuration=ManifestArtifactId.from_content(
            ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
            b"test.alternative_behavior_rule.config\x00v1",
        ),
    )


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class TorchBehaviorPublicationTests(unittest.TestCase):
    def test_promotion_uses_fresh_frozen_checkpoint_not_live_shadow_model(self) -> None:
        torch.manual_seed(21)
        shadow = _scorer()
        batch = semantic_batch_fixture()
        expected = shadow(batch).values.detach().clone()
        registry = BehaviorManifestRegistry(capacity=3)

        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publication = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            ).publish(shadow, training_step=4)
            with torch.no_grad():
                shadow.scorer[-1].bias.add_(20.0)

            policy = CheckpointedGreedyTorchPolicy.promote(
                publication,
                store,
                catalog,
                registry,
                _scorer,
            )
            actual = policy.score(batch).values
            choice = policy.choose(batch)

            torch.testing.assert_close(actual, expected)
            self.assertFalse(actual.requires_grad)
            self.assertEqual(choice.behavior_manifest_id, publication.manifest_id)
            self.assertEqual(publication.manifest.training_step, 4)
            self.assertNotEqual(shadow(batch).values.detach().tolist(), expected.tolist())

    def test_registry_capacity_failure_publishes_no_checkpoint(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        registry.register(behavior_manifest_fixture())
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            )

            with self.assertRaisesRegex(ValueError, "capacity"):
                publisher.publish(_scorer(), training_step=1)
            self.assertEqual(store.snapshot.checkpoints, 0)
            self.assertEqual(catalog.snapshot.manifests, 0)

    def test_store_failure_registers_no_manifest(self) -> None:
        registry = BehaviorManifestRegistry(capacity=2)
        torch.manual_seed(22)
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"), checkpoints=1)
            catalog = _catalog(Path(root, "manifests"))
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            )
            publisher.publish(_scorer(), training_step=0)
            updated = _scorer()

            with self.assertRaisesRegex(TorchCheckpointError, "capacity"):
                publisher.publish(updated, training_step=1)
            self.assertEqual(registry.snapshot.registered_manifests, 1)

    def test_exact_preview_is_idempotent_but_novel_preview_requires_capacity(
        self,
    ) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        scorer = _scorer()
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"), checkpoints=1)
            catalog = _catalog(Path(root, "manifests"), manifests=1)
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            )
            publication = publisher.publish(scorer, training_step=0)

            exact = publisher.preview(scorer, training_step=0)
            self.assertEqual(exact.manifest_id, publication.manifest_id)
            self.assertEqual(exact.checkpoint_id, publication.checkpoint_id)
            self.assertEqual(exact.training_step, 0)
            self.assertGreater(exact.checkpoint_payload_bytes, 0)
            self.assertGreater(exact.manifest_payload_bytes, 0)
            self.assertFalse(exact.requires_novel_capacity)
            with self.assertRaisesRegex(TorchBehaviorError, "typed publication"):
                CheckpointedGreedyTorchPolicy.promote(
                    exact,  # type: ignore[arg-type]
                    store,
                    catalog,
                    registry,
                    _scorer,
                )
            with self.assertRaisesRegex(TorchCheckpointError, "capacity"):
                publisher.preview_novel(scorer, training_step=1)

            self.assertEqual(store.snapshot.checkpoints, 1)
            self.assertEqual(catalog.snapshot.manifests, 1)
            self.assertEqual(registry.snapshot.registered_manifests, 1)

    def test_catalog_capacity_failure_publishes_no_checkpoint_or_registry_row(self) -> None:
        registry = BehaviorManifestRegistry(capacity=2)
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"), manifests=1)
            catalog.commit(catalog.prepare(behavior_manifest_fixture()))
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            )

            with self.assertRaisesRegex(RuntimeError, "capacity"):
                publisher.publish(_scorer(), training_step=8)
            self.assertEqual(store.snapshot.checkpoints, 0)
            self.assertEqual(registry.snapshot.registered_manifests, 0)

    def test_training_step_changes_manifest_but_reuses_identical_checkpoint(self) -> None:
        registry = BehaviorManifestRegistry(capacity=2)
        scorer = _scorer()
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            )

            first = publisher.publish(scorer, training_step=2)
            second = publisher.publish(scorer, training_step=3)

            self.assertEqual(first.checkpoint_id, second.checkpoint_id)
            self.assertNotEqual(first.manifest_id, second.manifest_id)
            self.assertEqual(store.snapshot.checkpoints, 1)
            self.assertEqual(registry.snapshot.registered_manifests, 2)

    def test_behavior_rule_changes_manifest_and_greedy_recovery_fails_closed(
        self,
    ) -> None:
        registry = BehaviorManifestRegistry(capacity=2)
        scorer = _scorer()
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            greedy = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            ).publish(scorer, training_step=0)
            alternative = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    behavior_rule=_alternative_behavior_rule(),
                ),
            ).publish(scorer, training_step=0)

            self.assertEqual(greedy.checkpoint_id, alternative.checkpoint_id)
            self.assertNotEqual(greedy.manifest_id, alternative.manifest_id)
            with self.assertRaisesRegex(TorchBehaviorError, "greedy candidate rule"):
                CheckpointedGreedyTorchPolicy.promote(
                    alternative,
                    store,
                    catalog,
                    registry,
                    _scorer,
                )
            fresh_registry = BehaviorManifestRegistry(capacity=1)
            with self.assertRaisesRegex(TorchBehaviorError, "greedy candidate rule"):
                CheckpointedGreedyTorchPolicy.recover(
                    alternative.manifest_id,
                    store,
                    catalog,
                    fresh_registry,
                    _scorer,
                )
            self.assertEqual(fresh_registry.snapshot.registered_manifests, 0)

    def test_categorical_promotion_and_recovery_reproduce_injected_rng(self) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.75)
        registry = BehaviorManifestRegistry(capacity=1)
        batch = semantic_batch_fixture()
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publication = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    behavior_rule=config.behavior_rule,
                ),
            ).publish(_scorer(), training_step=0)
            promoted = CheckpointedCategoricalTorchPolicy.promote(
                publication,
                store,
                catalog,
                registry,
                _scorer,
                config,
                torch.Generator().manual_seed(55),
            )
            recovered_registry = BehaviorManifestRegistry(capacity=1)
            recovered = CheckpointedCategoricalTorchPolicy.recover(
                publication.manifest_id,
                store,
                catalog,
                recovered_registry,
                _scorer,
                config,
                torch.Generator().manual_seed(55),
            )
            global_state = torch.random.get_rng_state().clone()

            promoted_choice = promoted.choose(batch)
            recovered_choice = recovered.choose(batch)

            self.assertEqual(promoted_choice, recovered_choice)
            self.assertTrue(torch.equal(torch.random.get_rng_state(), global_state))
            self.assertEqual(
                promoted_choice.behavior_manifest_id,
                publication.manifest_id,
            )
            self.assertTrue(
                all(
                    probability.value is not None
                    and 0.0 < probability.value <= 1.0
                    for probability in promoted_choice.selection_probabilities
                )
            )
            self.assertEqual(
                recovered_registry.resolve(publication.manifest_id),
                publication.manifest,
            )

    def test_combat_scoped_greedy_preserves_strategic_sampling(self) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.75)
        registry = BehaviorManifestRegistry(capacity=1)
        batch = semantic_batch_fixture()

        class _ProgressProvider:
            def __init__(self) -> None:
                self.calls: list[tuple[int, ...]] = []

            def capture(self, slots):
                normalized = tuple(slots)
                self.calls.append(normalized)
                return (
                    DecisionRunProgress(100, 1, 3, True, None),
                    DecisionRunProgress(101, 1, 4, False, 2),
                )

        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publication = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    behavior_rule=config.behavior_rule,
                ),
            ).publish(_scorer(), training_step=0)
            source = CheckpointedCategoricalTorchPolicy.promote(
                publication,
                store,
                catalog,
                registry,
                _scorer,
                config,
                torch.Generator().manual_seed(55),
            )
            sampled_reference = source.fork(torch.Generator().manual_seed(55))
            provider = _ProgressProvider()
            scoped = FrozenCombatGreedyTorchPolicy.from_categorical(
                source,
                provider,
            )

            expected_greedy = source.score(batch).greedy_ordinals()
            expected_sampled = sampled_reference.choose(batch)
            choice = scoped.choose(batch)

            self.assertEqual(provider.calls, [(4, 9)])
            self.assertEqual(choice.ordinals[0], expected_greedy[0])
            self.assertEqual(choice.selection_probabilities[0].value, 1.0)
            self.assertEqual(choice.ordinals[1], expected_sampled.ordinals[1])
            self.assertEqual(
                choice.selection_probabilities[1],
                expected_sampled.selection_probabilities[1],
            )
            self.assertEqual(scoped.source_manifest_id, source.behavior_manifest_id)
            self.assertNotEqual(
                scoped.behavior_manifest_id,
                source.behavior_manifest_id,
            )

    def test_combat_scoped_policy_can_use_a_distinct_verified_anchor(self) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.75)

        class _ProgressProvider:
            def capture(self, slots):
                assert tuple(slots) == (4, 9)
                return (
                    DecisionRunProgress(100, 1, 3, True, None),
                    DecisionRunProgress(101, 1, 4, False, 2),
                )

        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            template = behavior_manifest_template_fixture(
                behavior_rule=config.behavior_rule,
            )
            strategic_registry = BehaviorManifestRegistry(capacity=1)
            combat_registry = BehaviorManifestRegistry(capacity=1)
            with torch.random.fork_rng(devices=[]):
                torch.manual_seed(1)
                strategic_scorer = _scorer()
                torch.manual_seed(2)
                combat_scorer = _scorer()
            strategic_publication = TorchBehaviorPublisher(
                store,
                catalog,
                strategic_registry,
                template,
            ).publish(strategic_scorer, training_step=0)
            combat_publication = TorchBehaviorPublisher(
                store,
                catalog,
                combat_registry,
                template,
            ).publish(combat_scorer, training_step=0)
            strategic = CheckpointedCategoricalTorchPolicy.promote(
                strategic_publication,
                store,
                catalog,
                strategic_registry,
                _scorer,
                config,
                torch.Generator().manual_seed(55),
            )
            combat = CheckpointedCategoricalTorchPolicy.promote(
                combat_publication,
                store,
                catalog,
                combat_registry,
                _scorer,
                config,
                torch.Generator().manual_seed(77),
            )
            sampled_reference = strategic.fork(torch.Generator().manual_seed(55))
            anchor = FrozenCombatAnchor.from_behavior(combat)
            scoped = FrozenCombatGreedyTorchPolicy.from_categorical(
                strategic,
                _ProgressProvider(),
                anchor,
            )
            batch = semantic_batch_fixture()
            choice = scoped.choose(batch)

            self.assertEqual(
                choice.ordinals[0],
                anchor.scorer(batch).greedy_ordinals()[0],
            )
            self.assertEqual(
                choice.ordinals[1],
                sampled_reference.choose(batch).ordinals[1],
            )
            self.assertEqual(scoped.source_manifest_id, strategic.behavior_manifest_id)
            self.assertEqual(scoped.combat_anchor.manifest_id, combat.behavior_manifest_id)
            self.assertEqual(
                scoped.binding.manifest.behavior_rule,
                combat_anchored_greedy_strategic_sampled_rule_v1(
                    config.behavior_rule,
                    combat.behavior_manifest_id,
                ),
            )

    def test_combat_scoped_controller_publishes_and_recovers_exact_mixed_rule(
        self,
    ) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.75)
        mixed_rule = combat_greedy_strategic_sampled_rule_v1(
            config.behavior_rule
        )

        class _ProgressProvider:
            def capture(self, slots):
                assert tuple(slots) == (4, 9)
                return (
                    DecisionRunProgress(100, 1, 3, True, None),
                    DecisionRunProgress(101, 1, 4, False, 2),
                )

        provider = _ProgressProvider()
        batch = semantic_batch_fixture()
        # This scenario must cross source publication, two strategic promotions,
        # durable mixed recovery, and greedy combat derivation to protect the
        # anchor identity chain rather than one wrapper in isolation.
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            registry = BehaviorManifestRegistry(capacity=3)
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    behavior_rule=mixed_rule,
                ),
            )
            controller = CategoricalTorchBehaviorController(
                publisher,
                _scorer,
                config,
                torch.Generator().manual_seed(55),
                combat_decision_rule=FrozenDecisionRule.GREEDY,
                progress_provider=provider,
            )
            binding = controller.promote_live(_scorer(), training_step=0)
            choice = controller.choose(batch)

            self.assertEqual(binding.manifest.behavior_rule, mixed_rule)
            self.assertEqual(choice.behavior_manifest_id, binding.manifest_id)
            self.assertEqual(choice.selection_probabilities[0].value, 1.0)
            self.assertIsNotNone(choice.selection_probabilities[1].value)
            publication = controller.publish_active()

            recovered = FrozenCombatGreedyTorchPolicy.recover(
                publication.manifest_id,
                store,
                catalog,
                BehaviorManifestRegistry(capacity=1),
                _scorer,
                config,
                torch.Generator().manual_seed(55),
            )
            with self.assertRaisesRegex(TorchBehaviorError, "not bound"):
                recovered.choose(batch)
            recovered_choice = recovered.bind_progress_provider(provider).choose(
                batch
            )
            self.assertEqual(
                recovered_choice.behavior_manifest_id,
                publication.manifest_id,
            )
            self.assertEqual(
                recovered_choice.selection_probabilities[0].value,
                1.0,
            )
            combat_only = recovered.bind_combat_only().choose(batch)
            self.assertTrue(
                all(
                    probability.value == 1.0
                    for probability in combat_only.selection_probabilities
                )
            )
            fully_greedy = FrozenGreedyTorchPolicy.from_behavior(recovered)
            self.assertNotEqual(
                fully_greedy.behavior_manifest_id,
                recovered.behavior_manifest_id,
            )
            self.assertTrue(
                all(
                    probability.value == 1.0
                    for probability in fully_greedy.choose(
                        batch
                    ).selection_probabilities
                )
            )

    def test_combat_anchor_survives_strategic_promotion_and_recovery(self) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.75)

        class _ProgressProvider:
            def capture(self, slots):
                assert tuple(slots) == (4, 9)
                return (
                    DecisionRunProgress(100, 1, 3, True, None),
                    DecisionRunProgress(101, 1, 4, False, 2),
                )

        provider = _ProgressProvider()
        batch = semantic_batch_fixture()
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            with torch.random.fork_rng(devices=[]):
                torch.manual_seed(1)
                anchor_scorer = _scorer()
            anchor_registry = BehaviorManifestRegistry(capacity=1)
            anchor_publication = TorchBehaviorPublisher(
                store,
                catalog,
                anchor_registry,
                behavior_manifest_template_fixture(
                    behavior_rule=config.behavior_rule,
                ),
            ).publish(anchor_scorer, training_step=0)
            anchor_policy = CheckpointedCategoricalTorchPolicy.promote(
                anchor_publication,
                store,
                catalog,
                anchor_registry,
                _scorer,
                config,
                torch.Generator().manual_seed(19),
            )
            anchor = FrozenCombatAnchor.from_behavior(anchor_policy)
            anchored_rule = combat_anchored_greedy_strategic_sampled_rule_v1(
                config.behavior_rule,
                anchor.manifest_id,
            )
            controller = CategoricalTorchBehaviorController(
                TorchBehaviorPublisher(
                    store,
                    catalog,
                    BehaviorManifestRegistry(capacity=3),
                    behavior_manifest_template_fixture(
                        behavior_rule=anchored_rule,
                    ),
                ),
                _scorer,
                config,
                torch.Generator().manual_seed(55),
                combat_decision_rule=FrozenDecisionRule.GREEDY,
                progress_provider=provider,
                combat_anchor=anchor,
            )
            with torch.random.fork_rng(devices=[]):
                torch.manual_seed(2)
                first_strategic = _scorer()
            # Require a controlled disagreement with the combat anchor, rather
            # than hoping two random initializations choose different actions.
            first_strategic.load_state_dict(anchor_scorer.state_dict())
            with torch.no_grad(): first_strategic.scorer[-1].weight.neg_()
            first_binding = controller.promote_live(
                first_strategic,
                training_step=0,
            )
            first_policy = controller.fork_active(
                torch.Generator().manual_seed(71)
            )
            anchor_ordinals = anchor.scorer(batch).greedy_ordinals()
            strategic_ordinals = first_policy.score(batch).greedy_ordinals()
            self.assertNotEqual(anchor_ordinals[0], strategic_ordinals[0])
            self.assertEqual(
                controller.choose(batch).ordinals[0],
                anchor_ordinals[0],
            )

            with torch.random.fork_rng(devices=[]):
                torch.manual_seed(3)
                second_strategic = _scorer()
            second_binding = controller.promote_live(
                second_strategic,
                training_step=1,
            )
            second_policy = controller.fork_active(
                torch.Generator().manual_seed(71)
            )
            self.assertNotEqual(
                first_binding.manifest_id,
                second_binding.manifest_id,
            )
            self.assertFalse(
                torch.equal(
                    first_policy.score(batch).values,
                    second_policy.score(batch).values,
                )
            )
            self.assertEqual(
                controller.choose(batch).ordinals[0],
                anchor_ordinals[0],
            )

            publication = controller.publish_active()
            recovered_anchor = FrozenCombatAnchor.recover(
                anchor.manifest_id,
                store,
                catalog,
                _scorer,
            )
            recovered = FrozenCombatGreedyTorchPolicy.recover(
                publication.manifest_id,
                store,
                catalog,
                BehaviorManifestRegistry(capacity=1),
                _scorer,
                config,
                torch.Generator().manual_seed(91),
                provider,
                recovered_anchor,
            )
            self.assertEqual(
                recovered.combat_anchor.manifest_id,
                anchor.manifest_id,
            )
            self.assertEqual(
                recovered.choose(batch).ordinals[0],
                anchor_ordinals[0],
            )
            self.assertEqual(
                FrozenGreedyTorchPolicy.from_behavior(recovered)
                .choose(batch)
                .ordinals,
                tuple(anchor_ordinals),
            )

    def test_categorical_controller_rotates_live_behavior_then_publishes_explicitly(self) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.75)
        registry = BehaviorManifestRegistry(capacity=3)
        generator = torch.Generator().manual_seed(71)
        shadow = _scorer()
        batch = semantic_batch_fixture()
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    behavior_rule=config.behavior_rule,
                ),
            )
            controller = CategoricalTorchBehaviorController(
                publisher,
                _scorer,
                config,
                generator,
            )
            first = controller.promote_live(shadow, training_step=0)
            with self.assertRaisesRegex(TorchBehaviorError, "must increase"):
                controller.promote_live(shadow, training_step=0)
            self.assertEqual(store.snapshot.checkpoints, 0)
            self.assertEqual(catalog.snapshot.manifests, 0)
            self.assertEqual(registry.snapshot.registered_manifests, 1)
            with torch.no_grad():
                shadow.scorer[-1].bias.add_(1.0)
            state_before = generator.get_state().clone()
            second = controller.promote_live(shadow, training_step=1)

            self.assertNotEqual(first.manifest_id, second.manifest_id)
            self.assertTrue(torch.equal(generator.get_state(), state_before))
            self.assertEqual(controller.snapshot.active_manifest_id, second.manifest_id)
            self.assertEqual(controller.snapshot.active_training_step, 1)
            self.assertEqual(controller.snapshot.successful_promotions, 2)
            self.assertEqual(store.snapshot.checkpoints, 0)
            self.assertEqual(catalog.snapshot.manifests, 0)
            self.assertEqual(registry.snapshot.registered_manifests, 1)
            durable_second = controller.publish_active()
            self.assertEqual(durable_second.manifest_id, second.manifest_id)
            self.assertEqual(store.snapshot.checkpoints, 1)
            self.assertEqual(catalog.snapshot.manifests, 1)

            recovered_registry = BehaviorManifestRegistry(capacity=1)
            recovered = CategoricalTorchBehaviorController(
                TorchBehaviorPublisher(
                    store,
                    catalog,
                    recovered_registry,
                    behavior_manifest_template_fixture(
                        behavior_rule=config.behavior_rule,
                    ),
                ),
                _scorer,
                config,
                torch.Generator().manual_seed(71),
            )
            recovered_publication = recovered.recover_and_promote(
                second.manifest_id,
                successful_promotions=controller.snapshot.successful_promotions,
            )

            self.assertEqual(recovered_publication, durable_second)
            self.assertEqual(recovered.snapshot, controller.snapshot)
            self.assertEqual(controller.choose(batch), recovered.choose(batch))

    def test_categorical_controller_keeps_old_policy_when_promotion_fails(self) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.75)
        registry = BehaviorManifestRegistry(capacity=3)
        generator = torch.Generator().manual_seed(72)
        shadow = _scorer()

        class FailSecondFactory:
            def __init__(self) -> None:
                self.calls = 0

            def __call__(self):
                self.calls += 1
                if self.calls == 2:
                    mismatched = dict(semantic_schema_fixture())
                    mismatched["version"] = 99
                    return _scorer(schema=mismatched)
                return _scorer()

        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            controller = CategoricalTorchBehaviorController(
                TorchBehaviorPublisher(
                    store,
                    catalog,
                    registry,
                    behavior_manifest_template_fixture(
                        behavior_rule=config.behavior_rule,
                    ),
                ),
                FailSecondFactory(),
                config,
                generator,
            )
            first = controller.promote_live(shadow, training_step=0)
            with torch.no_grad():
                shadow.scorer[-1].bias.add_(1.0)
            generator_state = generator.get_state().clone()

            with self.assertRaisesRegex(TorchBehaviorError, "schema version"):
                controller.promote_live(shadow, training_step=1)

            self.assertEqual(controller.snapshot.active_manifest_id, first.manifest_id)
            self.assertEqual(controller.snapshot.successful_promotions, 1)
            self.assertTrue(torch.equal(generator.get_state(), generator_state))
            second = controller.promote_live(shadow, training_step=1)
            self.assertEqual(controller.snapshot.active_manifest_id, second.manifest_id)
            self.assertEqual(controller.snapshot.successful_promotions, 2)
            self.assertEqual(store.snapshot.checkpoints, 0)
            self.assertEqual(catalog.snapshot.manifests, 0)
            self.assertEqual(registry.snapshot.registered_manifests, 1)

    def test_categorical_rule_mismatch_fails_before_rng_or_registry_mutation(
        self,
    ) -> None:
        config = RaggedCategoricalPolicyConfig(temperature=0.5)
        wrong_config = RaggedCategoricalPolicyConfig(temperature=1.0)
        registry = BehaviorManifestRegistry(capacity=1)
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publication = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    behavior_rule=config.behavior_rule,
                ),
            ).publish(_scorer(), training_step=0)
            generator = torch.Generator().manual_seed(66)
            generator_state = generator.get_state().clone()
            fresh_registry = BehaviorManifestRegistry(capacity=1)

            with self.assertRaisesRegex(
                TorchBehaviorError,
                "categorical candidate rule",
            ):
                CheckpointedCategoricalTorchPolicy.recover(
                    publication.manifest_id,
                    store,
                    catalog,
                    fresh_registry,
                    _scorer,
                    wrong_config,
                    generator,
                )
            self.assertEqual(fresh_registry.snapshot.registered_manifests, 0)
            self.assertTrue(torch.equal(generator.get_state(), generator_state))
            with self.assertRaisesRegex(TorchBehaviorError, "global generator"):
                CheckpointedCategoricalTorchPolicy.promote(
                    publication,
                    store,
                    catalog,
                    registry,
                    _scorer,
                    config,
                    torch.default_generator,
                )

    def test_unregistered_or_schema_mismatched_publication_cannot_run(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publication = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(),
            ).publish(_scorer(), training_step=0)

            with self.assertRaisesRegex(TorchBehaviorError, "not registered"):
                CheckpointedGreedyTorchPolicy.promote(
                    publication,
                    store,
                    catalog,
                    BehaviorManifestRegistry(capacity=1),
                    _scorer,
                )

        mismatched_schema = dict(semantic_schema_fixture())
        mismatched_schema["version"] = 9
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                BehaviorManifestRegistry(capacity=1),
                behavior_manifest_template_fixture(),
            )
            with self.assertRaisesRegex(TorchBehaviorError, "schema version"):
                publisher.publish(_scorer(schema=mismatched_schema), training_step=0)
            self.assertEqual(store.snapshot.checkpoints, 0)

    def test_missing_checkpoint_does_not_partly_hydrate_recovery_registry(self) -> None:
        manifest = behavior_manifest_fixture()
        with tempfile.TemporaryDirectory() as root:
            store = _store(Path(root, "checkpoints"))
            catalog = _catalog(Path(root, "manifests"))
            catalog.commit(catalog.prepare(manifest))
            registry = BehaviorManifestRegistry(capacity=1)

            with self.assertRaisesRegex(TorchCheckpointError, "unknown model"):
                CheckpointedGreedyTorchPolicy.recover(
                    manifest.identity,
                    store,
                    catalog,
                    registry,
                    _scorer,
                )
            self.assertEqual(registry.snapshot.registered_manifests, 0)


if __name__ == "__main__":
    unittest.main()
