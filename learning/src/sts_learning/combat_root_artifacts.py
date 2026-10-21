"""Shared bounded import boundary for opaque combat-root artifacts."""

from __future__ import annotations

from pathlib import Path

from .torch_combat_session_config import (
    CombatSessionBridge,
    TorchCombatSessionError,
)


def normalize_combat_root_artifact(
    payload: bytes | bytearray | memoryview,
    *,
    max_bytes: int,
) -> bytes:
    if not isinstance(payload, (bytes, bytearray, memoryview)):
        raise TorchCombatSessionError("combat-root artifact must be bytes-like")
    normalized = bytes(payload)
    if not normalized:
        raise TorchCombatSessionError("combat-root artifact is empty")
    if len(normalized) > max_bytes:
        raise TorchCombatSessionError(
            "combat-root artifact exceeds its byte limit"
        )
    return normalized


def read_combat_root_artifact(
    artifact: str | Path,
    *,
    max_bytes: int,
) -> bytes:
    path = Path(artifact).resolve()
    if not path.is_file():
        raise TorchCombatSessionError("combat-root artifact is not a file")
    size = path.stat().st_size
    if size <= 0:
        raise TorchCombatSessionError("combat-root artifact is empty")
    if size > max_bytes:
        raise TorchCombatSessionError(
            "combat-root artifact exceeds its byte limit"
        )
    try:
        return normalize_combat_root_artifact(
            path.read_bytes(),
            max_bytes=max_bytes,
        )
    except OSError as error:
        raise TorchCombatSessionError(
            "combat-root artifact could not be read"
        ) from error


def load_combat_root_source(
    bridge: CombatSessionBridge,
    artifact: bytes,
    *,
    expected_roots: int,
    max_bytes: int,
) -> object:
    if not isinstance(bridge, CombatSessionBridge):
        raise TorchCombatSessionError(
            "combat-root artifact import requires a typed bridge"
        )
    try:
        source = bridge.combat_roots_from_artifact(
            artifact,
            expected_roots=expected_roots,
            max_bytes=max_bytes,
        )
    except Exception as error:
        raise TorchCombatSessionError(
            f"combat-root artifact import failed: {error}"
        ) from error
    if not callable(getattr(source, "combat_group", None)):
        raise TorchCombatSessionError(
            "combat-root artifact loader returned an invalid source"
        )
    return source
