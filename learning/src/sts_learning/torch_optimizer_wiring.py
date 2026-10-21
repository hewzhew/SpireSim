"""Shared exact optimizer-to-model ownership validation."""

from __future__ import annotations

import torch


class TorchOptimizerWiringError(ValueError):
    """An optimizer does not own exactly one model's parameters."""


def require_exact_optimizer_parameters(
    optimizer: torch.optim.Optimizer,
    model: torch.nn.Module,
) -> None:
    if not isinstance(optimizer, torch.optim.Optimizer):
        raise TorchOptimizerWiringError("optimizer must be a torch Optimizer")
    if not isinstance(model, torch.nn.Module):
        raise TorchOptimizerWiringError("model must be a torch Module")
    optimizer_parameters = tuple(
        parameter
        for group in optimizer.param_groups
        for parameter in group["params"]
    )
    model_parameters = tuple(model.parameters())
    optimizer_ids = tuple(id(parameter) for parameter in optimizer_parameters)
    model_ids = tuple(id(parameter) for parameter in model_parameters)
    if len(set(optimizer_ids)) != len(optimizer_ids):
        raise TorchOptimizerWiringError("optimizer repeats a model parameter")
    if set(optimizer_ids) != set(model_ids):
        raise TorchOptimizerWiringError(
            "optimizer does not own exactly the model parameters"
        )
