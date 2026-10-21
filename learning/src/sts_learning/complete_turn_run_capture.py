"""Run-grouped loader for replay-verified complete-turn teacher lines.

The manifest and file layout are ordinary corpus metadata. Combat replay and
player-turn boundary detection remain Rust-owned and enter through one bridge
call per captured combat.
"""

from __future__ import annotations

import json
import operator
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class CompleteTurnRunCaptureError(ValueError):
    """A captured run is malformed, incomplete, or not replay-verifiable."""


@dataclass(frozen=True)
class ReplayVerifiedCompleteTurn:
    from_turn: int
    to_turn: int
    boundary: str
    inputs_json: bytes
    input_prefixes_json: tuple[bytes, ...]


@dataclass(frozen=True)
class CompleteTurnCapturedCombat:
    ordinal: int
    act: int
    floor: int
    encounter_id: str | None
    public_snapshot_id: str
    root_exact_state_hash: str
    run_session_fingerprint: str
    case_path: Path
    actions_path: Path
    case_payload: bytes
    actions_payload: bytes
    action_count: int
    turns: tuple[ReplayVerifiedCompleteTurn, ...]


@dataclass(frozen=True)
class CompleteTurnRunCapture:
    manifest_path: Path
    source_receipt_path: Path
    seed: int
    episode_outcome: Mapping[str, object]
    curriculum_eligible: bool
    curriculum_ineligibility: str | None
    captured_combat_entries: int
    committed_combat_actions: int
    actor_installed_plans: int
    actor_plan_actions: int
    combats: tuple[CompleteTurnCapturedCombat, ...]

    def require_curriculum_eligible(self) -> None:
        """Fail before a diagnostic capture is promoted into a curriculum."""

        if not self.curriculum_eligible:
            reason = self.curriculum_ineligibility or "unspecified ineligibility"
            raise CompleteTurnRunCaptureError(
                f"combat run capture is not curriculum eligible: {reason}"
            )


WinningLineSplitter = Callable[[bytes, bytes], Mapping[str, Any]]


