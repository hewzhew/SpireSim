"""Publish one exact categorical generation resume point from live owners."""

from __future__ import annotations

import operator
from dataclasses import dataclass

from .resume_store import (
    BoundedResumeStore,
    ResumeComponentKind,
    ResumeManifestId,
    ResumeStoreError,
)
from .torch_generation import (
    BoundedCategoricalGenerationRunner,
    CategoricalGenerationResumeBoundary,
)
from .torch_behavior import TorchBehaviorError
from .torch_resume import (
    TorchResumeStateError,
    encode_generator_state,
    encode_optimizer_state,
    encode_shadow_model_state,
)
from .torch_resume_metadata import encode_generation_resume_state


class TorchResumePublicationError(RuntimeError):
    """A live generation cannot publish one exact durable resume point."""


@dataclass(frozen=True)
class CategoricalResumePayloadLimits:
    max_environment_bytes: int
    max_episode_root_bank_bytes: int
    max_shadow_model_bytes: int
    max_optimizer_bytes: int
    max_generator_bytes: int
    max_metadata_bytes: int

    def __post_init__(self) -> None:
        for name in (
            "max_environment_bytes",
            "max_episode_root_bank_bytes",
            "max_shadow_model_bytes",
            "max_optimizer_bytes",
            "max_generator_bytes",
            "max_metadata_bytes",
        ):
            object.__setattr__(self, name, _positive(getattr(self, name), name))


@dataclass(frozen=True)
class CategoricalResumePublication:
    manifest_id: ResumeManifestId
    boundary: CategoricalGenerationResumeBoundary


class CategoricalGenerationResumePublisher:
    """Capture all live owners, then atomically publish the manifest last."""

    def __init__(
        self,
        store: BoundedResumeStore,
        limits: CategoricalResumePayloadLimits,
    ) -> None:
        if not isinstance(store, BoundedResumeStore):
            raise TorchResumePublicationError("resume publisher requires a resume store")
        if not isinstance(limits, CategoricalResumePayloadLimits):
            raise TorchResumePublicationError("resume payload limits must be typed")
        self.store = store
        self.limits = limits

    def publish(
        self,
        runner: BoundedCategoricalGenerationRunner,
    ) -> CategoricalResumePublication:
        if not isinstance(runner, BoundedCategoricalGenerationRunner):
            raise TorchResumePublicationError("resume publication requires a generation runner")
        boundary = runner.require_resume_boundary()
        try:
            # Resume metadata names the active behavior manifest, so make that
            # exact frozen policy durable only at this explicit checkpoint.
            runner.controller.publish_active()
            environment = bytes(
                runner.driver.env.checkpoint_bytes(
                    max_bytes=self.limits.max_environment_bytes
                )
            )
            bank = bytes(
                runner.driver.checkpoint_bank.checkpoint_bytes(
                    max_bytes=self.limits.max_episode_root_bank_bytes
                )
            )
            payloads = {
                ResumeComponentKind.ENVIRONMENT: environment,
                ResumeComponentKind.EPISODE_ROOT_BANK: bank,
                ResumeComponentKind.SHADOW_MODEL: encode_shadow_model_state(
                    runner.shadow_scorer,
                    max_bytes=self.limits.max_shadow_model_bytes,
                ),
                ResumeComponentKind.OPTIMIZER: encode_optimizer_state(
                    runner.trainer.optimizer,
                    max_bytes=self.limits.max_optimizer_bytes,
                ),
                ResumeComponentKind.CATEGORICAL_GENERATOR: encode_generator_state(
                    runner.controller.generator,
                    max_bytes=self.limits.max_generator_bytes,
                ),
                ResumeComponentKind.GENERATION_METADATA: (
                    encode_generation_resume_state(
                        boundary,
                        optimizer_steps_per_generation=(
                            runner.optimizer_steps_per_generation
                        ),
                        max_bytes=self.limits.max_metadata_bytes,
                    )
                ),
            }
            prepared = self.store.prepare(payloads)
            manifest_id = self.store.commit(prepared)
        except (
            AttributeError,
            TypeError,
            ValueError,
            ResumeStoreError,
            TorchBehaviorError,
            TorchResumeStateError,
        ) as error:
            raise TorchResumePublicationError(str(error)) from error
        return CategoricalResumePublication(
            manifest_id=manifest_id,
            boundary=boundary,
        )


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchResumePublicationError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchResumePublicationError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise TorchResumePublicationError(f"{name} must be positive")
    return normalized
