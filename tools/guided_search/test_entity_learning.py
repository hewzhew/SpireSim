import copy
from dataclasses import replace
import json

import numpy as np
import pytest

torch = pytest.importorskip("torch")
bridge = pytest.importorskip("sts_learning_bridge")
from tools.guided_search.entity_learning import (
    CaseSampling, Cohort, EntityNetwork, bounded_update, collect, pack, positions, update,
)


def spec(deck, *, enemy="JawWorm", relics=(), potions=(), hp=68, seed=73, act=1, room="MonsterRoom"):
    return {"seed":seed, "encounter":enemy, "act":act, "room":room, "hp":hp,
            "deck":deck, "relics":list(relics), "potions":list(potions), "max_potion_uses":None}


def batch(specs, limit=128, **kwargs):
    return bridge.GuidedCombatBatch.from_specifications(specs, limit, **kwargs)


def test_exact_native_entry_preserves_state_and_uses_the_shared_encoder():
    env = batch([spec([("Strike", 0)] * 5, potions=["DexterityPotion"])])
    evidence = json.loads(env.evidence_json())
    root = evidence["episodes"][0]["root"]
    imported = bridge.combat_entity_batch(json.dumps([root]))
    expected, = positions(env.decision_batch())
    actual, = positions(imported)
    for column in expected.columns:
        np.testing.assert_array_equal(actual.columns[column], expected.columns[column])
    assert json.loads(env.evidence_json()) == evidence

    exact = copy.deepcopy(root)
    exact["combat"]["entities"]["player"].update(current_hp=84, max_hp=109)
    p, = positions(bridge.combat_entity_batch(json.dumps([exact]), potion_uses=[1], max_potion_uses=1))
    np.testing.assert_allclose(p.columns["entity_values"][0, :2], [84 / 75, 109 / 75])
    assert p.columns["entity_values"][0, 8] == 1
    assert not np.any(p.columns["action_ids"][:, 0] == 3)
    with pytest.raises(ValueError, match="potion-use counts"):
        bridge.combat_entity_batch(json.dumps([exact]), potion_uses=[])
    with pytest.raises(ValueError, match="allowance"):
        bridge.combat_entity_batch(json.dumps([exact]), potion_uses=[2], max_potion_uses=1)
    # Old weights/records must not silently inherit changed relation meanings.
    with pytest.raises(ValueError, match="schema"):
        positions(dict(imported, schema=7))

    model = EntityNetwork(hidden=16, layers=1).eval()
    with torch.inference_mode():
        logits, value = model(pack([actual, p]))
    assert torch.isfinite(value).all()
    assert torch.isfinite(logits[pack([actual, p])["mask"]]).all()


def test_case_sampling_isolates_padding_reordering_and_other_case_lifetime():
    first = CaseSampling([31, 37, 41])
    second = CaseSampling([31, 37, 41])
    probability = torch.tensor([[.1, .2, .7]])
    for step in range(20):
        alone = first.sample(probability, [1], [3])
        # Another case continues alone for arbitrary extra decisions.
        for _ in range(step % 4):
            second.sample(torch.tensor([[.5, .5]]), [0], [2])
        rows = torch.tensor([[.2, .3, .1, .4, 0], [.1, .2, .7, 0, 0]])
        slots, counts = [2, 1], [4, 3]
        if step % 2:
            rows, slots, counts = rows.flip(0), slots[::-1], counts[::-1]
        mixed = second.sample(rows, slots, counts)
        assert alone.item() == mixed[slots.index(1)].item()
    # An invalid later row must not consume the earlier case's random stream.
    states = [g.get_state().clone() for g in second.generators]
    with pytest.raises(ValueError, match="probabilities"):
        second.sample(torch.tensor([[.1, .9], [float('nan'), .5]]), [0, 1], [2, 2])
    for state, generator in zip(states, second.generators):
        assert torch.equal(state, generator.get_state())
    for slots in ([0, 0], [0, 3], [0, 1.5], [0, True]):
        with pytest.raises(ValueError, match="slots"):
            second.sample(torch.ones(2, 2), slots, [2, 2])
    for seeds in ([], [1, 1], [-1], [2**64], [True]):
        with pytest.raises(ValueError, match="seeds"):
            CaseSampling(seeds)


