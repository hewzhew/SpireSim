from __future__ import annotations

from pathlib import Path

import pytest

torch = pytest.importorskip("torch")

from learning.tests.test_torch_session import _factory


def test_session_warm_start_copies_compatible_scorer_without_aliasing(
    tmp_path: Path,
) -> None:
    source = _factory(tmp_path / "source").new(
        model_seed=43,
        behavior_seed=94,
    )
    source_scorer = source.runner.shadow_scorer
    with torch.no_grad():
        next(source_scorer.parameters()).fill_(0.125)

    target = _factory(tmp_path / "target").new(
        model_seed=999,
        behavior_seed=95,
        initial_scorer=source_scorer,
    )
    target_scorer = target.runner.shadow_scorer
    for source_value, target_value in zip(
        source_scorer.state_dict().values(),
        target_scorer.state_dict().values(),
        strict=True,
    ):
        assert torch.equal(source_value, target_value)
        assert source_value.data_ptr() != target_value.data_ptr()

    target_before = tuple(
        value.detach().clone() for value in target_scorer.state_dict().values()
    )
    with torch.no_grad():
        next(source_scorer.parameters()).add_(1.0)
    assert all(
        torch.equal(before, after)
        for before, after in zip(
            target_before,
            target_scorer.state_dict().values(),
            strict=True,
        )
    )
