"""Pickle-free optimizer and categorical-generator resume components."""

from __future__ import annotations

from collections.abc import Mapping

import torch
from torch import nn

from ._torch_owner_state_codec import (
    TorchOwnerStateError,
    decode_owner_state,
    encode_owner_state,
)
from ._torch_checkpoint_codec import (
    TorchCheckpointError,
    decode_state_dict,
    encode_state_dict,
    validate_compatible_state,
)


class TorchResumeStateError(RuntimeError):
    """A mutable PyTorch owner cannot be restored exactly and atomically."""


_OPTIMIZER_COMPONENT = "optimizer_state"
_GENERATOR_COMPONENT = "categorical_generator_state"
_COMPONENT_VERSION = 1


def encode_shadow_model_state(model: nn.Module, *, max_bytes: int) -> bytes:
    """Encode the mutable shadow model independently from frozen behavior."""

    if not isinstance(model, nn.Module):
        raise TorchResumeStateError("shadow checkpoint requires a torch Module")
    try:
        return encode_state_dict(model.state_dict(), max_bytes=max_bytes)
    except TorchCheckpointError as error:
        raise TorchResumeStateError(str(error)) from error


def materialize_shadow_model_state(
    payload: bytes,
    factory,
    *,
    max_bytes: int,
) -> nn.Module:
    """Build a fresh shadow model and reject any partial or incompatible state."""

    if not isinstance(payload, bytes) or len(payload) > max_bytes:
        raise TorchResumeStateError("shadow checkpoint exceeds its byte limit")
    if not callable(factory):
        raise TorchResumeStateError("shadow checkpoint factory must be callable")
    try:
        state = decode_state_dict(payload)
        model = factory()
        if not isinstance(model, nn.Module):
            raise TorchResumeStateError("shadow checkpoint factory returned no Module")
        validate_compatible_state(model.state_dict(), state)
        model.load_state_dict(state, strict=True)
        if encode_shadow_model_state(model, max_bytes=max_bytes) != payload:
            raise TorchResumeStateError(
                "hydrated shadow model does not reproduce its canonical checkpoint"
            )
        return model
    except TorchCheckpointError as error:
        raise TorchResumeStateError(str(error)) from error


def encode_optimizer_state(
    optimizer: torch.optim.Optimizer,
    *,
    max_bytes: int,
) -> bytes:
    """Encode every tensor and scalar in one optimizer state dictionary."""

    if not isinstance(optimizer, torch.optim.Optimizer):
        raise TorchResumeStateError("optimizer checkpoint requires a torch Optimizer")
    try:
        return encode_owner_state(
            {
                "component": _OPTIMIZER_COMPONENT,
                "version": _COMPONENT_VERSION,
                "state_dict": optimizer.state_dict(),
            },
            max_bytes=max_bytes,
        )
    except TorchOwnerStateError as error:
        raise TorchResumeStateError(str(error)) from error


def hydrate_fresh_optimizer(
    optimizer: torch.optim.Optimizer,
    payload: bytes,
    *,
    max_bytes: int,
) -> None:
    """Hydrate a disposable fresh optimizer; discard it if this call fails."""

    if not isinstance(optimizer, torch.optim.Optimizer):
        raise TorchResumeStateError("optimizer restore requires a torch Optimizer")
    root = _decode_component(payload, max_bytes=max_bytes)
    if root.get("component") != _OPTIMIZER_COMPONENT:
        raise TorchResumeStateError("resume component is not optimizer state")
    state_dict = root.get("state_dict")
    if not isinstance(state_dict, Mapping):
        raise TorchResumeStateError("optimizer state_dict is not a mapping")
    _validate_optimizer_topology(optimizer.state_dict(), state_dict)
    try:
        optimizer.load_state_dict(dict(state_dict))
    except Exception as error:
        raise TorchResumeStateError("optimizer rejected restored state") from error
    if encode_optimizer_state(optimizer, max_bytes=max_bytes) != payload:
        raise TorchResumeStateError(
            "hydrated optimizer does not reproduce its canonical checkpoint"
        )


