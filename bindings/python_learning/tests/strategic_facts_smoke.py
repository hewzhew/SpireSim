"""Public deck dependencies and reward collections through the live bridge."""
import json

import numpy as np
from sts_learning_bridge import LearningBatchEnv, semantic_schema

from campfire_smoke import recipe, _candidate, _edge, _value


def _optional_value(semantic, schema, table, token, name):
    fields = schema["scalar_field" if table == "scalar" else "categorical_field"]
    values = semantic[table]
    indices = np.flatnonzero((values["token_indices"] == token)
                            & (values["field"] == fields[name]))
    assert indices.size <= 1, (name, indices)
    return None if not indices.size else values["value"][indices[0]].item()


def _owned(semantic, schema):
    return np.flatnonzero(semantic["token"]["kind"] == schema["token_kind"]["Card"])


def assert_strategic_facts():
    schema = semantic_schema()
    assert schema["version"] == 32
    for card, delivery, before, after in (
        ("Inflame", "OnPlay", 2, 3),
        ("SpotWeakness", "IfTargetIntentBaseDamageNonnegativeAtResolution", 3, 4),
        ("Flex", "OnPlayWithSeparateLoseStrength", 2, 4),
        ("DemonForm", "EachPlayerTurnStartPostDraw", 2, 3),
    ):
        env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(cards=[{"id": card}])]))
        batch = env.decision_batch(semantic=True)
        ordinal, token = _candidate(batch, schema, "Smith")
        current = _edge(batch["semantic"], schema, token, "CandidateTargets")
        upgraded = _edge(batch["semantic"], schema, token, "CandidateUpgradesTo")
        for source, amount in ((current, before), (upgraded, after)):
            assert _value(batch["semantic"], schema, "categorical", source, "CardStrengthGainDelivery") == schema["card_strength_gain_delivery"][delivery]
            assert _value(batch["semantic"], schema, "scalar", source, "CardStrengthGainParameter") == amount
        env.choose(batch["decision_token_ids"], [ordinal])
        env.step()
        actual = env.decision_batch(semantic=True)["semantic"]
        assert _value(actual, schema, "scalar", _owned(actual, schema)[0], "CardStrengthGainParameter") == after

    for card, hits, targets, count in (
        ("HeavyBlade", "Fixed", "SelectedEnemy", 1),
        ("TwinStrike", "Fixed", "SelectedEnemy", 2),
        ("SwordBoomerang", "Fixed", "RandomEnemyPerHit", 3),
        ("Whirlwind", "PerEffectiveX", "EachEnemy", None),
        ("FiendFire", "PerHandCardAtResolutionBeforeExhaust", "SelectedEnemy", None),
    ):
        env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(cards=[{"id": card}])]))
        batch = env.decision_batch(semantic=True)
        owned = _owned(batch["semantic"], schema)[0]
        assert _value(batch["semantic"], schema, "categorical", owned, "CardAttackHitsKind") == schema["card_attack_hits"][hits]
        assert _value(batch["semantic"], schema, "categorical", owned, "CardAttackTargetsKind") == schema["card_attack_targets"][targets]
        assert _optional_value(batch["semantic"], schema, "scalar", owned, "CardAttackFixedHits") == count
        if count is None:
            assert _optional_value(batch["semantic"], schema, "scalar", owned, "CardUnconditionalBaseDamage") is None
        _, token = _candidate(batch, schema, "Smith")
        after = _edge(batch["semantic"], schema, token, "CandidateUpgradesTo")
        if card == "HeavyBlade":
            assert _value(batch["semantic"], schema, "scalar", owned, "CardAttackStrengthMultiplier") == 3
            assert _value(batch["semantic"], schema, "scalar", after, "CardAttackStrengthMultiplier") == 5
            assert _value(batch["semantic"], schema, "scalar", after, "CardPersistentDamageParameter") == 14
        if card == "SwordBoomerang":
            assert _value(batch["semantic"], schema, "scalar", after, "CardAttackFixedHits") == 4

    env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(cards=[{"id": "Clash"}, {"id": "BodySlam"}, {"id": "Wish"}, {"id": "LimitBreak"}, {"id": "Defend"}])]))
    semantic = env.decision_batch(semantic=True)["semantic"]
    clash, body_slam, wish, limit_break, defend = _owned(semantic, schema)
    assert _value(semantic, schema, "categorical", clash, "CardAttackPlayCondition") == schema["card_attack_play_condition"]["HandContainsOnlyAttacks"]
    assert _optional_value(semantic, schema, "scalar", clash, "CardUnconditionalBaseDamage") is None
    assert _value(semantic, schema, "categorical", body_slam, "CardAttackAmountKind") == schema["card_attack_amount"]["PlayerBlock"]
    assert _value(semantic, schema, "categorical", wish, "CardDamageDeliveryKind") == schema["card_damage_delivery"]["StrengthIfChosen"]
    assert _optional_value(semantic, schema, "scalar", wish, "CardAttackStrengthMultiplier") is None
    assert _value(semantic, schema, "categorical", limit_break, "CardStrengthConversion") == schema["card_strength_conversion"]["AmplifyCurrentStrength"]
    assert _optional_value(semantic, schema, "scalar", limit_break, "CardStrengthGainParameter") is None
    assert _optional_value(semantic, schema, "categorical", defend, "CardStrengthGainDelivery") is None

    cards = [{"id": "Strike"}, {"id": "Strike", "cost_modifier": -4},
             {"id": "Bash", "cost_modifier": -1}, {"id": "Offering"},
             {"id": "Whirlwind"}, {"id": "DemonForm"}, {"id": "DarkEmbrace"},
             {"id": "AscendersBane"}, {"id": "PowerThrough"}, {"id": "Burn"}]
    expected = dict(DeckSize=10, DeckAttackCount=4, DeckSkillCount=2,
                    DeckPowerCount=2, DeckStatusCount=1, DeckCurseCount=1,
                    DeckCostZeroCount=2, DeckCostOneCount=3, DeckCostTwoCount=1,
                    DeckCostThreeOrMoreCount=1, DeckCostXCount=1, DeckCostUnplayableCount=2)
    for order in (cards, list(reversed(cards))):
        env = LearningBatchEnv.constructed_campfires(json.dumps([recipe(cards=order)]))
        batch = env.decision_batch(semantic=True)
        run = int(np.flatnonzero(batch["semantic"]["token"]["kind"] == schema["token_kind"]["Run"])[0])
        assert {name: _value(batch["semantic"], schema, "scalar", run, name) for name in expected} == expected
        # Candidate previews and offered cards are excluded from deck counts.
        audit = json.loads(env.strategic_decision_audit_json(0))
        ordinal = next(i for i, c in enumerate(audit["candidates"]) if c["kind"] == "smith" and c["card"] == "DarkEmbrace")
        env.choose(batch["decision_token_ids"], [ordinal])
        env.step()
        actual = env.decision_batch(semantic=True)["semantic"]
        run = int(np.flatnonzero(actual["token"]["kind"] == schema["token_kind"]["Run"])[0])
        assert _value(actual, schema, "scalar", run, "DeckCostTwoCount") == 0
        assert _value(actual, schema, "scalar", run, "DeckCostOneCount") == 4
        assert _value(actual, schema, "scalar", run, "DeckSize") == 10

    # Real fresh-run reward, no checkpoint or hidden-state search. Opening is
    # reversible and must preserve the whole visible collection in the model.
    env = LearningBatchEnv([0], 0)
    for _ in range(256):
        batch = env.decision_batch(semantic=True, legality_bootstrap=True)
        context = dict(env.public_run_contexts())[0]
        if context.strategic_context_kind == schema["context_kind"]["Reward"]:
            audit = json.loads(env.strategic_decision_audit_json(0))
            open_ordinals = [i for i, c in enumerate(audit["candidates"]) if c["kind"] == "open_card_reward"]
            if open_ordinals:
                before = _reward_collection(batch["semantic"], schema)
                if len(before) >= 2:
                    env.choose(batch["decision_token_ids"], [open_ordinals[0]])
                    env.step()
                    opened = env.decision_batch(semantic=True)
                    assert _reward_collection(opened["semantic"], schema) == before
                    return
        ordinals = batch["legality_bootstrap_ordinals"].tolist()
        if context.strategic_context_kind == schema["context_kind"]["CardReward"]:
            audit = json.loads(env.strategic_decision_audit_json(0))
            ordinals = [next(i for i, c in enumerate(audit["candidates"]) if c["kind"] == "take_card")]
        env.choose(batch["decision_token_ids"], ordinals)
        if env.ready:
            env.step()
        if env.terminal_count:
            break
    raise AssertionError("bounded fresh prefix did not reach a card-containing reward")


def _reward_collection(semantic, schema):
    result = []
    for token in np.flatnonzero(semantic["token"]["kind"] == schema["token_kind"]["PendingReward"]):
        index = _value(semantic, schema, "scalar", token, "PendingRewardItemIndex")
        fields = semantic["categorical"]
        values = [(int(f), int(v)) for f, v in zip(fields["field"][fields["token_indices"] == token], fields["value"][fields["token_indices"] == token])]
        scalars = semantic["scalar"]
        numbers = [(int(f), float(v)) for f, v in zip(scalars["field"][scalars["token_indices"] == token], scalars["value"][scalars["token_indices"] == token])]
        relation = semantic["relation"]
        offers = relation["target_token_indices"][(relation["source_token_indices"] == token) & (relation["relation"] == schema["relation_kind"]["RewardHasOfferedCard"])]
        cards = [(_value(semantic, schema, "categorical", t, "CardId"), _value(semantic, schema, "scalar", t, "CardUpgrades")) for t in offers]
        result.append((index, values, numbers, cards))
    return result


if __name__ == "__main__":
    assert_strategic_facts()