def load_complete_turn_run_capture(
    manifest_path: str | Path,
    *,
    winning_line_splitter: WinningLineSplitter | None = None,
) -> CompleteTurnRunCapture:
    """Load one whole run without splitting its combats across data partitions."""

    manifest_path = Path(manifest_path).resolve()
    try:
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise CompleteTurnRunCaptureError(
            f"failed to read combat run manifest {manifest_path}: {error}"
        ) from error
    manifest = _mapping(payload, "combat run manifest")
    if manifest.get("schema") != "sts-combat-run-capture-v1":
        raise CompleteTurnRunCaptureError(
            "expected sts-combat-run-capture-v1 combat run manifest"
        )
    splitter = winning_line_splitter or _native_winning_line_splitter()
    directory = manifest_path.parent
    source_receipt_path = _local_file(
        directory,
        _string(manifest.get("source_receipt_file"), "source_receipt_file"),
        "source receipt",
    )
    if not source_receipt_path.is_file():
        raise CompleteTurnRunCaptureError(
            f"combat run source receipt does not exist: {source_receipt_path}"
        )
    seed = _non_negative_int(manifest.get("seed"), "seed")
    eligible = manifest.get("curriculum_eligible")
    if not isinstance(eligible, bool):
        raise CompleteTurnRunCaptureError("curriculum_eligible must be boolean")
    ineligibility = manifest.get("curriculum_ineligibility")
    if ineligibility is not None and not isinstance(ineligibility, str):
        raise CompleteTurnRunCaptureError(
            "curriculum_ineligibility must be a string or null"
        )
    raw_combats = manifest.get("combats")
    if not isinstance(raw_combats, list):
        raise CompleteTurnRunCaptureError("combats must be a list")

    combats: list[CompleteTurnCapturedCombat] = []
    seen_files: set[Path] = set()
    seen_snapshots: set[str] = set()
    for expected_ordinal, raw_entry in enumerate(raw_combats):
        entry = _mapping(raw_entry, f"combat {expected_ordinal}")
        ordinal = _non_negative_int(entry.get("ordinal"), "combat ordinal")
        if ordinal != expected_ordinal:
            raise CompleteTurnRunCaptureError(
                f"combat ordinal {ordinal} is not contiguous at {expected_ordinal}"
            )
        case_path = _local_file(
            directory,
            _string(entry.get("case_file"), f"combat {ordinal} case_file"),
            f"combat {ordinal} case",
        )
        actions_path = _local_file(
            directory,
            _string(entry.get("actions_file"), f"combat {ordinal} actions_file"),
            f"combat {ordinal} actions",
        )
        for path in (case_path, actions_path):
            if path in seen_files:
                raise CompleteTurnRunCaptureError(
                    f"combat run capture repeats file {path.name}"
                )
            seen_files.add(path)
        try:
            case_payload = case_path.read_bytes()
            actions_payload = actions_path.read_bytes()
        except OSError as error:
            raise CompleteTurnRunCaptureError(
                f"failed to read combat {ordinal} capture files: {error}"
            ) from error
        try:
            split = splitter(case_payload, actions_payload)
        except Exception as error:
            raise CompleteTurnRunCaptureError(
                f"combat {ordinal} exact winning-line replay failed: {error}"
            ) from error
        action_count = _non_negative_int(split.get("action_count"), "action_count")
        expected_action_count = _non_negative_int(
            entry.get("action_count"), f"combat {ordinal} action_count"
        )
        if action_count != expected_action_count:
            raise CompleteTurnRunCaptureError(
                f"combat {ordinal} action count mismatch: manifest={expected_action_count} replay={action_count}"
            )
        turns = _verified_turns(split, ordinal)
        snapshot = _string(
            entry.get("public_snapshot_id"), f"combat {ordinal} public_snapshot_id"
        )
        if snapshot in seen_snapshots:
            raise CompleteTurnRunCaptureError(
                f"combat run capture repeats public snapshot {snapshot}"
            )
        seen_snapshots.add(snapshot)
        encounter = entry.get("encounter_id")
        if encounter is not None and not isinstance(encounter, str):
            raise CompleteTurnRunCaptureError(
                f"combat {ordinal} encounter_id must be a string or null"
            )
        outcome = _mapping(entry.get("outcome"), f"combat {ordinal} outcome")
        if outcome.get("terminal") != "win":
            raise CompleteTurnRunCaptureError(
                f"combat {ordinal} manifest outcome is not win"
            )
        combats.append(
            CompleteTurnCapturedCombat(
                ordinal=ordinal,
                act=_non_negative_int(entry.get("act"), f"combat {ordinal} act"),
                floor=_non_negative_int(entry.get("floor"), f"combat {ordinal} floor"),
                encounter_id=encounter,
                public_snapshot_id=snapshot,
                root_exact_state_hash=_string(
                    entry.get("root_exact_state_hash"),
                    f"combat {ordinal} root_exact_state_hash",
                ),
                run_session_fingerprint=_string(
                    entry.get("run_session_fingerprint"),
                    f"combat {ordinal} run_session_fingerprint",
                ),
                case_path=case_path,
                actions_path=actions_path,
                case_payload=case_payload,
                actions_payload=actions_payload,
                action_count=action_count,
                turns=turns,
            )
        )

    committed_count = _non_negative_int(
        manifest.get("committed_combat_count"), "committed_combat_count"
    )
    if committed_count != len(combats):
        raise CompleteTurnRunCaptureError(
            f"committed combat count mismatch: manifest={committed_count} loaded={len(combats)}"
        )
    committed_actions = _non_negative_int(
        manifest.get("committed_combat_actions"), "committed_combat_actions"
    )
    loaded_actions = sum(combat.action_count for combat in combats)
    if committed_actions != loaded_actions:
        raise CompleteTurnRunCaptureError(
            f"committed action count mismatch: manifest={committed_actions} loaded={loaded_actions}"
        )
    captured_entries = _non_negative_int(
        manifest.get("captured_combat_entries"), "captured_combat_entries"
    )
    actor_installed_plans = _non_negative_int(
        manifest.get("actor_installed_plans"), "actor_installed_plans"
    )
    actor_plan_actions = _non_negative_int(
        manifest.get("actor_plan_actions"), "actor_plan_actions"
    )
    episode_outcome = dict(
        _mapping(manifest.get("episode_outcome"), "episode_outcome")
    )
    if eligible and not (
        episode_outcome.get("status") == "success"
        and captured_entries == committed_count == actor_installed_plans
        and committed_actions == actor_plan_actions
    ):
        raise CompleteTurnRunCaptureError(
            "curriculum-eligible manifest does not satisfy complete-run alignment"
        )
    return CompleteTurnRunCapture(
        manifest_path=manifest_path,
        source_receipt_path=source_receipt_path,
        seed=seed,
        episode_outcome=episode_outcome,
        curriculum_eligible=eligible,
        curriculum_ineligibility=ineligibility,
        captured_combat_entries=captured_entries,
        committed_combat_actions=committed_actions,
        actor_installed_plans=actor_installed_plans,
        actor_plan_actions=actor_plan_actions,
        combats=tuple(combats),
    )


