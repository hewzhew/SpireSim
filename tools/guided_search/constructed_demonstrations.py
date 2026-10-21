"""Attributed supplied-combat choices, replayed from recipes before learning use."""
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import time

import numpy as np

from tools.guided_search.entity_learning import SCHEMA, COLUMNS, positions
from tools.guided_search.natural_demonstrations import Demonstration

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = "constructed_assisted_demo_v1"


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write(path, value):
    with Path(path).open("x", encoding="utf-8") as f:
        json.dump(value, f, indent=2, allow_nan=False)


def evidence(env):
    return json.loads(env.evidence_json())["episodes"][0]


def validate(plan):
    if (plan.get("actor") != "codex_assisted"
            or plan.get("information") != "oracle_observation_no_search_no_sl"
            or plan.get("purpose") != "demonstration_development"):
        raise ValueError("unadmitted constructed demonstration attribution")
    cases = plan["cases"]
    if (not 1 <= len(cases) <= 128 or len({c["id"] for c in cases}) != len(cases)
            or any(not c["id"] or not c["source"] or not c["family"] for c in cases)):
        raise ValueError("declare distinct cases and recipe provenance")
    if type(plan["max_decisions"]) is not int or not 1 <= plan["max_decisions"] <= 512:
        raise ValueError("invalid decision budget")
    if not np.isfinite(plan["max_seconds"]) or not 0 < plan["max_seconds"] <= 14400:
        raise ValueError("invalid collection budget")
    if len({digest(c["spec"]) for c in cases}) != len(cases):
        raise ValueError("duplicate construction is not a new demonstration")


class Collection:
    """One resident environment per declared case; callers supply every action."""

    def __init__(self, plan, directory):
        from sts_learning_bridge import GuidedCombatBatch
        import sts_learning_bridge._native as native
        validate(plan)
        self.plan = json.loads(json.dumps(plan))
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=False)
        self.started = time.monotonic()
        self.closed = False
        self.envs = []
        files = subprocess.check_output(["git", "ls-files", "*.rs", "Cargo.lock", "Cargo.toml",
            "tools/guided_search/entity_learning.py", "tools/guided_search/constructed_demonstrations.py"], cwd=ROOT).decode().splitlines()
        # Include this owner before its first commit, too.
        files = sorted(set(files) | {"tools/guided_search/constructed_demonstrations.py"})
        write(self.directory / "source.json", dict(files=[dict(path=p, sha256=sha(ROOT / p)) for p in files]))
        write(self.directory / "producer.json", dict(contract=CONTRACT, entity_schema=SCHEMA,
            plan=self.plan, labels="chosen_actions_only", native_sha256=sha(native.__file__),
            seeds=sorted({c["spec"]["seed"] for c in self.plan["cases"]}),
            source_sha256=sha(self.directory / "source.json"),
            revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
            search_transitions=0, sl_branches=0))
        self.journal = (self.directory / "trajectory.jsonl").open("x", encoding="utf-8")
        self.requests = (self.directory / "requests.jsonl").open("x", encoding="utf-8")
        try:
            for case in self.plan["cases"]:
                self.envs.append(GuidedCombatBatch.from_specifications([case["spec"]], plan["max_decisions"]))
        except BaseException as exc:
            self._fail(exc)
            raise

    @staticmethod
    def _append(file, row):
        file.write(json.dumps(row, allow_nan=False) + "\n")
        file.flush()

    def _fail(self, exc):
        self.closed = True
        self.journal.close()
        self.requests.close()
        snapshots = []
        for env in self.envs:
            try:
                snapshots.append(evidence(env))
            except BaseException as failure:
                snapshots.append(dict(evidence_error=f"{type(failure).__name__}: {failure}"))
        write(self.directory / "execution-failure.json", dict(error=f"{type(exc).__name__}: {exc}",
            declared=len(self.plan["cases"]), constructed=len(self.envs), evidence=snapshots))

    def _env(self, episode):
        if self.closed:
            raise ValueError("collection is closed")
        if type(episode) is not int or not 0 <= episode < len(self.envs):
            raise ValueError("invalid episode")
        return self.envs[episode]

    def observe(self, episode):
        env = self._env(episode)
        frame = json.loads(env.readings_json())
        return dict(episode=episode, source=frame["revision"],
            reading=frame["readings"][0] if frame["readings"] else None,
            outcome=next(iter(json.loads(env.outcomes_json())), None))

    def choose(self, episode, source, selected, reason):
        env = self._env(episode)
        if time.monotonic() - self.started >= self.plan["max_seconds"]:
            raise TimeoutError("collection budget exhausted; finish to preserve incomplete cases")
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError("demonstration reason missing")
        frame = env.decision_batch()
        if type(source) is not int or source != frame["revision"]:
            raise ValueError("stale source")
        if not len(frame["slots"]):
            raise ValueError("episode already stopped")
        p, = positions(frame)
        if type(selected) is not int or not 0 <= selected < len(p.columns["actions"]):
            raise ValueError("choice outside source")
        before = evidence(env)
        reading = self.observe(episode)["reading"]
        request = dict(episode=episode, source=source, selected=selected, reason=reason)
        self._append(self.requests, request)
        try:
            env.advance(source, [selected])
            after = evidence(env)
            self._append(self.journal, dict(**request,
                numeric={k: v.tolist() for k, v in p.columns.items()}, reading=reading,
                before=digest(before["successor"]), after=digest(after["successor"]),
                inputs=after["inputs"][len(before["inputs"]):]))
        except BaseException as exc:
            self._fail(exc)
            raise
        return self.observe(episode)

    def finish(self):
        if self.closed:
            raise ValueError("collection is closed")
        self.closed = True
        self.journal.close()
        self.requests.close()
        try:
            with gzip.open(self.directory / "evidence.json.gz", "xt", encoding="utf-8") as f:
                json.dump([evidence(env) for env in self.envs], f)
            receipt = verify(self.directory)
            write(self.directory / "verification.json", receipt)
            return receipt
        except BaseException as exc:
            write(self.directory / "execution-failure.json", dict(error=f"{type(exc).__name__}: {exc}"))
            raise


