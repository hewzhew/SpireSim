from __future__ import annotations

import json
from pathlib import Path

import pytest

from sts_learning import (
    CompleteTurnRunCaptureError,
    load_complete_turn_run_capture,
)


def _write_capture(directory: Path) -> Path:
    directory.mkdir()
    (directory / "receipt.json").write_text("{}", encoding="utf-8")
    combats = []
    for ordinal, action_count in enumerate((2, 1)):
        (directory / f"combat-{ordinal:03}.case.json").write_bytes(
            f"case-{ordinal}".encode()
        )
        (directory / f"combat-{ordinal:03}.actions.json").write_bytes(
            str(action_count).encode()
        )
        combats.append(
            {
                "ordinal": ordinal,
                "public_snapshot_id": f"snapshot-{ordinal}",
                "act": 1,
                "floor": ordinal + 1,
                "encounter_id": "Cultist",
                "root_exact_state_hash": f"root-{ordinal}",
                "run_session_fingerprint": f"session-{ordinal}",
                "action_count": action_count,
                "outcome": {"terminal": "win"},
                "case_file": f"combat-{ordinal:03}.case.json",
                "actions_file": f"combat-{ordinal:03}.actions.json",
            }
        )
    manifest = {
        "schema": "sts-combat-run-capture-v1",
        "source_receipt_file": "receipt.json",
        "seed": 7,
        "episode_outcome": {"status": "failure"},
        "curriculum_eligible": False,
        "curriculum_ineligibility": "episode stopped after the captured wins",
        "captured_combat_entries": 3,
        "committed_combat_count": 2,
        "committed_combat_actions": 3,
        "actor_installed_plans": 2,
        "actor_plan_actions": 3,
        "combats": combats,
    }
    path = directory / "manifest.json"
    path.write_text(json.dumps(manifest), encoding="utf-8")
    return path


def _splitter(case_payload: bytes, actions_payload: bytes) -> dict[str, object]:
    assert case_payload.startswith(b"case-")
    action_count = int(actions_payload)
    return {
        "action_count": action_count,
        "turn_count": 1,
        "turns": [
            {
                "from_turn": 1,
                "to_turn": 1,
                "boundary": "terminal_win",
                "inputs_json": b"[{}]",
                "input_prefixes_json": [b"[{}]"],
            }
        ],
    }


def test_loader_keeps_replay_verified_combats_grouped_by_run(tmp_path: Path) -> None:
    manifest = _write_capture(tmp_path / "capture")

    capture = load_complete_turn_run_capture(
        manifest, winning_line_splitter=_splitter
    )

    assert capture.seed == 7
    assert capture.captured_combat_entries == 3
    assert [combat.ordinal for combat in capture.combats] == [0, 1]
    assert [combat.action_count for combat in capture.combats] == [2, 1]
    assert capture.combats[0].turns[0].boundary == "terminal_win"
    with pytest.raises(CompleteTurnRunCaptureError, match="not curriculum eligible"):
        capture.require_curriculum_eligible()


def test_loader_rejects_a_manifest_action_count_that_rust_did_not_replay(
    tmp_path: Path,
) -> None:
    manifest_path = _write_capture(tmp_path / "capture")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["combats"][0]["action_count"] = 9
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    with pytest.raises(CompleteTurnRunCaptureError, match="action count mismatch"):
        load_complete_turn_run_capture(
            manifest_path, winning_line_splitter=_splitter
        )


def test_loader_rejects_files_outside_the_run_capture_directory(
    tmp_path: Path,
) -> None:
    manifest_path = _write_capture(tmp_path / "capture")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["combats"][0]["case_file"] = "../other.case.json"
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    with pytest.raises(CompleteTurnRunCaptureError, match="inside the capture directory"):
        load_complete_turn_run_capture(
            manifest_path, winning_line_splitter=_splitter
        )


def test_loader_rechecks_curriculum_eligibility_alignment(tmp_path: Path) -> None:
    manifest_path = _write_capture(tmp_path / "capture")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest["curriculum_eligible"] = True
    manifest["curriculum_ineligibility"] = None
    manifest["episode_outcome"] = {"status": "success"}
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    with pytest.raises(CompleteTurnRunCaptureError, match="complete-run alignment"):
        load_complete_turn_run_capture(
            manifest_path, winning_line_splitter=_splitter
        )
