"""Partial relic rules remain requests and preserve public ownership."""

import json

import numpy as np
import pytest

bridge = pytest.importorskip("sts_learning_bridge")


def _facts(batch, schema, token, table):
    values = batch["semantic"][table]
    names = {value: name for name, value in schema[
        "categorical_field" if table == "categorical" else "scalar_field"].items()}
    return {names[int(field)]: value.item() for index, field, value in zip(
        values["token_indices"], values["field"], values["value"], strict=True)
        if index == token}


def _loadout(relics):
    return dict(seed=2026101151, ascension_level=20, current_hp=10, max_hp=78,
                boss="Hexaghost", cards=[dict(id="Bash")], relics=relics)


@pytest.mark.parametrize("blood,heal_request", [("BurningBlood", 6), ("BlackBlood", 12)])
def test_partial_relic_requests_do_not_mutate_hp_potions_or_current_powers(blood, heal_request):
    schema = bridge.semantic_schema()
    recipe = _loadout([blood, "Ginger", "Turnip", "WhiteBeastStatue", "MarkOfTheBloom"])
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([recipe]))
    batch = env.decision_batch(semantic=True)
    kinds = batch["semantic"]["token"]["kind"]
    tokens = np.flatnonzero(kinds == schema["token_kind"]["Relic"])
    assert len(tokens) == 5
    categories = [_facts(batch, schema, token, "categorical") for token in tokens]
    scalars = [_facts(batch, schema, token, "scalar") for token in tokens]
    immune = "RelicPreventPlayerDebuffBeforeArtifactPowerId"
    assert sum(immune in facts for facts in categories) == 2
    assert len({facts[immune] for facts in categories if immune in facts}) == 2
    assert [facts["RelicVictoryHealingRequest"] for facts in scalars
            if "RelicVictoryHealingRequest" in facts] == [heal_request]
    overrides = [facts for facts in scalars if
                 "RelicCombatPotionDropChanceOverridePercent" in facts]
    assert len(overrides) == 1
    assert (overrides[0]["RelicCombatPotionDropChanceOverridePercent"],
            overrides[0]["RelicCombatPotionDropBlockedAtRewardEntryCount"]) == (100, 4)
    assert all(batch["semantic"]["token"]["scope"][token] == 0 for token in tokens)
    run, = np.flatnonzero(kinds == schema["token_kind"]["Run"])
    assert _facts(batch, schema, run, "scalar")["CurrentHp"] == 10
    assert not np.any(kinds == schema["token_kind"]["CombatPower"])
    potions = np.flatnonzero(kinds == schema["token_kind"]["PotionSlot"])
    assert all(_facts(batch, schema, token, "categorical")["PotionOccupied"] == 0
               for token in potions)
    audit = json.loads(env.strategic_decision_audit_json(0))
    rest = next(i for i, action in enumerate(audit["candidates"])
                if action["kind"] == "rest")
    assert _facts(batch, schema, batch["semantic"]["candidate_token_indices"][rest],
                  "scalar")["ActionRestHpGain"] == 0


def test_unreviewed_relics_do_not_receive_false_zero_effects_and_order_is_irrelevant():
    schema = bridge.semantic_schema()
    relics = ["Ginger", "WhiteBeastStatue", "MeatOnTheBone"]
    rows = []
    for order in [relics, list(reversed(relics))]:
        recipe = dict(loadout=_loadout(order), floor=3, offers=[dict(id="Shockwave")])
        env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps([recipe]))
        batch = env.decision_batch(semantic=True)
        tokens = np.flatnonzero(batch["semantic"]["token"]["kind"] ==
                               schema["token_kind"]["Relic"])
        rows.append([(_facts(batch, schema, token, "categorical"),
                      _facts(batch, schema, token, "scalar")) for token in tokens])
    assert rows[0] == rows[1]
    assert sum("RelicVictoryHealingRequest" in facts for _, facts in rows[0]) == 0
    assert sum(any(name.startswith("RelicCombatPotionDrop") for name in facts)
               for _, facts in rows[0]) == 1
