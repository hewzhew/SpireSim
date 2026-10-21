"""Native campfire facts checked against actual engine execution."""
import json

import numpy as np
from sts_learning_bridge import LearningBatchEnv, semantic_schema


def recipe(hp=10, *, cards=None, relics=None):
    return dict(seed=17, ascension_level=20, current_hp=hp, max_hp=78,
                boss="Hexaghost", cards=cards or [{"id": "Bash"}], relics=relics or [])


def _value(semantic, schema, table, token, name):
    fields = schema["scalar_field" if table == "scalar" else "categorical_field"]
    values = semantic[table]
    indices = np.flatnonzero((values["token_indices"] == token) & (values["field"] == fields[name]))
    assert indices.size == 1, (table, name, token, indices)
    return values["value"][indices[0]].item()


def _edge(semantic, schema, source, name):
    relation = semantic["relation"]
    indices = np.flatnonzero((relation["source_token_indices"] == source)
                            & (relation["relation"] == schema["relation_kind"][name]))
    assert indices.size == 1, (name, indices)
    return int(relation["target_token_indices"][indices[0]])


def _candidate(batch, schema, kind, row=0):
    start, end = batch["candidate_row_splits"][row:row+2]
    for ordinal, token in enumerate(batch["semantic"]["candidate_token_indices"][int(start):int(end)]):
        if _value(batch["semantic"], schema, "categorical", token, "ActionKind") == schema["action_kind"][kind]:
            return ordinal, int(token)
    raise AssertionError(("missing candidate", kind))


