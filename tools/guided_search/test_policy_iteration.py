import json
import gzip
import copy
from dataclasses import replace

import numpy as np
import torch
import pytest
from policy_iteration import (Sample, collect, update, FrozenPolicyValue, LeafReturns,
                              update_value, improve_policy, bootstrap, population, training_population,
                              student_demonstrations, play, append_policy_examples)
from entity_learning import EntityNetwork, pack, positions
from sts_learning_bridge import PolicySearchBatch, policy_demonstration


def spec():
    return dict(seed=41, encounter="JawWorm", act=1, room="MonsterRoom", hp=60,
                deck=[("SearingBlow", 10)], relics=[], potions=[], max_potion_uses=0)


def test_fresh_demonstration_train_search_and_actual_return(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(31)
    demo = policy_demonstration(spec(), 256)
    p, = positions(demo)
    target = np.zeros(len(p.columns["actions"]), dtype=np.float32)
    target[demo["choices"][0]] = 1
    sample = Sample(p, target, demo["value"], 0, demo["choices"][0])
    model = EntityNetwork(hidden=16, layers=1)
    optimizer = torch.optim.AdamW(model.parameters(), lr=0.003)
    before = {k: v.clone() for k, v in model.state_dict().items()}
    loss = update(model, optimizer, [sample], 12, np.random.default_rng(37))
    assert np.isfinite(loss).all()
    assert any(not torch.equal(before[k], v) for k, v in model.state_dict().items())
    rows, result = collect(model, [spec()], 16, tmp_path / "fight", np.random.default_rng(39), training=False)
    assert result["counts"] == {"win": 1}
    assert result["outcomes"][0]["hp"] == 66  # settled Burning Blood
    assert rows and all(r.value == 0.94 for r in rows)
    assert all(np.isclose(r.policy.sum(), 1) for r in rows)
    assert json.loads((tmp_path / "fight/summary.json").read_text())["outcomes"][0]["potion_uses"] == 0
    with gzip.open(tmp_path / "fight/search.json.gz", "rt") as f:
        statistics = json.load(f)
    assert statistics == [r.search for r in rows]
    chosen = rows[-1].choice
    assert statistics[-1]["terminal_values"][chosen] == 0.94
    assert statistics[-1]["terminal_visits"][chosen] == statistics[-1]["visits"][chosen]
    taught = student_demonstrations([spec()], rows, 256, tmp_path / "correction", np.random.default_rng(40))
    assert taught and all(r.value >= rows[0].value for r in taught)
    source = json.loads((tmp_path / "correction/solver/sources.json").read_text())
    assert source["prefix_choices"] == [[]]


@pytest.mark.parametrize("training", [False, True])
def test_collected_policy_teaches_adopted_action_and_retains_exploration(tmp_path, training):
    class Uniform(torch.nn.Module):
        def forward(self, data):
            return (torch.zeros_like(data["mask"], dtype=torch.float32).masked_fill(~data["mask"], -torch.inf),
                    torch.zeros(len(data["mask"])))

    rows, result = collect(Uniform(), [spec()], 16, tmp_path / "teacher",
                           np.random.default_rng(101), training=training)
    assert result["actor"]["action_selection"] == ("sample_visits" if training else "argmax_visits_then_q")
    assert any(np.count_nonzero(r.search["visits"]) > 1 for r in rows)
    for row in rows:
        assert row.policy[row.choice] == 1 and np.count_nonzero(row.policy) == 1
        assert sum(row.search["visits"]) == 16
        assert row.search["visits"][row.choice] > 0
        if not training:
            leaders = np.flatnonzero(np.asarray(row.search["visits"]) == max(row.search["visits"]))
            assert row.choice in leaders
            assert row.search["qvalues"][row.choice] == max(row.search["qvalues"][i] for i in leaders)


def test_equal_visits_use_terminal_return_instead_of_end_turn_ordinal(tmp_path):
    class Uniform(torch.nn.Module):
        def forward(self, data):
            return (torch.zeros_like(data["mask"], dtype=torch.float32).masked_fill(~data["mask"], -torch.inf),
                    torch.zeros(len(data["mask"])))

    # With two simulations both EndTurn and the lethal card receive one visit.
    # The real terminal return must beat the first action's nonterminal estimate.
    rows, result = collect(Uniform(), [spec()], 2, tmp_path / "fight",
                          np.random.default_rng(103), training=False)
    assert result["actor"]["action_selection"] == "argmax_visits_then_q"
    assert result["counts"] == {"win": 1} and len(rows) == 1
    row, = rows
    assert row.position.columns["action_ids"][0, 0] == 2
    assert row.search["visits"] == [1, 1]
    assert row.position.columns["action_ids"][row.choice, 0] == 1
    assert row.search["terminal_values"][row.choice] == .94
    assert row.search["qvalues"][row.choice] > row.search["qvalues"][0]


def test_truncated_search_target_does_not_train_a_fictitious_loss():
    torch.set_num_threads(1)
    s = spec(); s["deck"] = [("Defend", 0)]
    env = PolicySearchBatch([s], 1, 1)
    while True:
        frame = env.evaluation_batch()
        if not len(frame["slots"]): break
        env.evaluate(frame["revision"], [[1.] * len(p.columns["actions"]) for p in positions(frame)], [0.5])
    frame = env.decision_batch(); p, = positions(frame)
    policy = np.asarray(frame["visits"][0], dtype=np.float32)
    choice = int(policy.argmax()); env.advance(frame["revision"], [choice])
    outcome, = json.loads(env.outcomes_json())
    assert outcome["kind"] == "truncated" and outcome["value"] is None
    model = EntityNetwork(hidden=16, layers=1)
    optimizer = torch.optim.SGD(model.parameters(), lr=.01)
    before = model.value.weight.detach().clone()
    update(model, optimizer, [Sample(p, policy, None, 0, choice)], 1, np.random.default_rng(43))
    torch.testing.assert_close(model.value.weight, before)
    assert torch.isfinite(model(pack([p]))[1]).all()


def test_leaf_returns_fit_separate_value_without_changing_policy_or_parent(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(61)
    policy = EntityNetwork(hidden=16, layers=1).eval().requires_grad_(False)
    critic = copy.deepcopy(policy).requires_grad_(True)
    parent = PolicySearchBatch([spec()], 4, 100)
    frame = parent.evaluation_batch()
    items = positions(frame)

    class EveryQuery:
        def integers(self, _): return 0

    leaves = LeafReturns(policy, critic, EveryQuery(), 4)
    before = {k:v.clone() for k,v in policy.state_dict().items()}
    with torch.no_grad(): old_value = critic(pack(items))[1].clone()
    leaves(parent, frame, items)
    assert leaves.queries[0]["kind"] in ("win", "loss")
    assert leaves.rows[leaves.queries[0]["sample_index"]].position.columns.keys() == items[0].columns.keys()
    for key in items[0].columns:
        np.testing.assert_array_equal(leaves.rows[0].position.columns[key], items[0].columns[key])
    assert parent.evaluation_batch()["revision"] == frame["revision"]
    assert json.loads(parent.outcomes_json())[0]["decisions"] == 0
    assert all(r.value == leaves.queries[0]["value"] for r in leaves.rows)
    summary = leaves.save(tmp_path / "leaves")
    assert summary["queries"] == 1 and summary["truncated"] == 0
    assert summary["action_selection"] == "argmax_visits_then_q"
    assert summary["simulations"] == 4 and summary["search_transitions"] > 0
    assert all(sum(r.search["visits"]) == 4 for r in leaves.rows)
    assert all(not r.policy.any() for r in leaves.rows)
    assert sum(r.query for r in leaves.rows) == 1
    assert leaves.rows[leaves.queries[0]["sample_index"]].query
    optimizer = torch.optim.AdamW(critic.parameters(), lr=.003)
    loss = update_value(critic, optimizer, leaves.rows, 12, np.random.default_rng(67))
    assert np.isfinite(loss)
    for k, v in policy.state_dict().items(): torch.testing.assert_close(v, before[k], rtol=0, atol=0)
    with torch.no_grad():
        logits, value = FrozenPolicyValue(policy, critic)(pack(items))
        torch.testing.assert_close(logits, policy(pack(items))[0], rtol=0, atol=0)
        assert not torch.equal(value, old_value)


def test_leaf_labels_use_search_and_respect_the_query_budget():
    class PreferEnd(torch.nn.Module):
        def forward(self, data):
            logits = torch.where(data["action_ids"][:, :, 0] == 2,
                                 torch.log(torch.tensor(.8)), torch.log(torch.tensor(.2)))
            return logits.masked_fill(~data["mask"], -torch.inf), torch.full((len(logits),), -30.)

    class EveryQuery:
        def integers(self, _): return 0

    parent = PolicySearchBatch([spec(), spec()], 32, 100)
    frame = parent.evaluation_batch()
    items = positions(frame)
    model = PreferEnd()
    child = parent.leaf_rollouts(frame["revision"], [0], 32, 100)
    child_frame = child.evaluation_batch()
    grandchild = child.leaf_rollouts(child_frame["revision"], [0], 32, 100)
    source = json.loads(grandchild.evidence_json())["sources"][0]
    assert source["parent_source"]["specification"] == json.loads(child.evidence_json())["sources"][0]["specification"]
    assert source["parent_source"]["slot"] == 0
    for key in items[0].columns:
        np.testing.assert_array_equal(positions(grandchild.evaluation_batch())[0].columns[key], items[0].columns[key])
    with torch.no_grad():
        logits, _ = model(pack(items))
    assert all(p.columns["action_ids"][c, 0] == 2 for p, c in zip(items, logits.argmax(-1)))
    leaves = LeafReturns(model, model, EveryQuery(), 32, query_limit=1)
    leaves(parent, frame, items)
    leaves(parent, frame, items)
    assert len(leaves.queries) == 1
    assert leaves.queries[0]["kind"] == "win" and leaves.queries[0]["value"] == .94
    assert parent.evaluation_batch()["revision"] == frame["revision"]
    assert all(o["decisions"] == 0 for o in json.loads(parent.outcomes_json()))


def test_time_limited_actor_preserves_unfinished_outcomes():
    model = EntityNetwork(hidden=16, layers=1)
    env = PolicySearchBatch([spec()], 4, 100)
    rows, summary = play(model, env, 4, np.random.default_rng(43), max_seconds=0.)
    assert rows == [] and summary["stop"] == "time_budget"
    assert summary["outcomes"][0]["kind"] == "active"
    assert summary["outcomes"][0]["value"] is None


def test_value_objective_keeps_query_mass_when_continuations_are_long():
    torch.set_num_threads(1)
    parent = PolicySearchBatch([spec()], 1, 100)
    p, = positions(parent.evaluation_batch())
    target = np.zeros(len(p.columns["actions"]), dtype=np.float32)

    class ConstantValue(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.logit = torch.nn.Parameter(torch.tensor(0.))

        def forward(self, data):
            return torch.zeros_like(data["mask"], dtype=torch.float32), self.logit.expand(len(data["mask"]))

    model = ConstantValue()
    optimizer = torch.optim.SGD(model.parameters(), lr=1.)
    # Deliberately contradictory labels isolate the sampling measure: one query
    # and 99 suffix rows must have the same mass as one query and one suffix.
    rows = [Sample(p, target, 0., 0, 0, query=True)]
    rows += [Sample(p, target, 1., 0, 0) for _ in range(99)]
    rows += [Sample(p, target, None, 0, 0, query=True)]
    update_value(model, optimizer, rows, 1, np.random.default_rng(71))
    assert model.logit.item() == 0.
    # A cohort with only one role remains usable, without fictitious partners.
    update_value(model, optimizer, rows[1:-1], 1, np.random.default_rng(73))
    assert model.logit.item() > 0.


def test_leaf_query_rows_survive_unequal_continuations_and_multiple_batches(tmp_path):
    torch.set_num_threads(1)
    slow = spec(); slow["deck"] = [("Defend", 0)]
    parent = PolicySearchBatch([spec(), slow], 1, 100)
    frame = parent.evaluation_batch()

    class EveryQuery:
        def integers(self, _): return 0

    class PlayFirst(torch.nn.Module):
        def forward(self, data):
            logits = (data["action_ids"][:, :, 0] == 1).float()
            return logits.masked_fill(~data["mask"], -torch.inf), torch.zeros(len(logits))

    model = PlayFirst()
    leaves = LeafReturns(model, model, EveryQuery(), 1)
    items = positions(frame)
    for _ in range(2):
        leaves(parent, frame, items)
    assert len(leaves.queries) == sum(r.query for r in leaves.rows) == 4
    assert len(leaves.rows) > 4
    for i, q in enumerate(leaves.queries):
        row = leaves.rows[q["sample_index"]]
        assert row.query and row.slot == i % 2
        assert row.value == q["value"]
        for key in row.position.columns:
            np.testing.assert_array_equal(row.position.columns[key], items[i % 2].columns[key])
    leaves.save(tmp_path / "leaves")
    with np.load(tmp_path / "leaves/samples.npz") as saved:
        assert saved["query"].sum() == 4


@pytest.mark.parametrize("new_plans", [False, True])
def test_solver_distillation_improves_prior_without_training_critic_or_value_head(tmp_path, new_plans):
    torch.set_num_threads(1); torch.manual_seed(79)
    policy = EntityNetwork(hidden=16, layers=1).eval().requires_grad_(False)
    critic = copy.deepcopy(policy)
    actor = FrozenPolicyValue(policy, critic)
    rows = bootstrap([spec()], 256, tmp_path / "teacher")
    assert rows and all(r.value == .94 and r.search is None for r in rows)
    p = rows[0].position
    data = pack([p])
    target = torch.tensor(rows[0].policy)
    with torch.no_grad():
        old_logits, old_value = actor(data)
        before = -(target * old_logits[0].log_softmax(-1)).sum()
    head = {k:v.clone() for k,v in policy.value.state_dict().items()}
    critic_weights = {k:v.clone() for k,v in critic.state_dict().items()}
    optimizer = torch.optim.AdamW(policy.parameters(), lr=.003)
    improve_policy(policy, optimizer, rows if new_plans else [], rows, 16, np.random.default_rng(89))
    with torch.no_grad():
        logits, value = actor(data)
        assert -(target * logits[0].log_softmax(-1)).sum() < before
        torch.testing.assert_close(value, old_value, rtol=0, atol=0)
    for k,v in policy.value.state_dict().items(): torch.testing.assert_close(v,head[k],rtol=0,atol=0)
    for k,v in critic.state_dict().items(): torch.testing.assert_close(v,critic_weights[k],rtol=0,atol=0)
    assert not policy.training and not any(p.requires_grad for p in policy.parameters())
    with pytest.raises(ValueError, match="normalized action"):
        improve_policy(policy, optimizer, [Sample(p,np.zeros_like(rows[0].policy),None,0,0)],rows,1,np.random.default_rng(97))


def test_solver_without_a_found_win_keeps_sources_without_inventing_action_labels(tmp_path):
    no_attack = spec(); no_attack["deck"] = [("Defend", 0)]
    rows = bootstrap([no_attack], 128, tmp_path / "teacher")
    assert rows == []
    source = json.loads((tmp_path / "teacher/sources.json").read_text())
    assert source["results"] == [dict(slot=0, found=False, value=None)]
    with np.load(tmp_path / "teacher/samples.npz") as samples:
        assert len(samples["choice"]) == 0


def test_training_contexts_vary_within_each_encounter_without_shared_decks():
    from collections import Counter

    initial = population(101, 36)
    varied = training_population(101, 36)
    groups = {}
    for original, changed in zip(initial, varied):
        assert changed["seed"] == original["seed"] and len(changed["deck"]) == len(original["deck"])
        attack_delta = changed["deck"].count(("Strike", 0)) - original["deck"].count(("Strike", 0))
        block_delta = changed["deck"].count(("Defend", 0)) - original["deck"].count(("Defend", 0))
        groups.setdefault(changed["encounter"], Counter())[(changed["hp"], attack_delta, block_delta)] += 1
    expected = Counter((hp, attack, -attack) for hp in (45, 60, 75) for attack in (-1, 0, 1))
    assert len(groups) == 4 and all(group == expected for group in groups.values())
    # A later construction edit must not silently change another fight's source.
    before = copy.deepcopy(varied[4])
    varied[0]["deck"].clear()
    assert varied[4] == before
    before = copy.deepcopy(initial[4])
    initial[0]["deck"].clear()
    assert initial[4] == before


@pytest.mark.parametrize("student_return,solver_return,source", [
    (.94, .7, "student"), (.94, .94, "student"), (.94, .98, "solver"),
    (.94, None, "student"), (0., .7, "solver"), (0., None, "none"),
])
def test_student_teaching_retains_the_better_executed_continuation(
        tmp_path, monkeypatch, student_return, solver_return, source):
    import policy_iteration as iteration

    frame = policy_demonstration(spec(), 256)
    p, = positions(frame)
    choice = frame["choices"][0]
    target = np.zeros(len(p.columns["actions"]), dtype=np.float32); target[choice] = 1.
    played = Sample(p, target, student_return, 0, choice)
    teacher_choice = next(i for i in range(len(target)) if i != choice)
    teacher_target = np.zeros_like(target); teacher_target[teacher_choice] = 1.

    def teacher(specs, work, directory, *, prefixes):
        assert prefixes == [[]]
        return [] if solver_return is None else [Sample(p, teacher_target, solver_return, 0, teacher_choice)]

    # Returns deliberately differ at the same source. Only this selection rule is
    # stubbed; exact prefix replay and real selection choices have native tests.
    monkeypatch.setattr(iteration, "bootstrap", teacher)
    result = student_demonstrations([spec()], [played], 256, tmp_path / "teaching", np.random.default_rng(107))
    receipt, = json.loads((tmp_path / "teaching/corrections.json").read_text())
    assert receipt["adopted"] == source
    if source == "none":
        assert result == []
    else:
        assert result[0].choice == (choice if source == "student" else teacher_choice)
        assert result[0].value == (student_return if source == "student" else solver_return)
    np.testing.assert_array_equal(played.policy, target)
    assert not played.query


def test_teacher_queries_cover_attached_selections_and_keep_original_fight_slots(tmp_path, monkeypatch):
    import policy_iteration as iteration

    def selection(card, category):
        recipe = spec()
        recipe["deck"] = [(card, 1 if card == "TrueGrit" else 0),
                          ("Defend", 0), ("Defend", 0), ("Strike", 0), ("Strike", 0)]
        env = PolicySearchBatch([recipe], 1, 100)
        frame = env.evaluation_batch(); ordinary, = positions(frame)
        choice = next(i for i, refs in enumerate(ordinary.columns["action_refs"])
                      if ordinary.columns["action_ids"][i, 0] == 1
                      and ordinary.columns["entity_ids"][refs[0], 1] == category)
        first = True
        while len(frame["slots"]):
            p, = positions(frame)
            env.evaluate(frame["revision"], [[float(i == choice) if first else 1.
                for i in range(len(p.columns["actions"]))]], [.5])
            first = False
            frame = env.evaluation_batch()
        frame = env.decision_batch(); env.advance(frame["revision"], [choice])
        pending, = positions(env.evaluation_batch())
        assert pending.columns["action_ids"][0, 0] == 5
        return ordinary, choice, pending

    ordinary, choice, exhaust = selection("TrueGrit", 10)
    _, _, upgrade = selection("Armaments", 60)

    def row(p, slot, action=0):
        target = np.zeros(len(p.columns["actions"]), dtype=np.float32); target[action] = 1.
        return Sample(p, target, 0., slot, action)

    # Long ordinary traces must not dilute rare choices. Their duplicated rows
    # isolate sampling; bootstrap is stubbed, so these are not replay evidence.
    played = [row(ordinary, slot, choice) for slot in range(2) for _ in range(20)]
    for slot in range(2):
        played.extend((row(exhaust, slot), row(upgrade, slot)))
    captured = {}

    def teacher(specs, work, directory, *, prefixes):
        captured["prefixes"] = prefixes
        result = []
        for query, prefix in enumerate(prefixes):
            p = ordinary if len(prefix) < 20 else (exhaust if len(prefix) == 20 else upgrade)
            target = np.zeros(len(p.columns["actions"]), dtype=np.float32); target[0] = 1.
            result.append(Sample(p, target, .8, query, 0))
        return result

    monkeypatch.setattr(iteration, "bootstrap", teacher)
    taught = student_demonstrations([spec(), spec()], played, 256,
        tmp_path / "teaching", np.random.default_rng(109))
    receipts = json.loads((tmp_path / "teaching/corrections.json").read_text())
    assert [(r["slot"], r["kind"]) for r in receipts] == [
        (0, "ordinary"), (0, "selection"), (1, "ordinary"), (1, "selection")]
    assert {len(prefix) for prefix in captured["prefixes"] if len(prefix) >= 20} == {20, 21}
    assert [r.slot for r in taught] == [0, 0, 1, 1]
    assert all(r["adopted"] == "solver" for r in receipts)


def test_policy_rehearsal_keeps_reused_generation_slots_separate():
    torch.set_num_threads(1)
    p, = positions(policy_demonstration(spec(), 256))
    left = Sample(p, np.array([1., 0.], dtype=np.float32), .8, 0, 0)
    right = Sample(p, np.array([0., 1.], dtype=np.float32), .4, 0, 1)
    pool = []
    append_policy_examples(pool, [left])
    append_policy_examples(pool, [])
    append_policy_examples(pool, [right]*99)
    assert left.slot == right.slot == 0
    assert [r.slot for r in pool] == [0] + [1]*99

    class ConstantPolicy(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.logit = torch.nn.Parameter(torch.tensor(.2))

        def forward(self, data):
            logits = torch.stack((self.logit, -self.logit)).expand(len(data["mask"]), -1)
            return logits, torch.zeros(len(logits))

    reference = [replace(left, slot=0), *([replace(right, slot=1)]*99)]
    models = [ConstantPolicy(), ConstantPolicy()]
    for model, examples in zip(models, (pool, reference)):
        improve_policy(model, torch.optim.SGD(model.parameters(), lr=.1), [], examples,
                       1, np.random.default_rng(113))
    torch.testing.assert_close(models[0].logit, models[1].logit, rtol=0, atol=0)


def test_extra_rows_from_one_fight_do_not_reweight_policy_teaching():
    torch.set_num_threads(1)
    p, = positions(policy_demonstration(spec(), 256))
    left = np.array([1., 0.], dtype=np.float32)
    right = np.array([0., 1.], dtype=np.float32)
    short = [Sample(p, left, .8, 0, 0), Sample(p, right, .8, 1, 1)]
    long = [*short[:1]] * 99 + short[1:]

    class ConstantPolicy(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.logit = torch.nn.Parameter(torch.tensor(0.))

        def forward(self, data):
            logits = torch.stack((self.logit, -self.logit)).expand(len(data["mask"]), -1)
            return logits, torch.zeros(len(logits))

    models = [ConstantPolicy(), ConstantPolicy()]
    for model, examples in zip(models, (short, long)):
        improve_policy(model, torch.optim.SGD(model.parameters(), lr=.1),
                       [], examples, 1, np.random.default_rng(113))
    torch.testing.assert_close(models[0].logit, models[1].logit, rtol=0, atol=0)


@pytest.mark.parametrize("forced_fight,forced_pool", [(False, False), (True, False), (True, True)])
def test_choice_sampling_preserves_decision_mass_and_skips_forced_gradients(forced_fight, forced_pool):
    torch.set_num_threads(1)
    recipe = spec(); recipe["deck"] = [("SearingBlow", 4)]
    plan = positions(policy_demonstration(recipe, 1024))
    p = next(p for p in plan if len(p.columns["actions"]) == 2)
    forced = next(p for p in plan if len(p.columns["actions"]) == 1)
    choices = [Sample(p, np.array([1., 0.], dtype=np.float32), .8, 0, 0)]
    automatic = Sample(forced, np.ones(1, dtype=np.float32), .8, 0, 0)

    class ConstantPolicy(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.logit = torch.nn.Parameter(torch.tensor(.2))

        def forward(self, data):
            assert (data["mask"].sum(-1) > 1).all()
            logits = torch.stack((self.logit, -self.logit)).expand(len(data["mask"]), -1)
            logits = logits[:, :data["mask"].shape[1]].masked_fill(~data["mask"], -torch.inf)
            return logits, torch.zeros(len(logits))

    # One informative decision in 100 has 1/100 of its fight's loss mass.
    # Omitted forced-only fights/pools must retain their original zero-loss mass.
    # The native positions are real; these targets isolate the sampling measure.
    rows = [*choices, *([automatic]*99)]
    if forced_fight:
        rows.append(Sample(forced, np.ones(1, dtype=np.float32), .8, 1, 0))
    model = ConstantPolicy()
    improve_policy(model, torch.optim.SGD(model.parameters(), lr=.1), rows if forced_pool else [],
                   [automatic] if forced_pool else rows, 1, np.random.default_rng(127))
    mass = .01 / (2 if forced_fight else 1) / (2 if forced_pool else 1)
    expected = .2 + .1*mass*2/(1+np.exp(.4))
    assert model.logit.item() == pytest.approx(expected, abs=1e-8)

    model = ConstantPolicy()
    before = model.logit.detach().clone()
    optimizer = torch.optim.AdamW(model.parameters(), lr=.1, weight_decay=.1)
    assert improve_policy(model, optimizer, [], [automatic], 2, np.random.default_rng(131)) == 0.
    torch.testing.assert_close(model.logit, before, rtol=0, atol=0)
    assert optimizer.state == {} and not model.training and not model.logit.requires_grad