@pytest.mark.parametrize("kind, uses, remaining", [(3, 1, 0), (4, 0, 0), (2, 0, 1)])
def test_terminal_resource_report_distinguishes_potion_use_discard_and_retention(kind, uses, remaining):
    env = batch([spec([("Defend", 0)], potions=["DexterityPotion"])], limit=1)
    frame = env.decision_batch()
    p, = positions(frame)
    choice = int(np.flatnonzero(p.columns["action_ids"][:, 0] == kind)[0])
    env.advance(frame["revision"], [choice])
    outcome, = json.loads(env.outcomes_json())
    assert outcome["kind"] == "unknown_decision_limit"
    assert outcome["potion_uses"] == uses
    assert sum(p is not None for p in outcome["potions"]) == remaining


def test_automatic_fairy_consumption_is_inventory_change_not_explicit_use():
    env = batch([spec([("Defend", 0)], enemy="Cultist", hp=1, potions=["FairyPotion"])], limit=2)
    for _ in range(2):
        frame = env.decision_batch()
        p, = positions(frame)
        end = int(np.flatnonzero(p.columns["action_ids"][:, 0] == 2)[0])
        env.advance(frame["revision"], [end])
    outcome, = json.loads(env.outcomes_json())
    assert outcome["kind"] == "unknown_decision_limit"
    assert outcome["hp"] > 0
    assert outcome["potion_uses"] == 0
    assert all(p is None for p in outcome["potions"])


@pytest.mark.parametrize("starting_hp, terminal_hp", [(68, 74), (73, 75)])
def test_terminal_hp_includes_capped_victory_healing(starting_hp, terminal_hp):
    # A synthetic one-action kill isolates reporting, not a training recipe.
    env = batch([spec([("SearingBlow", 10)], hp=starting_hp)], limit=1)
    frame = env.decision_batch()
    p, = positions(frame)
    play = int(np.flatnonzero(p.columns["action_ids"][:, 0] == 1)[0])
    env.advance(frame["revision"], [play])
    outcome, = json.loads(env.outcomes_json())
    assert outcome["kind"] == "win" and outcome["win_replayed"]
    assert outcome["starting_hp"] == starting_hp
    assert outcome["hp"] == terminal_hp
    episode, = json.loads(env.evidence_json())["episodes"]
    assert episode["successor"]["combat"]["entities"]["player"]["current_hp"] == terminal_hp


def test_act1_elite_population_keeps_sleep_facts_and_ordinary_potions_through_native_bridge():
    deck = [("Strike", 0)] * 5 + [("Defend", 0)] * 4 + [("Bash", 0), ("AscendersBane", 0), ("Bludgeon", 0)]
    env = batch([spec(deck, enemy=enemy, room="MonsterRoomElite", potions=["FirePotion"])
                 for enemy in ("GremlinNob", "Lagavulin", "ThreeSentries")])
    frame = env.decision_batch()
    initial = positions(frame)
    assert len(initial) == 3
    # The native encoder, not Python, supplies these stable fact categories.
    laga = initial[1].columns
    sleep = np.flatnonzero((laga["entity_ids"][:, 0] == 14) & (laga["entity_ids"][:, 1] == 14))
    assert len(sleep) == 1
    assert laga["entity_values"][sleep[0], 0] == 0
    for item in initial:
        assert sum(item.columns["entity_ids"][:, 0] == 2) == len(deck)
        assert np.any(item.columns["action_ids"][:, 0] == 3)  # actual potion use
    ends = [int(np.flatnonzero(p.columns["action_ids"][:, 0] == 2)[0]) for p in initial]
    env.advance(frame["revision"], ends)
    after = positions(env.decision_batch())
    laga = after[1].columns
    sleep = np.flatnonzero((laga["entity_ids"][:, 0] == 14) & (laga["entity_ids"][:, 1] == 14))
    assert len(sleep) == 1
    assert laga["entity_values"][sleep[0], 0] == pytest.approx(.1)
    torch.manual_seed(3)
    model = EntityNetwork(hidden=16, layers=1).eval()
    data = pack(after)
    with torch.no_grad():
        logits, values = model(data)
    assert torch.isfinite(values).all()
    assert torch.isfinite(logits[data["mask"]]).all()


