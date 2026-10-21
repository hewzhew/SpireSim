"""Run an exact raw-evidence trajectory through the installed Python package.

The Rust replay derives fresh expectations; this process performs every real
choose/step and records its own results. It never launches the original game.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import subprocess
import sys
import time

from sts_learning_bridge import LearningBatchEnv
import sts_learning_bridge._native as native


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def check_snapshots(env: LearningBatchEnv, expected: dict[int, dict | None], where: str, batch: dict | None = None) -> None:
    expected = {slot: value for slot, value in expected.items() if value is not None}
    actual = dict(env.public_information_snapshots())
    require(list(actual) == list(expected), f"{where}: public snapshot slots differ")
    for slot, value in expected.items():
        fields = {
            "snapshot_id": value["snapshot_id"],
            "observation_id": value["observation"]["observation_id"],
            "history_snapshot_id": value["history_snapshot"]["history_snapshot_id"],
            "candidate_surface_id": value["candidate_surface"]["candidate_surface_id"],
            "candidate_ids": value["candidate_surface"]["ordered_candidate_ids"],
        }
        for name, field in fields.items():
            require(getattr(actual[slot], name) == field, f"{where}, slot {slot}: public {name} differs")
    if batch is None:
        batch = env.decision_batch(semantic=True)
    require(batch["slot_indices"].tolist() == list(expected), f"{where}: decision slots differ")
    require(batch["candidate_counts"].tolist() == [len(actual[slot].candidate_ids) for slot in expected], f"{where}: candidate counts differ")


def execute(expected: dict, journal: Path, offsets: list[int], reject_extra_input: bool) -> dict:
    require(expected["schema"] == "sts-training-trajectory-v2", "unknown trajectory schema")
    bootstrap = expected["bootstrap"]
    classes = {"IRONCLAD": "ironclad", "THE_SILENT": "silent", "DEFECT": "defect", "WATCHER": "watcher"}
    commits = expected["commits"]
    require(commits and 0 in offsets, "acceptance requires a slot starting at Neow")
    require(all(0 <= offset <= len(commits) for offset in offsets), "commit offset outside adopted trajectory")
    require(len(set(offsets)) == 1 or commits[-1]["terminated"], "staggered slots require a real terminal trajectory")
    slots = list(range(len(offsets)))
    env = LearningBatchEnv(
        [int(bootstrap["seed"])] * len(slots), bootstrap["ascension"], final_act=bootstrap["final_act"],
        candidate_surface="legal", player_class=classes[bootstrap["player_class"]],
        neow_eligibility=bootstrap["neow_eligibility"],
    )
    def fingerprints(positions: list[int]) -> list[str]:
        return [commits[position - 1]["after_continuation"] if position else expected["initial_continuation"] for position in positions]

    def snapshots(positions: list[int]) -> dict[int, dict | None]:
        return {slot: commits[position - 1]["after_public_snapshot"] if position else expected["initial_public_snapshot"] for slot, position in enumerate(positions)}

    def counters() -> dict:
        return {"choose_calls": 0, "step_calls": 0, "phase_counts": {}, "external_time_samples": 0, "rollback_checks": []}

    has_external_inputs = any(commit["execution_inputs"]["secret_portal_playtime_bits"] for commit in commits)

    def advance(positions: list[int], stats: dict, inspect: bool) -> tuple[dict, dict[int, str]]:
        active = [slot for slot, position in enumerate(positions) if position < len(commits)]
        before = fingerprints(positions)
        require(env.continuation_fingerprints() == before, f"{positions}: predecessor differs")
        prefixes = {slot: 0 for slot in active}
        source_tokens: dict[int, str] = {}
        while not env.ready:
            decision = env.decision_batch(semantic=inspect)
            rows = decision["slot_indices"].tolist()
            pending = [slot for slot in active if prefixes[slot] < len(commits[positions[slot]]["ordinals"])]
            require(rows == pending, f"{positions}: pending decision slots differ")
            if inspect:
                check_snapshots(env, {slot: commits[positions[slot]]["choice_snapshots"][prefixes[slot]] for slot in pending}, str(positions), decision)
            ordinals = []
            for row, slot in enumerate(rows):
                commit = commits[positions[slot]]
                ordinal = commit["ordinals"][prefixes[slot]]
                require(0 <= ordinal < int(decision["candidate_counts"][row]), f"slot {slot}: illegal ordinal")
                source_tokens.setdefault(slot, decision["decision_token_ids"][row])
                phase = str(int(decision["phase"][row]))
                stats["phase_counts"][phase] = stats["phase_counts"].get(phase, 0) + 1
                ordinals.append(ordinal)
                prefixes[slot] += 1
            env.choose(list(decision["decision_token_ids"]), ordinals)
            stats["choose_calls"] += 1
            require(env.continuation_fingerprints() == before, f"{positions}: choose mutated game state")
        require(all(prefixes[slot] == len(commits[positions[slot]]["ordinals"]) for slot in active), "incomplete selection became ready")
        inputs = [commits[positions[slot]]["execution_inputs"] for slot in active]
        contains_external = any(value["secret_portal_playtime_bits"] for value in inputs)
        if inspect and reject_extra_input and not stats["rollback_checks"] and len(active) > 1 and (contains_external or not has_external_inputs):
            bad_inputs = json.loads(json.dumps(inputs))
            bad_inputs[-1]["secret_portal_playtime_bits"].append(0x3F800000)
            checkpoint = bytes(env.checkpoint_bytes(max_bytes=32_000_000))
            try:
                env.step(execution_inputs_json=json.dumps(bad_inputs))
            except (RuntimeError, ValueError) as error:
                stats["rollback_checks"].append({"positions": positions.copy(), "rejected_slot": active[-1], "error": str(error)})
            else:
                raise AssertionError("surplus external input was accepted")
            require(env.ready and env.continuation_fingerprints() == before, "rejected batch changed game or ready state")
            require(bytes(env.checkpoint_bytes(max_bytes=32_000_000)) == checkpoint, "rejected batch changed pending choices")
        result = env.step(execution_inputs_json=json.dumps(inputs))
        stats["step_calls"] += 1
        stats["external_time_samples"] += sum(len(value["secret_portal_playtime_bits"]) for value in inputs)
        require(result["slot_indices"].tolist() == active, f"{positions}: transition slots differ")
        require(result["source_decision_token_ids"] == [source_tokens[slot] for slot in active], "source tokens differ")
        paths = [commits[positions[slot]]["ordinals"] for slot in active]
        splits = [0]
        for path in paths:
            splits.append(splits[-1] + len(path))
        require(result["action_ordinals"].tolist() == [ordinal for path in paths for ordinal in path], "executed ordinal paths differ")
        require(result["action_ordinal_row_splits"].tolist() == splits, "action row splits differ")
        require(result["reward"].tolist() == [commits[positions[slot]]["reward"] for slot in active], "rewards differ")
        require(result["terminated"].tolist() == [commits[positions[slot]]["terminated"] for slot in active], "termination differs")
        require(result["terminal_slot_indices"].tolist() == [slot for slot in active if commits[positions[slot]]["terminated"]], "terminal slots differ")
        for slot in active:
            positions[slot] += 1
        require(env.continuation_fingerprints() == fingerprints(positions), f"{positions}: successor continuation differs")
        contexts = dict(env.public_run_contexts())
        terminal_slots = result["terminal_slot_indices"].tolist()
        require(result["terminal_reward"].tolist() == [commits[positions[slot] - 1]["reward"] for slot in terminal_slots], "terminal rewards differ")
        for field in ("act", "floor", "hp", "max_hp", "gold"):
            require(result[f"terminal_{field}"].tolist() == [getattr(contexts[slot], field) for slot in terminal_slots], f"terminal {field} differs from the verified successor")
        if inspect:
            check_snapshots(env, snapshots(positions), str(positions))
        return result, source_tokens

    positions = [0] * len(slots)
    require(env.continuation_fingerprints() == fingerprints(positions), "initial continuation differs")
    check_snapshots(env, snapshots(positions), "initial")
    warmup = counters()
    if max(offsets):
        bank = env.checkpoint_slots(slots)
        for position in range(1, max(offsets) + 1):
            advance(positions, warmup, False)
            capture = [slot for slot, offset in enumerate(offsets) if offset == position]
            if capture:
                bank = bank.updated(env.checkpoint_slots(capture))
        env.restore_slots(slots, bank)
        positions = offsets.copy()
        require(env.continuation_fingerprints() == fingerprints(positions), "restored curriculum offsets differ")
        check_snapshots(env, snapshots(positions), "restored curriculum")
    stats = counters()
    semantic_actions: list[list[int]] = [[] for _ in slots]
    rewards = [0] * len(slots)
    with journal.open("x", encoding="utf-8") as log:
        while any(position < len(commits) for position in positions):
            before = positions.copy()
            result, _ = advance(positions, stats, True)
            rows = []
            for row, slot in enumerate(result["slot_indices"].tolist()):
                commit = commits[before[slot]]
                semantic_actions[slot].extend(commit["semantic_actions"])
                rewards[slot] += int(result["reward"][row])
                rows.append({"slot": slot, "commit": before[slot] + 1, "semantic_actions": commit["semantic_actions"],
                             "ordinals": commit["ordinals"], "continuation": env.continuation_fingerprints()[slot],
                             "reward": int(result["reward"][row]), "terminated": bool(result["terminated"][row])})
            log.write(json.dumps({"batch_step": stats["step_calls"], "slots": rows}) + "\n")
            log.flush()
    for slot, offset in enumerate(offsets):
        required = [action for commit in commits[offset:] for action in commit["semantic_actions"]]
        require(semantic_actions[slot] == required, f"slot {slot}: semantic correspondence is incomplete or out of order")
    complete = expected["replay"]["coverage"]["complete_heart_run"]
    if complete:
        require(all(reward == 1 for slot, reward in enumerate(rewards) if offsets[slot] < len(commits)), "Heart suffix lacks terminal reward")
    if reject_extra_input:
        require(bool(stats["rollback_checks"]), "requested atomic rejection was not exercised")
    finals = [{"slot": slot, "act": context.act, "floor": context.floor, "hp": context.hp,
               "max_hp": context.max_hp, "gold": context.gold} for slot, context in env.public_run_contexts()]
    return {
        "semantic_actions": expected["replay"]["committed_actions"], "python_choose_calls": stats["choose_calls"],
        "python_step_calls": stats["step_calls"], "phase_choice_counts": stats["phase_counts"],
        "reward_sum": sum(rewards), "complete_heart_run": complete, "external_time_samples": stats["external_time_samples"],
        "batch_slots": len(slots), "initial_commit_offsets": offsets, "slot_semantic_actions": [len(actions) for actions in semantic_actions],
        "slot_rewards": rewards, "warmup": warmup, "rollback_checks": stats["rollback_checks"],
        "final": {key: value for key, value in finals[0].items() if key != "slot"}, "slot_finals": finals,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--evidence", type=Path)
    source.add_argument("--session", help="Manual source ID; follows its typed adopted parent prefixes")
    source.add_argument("--trajectory", type=Path, help="Frozen training trajectory exported by raw-evidence replay; usable on another platform")
    parser.add_argument("--trajectory-sha256", help="Required SHA-256 of --trajectory, taken from the exporting acceptance report")
    parser.add_argument("--through", type=int, help="Freeze at this many adopted actions of the selected session; parent prefixes retain their recorded limits")
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--runner", type=Path, default=Path(__file__).resolve().parents[3] / "target/dev-opt" / ("sts_lockstep.exe" if sys.platform == "win32" else "sts_lockstep"))
    parser.add_argument("--output", type=Path, required=True, help="New artifact directory")
    parser.add_argument("--stagger-commits", type=int, nargs="+", default=[0], help="Source commit offsets for slot-bound checkpoints; include 0. Different offsets require a terminal source.")
    parser.add_argument("--require-build-profile", choices=["dev", "release"], help="Fail before replay if a different native build was imported")
    parser.add_argument("--reject-extra-input-once", action="store_true", help="Reject one surplus external-input batch and verify pending choices and exact state stay intact")
    args = parser.parse_args()
    if args.trajectory is not None:
        if (args.trajectory_sha256 is None or len(args.trajectory_sha256) != 64
                or any(c not in "0123456789abcdefABCDEF" for c in args.trajectory_sha256)):
            parser.error("--trajectory requires its 64-digit --trajectory-sha256")
    elif args.trajectory_sha256 is not None:
        parser.error("--trajectory-sha256 requires --trajectory")
    if args.through is not None and (args.session is None or args.through < 0):
        parser.error("--through requires --session and a non-negative source action count")
    if 0 not in args.stagger_commits or any(offset < 0 for offset in args.stagger_commits):
        parser.error("--stagger-commits requires non-negative offsets including 0")
    if args.reject_extra_input_once and len(args.stagger_commits) < 2:
        parser.error("--reject-extra-input-once requires at least two slots")
    args.output.mkdir(parents=True, exist_ok=False)
    started = time.perf_counter()
    native_path = Path(native.__file__).resolve()
    report = {
        "schema": "sts-python-trajectory-result-v2", "status": "failed",
        "source": ({"trajectory": str(args.trajectory.resolve()), "sha256": args.trajectory_sha256.lower()} if args.trajectory else
                   {"evidence": str(args.evidence.resolve())} if args.evidence else
                   {"session": args.session, "repository": str(args.repository.resolve()), "through": args.through}),
        "native_module": str(native_path),
        "native_sha256": hashlib.sha256(native_path.read_bytes()).hexdigest(),
        "native_build_profile": native.NATIVE_BUILD_PROFILE,
        "python_runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "package_version": importlib.metadata.version("sts-learning-bridge"),
    }
    try:
        if args.require_build_profile:
            require(native.NATIVE_BUILD_PROFILE == args.require_build_profile,
                    f"expected native {args.require_build_profile} build, imported {native.NATIVE_BUILD_PROFILE} from {native_path}")
        if args.trajectory:
            expected_bytes = args.trajectory.read_bytes()
            require(hashlib.sha256(expected_bytes).hexdigest() == args.trajectory_sha256.lower(),
                    "exported trajectory SHA-256 differs; execution was not started")
            report["verification"] = "installed_package_against_frozen_replay_export"
        else:
            report["verification"] = "raw_evidence_replay_and_installed_package"
            report["runner_sha256"] = hashlib.sha256(args.runner.read_bytes()).hexdigest()
            source_args = (["--evidence", str(args.evidence.resolve())] if args.evidence else
                           ["--session", args.session, "--repository", str(args.repository.resolve())])
            if args.through is not None:
                source_args.extend(["--through", str(args.through)])
            result = subprocess.run([
                str(args.runner.resolve()), "replay", *source_args,
                "--learning-output", str((args.output / "expected.json").resolve()),
            ], text=True, encoding="utf-8", capture_output=True, check=False)
            (args.output / "replay.log").write_text(result.stdout + result.stderr, encoding="utf-8")
            require(result.returncode == 0, f"raw evidence replay failed: {result.stderr.strip()}")
            expected_bytes = (args.output / "expected.json").read_bytes()
        report["expected_sha256"] = hashlib.sha256(expected_bytes).hexdigest()
        expected = json.loads(expected_bytes)
        report["trajectory_producer_version"] = expected["producer_version"]
        report["source_scopes"] = [
            {"session_id": scope["session_id"], "evidence_directory": scope["evidence_directory"],
             "first_semantic_action": scope["first_semantic_action"],
             "semantic_actions": scope["replay"]["committed_actions"],
             "java_actions": scope["replay"]["java_actions"], "java_producer": scope["java_producer"]}
            for scope in expected["sources"]
        ]
        report.update(execute(expected, args.output / "actual.jsonl", args.stagger_commits, args.reject_extra_input_once))
        report["status"] = "passed"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
    report["elapsed_seconds"] = time.perf_counter() - started
    (args.output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    display = {key: value for key, value in report.items() if key != "source_scopes"}
    display["source_scope_count"] = len(report.get("source_scopes", []))
    print(json.dumps(display, ensure_ascii=False, indent=2))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