def assert_campfire_facts():
    schema = semantic_schema()
    assert schema["version"] >= 23
    cases = [(78, [], 0), (76, [], 2), (1, [], 23), (1, ["RegalPillow"], 38),
             (70, ["RegalPillow"], 8), (1, ["MarkOfTheBloom"], 0),
             (1, ["RegalPillow", "MarkOfTheBloom"], 0)]
    for hp, relics, expected in cases:
        env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(hp, relics=relics)]))
        batch = env.decision_batch(semantic=True)
        ordinal, token = _candidate(batch, schema, "Rest")
        assert _value(batch["semantic"], schema, "scalar", token, "ActionRestHpGain") == expected
        env.choose(batch["decision_token_ids"], [ordinal])
        env.step()
        context = dict(env.public_run_contexts())[0]
        assert context.hp == hp + expected

    upgrades = [
        ({"id": "Offering"}, "scalar", "CardPersistentEnergyCost", 0, 0),
        ({"id": "Offering"}, "scalar", "CardPersistentMagicParameter", 3, 5),
        ({"id": "Corruption"}, "scalar", "CardPersistentEnergyCost", 3, 2),
        ({"id": "DarkEmbrace"}, "scalar", "CardPersistentEnergyCost", 2, 1),
        ({"id": "PommelStrike"}, "scalar", "CardPersistentMagicParameter", 1, 2),
        ({"id": "LimitBreak"}, "categorical", "CardExhaust", 1, 0),
        ({"id": "Armaments"}, "categorical", "CardCombatUpgradeScope", 0, 1),
        ({"id": "RitualDagger", "misc_value": 40}, "scalar", "CardPersistentDamageParameter", 40, 40),
        ({"id": "Strike", "base_damage_override": 20}, "scalar", "CardPersistentDamageParameter", 20, 23),
        ({"id": "Defend", "base_block_override": 12}, "scalar", "CardPersistentBlockParameter", 12, 15),
        ({"id": "Bash", "cost_modifier": -1}, "scalar", "CardPersistentEnergyCost", 1, 1),
        ({"id": "SearingBlow", "upgrades": 2}, "scalar", "CardPersistentDamageParameter", 21, 27),
    ]
    for card, table, field, before, after in upgrades:
        env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(cards=[card])]))
        batch = env.decision_batch(semantic=True)
        ordinal, token = _candidate(batch, schema, "Smith")
        current = _edge(batch["semantic"], schema, token, "CandidateTargets")
        upgraded = _edge(batch["semantic"], schema, token, "CandidateUpgradesTo")
        assert batch["semantic"]["token"]["kind"][upgraded] == schema["token_kind"]["UpgradedCard"]
        assert _value(batch["semantic"], schema, table, current, field) == before, (card, field)
        projected = _value(batch["semantic"], schema, table, upgraded, field)
        assert projected == after, (card, field, projected, after)
        env.choose(batch["decision_token_ids"], [ordinal])
        env.step()
        actual = env.decision_batch(semantic=True)["semantic"]
        owned = np.flatnonzero(actual["token"]["kind"] == schema["token_kind"]["Card"])
        assert owned.size == 1
        assert _value(actual, schema, table, owned[0], field) == projected

    env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(cards=[{"id": "TrueGrit"}])]))
    batch = env.decision_batch(semantic=True)
    _, token = _candidate(batch, schema, "Smith")
    upgraded = _edge(batch["semantic"], schema, token, "CandidateUpgradesTo")
    roles = batch["semantic"]["categorical"]
    current = _edge(batch["semantic"], schema, token, "CandidateTargets")
    prior = roles["value"][(roles["token_indices"] == current)
                          & (roles["field"] == schema["categorical_field"]["CardMechanicRole"])]
    assert schema["card_mechanic_role"]["SelectFromHand"] not in prior
    chosen = roles["value"][(roles["token_indices"] == upgraded)
                            & (roles["field"] == schema["categorical_field"]["CardMechanicRole"])]
    assert schema["card_mechanic_role"]["SelectFromHand"] in chosen

    # A zero healing prefix is not the complete Rest effect with Dream Catcher.
    env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(78, relics=["DreamCatcher"])]))
    batch = env.decision_batch(semantic=True)
    ordinal, token = _candidate(batch, schema, "Rest")
    assert _value(batch["semantic"], schema, "scalar", token, "ActionRestHpGain") == 0
    env.choose(batch["decision_token_ids"], [ordinal])
    env.step()
    audit = json.loads(env.strategic_decision_audit_json(0))
    assert audit["decision_site"] == "reward"

    for relic, forbidden in [("CoffeeDripper", "Rest"), ("FusionHammer", "Smith")]:
        env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(relics=[relic])]))
        batch = env.decision_batch(semantic=True)
        try:
            _candidate(batch, schema, forbidden)
        except AssertionError:
            pass
        else:
            raise AssertionError(("forbidden action", relic, forbidden))
    for malformed in [[], [dict(recipe(), current_hp=79)], [dict(recipe(), unknown=1)],
                      [dict(recipe(), gold=-1)], [dict(recipe(), gold=1000001)],
                      [recipe(cards=[{"id": "Bash", "upgrades": 2}])], [dict(recipe(), boss="TheHeart")]]:
        try:
            LearningBatchEnv.constructed_campfires(json.dumps(malformed))
        except ValueError:
            pass
        else:
            raise AssertionError(("malformed recipe accepted", malformed))
    env = LearningBatchEnv.constructed_campfires(json.dumps([dict(recipe(), gold=217)]))
    assert dict(env.public_run_contexts())[0].gold == 217
    batch = env.decision_batch(semantic=True)
    run_tokens = np.flatnonzero(batch["semantic"]["token"]["kind"] == schema["token_kind"]["Run"])
    assert run_tokens.size == 1
    assert _value(batch["semantic"], schema, "scalar", run_tokens[0], "Gold") == 217
    ordinal, _ = _candidate(batch, schema, "Smith")
    env.choose(batch["decision_token_ids"], [ordinal])
    env.step()
    assert dict(env.public_run_contexts())[0].gold == 217
    print("campfire_facts_smoke healing=7 upgrade_execution=12 gates=2 dream_catcher=passed")


if __name__ == "__main__":
    assert_campfire_facts()