def test_mixed_shapes_and_batch_order_preserve_policy_value_and_recorded_references():
    torch.set_num_threads(1)
    torch.manual_seed(32)
    a = spec([("Strike", 0), ("Strike", 1), ("Defend", 0)])
    b = spec([("Strike", 0)] * 24, enemy="ThreeLouse", relics=["Shuriken"])
    frame = batch([a, b]).decision_batch()
    items = positions(frame)
    assert sum(items[1].columns["entity_ids"][:, 0] == 2) == 24
    before = copy.deepcopy(items)
    model = EntityNetwork(hidden=16, layers=1).eval()
    with torch.no_grad():
        alone, av = model(pack([items[0]]))
        mixed, mv = model(pack(items))
        reverse, rv = model(pack(items[::-1]))
    count = len(items[0].columns["actions"])
    torch.testing.assert_close(alone[0], mixed[0, :count], atol=2e-6, rtol=2e-5)
    torch.testing.assert_close(av[0], mv[0], atol=2e-6, rtol=2e-5)
    torch.testing.assert_close(mixed, reverse.flip(0), atol=2e-6, rtol=2e-5)
    torch.testing.assert_close(mv, rv.flip(0), atol=2e-6, rtol=2e-5)
    modified = pack(items)
    modified["entity_values"][~modified["entity_mask"]] = 12345
    with torch.no_grad():
        masked, value = model(modified)
    torch.testing.assert_close(mixed, masked)
    torch.testing.assert_close(mv, value)
    frame["entity_values"][:] = 900
    frame["slots"][:] = 888
    for left, right in zip(before, items):
        for name in left.columns:
            np.testing.assert_array_equal(left.columns[name], right.columns[name])
    with pytest.raises(ValueError, match="attention budget"):
        pack(items, max_attention_cells=1)


def test_power_descriptions_are_equivariant_to_local_row_renumbering():
    from tools.guided_search.entity_learning import Position
    torch.set_num_threads(1)
    torch.manual_seed(49)
    item, = positions(batch([spec([('Disarm', 1), ('DarkShackles', 0), ('Strike', 0)],
                                 enemy='GremlinNob')]).decision_batch())
    columns = item.columns
    n = len(columns['entity_ids'])
    permutation = np.r_[0, np.arange(n - 1, 0, -1)]
    inverse = np.argsort(permutation)
    renamed = {}
    for name, original in columns.items():
        rows = np.array(original[permutation] if name.startswith('entity_') else original, copy=True)
        if name.endswith('_refs'):
            present = rows >= 0
            rows[present] = inverse[rows[present]]
        renamed[name] = rows
    model = EntityNetwork(hidden=16, layers=1).eval()
    with torch.inference_mode():
        before, value = model(pack([item]))
        after, rv = model(pack([Position(renamed)]))
    torch.testing.assert_close(before, after, atol=2e-6, rtol=2e-5)
    torch.testing.assert_close(value, rv, atol=2e-6, rtol=2e-5)


def take_kind(env, kind):
    f = env.decision_batch()
    p = positions(f)[0].columns
    env.advance(f["revision"], [int(np.flatnonzero(p["action_ids"][:, 0] == kind)[0])])