def _native_winning_line_splitter() -> WinningLineSplitter:
    try:
        from sts_learning_bridge import replay_verified_winning_complete_turn_line
    except ImportError as error:
        raise CompleteTurnRunCaptureError(
            "the installed sts_learning_bridge is required to replay captured combat lines"
        ) from error
    return replay_verified_winning_complete_turn_line


def _verified_turns(
    split: Mapping[str, Any], combat_ordinal: int
) -> tuple[ReplayVerifiedCompleteTurn, ...]:
    raw_turns = split.get("turns")
    if not isinstance(raw_turns, list) or not raw_turns:
        raise CompleteTurnRunCaptureError(
            f"combat {combat_ordinal} replay returned no complete turns"
        )
    expected_turn_count = _non_negative_int(split.get("turn_count"), "turn_count")
    if expected_turn_count != len(raw_turns):
        raise CompleteTurnRunCaptureError(
            f"combat {combat_ordinal} replay turn count is inconsistent"
        )
    turns: list[ReplayVerifiedCompleteTurn] = []
    for index, raw_turn in enumerate(raw_turns):
        turn = _mapping(raw_turn, f"combat {combat_ordinal} turn {index}")
        boundary = _string(turn.get("boundary"), "turn boundary")
        expected_boundary = (
            "terminal_win" if index + 1 == len(raw_turns) else "next_player_turn"
        )
        if boundary != expected_boundary:
            raise CompleteTurnRunCaptureError(
                f"combat {combat_ordinal} turn {index} has unexpected boundary {boundary}"
            )
        inputs_json = turn.get("inputs_json")
        if not isinstance(inputs_json, bytes) or not inputs_json:
            raise CompleteTurnRunCaptureError(
                f"combat {combat_ordinal} turn {index} inputs_json must be non-empty bytes"
            )
        raw_prefixes = turn.get("input_prefixes_json")
        if not isinstance(raw_prefixes, list) or not raw_prefixes:
            raise CompleteTurnRunCaptureError(
                f"combat {combat_ordinal} turn {index} input prefixes must be a non-empty list"
            )
        if not all(isinstance(prefix, bytes) and prefix for prefix in raw_prefixes):
            raise CompleteTurnRunCaptureError(
                f"combat {combat_ordinal} turn {index} input prefixes must contain non-empty bytes"
            )
        if raw_prefixes[-1] != inputs_json:
            raise CompleteTurnRunCaptureError(
                f"combat {combat_ordinal} turn {index} final input prefix is not the full turn"
            )
        turns.append(
            ReplayVerifiedCompleteTurn(
                from_turn=_non_negative_int(turn.get("from_turn"), "from_turn"),
                to_turn=_non_negative_int(turn.get("to_turn"), "to_turn"),
                boundary=boundary,
                inputs_json=inputs_json,
                input_prefixes_json=tuple(raw_prefixes),
            )
        )
    return tuple(turns)


def _local_file(directory: Path, value: str, name: str) -> Path:
    relative = Path(value)
    if relative.is_absolute() or relative.name != value:
        raise CompleteTurnRunCaptureError(
            f"{name} must be one file inside the capture directory"
        )
    path = (directory / relative).resolve()
    if path.parent != directory:
        raise CompleteTurnRunCaptureError(f"{name} escapes the capture directory")
    return path


def _mapping(value: object, name: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise CompleteTurnRunCaptureError(f"{name} must be an object")
    return value


def _string(value: object, name: str) -> str:
    if not isinstance(value, str) or not value:
        raise CompleteTurnRunCaptureError(f"{name} must be a non-empty string")
    return value


def _non_negative_int(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CompleteTurnRunCaptureError(f"{name} must be a non-negative integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CompleteTurnRunCaptureError(
            f"{name} must be a non-negative integer"
        ) from error
    if normalized < 0:
        raise CompleteTurnRunCaptureError(f"{name} must be a non-negative integer")
    return normalized