def _producer(directory):
    if (directory / "execution-failure.json").exists():
        raise ValueError("failed collection")
    producer = read(directory / "producer.json")
    if (producer.get("contract") != CONTRACT or producer.get("entity_schema") != SCHEMA
            or producer.get("labels") != "chosen_actions_only"
            or producer.get("search_transitions") != 0 or producer.get("sl_branches") != 0):
        raise ValueError("unadmitted constructed demonstration")
    validate(producer["plan"])
    if producer["seeds"] != sorted({c["spec"]["seed"] for c in producer["plan"]["cases"]}):
        raise ValueError("seed accounting differs from construction")
    if producer["source_sha256"] != sha(directory / "source.json"):
        raise ValueError("source manifest changed")
    return producer


def _rows(directory, producer):
    rows = [json.loads(s) for s in (directory / "trajectory.jsonl").read_text(encoding="utf-8").splitlines()]
    steps, previous, examples = {}, {}, []
    for row in rows:
        ep, selected = row["episode"], row["selected"]
        if type(ep) is not int or not 0 <= ep < len(producer["plan"]["cases"]):
            raise ValueError("invalid episode accounting")
        if ep in previous and previous[ep] != row["before"]:
            raise ValueError("discontinuous trajectory")
        previous[ep] = row["after"]
        c = {k: np.asarray(row["numeric"][k], dtype=np.int64 if integer else np.float32)
             for k, (_, integer) in COLUMNS.items()}
        # Reuse the schema validator, including reference and shape checks.
        p, = positions(dict(schema=SCHEMA, slots=[0], entity_splits=[0, len(c["entity_ids"])],
                           action_splits=[0, len(c["actions"])], **c))
        if (type(selected) is not int or not 0 <= selected < len(c["actions"])
                or len(row["reading"]["choices"]) != len(c["actions"]) or not row["reason"].strip()):
            raise ValueError("invalid selected action")
        step = steps.get(ep, 0)
        steps[ep] = step + 1
        examples.append(Demonstration(ep, producer["plan"]["cases"][ep]["spec"]["seed"], step, p, selected))
    return rows, examples


def verify(directory):
    """Same native/source producer; reconstruct every case and replay every choice."""
    from sts_learning_bridge import GuidedCombatBatch
    import sts_learning_bridge._native as native
    directory = Path(directory)
    producer = _producer(directory)
    if sha(native.__file__) != producer["native_sha256"]:
        raise ValueError("native producer mismatch")
    for entry in read(directory / "source.json")["files"]:
        if sha(ROOT / entry["path"]) != entry["sha256"]:
            raise ValueError("source producer mismatch")
    rows, examples = _rows(directory, producer)
    plan = producer["plan"]
    envs = [GuidedCombatBatch.from_specifications([c["spec"]], plan["max_decisions"]) for c in plan["cases"]]
    for row, example in zip(rows, examples):
        env = envs[example.episode]
        before = evidence(env)
        if digest(before["successor"]) != row["before"]:
            raise ValueError("replay source differs")
        frame = env.decision_batch()
        p, = positions(frame)
        if any(not np.array_equal(p.columns[k], example.position.columns[k]) for k in COLUMNS):
            raise ValueError("replay numerical input differs")
        if json.loads(env.readings_json())["readings"][0] != row["reading"]:
            raise ValueError("replay structured choices differ")
        env.advance(frame["revision"], [example.selected])
        after = evidence(env)
        if digest(after["successor"]) != row["after"] or after["inputs"][len(before["inputs"]):] != row["inputs"]:
            raise ValueError("replay successor differs")
    with gzip.open(directory / "evidence.json.gz", "rt", encoding="utf-8") as f:
        saved = json.load(f)
    if [evidence(env) for env in envs] != saved:
        raise ValueError("replay final evidence differs")
    counts = {i: sum(e.episode == i for e in examples) for i in range(len(envs))}
    return dict(replay_verified=True, choices=len(rows),
        sha256={name: sha(directory / name) for name in ("producer.json", "trajectory.jsonl", "evidence.json.gz")},
        episodes=[dict(episode=i, seed=c["spec"]["seed"], choices=counts[i],
            outcome=next(iter(json.loads(envs[i].outcomes_json())), None)) for i, c in enumerate(plan["cases"])])


def load(directory):
    directory = Path(directory)
    producer = _producer(directory)
    receipt = read(directory / "verification.json")
    if receipt.get("replay_verified") is not True:
        raise ValueError("missing replay verification")
    for name in ("producer.json", "trajectory.jsonl", "evidence.json.gz"):
        if receipt["sha256"].get(name) != sha(directory / name):
            raise ValueError("receipt does not cover current material")
    rows, examples = _rows(directory, producer)
    if receipt["choices"] != len(rows) or len(receipt["episodes"]) != len(producer["plan"]["cases"]):
        raise ValueError("missing episode accounting")
    for i, case in enumerate(producer["plan"]["cases"]):
        summary = receipt["episodes"][i]
        if (summary["episode"] != i or summary["seed"] != case["spec"]["seed"]
                or summary["choices"] != sum(e.episode == i for e in examples)):
            raise ValueError("episode accounting mismatch")
    return examples