def test_potion_mechanics_reach_model_arrays_with_actual_potency_and_discovery_parameters():
    env = batch([spec([("Defend", 0)] * 5, relics=["SacredBark"],
                      potions=["BlockPotion", "PowerPotion"])])
    p = positions(env.decision_batch())[0].columns
    slots = np.flatnonzero(p["entity_ids"][:, 0] == 6)
    block, power = slots[:2]
    assert p["entity_values"][block, 2] == pytest.approx(24 / 20)
    assert p["entity_values"][block, 3] == pytest.approx(12 / 20)
    assert p["entity_values"][power, 12] == 2  # selected copies
    assert p["entity_values"][power, 16] == 3  # offered options
    assert p["entity_values"][power, 13:16].tolist() == [1., 0., 1.]  # zero cost, skip
    role_rows = np.flatnonzero(p["entity_ids"][:, 0] == 22)
    assert set(p["entity_refs"][role_rows, 0]) == {block, power}
    assert {tuple(v) for v in p["entity_ids"][role_rows]} == {(22, 5), (22, 15)}
    f = env.decision_batch()
    ordinal = next(i for i, (kind, _) in enumerate(p["action_ids"])
                   if kind == 3 and p["action_refs"][i, 0] == block)
    env.advance(f["revision"], [ordinal])
    after = positions(env.decision_batch())[0].columns
    assert after["entity_values"][0, 2] == pytest.approx(24 / 50)
    assert not any(tuple(v) == (22, 5) for v in after["entity_ids"])


def test_generation_discovery_and_ordered_selection_flow_through_native_arrays():
    env = batch([spec([("PowerThrough", 0), ("Strike", 0), ("Defend", 0)])])
    initial = positions(env.decision_batch())[0]
    # Power Through is the only skill here that is not Defend: choose by source
    # numerical card category obtained from its distinct action damage/block facts.
    f = env.decision_batch()
    p = positions(f)[0].columns
    choice = next(i for i, (kind, _) in enumerate(p["action_ids"])
                  if kind == 1 and p["entity_values"][p["action_refs"][i, 0], 21] == .75)
    env.advance(f["revision"], [choice])
    after = positions(env.decision_batch())[0]
    assert sum(after.columns["entity_ids"][:, 0] == 2) == sum(initial.columns["entity_ids"][:, 0] == 2) + 2
    env = batch([spec([("Strike", 0)] * 5, potions=["PowerPotion"])])
    take_kind(env, 3)
    p = positions(env.decision_batch())[0].columns
    assert sum(p["action_ids"][:, 0] == 8) == 3
    refs = p["action_refs"][p["action_ids"][:, 0] == 8, 0]
    assert len(set(refs)) == 3 and (p["entity_ids"][refs, 0] == 9).all()
    take_kind(env, 8)
    p = positions(env.decision_batch())[0].columns
    assert sum(p["entity_ids"][:, 0] == 2) == 6
    assert any(v[8] == 1 and v[9] == 0 for kind, v in zip(p["entity_ids"][:, 0], p["entity_values"]) if kind == 2)
    env = batch([spec([("BurningPact", 0), ("Strike", 0), ("Defend", 0)])])
    f = env.decision_batch()
    p = positions(f)[0].columns
    evidence = json.loads(env.evidence_json())
    # The unique draw-two card's observed facts select this diagnostic input.
    i = next(i for i, (kind, _) in enumerate(p["action_ids"])
             if kind == 1 and p["entity_values"][p["action_refs"][i, 0], 9] == pytest.approx(1 / 3)
             and p["entity_values"][p["action_refs"][i, 0], 22] == pytest.approx(2 / 5))
    env.advance(f["revision"], [i])
    take_kind(env, 5)
    take_kind(env, 6)
    p = positions(env.decision_batch())[0].columns
    members = p["entity_values"][p["entity_ids"][:, 0] == 8]
    assert sum(members[:, 1]) == 1
    take_kind(env, 7)
    assert json.loads(env.evidence_json())["episodes"][0]["root"] == evidence["episodes"][0]["root"]


def test_bad_batch_and_bad_references_reject_before_any_game_input():
    env = batch([spec([("Strike", 0)] * 5), spec([("Defend", 0)] * 20)])
    frame = env.decision_batch()
    evidence = env.evidence_json()
    with pytest.raises(ValueError, match="schema"):
        positions(dict(frame, schema=3))
    with pytest.raises(ValueError, match="source"):
        env.advance(frame["revision"], [0, 999999])
    assert env.evidence_json() == evidence
    broken = copy.deepcopy(frame)
    broken["action_refs"][0, 0] = len(frame["entity_values"])
    with pytest.raises(ValueError, match="source position"):
        positions(broken)
    broken = copy.deepcopy(frame)
    broken["entity_splits"][1] = broken["entity_splits"][-1] + 1
    with pytest.raises(ValueError, match="descending"):
        positions(broken)
    env.advance(frame["revision"], [0, 0])
    with pytest.raises(ValueError, match="stale"):
        env.advance(frame["revision"], [0, 0])


