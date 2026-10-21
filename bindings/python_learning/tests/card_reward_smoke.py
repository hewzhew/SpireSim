"""Constructed rewards retain the full native surface and real acquisition."""
import json

from sts_learning_bridge import LearningBatchEnv, semantic_schema
from campfire_smoke import _candidate, _value


def recipe(*, relics=(), floor=1):
    return dict(loadout=dict(seed=17, ascension_level=20, current_hp=71, max_hp=80,
        boss="Hexaghost", cards=[{"id": "Strike"}, {"id": "Bash"}], relics=list(relics)),
        floor=floor, offers=[{"id": "PowerThrough"}, {"id": "Anger", "upgrades": 1},
                             {"id": "Flex"}])


def assert_card_reward_facts():
    schema = semantic_schema()
    env = LearningBatchEnv.constructed_card_rewards(json.dumps([recipe()]))
    batch = env.decision_batch(semantic=True)
    audit = json.loads(env.strategic_decision_audit_json(0))
    assert audit["decision_site"] == "card_reward"
    assert len(audit["candidates"]) == int(batch["candidate_counts"][0]) == 4
    assert [c["card"] for c in audit["candidates"] if c["kind"] == "take_card"] == ["PowerThrough", "Anger", "Flex"]
    context = dict(env.public_run_contexts())[0]
    assert (context.act, context.floor, context.hp, context.max_hp) == (1, 1, 71, 80)
    checkpoint = env.checkpoint_slot(0)
    target = next(i for i,c in enumerate(audit["candidates"]) if c.get("card") == "Anger")
    target_token = int(batch["semantic"]["candidate_token_indices"][target])
    anger_id = _value(batch["semantic"], schema, "categorical", target_token, "ActionCardId")
    env.choose(batch["decision_token_ids"], [target])
    env.step()
    after = env.decision_batch(semantic=True)["semantic"]
    owned = after["token"]["kind"] == schema["token_kind"]["Card"]
    anger = [t for t in range(len(owned)) if owned[t]
             and _value(after, schema, "categorical", t, "CardId") == anger_id]
    assert len(anger) == 1
    assert _value(after, schema, "scalar", anger[0], "CardUpgrades") == 1
    assert _value(after, schema, "scalar", anger[0], "CardPersistentDamageParameter") == 8
    env.restore_slot(0, checkpoint)
    batch = env.decision_batch(semantic=True)
    skip, _ = _candidate(batch, schema, "SkipCardReward")
    env.choose(batch["decision_token_ids"], [skip])
    env.step()
    after = env.decision_batch(semantic=True)["semantic"]
    assert sum(after["token"]["kind"] == schema["token_kind"]["Card"]) == 2

    # Duplicate offers are distinct legal occurrences; Singing Bowl adds an option.
    bowl = recipe(relics=["SingingBowl"], floor=14)
    bowl["offers"] = [{"id": "Anger"}, {"id": "Anger"}]
    env = LearningBatchEnv.constructed_card_rewards(json.dumps([bowl]))
    batch = env.decision_batch(semantic=True)
    assert int(batch["candidate_counts"][0]) == 4
    ordinal, _ = _candidate(batch, schema, "SingingBowl")
    env.choose(batch["decision_token_ids"], [ordinal])
    env.step()
    context = dict(env.public_run_contexts())[0]
    assert (context.max_hp, context.hp) == (82, 73)

    malformed = [[], [recipe(floor=0)], [recipe(floor=15)],
                 [dict(recipe(), offers=[])], [dict(recipe(), unexpected=True)],
                 [dict(recipe(), offers=[{"id": "Anger", "unexpected": True}])],
                 [dict(recipe(), offers=[{"id": "Hemokinesis", "upgrades": 2}])],
                 [dict(recipe(), offers=[{"id": "Strike"}])],
                 [dict(recipe(), offers=[{"id": "AscendersBane"}])],
                 [dict(recipe(), offers=[{"id": "Backflip"}])],
                 [dict(recipe(), offers=[{"id": "Anger"}]*9)], [recipe()]*129]
    for invalid in malformed:
        try:
            LearningBatchEnv.constructed_card_rewards(json.dumps(invalid))
        except ValueError:
            pass
        else:
            raise AssertionError(("invalid recipe accepted", invalid))


if __name__ == "__main__":
    assert_card_reward_facts()