def encode_generator_state(
    generator: torch.Generator,
    *,
    max_bytes: int,
) -> bytes:
    """Encode one explicitly injected generator without touching global RNG."""

    if not isinstance(generator, torch.Generator):
        raise TorchResumeStateError("generator checkpoint requires torch.Generator")
    try:
        return encode_owner_state(
            {
                "component": _GENERATOR_COMPONENT,
                "version": _COMPONENT_VERSION,
                "device_type": generator.device.type,
                "state": generator.get_state(),
            },
            max_bytes=max_bytes,
        )
    except TorchOwnerStateError as error:
        raise TorchResumeStateError(str(error)) from error


def materialize_generator_state(
    payload: bytes,
    *,
    expected_device_type: str,
    max_bytes: int,
) -> torch.Generator:
    """Return a fresh generator only after exact canonical reconstruction."""

    root = _decode_component(payload, max_bytes=max_bytes)
    if root.get("component") != _GENERATOR_COMPONENT:
        raise TorchResumeStateError("resume component is not generator state")
    device_type = root.get("device_type")
    if device_type != expected_device_type:
        raise TorchResumeStateError(
            f"generator device {device_type!r} does not match {expected_device_type!r}"
        )
    state = root.get("state")
    if (
        not isinstance(state, torch.Tensor)
        or state.dtype is not torch.uint8
        or state.ndim != 1
    ):
        raise TorchResumeStateError("generator state must be one uint8 tensor")
    try:
        generator = torch.Generator(device=expected_device_type)
        generator.set_state(state)
    except Exception as error:
        raise TorchResumeStateError("generator rejected restored state") from error
    if encode_generator_state(generator, max_bytes=max_bytes) != payload:
        raise TorchResumeStateError(
            "hydrated generator does not reproduce its canonical checkpoint"
        )
    return generator


def _decode_component(payload: bytes, *, max_bytes: int) -> dict[object, object]:
    try:
        root = decode_owner_state(payload, max_bytes=max_bytes)
    except TorchOwnerStateError as error:
        raise TorchResumeStateError(str(error)) from error
    if not isinstance(root, dict):
        raise TorchResumeStateError("resume component root is not a mapping")
    if set(root) != {"component", "version", "state_dict"} and set(root) != {
        "component",
        "version",
        "device_type",
        "state",
    }:
        raise TorchResumeStateError("resume component fields are unsupported")
    if root.get("version") != _COMPONENT_VERSION:
        raise TorchResumeStateError("resume component version is unsupported")
    return root


def _validate_optimizer_topology(
    fresh: Mapping[str, object],
    restored: Mapping[object, object],
) -> None:
    if set(restored) != {"state", "param_groups"}:
        raise TorchResumeStateError("optimizer state_dict fields are unsupported")
    fresh_groups = fresh.get("param_groups")
    restored_groups = restored.get("param_groups")
    if not isinstance(fresh_groups, list) or not isinstance(restored_groups, list):
        raise TorchResumeStateError("optimizer param_groups must be lists")
    if len(fresh_groups) != len(restored_groups):
        raise TorchResumeStateError("optimizer parameter-group count does not match")
    restored_parameter_ids: set[int] = set()
    for fresh_group, restored_group in zip(
        fresh_groups, restored_groups, strict=True
    ):
        if not isinstance(fresh_group, Mapping) or not isinstance(
            restored_group, Mapping
        ):
            raise TorchResumeStateError("optimizer parameter group is not a mapping")
        fresh_parameters = fresh_group.get("params")
        restored_parameters = restored_group.get("params")
        if fresh_parameters != restored_parameters:
            raise TorchResumeStateError("optimizer parameter topology does not match")
        if not isinstance(restored_parameters, list) or not all(
            type(parameter_id) is int for parameter_id in restored_parameters
        ):
            raise TorchResumeStateError("optimizer parameter ids are malformed")
        for parameter_id in restored_parameters:
            if parameter_id in restored_parameter_ids:
                raise TorchResumeStateError("optimizer repeats a parameter id")
            restored_parameter_ids.add(parameter_id)
    restored_state = restored.get("state")
    if not isinstance(restored_state, Mapping):
        raise TorchResumeStateError("optimizer state rows must be a mapping")
    if not all(
        type(parameter_id) is int and parameter_id in restored_parameter_ids
        for parameter_id in restored_state
    ):
        raise TorchResumeStateError("optimizer state contains a foreign parameter id")