@pytest.mark.parametrize("enemy,act,room", [
    ("ChosenAlone", 2, "MonsterRoom"),
    ("GremlinNob", 1, "MonsterRoomElite"),
    ("TheGuardian", 1, "MonsterRoomBoss"),
    ("TimeEater", 3, "MonsterRoomBoss"),
    ("TheHeart", 4, "MonsterRoomBoss"),
])
def test_encounter_pressure_can_be_observed_after_an_actual_enemy_turn(enemy, act, room):
    env = batch([spec([("Strike", 0)] * 20, enemy=enemy, act=act, room=room)])
    first = positions(env.decision_batch())[0]
    take_kind(env, 2)
    second = positions(env.decision_batch())[0]
    assert first.columns["entity_values"][0, 5] < second.columns["entity_values"][0, 5]
    assert (second.columns["entity_ids"][:, 0] == 3).any()
    if enemy == "TheHeart":
        assert sum(second.columns["entity_ids"][:, 0] == 2) == 25
    evidence = json.loads(env.evidence_json())["episodes"][0]
    assert evidence["construction"]["encounter"] == enemy
    assert len(evidence["inputs"]) == 1


def test_numerical_search_keeps_unknown_separate_from_actual_losses():
    env = batch([spec([("Defend", 0)] * 3, hp=1)], search_services=2)
    while True:
        f = env.decision_batch()
        if not len(f["slots"]):
            break
        p = positions(f)[0]
        env.guide(f["revision"], [[1.] * len(p.columns["actions"])], [None])
    assert json.loads(env.outcomes_json())[0]["kind"].startswith("unknown")


def test_declared_recipe_survives_relic_equipping_and_constructions_remain_explicit():
    recipe = [("Defend", 0)] * 5 + [("Strike", 1)] * 2 + [("AscendersBane", 0)]
    env = batch([spec(recipe, relics=["WarPaint", "MoltenEgg", "ToxicEgg",
        "Omamori", "CeramicFish", "DarkstonePeriapt", "DuVuDoll"])])
    p = positions(env.decision_batch())[0].columns
    cards = p["entity_values"][p["entity_ids"][:, 0] == 2]
    assert sorted(cards[:, 7].tolist()) == [0.] * 6 + [1.] * 2
    assert p["entity_values"][0, 1] == 1.  # no Darkstone obtain bonus
    assert p["entity_values"][0, 6] == pytest.approx(99 / 200)  # no Ceramic Fish obtain bonus
    for bad in [spec([]), spec(recipe, relics=["BurningBlood"]),
                spec([("Inflame", 0), ("Corruption", 0)], relics=["BottledTornado"]),
                spec([("Wound", 1)]), spec([("Doubt", 1)]), spec([("Strike", 2)]), spec(recipe, hp=0)]:
        with pytest.raises(ValueError):
            batch([bad])


