"""Fresh, state-only executor outcomes; no action imitation or witness memory."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch
from torch import nn
import sts_learning_bridge as bridge
from entity_learning import EntityEncoder, Position, SCHEMA, pack, positions

OBJECTIVE = "fixed_executor_actual_outcome_and_retained_hp_v1"
WORK = 4000


def save(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)


def prepare(spec, actions):
    """Execute preparation through the native legal surface, retaining real inputs."""
    env = bridge.GuidedCombatBatch.from_specifications([spec], 256)
    for action in actions:
        episode, = json.loads(env.evidence_json())["episodes"]
        frame = env.decision_batch()
        if len(frame['slots']) == 0:
            raise ValueError(f"preparation reached terminal before {action}: {spec['encounter']}, HP={spec['hp']}")
        item, = positions(frame)
        columns = item.columns
        if action == "EndTurn":
            candidates = np.flatnonzero(columns["action_ids"][:, 0] == 2)
        else:
            hand = episode["successor"]["combat"]["zones"]["hand"]
            index = next((i for i, c in enumerate(hand) if c["id"] == action), None)
            if index is None:
                raise ValueError(f"preparation lacks {action}")
            candidates = [i for i, kind in enumerate(columns["action_ids"][:, 0])
                if kind == 1 and columns["entity_values"][columns["action_refs"][i, 0], 0] == 1
                and round(columns["entity_values"][columns["action_refs"][i, 0], 6] * 32) == index]
        if len(candidates) != 1:
            raise ValueError("preparation requires one exact legal action")
        env.advance(frame["revision"], [int(candidates[0])])
    episode, = json.loads(env.evidence_json())["episodes"]
    return episode["inputs"], episode["successor"]


def recipes():
    rows = []
    for split, offset in (("train", 0), ("evaluation", 10000)):
        strength_count, draw_count = (12, 8) if split == "train" else (6, 4)
        for family, count in (("strength_duration", strength_count), ("draw_order", draw_count)):
            for i in range(count):
                enemy = ("TheGuardian", "JawWorm", "Cultist")[i % 3] if family == "strength_duration" else "TheGuardian"
                deck = ([("Disarm", 1), ("DarkShackles", 0), ("Bash", 1),
                         ("TwinStrike", 1), ("ShrugItOff", 1)] if family == "strength_duration" else
                        [("BattleTrance", 0), ("ShrugItOff", 1), ("PommelStrike", 1),
                         ("TwinStrike", 1), ("Bash", 1)] + [("Strike", 1)] * 2 + [("Defend", 1)] * 3)
                hp = ((12, 25, 45, 65) if split == 'train' else (18, 35))[i // 3]
                if family == 'strength_duration' and enemy != 'TheGuardian':
                    hp = min(75, hp + 15)  # The preparation crosses an actual enemy turn.
                spec = dict(seed=4300000 + offset + i + (1000 if family == "draw_order" else 0),
                    encounter=enemy, act=1, room="MonsterRoomBoss" if enemy == "TheGuardian" else "MonsterRoom",
                    hp=hp, deck=deck,
                    relics=["BagOfPreparation"] if family == "draw_order" else [],
                    potions=[], max_potion_uses=0)
                if family == "draw_order":
                    # Admit only an observed hand containing both cards; no outcome is inspected.
                    for _ in range(100):
                        _, root = prepare(spec, [])
                        if {"BattleTrance", "ShrugItOff"} <= {c["id"] for c in root["combat"]["zones"]["hand"]}:
                            break
                        spec["seed"] += 100
                    else:
                        raise ValueError("no admitted draw-order source")
                # After Bash and temporary reduction, exactly one energy remains:
                # permanent reduction and immediate block cannot both be played.
                choices = ([['EndTurn', 'Bash', 'DarkShackles', card] for card in ('Disarm', 'ShrugItOff')]
                           if family == "strength_duration" else
                           [['BattleTrance', 'ShrugItOff'], ['ShrugItOff', 'BattleTrance']])
                if i % 2:
                    choices.reverse()  # A stable first-candidate control has no privileged card.
                group = f"{family}-seed-{spec['seed']}"
                for j, actions in enumerate(choices):
                    prefix, position = prepare(spec, actions)
                    rows.append(dict(label=f"{split}-{family}-{i}-{j}", group=group, split=split,
                        family=family, actions=actions, spec=spec.copy(), prefix=prefix, position=position))
    train = {r['spec']['seed'] for r in rows if r['split'] == 'train'}
    evaluation = {r['spec']['seed'] for r in rows if r['split'] == 'evaluation'}
    if train & evaluation:
        raise ValueError("initialization seed leakage")
    return rows


def collect(exe, out, rows, name):
    folder = out / name
    case_path = out / f"{name}.json"
    save(case_path, [{k:r[k] for k in ('label', 'group', 'spec', 'prefix')} for r in rows])
    started = time.perf_counter()
    with (out / f"{name}.log").open("x", encoding="utf-8") as log:
        subprocess.run([str(exe), '--input', str(case_path), '--output', str(folder),
                        '--solver-work', str(WORK), '--max-anchors', '1'], check=True,
                       stdout=log, stderr=subprocess.STDOUT)
    documents = [json.loads((folder / f'case-{i:03}.json').read_text(encoding='utf-8'))
                 for i in range(len(rows))]
    contracts = [d['continuation'] for d in documents]
    if any(c != contracts[0] for c in contracts):
        raise ValueError("executor changed during collection")
    for i, (row, doc) in enumerate(zip(rows, documents)):
        if (doc['entity_schema'] != SCHEMA or doc['entity_objective'] != OBJECTIVE
                or doc['root'] != row['position'] or len(doc['rows']) != 1
                or len(doc['traces']) != 1 or doc['rows'][0]['offset'] != 0
                or 'witness_control' in doc['traces'][0]):
            raise ValueError("independent state-only outcome contract mismatch")
        row['trace'] = doc['traces'][0]
        row['receipt'] = str((folder / f'case-{i:03}.json').relative_to(out))
    return documents, contracts[0], time.perf_counter() - started


def entity_item(raw):
    names = dict(entity_ids='ids', entity_values='values', entity_refs='refs',
                 action_ids='action_ids', actions='actions', action_refs='action_refs')
    return Position({name:np.asarray(raw[key]) for name, key in names.items()})


def targets(rows):
    values = []
    for row in rows:
        trace = row['trace']
        if trace['terminal_kind'] not in ('win', 'loss'):
            raise ValueError("cutoff or escape is not a completed combat target")
        player = trace['endpoint']['combat']['entities']['player']
        won = float(trace['terminal_kind'] == 'win')
        if won and not 0 < player['current_hp'] <= player['max_hp']:
            raise ValueError("invalid actual victory HP")
        # Head 1 is unconditional retained HP: loss contributes zero, not missing data.
        values.append([won, won * max(0, player['current_hp']) / player['max_hp']])
    return torch.tensor(values, dtype=torch.float32)


class OutcomeNetwork(EntityEncoder):
    def __init__(self):
        super().__init__(hidden=32, layers=1)
        self.outcome = nn.Linear(32, 2)

    def forward(self, data):
        return self.outcome(self.encode_context(data)[:, 0])


def fit(rows, items):
    started = time.perf_counter()
    torch.manual_seed(4204)
    model = OutcomeNetwork()
    optimizer = torch.optim.AdamW(model.parameters(), lr=.002)
    truth = targets(rows)
    cohorts = [[i for i, r in enumerate(rows) if r['group'] == group] for group in sorted({r['group'] for r in rows})]
    rng = np.random.default_rng(4204)
    for _ in range(240):
        indices = [int(rng.choice(cohorts[int(rng.integers(len(cohorts)))])) for _ in range(32)]
        prediction = model(pack([items[i] for i in indices]))
        loss = (nn.functional.binary_cross_entropy_with_logits(prediction[:, 0], truth[indices, 0])
                + nn.functional.mse_loss(prediction[:, 1].sigmoid(), truth[indices, 1]))
        optimizer.zero_grad()
        loss.backward()
        nn.utils.clip_grad_norm_(model.parameters(), 1.)
        optimizer.step()
    model.eval()
    return model, dict(seconds=time.perf_counter() - started, steps=240,
                       parameters=sum(p.numel() for p in model.parameters()))


def choose(rows, predictions):
    result = []
    for group in sorted({r['group'] for r in rows}):
        ids = [i for i, r in enumerate(rows) if r['group'] == group]
        result.append(dict(group=group, family=rows[ids[0]]['family'], ids=ids,
            model=max(ids, key=lambda i:float(predictions[i].sum())), first=ids[0],
            current_hp=max(ids, key=lambda i:rows[i]['position']['combat']['entities']['player']['current_hp'])))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--collector', type=Path, default=Path('target/dev-opt/examples/continuation_value.exe'))
    args = parser.parse_args()
    out, exe = args.output.resolve(), args.collector.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / 'producer.py').write_bytes(Path(__file__).read_bytes())
    (out / 'collector-source.rs').write_bytes(Path('crates/sts_guided_search/examples/continuation_value.rs').read_bytes())
    torch.set_num_threads(1)
    rows = recipes()
    save(out / 'sources.json', rows)
    save(out / 'manifest.json', dict(entity_schema=SCHEMA, objective=OBJECTIVE, solver_work=WORK,
        collector_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        collector_source_sha256=hashlib.sha256(Path('crates/sts_guided_search/examples/continuation_value.rs').read_bytes()).hexdigest(),
        encoder_sha256=hashlib.sha256(Path(__file__).with_name('entity_learning.py').read_bytes()).hexdigest(),
        bridge_binaries={p.name:hashlib.sha256(p.read_bytes()).hexdigest()
                         for p in Path(bridge.__file__).parent.glob('*.pyd')},
        git_head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        limits='Constructed Oracle states, no potion use; no action imitation, NoSL or whole-run claim.'))
    train = [r for r in rows if r['split'] == 'train']
    evaluation = [r for r in rows if r['split'] == 'evaluation']
    docs, contract, train_seconds = collect(exe, out, train, 'training')
    items = [entity_item(d['rows'][0]['entities']) for d in docs]
    model, training = fit(train, items)
    torch.save(dict(schema=SCHEMA, objective=OBJECTIVE, state_dict=model.state_dict()), out / 'model.pt')
    started = time.perf_counter()
    eval_items = positions(bridge.combat_entity_batch(json.dumps([r['position'] for r in evaluation]), max_potion_uses=0))
    encoding_seconds = time.perf_counter() - started
    started = time.perf_counter()
    with torch.inference_mode():
        prediction = model(pack(eval_items)).sigmoid().numpy()
    inference_seconds = time.perf_counter() - started
    choices = choose(evaluation, prediction)
    save(out / 'frozen-predictions.json', dict(predictions=prediction.tolist(), choices=choices))
    print(f"Frozen model: {len(train)} training roots, {len(evaluation)} held-out roots", flush=True)
    eval_docs, eval_contract, eval_seconds = collect(exe, out, evaluation, 'evaluation')
    if contract != eval_contract:
        raise ValueError("held-out executor differs from training")
    for item, doc in zip(eval_items, eval_docs):
        actual = entity_item(doc['rows'][0]['entities'])
        for column in item.columns:
            np.testing.assert_allclose(item.columns[column], actual.columns[column], rtol=0, atol=1e-7)
    truth = targets(evaluation).numpy()
    utility = truth.mean(axis=1)
    for group in choices:
        group['actual'] = [{k:r[k] for k in ('actions',)} | dict(index=i,
            kind=r['trace']['terminal_kind'], hp=r['trace']['endpoint']['combat']['entities']['player']['current_hp'],
            prediction=prediction[i].tolist()) for i in group['ids'] for r in [evaluation[i]]]
        group['utility'] = {method:float(utility[group[method]]) for method in ('model', 'first', 'current_hp')}
        group['best_in_pool'] = float(utility[group['ids']].max())
        group['worst_in_pool'] = float(utility[group['ids']].min())
    different = [g for g in choices if g['best_in_pool'] - g['worst_in_pool'] > 1e-6]
    result = dict(training=training, contract=contract, groups=choices,
        evaluation_mae=np.abs(prediction - truth).mean(axis=0).tolist(),
        mean_utility={method:float(np.mean([g['utility'][method] for g in choices])) for method in ('model', 'first', 'current_hp')},
        model_best_in_pool=sum(abs(g['utility']['model'] - g['best_in_pool']) < 1e-6 for g in choices),
        distinct_result_groups=dict(count=len(different), **{method:sum(
            abs(g['utility'][method] - g['best_in_pool']) < 1e-6 for g in different)
            for method in ('model', 'first', 'current_hp')}),
        selected_wins={method:sum(evaluation[g[method]]['trace']['terminal_kind'] == 'win'
                                 for g in choices) for method in ('model', 'first', 'current_hp')},
        outcomes={split:{kind:sum(r['trace']['terminal_kind'] == kind for r in split_rows) for kind in ('win', 'loss', 'cutoff')}
                  for split, split_rows in [('train', train), ('evaluation', evaluation)]},
        costs=dict(train_collection_seconds=train_seconds, evaluation_collection_seconds=eval_seconds,
                   encoding_seconds=encoding_seconds, inference_seconds=inference_seconds))
    save(out / 'results.json', result)
    print(json.dumps({k:v for k, v in result.items() if k not in ('groups', 'contract')}, indent=2))


if __name__ == '__main__':
    main()
