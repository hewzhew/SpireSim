"""Bounded local actor/critic experiment; game execution stays in Rust.

This caller starts fresh prescribed combats and fresh network parameters.
It never loads an old policy, calls a provider, or assigns search teacher labels.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch
from torch import nn
from sts_learning_bridge import GuidedCombatBatch
import sts_learning_bridge

ROOT = Path(__file__).resolve().parents[2]
CONTEXT, CARDS, CARD_FEATURES, ACTIONS = 48, 11, 20, 12


@dataclass(frozen=True)
class Config:
    train_seed_start: int = 2000
    train_seed_count: int = 32
    eval_seed_start: int = 1000000
    eval_seed_count: int = 32
    model_seeds: tuple[int, ...] = (117, 231, 409)
    generations: int = 24
    max_decisions: int = 128
    hidden: int = 64
    layers: int = 2
    learning_rate: float = 0.001
    entropy_weight: float = 0.01
    value_weight: float = 0.5
    gradient_limit: float = 1.0
    max_wall_seconds: float = 900
    device: str = "cpu"
    threads: int = 1
    starting_hp: int = 68
    evaluation_partition: str = "final"
    task: str = "starter"

    def validate(self):
        if self.task not in ("starter", "draw_select"):
            raise ValueError("unknown local learning task")
        limit = 32 if self.task == "draw_select" else 64
        if not (1 <= self.train_seed_count <= limit and 1 <= self.eval_seed_count <= limit):
            raise ValueError(f"each panel must contain 1..{limit} seeds")
        if set(self.train_seeds()) & set(self.eval_seeds()):
            raise ValueError("training and final evaluation seed groups overlap")
        if min(self.train_seed_start, self.eval_seed_start, *self.model_seeds) < 0:
            raise ValueError("seeds must be nonnegative")
        if not self.model_seeds or len(set(self.model_seeds)) != len(self.model_seeds):
            raise ValueError("model initializations must be nonempty and distinct")
        if not (1 <= self.max_decisions <= 512 and self.generations > 0):
            raise ValueError("invalid learning or episode bound")
        if self.hidden < 4 or self.hidden % 4 or not (1 <= self.layers <= 4):
            raise ValueError("invalid attention model dimensions")
        if self.device not in ("cpu", "cuda") or self.threads < 1:
            raise ValueError("invalid compute configuration")
        if not isinstance(self.starting_hp, int) or not 1 <= self.starting_hp <= 75:
            raise ValueError("starting_hp must be in 1..75")
        if self.evaluation_partition not in ("development", "final"):
            raise ValueError("invalid evaluation partition")
        for name in ("learning_rate", "gradient_limit", "max_wall_seconds"):
            if not np.isfinite(getattr(self, name)) or getattr(self, name) <= 0:
                raise ValueError(f"invalid {name}")
        for name in ("entropy_weight", "value_weight"):
            if not np.isfinite(getattr(self, name)) or getattr(self, name) < 0:
                raise ValueError(f"invalid {name}")

    def train_seeds(self):
        return range(self.train_seed_start, self.train_seed_start + self.train_seed_count)

    def eval_seeds(self):
        return range(self.eval_seed_start, self.eval_seed_start + self.eval_seed_count)


def prepare(batch, device="cpu"):
    """Only these arrays enter the model; revision, slots and seeds never do."""
    if batch["schema"] == 2:
        from tools.guided_search.local_tactics import prepare as prepare_tactical
        return prepare_tactical(batch, device)
    if batch["schema"] != 1:
        raise ValueError("unsupported starter numeric schema")
    context = np.asarray(batch["context"])
    cards = np.asarray(batch["cards"])
    splits = np.asarray(batch["action_splits"], dtype=np.int64)
    actions = np.asarray(batch["action_cards"], dtype=np.int64)
    n = len(context)
    if context.shape != (n, CONTEXT) or cards.shape != (n, CARDS, CARD_FEATURES):
        raise ValueError("malformed numeric observation")
    if not np.isfinite(context).all() or not np.isfinite(cards).all():
        raise ValueError("nonfinite numeric observation")
    if splits.shape != (n + 1,) or splits[0] != 0 or splits[-1] != len(actions):
        raise ValueError("malformed action offsets")
    counts = np.diff(splits)
    if np.any(counts < 1) or np.any(counts > ACTIONS) or np.any(actions < -1) or np.any(actions >= CARDS):
        raise ValueError("invalid action domain")
    indices = np.zeros((n, ACTIONS), dtype=np.int64)
    mask = np.zeros((n, ACTIONS), dtype=bool)
    for i, (start, end) in enumerate(zip(splits[:-1], splits[1:])):
        # 0 is the global/end-turn token; card row k is token k+1.
        indices[i, :end-start] = actions[start:end] + 1
        mask[i, :end-start] = True
    return {"context": torch.as_tensor(context, dtype=torch.float32, device=device),
            "cards": torch.as_tensor(cards, dtype=torch.float32, device=device),
            "indices": torch.as_tensor(indices, device=device),
            "mask": torch.as_tensor(mask, device=device)}


class StarterNetwork(nn.Module):
    def __init__(self, config: Config):
        super().__init__()
        h = config.hidden
        self.context = nn.Linear(CONTEXT, h)
        self.card = nn.Linear(CARD_FEATURES, h)
        layer = nn.TransformerEncoderLayer(h, 4, h * 2, dropout=0, batch_first=True,
                                           activation="gelu")
        self.encoder = nn.TransformerEncoder(layer, config.layers, enable_nested_tensor=False)
        self.policy = nn.Sequential(nn.Linear(h * 2, h), nn.SiLU(), nn.Linear(h, 1))
        self.value = nn.Linear(h, 1)
        nn.init.zeros_(self.value.weight)
        nn.init.zeros_(self.value.bias)

    def forward(self, data):
        tokens = torch.cat((self.context(data["context"]).unsqueeze(1), self.card(data["cards"])), dim=1)
        encoded = self.encoder(tokens)
        global_state = encoded[:, 0]
        selected = encoded.gather(1, data["indices"].unsqueeze(-1).expand(-1, -1, encoded.shape[-1]))
        paired = torch.cat((selected, global_state.unsqueeze(1).expand(-1, ACTIONS, -1)), dim=-1)
        logits = self.policy(paired).squeeze(-1).masked_fill(~data["mask"], -torch.inf)
        return logits, self.value(global_state).squeeze(-1)


def network(config):
    if config.task == "draw_select":
        from tools.guided_search.local_tactics import TacticalNetwork
        return TacticalNetwork(config)
    if config.task != "starter":
        raise ValueError("unknown local learning task")
    return StarterNetwork(config)


def hash_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, value):
    with Path(path).open("x", encoding="utf-8") as f:
        json.dump(value, f, indent=2, allow_nan=False)


def save_weights(path, model):
    np.savez_compressed(path, **{k: v.detach().cpu().numpy() for k, v in model.state_dict().items()})


def check_deadline(deadline):
    if time.monotonic() >= deadline:
        raise TimeoutError("experiment wall budget exhausted; unfinished work is Unknown")


def rollout(model, cases, config, generator, directory, deadline, rule="sampled", *, deck=None,
            demonstrations=False):
    wall_start = time.perf_counter()
    directory.mkdir()
    env = GuidedCombatBatch(cases, config.max_decisions, starting_hp=config.starting_hp, task=config.task, deck=deck)
    records = []
    timings = {"observation": 0., "inference": 0., "advance_including_replay": 0.}
    completed = False
    try:
        while True:
            check_deadline(deadline)
            start = time.perf_counter()
            batch = env.decision_batch()
            timings["observation"] += time.perf_counter() - start
            if not len(batch["slots"]):
                completed = True
                break
            start = time.perf_counter()
            data = prepare(batch, config.device)
            with torch.no_grad():
                logits, values = model(data)
                if rule == "uniform":
                    logits = torch.zeros_like(logits).masked_fill(~data["mask"], -torch.inf)
                log_probs = logits.log_softmax(-1)
                probabilities = log_probs.exp()
                if rule == "greedy":
                    choices = logits.argmax(-1)
                elif rule in ("sampled", "uniform"):
                    choices = torch.multinomial(probabilities, 1, generator=generator).squeeze(-1)
                else:
                    raise ValueError("unknown decision rule")
                chosen_logp = log_probs.gather(1, choices[:, None]).squeeze(1)
                if not torch.isfinite(chosen_logp).all():
                    raise ValueError("nonfinite selected log probability")
            ordinals = choices.cpu().tolist()
            records.append({**{k: v.cpu() for k, v in data.items()},
                            "slots": torch.as_tensor(np.asarray(batch["slots"], dtype=np.int64)),
                            "choices": choices.cpu(), "logp": chosen_logp.cpu(),
                            "old_value": values.sigmoid().cpu()})
            timings["inference"] += time.perf_counter() - start
            start = time.perf_counter()
            env.advance(batch["revision"], ordinals)
            timings["advance_including_replay"] += time.perf_counter() - start
    finally:
        # Actual source, inputs and outcomes survive errors or budget interruption.
        with gzip.open(directory / "evidence.json.gz", "xt", encoding="utf-8") as f:
            f.write(env.evidence_json())
        outcomes = json.loads(env.outcomes_json())
        write_json(directory / "outcomes.json", {"completed": completed, "rule": rule,
                                                 "timing_seconds": timings, "outcomes": outcomes})
        if records:
            payload = {key: torch.cat([r[key] for r in records]).numpy() for key in records[0]}
            np.savez_compressed(directory / "experience.npz", **payload)
    if not completed or len(outcomes) != len(cases) or any(o["kind"] not in ("win", "loss") for o in outcomes):
        raise RuntimeError("incomplete combat cohort; no training update is authorized")
    if any(o["kind"] == "win" and not o["win_replayed"] for o in outcomes):
        raise RuntimeError("winning sample lacks shared-core replay")
    if demonstrations:
        from tools.guided_search.local_demonstrations import export_demonstrations
        start = time.perf_counter()
        export_demonstrations(env, directory)
        timings["demonstration_export"] = time.perf_counter() - start
    timings["cohort_wall"] = time.perf_counter() - wall_start
    write_json(directory / "wall.json", {"seconds": timings["cohort_wall"], "includes": "construction, preparation, execution, replay and evidence writing"})
    return records, outcomes, timings


def update(model, optimizer, records, outcomes, config):
    if any(o["kind"] not in ("win", "loss") for o in outcomes):
        raise ValueError("Unknown is not a training terminal")
    ordered = sorted(outcomes, key=lambda o: o["slot"])
    if [o["slot"] for o in ordered] != list(range(len(outcomes))):
        raise ValueError("terminal slots are not a complete cohort")
    batch = {key: torch.cat([r[key] for r in records]).to(config.device) for key in records[0]}
    returns = torch.tensor([float(o["kind"] == "win") for o in ordered], device=config.device)[batch["slots"]]
    logits, values = model(batch)
    log_probs = logits.log_softmax(-1)
    logp = log_probs.gather(1, batch["choices"][:, None]).squeeze(1)
    # This is one on-policy update. A stale collector may not become PPO data.
    if not torch.allclose(logp.detach(), batch["logp"], atol=1e-4, rtol=1e-4):
        raise ValueError("behavior probabilities changed before the on-policy update")
    advantage = returns - batch["old_value"]
    policy_loss = -(logp * advantage).sum() / len(outcomes)
    value_loss = nn.functional.binary_cross_entropy_with_logits(values, returns)
    safe_log_probs = log_probs.masked_fill(~batch["mask"], 0)
    entropy = -(log_probs.exp() * safe_log_probs).sum(-1).mean()
    loss = policy_loss + config.value_weight * value_loss - config.entropy_weight * entropy
    if not torch.isfinite(loss):
        raise ValueError("nonfinite learning loss")
    optimizer.zero_grad(set_to_none=True)
    loss.backward()
    gradient = nn.utils.clip_grad_norm_(model.parameters(), config.gradient_limit, error_if_nonfinite=True)
    optimizer.step()
    return {"policy_loss": float(policy_loss.detach()), "value_loss": float(value_loss.detach()),
            "entropy": float(entropy.detach()), "gradient_norm": float(gradient), "rows": len(returns)}


def cases(seeds, task="starter"):
    enemies = {"starter": ("JawWorm", "Cultist"),
               "draw_select": ("JawWorm", "Cultist", "TwoLouse", "ThreeLouse")}[task]
    return [(seed, enemy) for seed in seeds for enemy in enemies]


def run(config: Config, output: Path):
    wall_start = time.perf_counter()
    config.validate()
    output.mkdir(parents=True, exist_ok=False)
    torch.set_num_threads(config.threads)
    contract = {"config": asdict(config), "objective": "combat victory 1; actual defeat 0; no resource shaping",
                "information": "fixed_oracle", "source": f"prescribed {config.task} encounters",
                "numeric_schema": 2 if config.task == "draw_select" else 1,
                "training_cases": cases(config.train_seeds(), config.task), "evaluation_cases": cases(config.eval_seeds(), config.task),
                "evaluation_rules": ["sampled", "greedy"], "external_models": False,
                "search_teacher": False, "checkpoints": "all training generations; final selection is the last declared generation"}
    write_json(output / "contract.json", contract)
    manifest = {"head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "status": subprocess.check_output(["git", "status", "--short"], cwd=ROOT, text=True),
                "torch": torch.__version__, "numpy": np.__version__, "script_sha256": hash_file(__file__),
                "binary_sha256": {p.name: hash_file(p) for p in Path(sts_learning_bridge.__file__).parent.glob("*.pyd")}}
    write_json(output / "manifest.json", manifest)
    (output / "producer.patch").write_bytes(subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT))
    # New untracked experiment sources need provenance too.
    source_dir = output / "sources"
    source_dir.mkdir()
    for rel in ["tools/guided_search/local_train.py", "tools/guided_search/local_tactics.py",
                "crates/sts_guided_search/src/local.rs", "crates/sts_guided_search/src/local_tactics.rs",
                "bindings/python_learning/src/guided.rs"]:
        (source_dir / rel.replace("/", "__")).write_bytes((ROOT / rel).read_bytes())
    deadline = time.monotonic() + config.max_wall_seconds
    summary = []
    try:
        for model_seed in config.model_seeds:
            check_deadline(deadline)
            directory = output / f"model-{model_seed}"
            directory.mkdir()
            torch.manual_seed(model_seed)
            model = network(config).to(config.device).eval()
            initial = {k: v.detach().clone() for k, v in model.state_dict().items()}
            save_weights(directory / "model-0.npz", model)
            optimizer = torch.optim.Adam(model.parameters(), lr=config.learning_rate)
            generator = torch.Generator(device=config.device).manual_seed(model_seed + 8000)
            for generation in range(1, config.generations + 1):
                check_deadline(deadline)
                records, outcomes, timing = rollout(model, cases(config.train_seeds(), config.task), config, generator,
                                                     directory / f"train-{generation}", deadline)
                check_deadline(deadline)
                start = time.perf_counter()
                metrics = update(model, optimizer, records, outcomes, config)
                metrics.update(generation=generation, wins=sum(o["kind"] == "win" for o in outcomes),
                               episodes=len(outcomes), rollout_seconds=timing,
                               update_seconds=time.perf_counter() - start)
                write_json(directory / f"train-{generation}" / "update.json", metrics)
                save_weights(directory / f"model-{generation}.npz", model)
                if generation == 1 or generation % 6 == 0 or generation == config.generations:
                    print(json.dumps({"model": model_seed, "generation": generation,
                                      "train_wins": metrics["wins"], "episodes": len(outcomes)}), flush=True)
            final = {k: v.detach().clone() for k, v in model.state_dict().items()}
            # Held-out outcomes cannot influence training, RNG or checkpoint choice.
            for checkpoint, weights in [("initial", initial), ("final", final)]:
                model.load_state_dict(weights)
                for rule in ("sampled", "greedy"):
                    eval_rng = torch.Generator(device=config.device).manual_seed(90000 + model_seed)
                    _, outcomes, timing = rollout(model, cases(config.eval_seeds(), config.task), config, eval_rng,
                                                   directory / f"eval-{checkpoint}-{rule}", deadline, rule)
                    summary.append({"model": model_seed, "checkpoint": checkpoint, "rule": rule,
                                    "wins": sum(o["kind"] == "win" for o in outcomes), "episodes": len(outcomes),
                                    "outcomes": outcomes, "timing_seconds": timing})
            write_json(directory / "evaluation.json", summary[-4:])
        write_json(output / "result.json", {"status": "complete", "evaluation": summary,
                                           "wall_seconds": time.perf_counter() - wall_start})
        print(json.dumps({"status": "complete", "evaluation": [{k: row[k] for k in ("model", "checkpoint", "rule", "wins", "episodes")} for row in summary]}), flush=True)
    except Exception as error:
        write_json(output / "failure.json", {"type": type(error).__name__, "message": str(error), "completed_evaluations": summary})
        raise


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    config = Config(**json.loads(args.config.read_text(encoding="utf-8")))
    run(config, args.output)


if __name__ == "__main__":
    main()