def test_complete_outcomes_update_across_repacked_minibatches(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(53)
    model = EntityNetwork(hidden=16, layers=1).eval()
    env = batch([spec([("Strike", 0)] * 3), spec([("Defend", 0)] * 20, hp=12)], 256)
    cohort = collect(model, env, torch.Generator().manual_seed(17), tmp_path / "collect")
    assert len(cohort.outcomes) == 2
    assert all(o["kind"] in ("win", "loss") for o in cohort.outcomes)
    whole, chunks = copy.deepcopy(model), copy.deepcopy(model)
    update(whole, torch.optim.SGD(whole.parameters(), lr=.001), cohort, minibatch_size=len(cohort.records))
    update(chunks, torch.optim.SGD(chunks.parameters(), lr=.001), cohort, minibatch_size=7)
    for a, b in zip(whole.parameters(), chunks.parameters()):
        torch.testing.assert_close(a, b, atol=2e-6, rtol=2e-5)
    assert any(not torch.equal(a, b) for a, b in zip(model.parameters(), whole.parameters()))
    data = np.load(tmp_path / "collect/experience.npz")
    assert data["entity_values"].ndim == 2
    assert data["entity_splits"][-1] == len(data["entity_values"])
    invalid = Cohort(cohort.records, [dict(o, kind="unknown") for o in cohort.outcomes])
    with pytest.raises(ValueError, match="Unknown"):
        update(model, torch.optim.SGD(model.parameters(), lr=.001), invalid)
    corrupt = Cohort([replace(r, logp=r.logp + 1) for r in cohort.records], cohort.outcomes)
    saved = copy.deepcopy(model.state_dict())
    with pytest.raises(ValueError, match="probabilities"):
        update(model, torch.optim.SGD(model.parameters(), lr=.001), corrupt)
    for k, value in saved.items():
        torch.testing.assert_close(model.state_dict()[k], value)


def test_evaluation_preserves_unknown_without_using_it_for_learning(tmp_path):
    torch.set_num_threads(1)
    model = EntityNetwork(hidden=16, layers=1).eval()
    env = batch([spec([("Defend", 0)] * 5)], 1)
    cohort = collect(model, env, torch.Generator().manual_seed(8), tmp_path / "evaluation",
                     require_complete=False)
    assert cohort.outcomes[0]["kind"].startswith("unknown_")
    with pytest.raises(ValueError, match="Unknown"):
        update(model, torch.optim.SGD(model.parameters(), lr=.001), cohort)


def test_bounded_feedback_restores_optimizer_on_rejection_and_backtracks(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(73)
    model = EntityNetwork(hidden=16, layers=1).eval()
    optimizer = torch.optim.Adam(model.parameters(), lr=.003)
    cohort = collect(model, batch([spec([("Strike", 0)] * 3), spec([("Defend", 0)] * 6, hp=12)]),
                     CaseSampling([1, 2]), tmp_path / "first")
    # Populate moments before testing transactional rejection.
    update(model, optimizer, cohort, minibatch_size=8)
    cohort = collect(model, batch([spec([("Strike", 0)] * 3), spec([("Defend", 0)] * 6, hp=12)]),
                     CaseSampling([3, 4]), tmp_path / "second")
    weights, state = copy.deepcopy(model.state_dict()), copy.deepcopy(optimizer.state_dict())
    rejected = bounded_update(model, optimizer, cohort, max_policy_kl=1e-12,
                              max_backtracks=0, minibatch_size=8)
    assert not rejected["accepted"]
    for name, t in weights.items():
        assert torch.equal(model.state_dict()[name], t)
    restored = optimizer.state_dict()
    assert restored["param_groups"] == state["param_groups"]
    for key, entry in state["state"].items():
        for name, t in entry.items():
            torch.testing.assert_close(restored["state"][key][name], t, atol=0, rtol=0)
    # Set a limit below the known first step; an accepted smaller step must be
    # exactly the ordinary update at its reported learning rate.
    cap = rejected["attempts"][0]["mean_policy_kl"] * .3
    accepted = bounded_update(model, optimizer, cohort, max_policy_kl=cap,
                              max_backtracks=8, minibatch_size=8)
    assert accepted["accepted"] and len(accepted["attempts"]) > 1
    assert accepted["attempts"][-1]["mean_policy_kl"] <= cap
    assert optimizer.param_groups[0]["lr"] == .003
    direct = EntityNetwork(hidden=16, layers=1).eval()
    direct.load_state_dict(weights)
    direct_optimizer = torch.optim.Adam(direct.parameters())
    direct_optimizer.load_state_dict(state)
    direct_optimizer.param_groups[0]["lr"] *= accepted["attempts"][-1]["rate_scale"]
    update(direct, direct_optimizer, cohort, minibatch_size=8)
    for a, b in zip(model.parameters(), direct.parameters()):
        torch.testing.assert_close(a, b, atol=0, rtol=0)
