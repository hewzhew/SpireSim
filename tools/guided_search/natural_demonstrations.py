"""Read verified, attributed combat choices. No behavior probabilities or value labels."""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path

import numpy as np

from tools.guided_search.entity_learning import SCHEMA, Position, positions


@dataclass(frozen=True)
class Demonstration:
    episode: int
    seed: int
    step: int
    position: Position
    selected: int


def _sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(directory):
    """Integrity checks complement the native replay receipt, not replace it.

    Returned actions are behavior-cloning targets only. The reasons and episode
    results remain in the artifact; neither becomes a numerical policy input.
    Strategic choices are preserved there but have no admitted encoder here.
    """
    directory = Path(directory)
    if (directory / "execution-failure.json").exists():
        raise ValueError("failed collection")
    producer = json.loads((directory / "producer.json").read_text(encoding="utf-8"))
    verification = json.loads((directory / "verification.json").read_text(encoding="utf-8"))
    if (producer.get("contract") != "natural_assisted_demo_v1"
            or producer.get("entity_schema") != SCHEMA
            or producer.get("labels") != "chosen_actions_only"
            or producer.get("plan", {}).get("actor") != "codex_assisted"
            or producer.get("plan", {}).get("information") != "oracle_observation_no_search_no_sl"
            or producer.get("plan", {}).get("purpose") != "demonstration_development"):
        raise ValueError("unadmitted demonstration source")
    if verification.get("replay_verified") is not True:
        raise ValueError("missing successful native replay")
    for name in ("producer", "trajectory"):
        suffix = ".json" if name == "producer" else ".jsonl"
        if verification.get(name + "_sha256") != _sha(directory / (name + suffix)):
            raise ValueError("replay receipt does not cover these files")
    rows = [json.loads(s) for s in (directory / "trajectory.jsonl").read_text(encoding="utf-8").splitlines()]
    if len(rows) != verification.get("choices"):
        raise ValueError("choice count mismatch")
    seeds = producer["plan"]["seeds"]
    previous, counts, examples = {}, {}, []
    for row in rows:
        ep, selected = row["episode"], row["selected"]
        if type(ep) is not int or not 0 <= ep < len(seeds):
            raise ValueError("invalid episode")
        if ep in previous and row["before"] != previous[ep]:
            raise ValueError("discontinuous trajectory")
        previous[ep] = row["after"]
        step = counts.get(ep, 0)
        counts[ep] = step + 1
        options = row["options"]
        if (type(selected) is not int or not 0 <= selected < len(options)
                or options[selected]["action"] != row["action"]):
            raise ValueError("chosen action does not match its source")
        encoded = row["entities"]
        if encoded is None:
            continue
        if "Combat" not in row["action"] or len(encoded["actions"]) != len(options):
            raise ValueError("combat action encoding mismatch")
        aliases = {"ids": "entity_ids", "values": "entity_values", "refs": "entity_refs"}
        frame = {aliases.get(k, k): np.asarray(v, dtype=np.float32 if k in ("values", "actions") else None)
                 for k, v in encoded.items()}
        frame.update(schema=SCHEMA, slots=[0], entity_splits=[0, len(encoded["ids"])],
                     action_splits=[0, len(encoded["actions"])])
        p, = positions(frame)
        examples.append(Demonstration(ep, seeds[ep], step, p, selected))
    summaries = verification.get("episodes", [])
    if len(summaries) != len(seeds):
        raise ValueError("missing episode accounting")
    for i, summary in enumerate(summaries):
        if summary.get("episode") != i or summary.get("seed") != seeds[i] or summary.get("steps") != counts.get(i, 0):
            raise ValueError("episode accounting mismatch")
    return examples
