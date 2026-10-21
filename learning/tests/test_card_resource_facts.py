"""Native public facts retain requests, timing and owned/preview boundaries."""

import json

import numpy as np
import pytest

bridge = pytest.importorskip("sts_learning_bridge")


def _facts(batch, schema, token, table_name):
    table = batch["semantic"][table_name]
    names = {value: name for name, value in schema[
        "scalar_field" if table_name == "scalar" else "categorical_field"].items()}
    return {names[int(field)]: value.item() for index, field, value in zip(
        table["token_indices"], table["field"], table["value"], strict=True) if index == token}


def _tokens(batch, schema, kind):
    return np.flatnonzero(batch["semantic"]["token"]["kind"] == schema["token_kind"][kind])


def _recipe(cards):
    return dict(seed=2026101131, ascension_level=20, current_hp=6, max_hp=78,
                boss="TheGuardian", relics=["BurningBlood", "MarkOfTheBloom"], cards=cards)


def test_upgrade_counts_use_owned_eligibility_and_previews_keep_resource_requests():
    schema = bridge.semantic_schema()
    cards = [dict(id="Strike"), dict(id="Strike", upgrades=1), dict(id="Defend"),
             dict(id="Bash", upgrades=1), dict(id="AscendersBane"), dict(id="Wound"),
             dict(id="SearingBlow", upgrades=4), dict(id="Armaments"),
             dict(id="Offering"), dict(id="Offering", upgrades=1)]
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([_recipe(cards)]))
    batch = env.decision_batch(semantic=True)
    run, = _tokens(batch, schema, "Run")
    run_facts = _facts(batch, schema, run, "scalar")
    assert (run_facts["DeckSize"], run_facts["DeckUpgradeableCount"],
            run_facts["DeckUpgradeableBasicCount"]) == (10, 5, 2)
    owned = _tokens(batch, schema, "Card")
    assert [_facts(batch, schema, token, "categorical")["CardCanUpgrade"]
            for token in owned] == [1, 0, 1, 0, 0, 0, 1, 1, 1, 0]
    offering_id = _facts(batch, schema, owned[8], "categorical")["CardId"]
    armaments_id = _facts(batch, schema, owned[7], "categorical")["CardId"]
    offerings = [token for token in owned if
                 _facts(batch, schema, token, "categorical")["CardId"] == offering_id]
    assert [_facts(batch, schema, token, "scalar")["CardDrawOnPlayRequest"]
            for token in offerings] == [3, 5]
    previews = _tokens(batch, schema, "UpgradedCard")
    offering_preview = next(token for token in previews if
                            _facts(batch, schema, token, "categorical")["CardId"] ==
                            offering_id)
    for token in [*offerings, offering_preview]:
        scalars = _facts(batch, schema, token, "scalar")
        categories = _facts(batch, schema, token, "categorical")
        assert (scalars["CardSelfHpLossOnPlay"], scalars["CardEnergyGainRequest"]) == (6, 2)
        assert categories["CardEnergyGainDelivery"] == schema["card_energy_gain_delivery"]["OnPlay"]
        assert (categories["CardSelfHpLossBypassesBlock"],
                categories["CardSelfHpLossBypassesIntangible"],
                categories["CardSelfHpLossTriggersRupture"]) == (1, 0, 1)
    assert _facts(batch, schema, offering_preview, "scalar")["CardDrawOnPlayRequest"] == 5
    assert batch["semantic"]["token"]["scope"][offering_preview] > 0
    armaments = next(token for token in owned if
                     _facts(batch, schema, token, "categorical")["CardId"] ==
                     armaments_id)
    armaments_preview = next(token for token in previews if
                             _facts(batch, schema, token, "categorical")["CardId"] ==
                             armaments_id)
    assert _facts(batch, schema, armaments, "categorical")["CardCombatUpgradeScope"] == 0
    assert _facts(batch, schema, armaments_preview, "categorical")["CardCombatUpgradeScope"] == 1
    audit = json.loads(env.strategic_decision_audit_json(0))
    ordinal = next(i for i, action in enumerate(audit["candidates"])
                   if action["kind"] == "smith" and action["card"] == "Armaments")
    env.choose(batch["decision_token_ids"], np.array([ordinal], dtype=np.uint64))
    env.step()
    after = env.decision_batch(semantic=True)
    after_run, = _tokens(after, schema, "Run")
    facts = _facts(after, schema, after_run, "scalar")
    assert (facts["DeckSize"], facts["DeckUpgradeableCount"],
            facts["DeckUpgradeableBasicCount"]) == (10, 4, 2)


def test_energy_timing_conditional_draw_and_unreviewed_requests_remain_distinct():
    schema = bridge.semantic_schema()
    cards = [dict(id=card) for card in ["Sentinel", "Berserk", "Outmaneuver", "Dropkick",
                                      "DoubleEnergy", "Impatience", "Hemokinesis"]]
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([_recipe(cards)]))
    batch = env.decision_batch(semantic=True)
    by_id = {card["id"]: token for card, token in zip(
        cards, _tokens(batch, schema, "Card"), strict=True)}
    for card, amount, delivery in [("Sentinel", 2, "OnExhaust"),
                                    ("Berserk", 1, "EachPlayerTurnStart"),
                                    ("Outmaneuver", 2, "NextPlayerEnergyRecharge"),
                                    ("Dropkick", 1, "IfTargetVulnerableAtResolution")]:
        token = by_id[card]
        assert _facts(batch, schema, token, "scalar")["CardEnergyGainRequest"] == amount
        assert _facts(batch, schema, token, "categorical")["CardEnergyGainDelivery"] == \
            schema["card_energy_gain_delivery"][delivery]
    assert "CardEnergyGainRequest" not in _facts(
        batch, schema, by_id["DoubleEnergy"], "scalar")
    for card in ["Impatience", "Dropkick"]:
        assert "CardDrawOnPlayRequest" not in _facts(
            batch, schema, by_id[card], "scalar")
    assert _facts(batch, schema, by_id["Hemokinesis"], "scalar")[
        "CardSelfHpLossOnPlay"] == 2


def test_offered_upgradeable_cards_do_not_inflate_owned_deck_counts():
    schema = bridge.semantic_schema()
    recipe = dict(loadout=_recipe([dict(id="Strike", upgrades=1), dict(id="Defend", upgrades=1)]),
                  floor=1, offers=[dict(id=card) for card in ["Offering", "Armaments", "SearingBlow"]])
    env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps([recipe]))
    batch = env.decision_batch(semantic=True)
    run, = _tokens(batch, schema, "Run")
    facts = _facts(batch, schema, run, "scalar")
    assert (facts["DeckSize"], facts["DeckUpgradeableCount"],
            facts["DeckUpgradeableBasicCount"]) == (2, 0, 0)
    candidates = batch["semantic"]["candidate_token_indices"]
    offered = [token for token in candidates if
               "ActionCardId" in _facts(batch, schema, token, "categorical")]
    assert len(offered) == 3
    assert all(_facts(batch, schema, token, "categorical")["CardCanUpgrade"] == 1
               for token in offered)


def test_deployed_exhaust_requests_distinguish_draw_block_and_immediate_effects():
    schema = bridge.semantic_schema()
    cards = [dict(id="DarkEmbrace"), dict(id="DarkEmbrace", upgrades=1),
             dict(id="FeelNoPain"), dict(id="FeelNoPain", upgrades=1),
             dict(id="Warcry"), dict(id="Inflame", upgrades=1)]
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([_recipe(cards)]))
    batch = env.decision_batch(semantic=True)
    owned = _tokens(batch, schema, "Card")
    for token, field, amount in zip(owned[:4], [
            "CardDeployedExhaustDrawPerEventRequest"] * 2 + [
            "CardDeployedExhaustBlockPerEventRequest"] * 2, [1, 1, 3, 4], strict=True):
        facts = _facts(batch, schema, token, "scalar")
        assert facts[field] == amount
        assert "CardDrawOnPlayRequest" not in facts
        assert "CardUnconditionalBaseBlock" not in facts or facts["CardUnconditionalBaseBlock"] == 0
        other = ("CardDeployedExhaustBlockPerEventRequest" if "Draw" in field else
                 "CardDeployedExhaustDrawPerEventRequest")
        assert other not in facts
    for token in owned[4:]:
        facts = _facts(batch, schema, token, "scalar")
        assert "CardDeployedExhaustDrawPerEventRequest" not in facts
        assert "CardDeployedExhaustBlockPerEventRequest" not in facts
    previews = _tokens(batch, schema, "UpgradedCard")
    deployed = [_facts(batch, schema, token, "scalar") for token in previews]
    assert sorted(facts["CardDeployedExhaustDrawPerEventRequest"] for facts in deployed
                  if "CardDeployedExhaustDrawPerEventRequest" in facts) == [1]
    assert sorted(facts["CardDeployedExhaustBlockPerEventRequest"] for facts in deployed
                  if "CardDeployedExhaustBlockPerEventRequest" in facts) == [4]


def test_reviewed_source_counts_keep_timing_and_inventory_ownership():
    schema = bridge.semantic_schema()
    cards = [dict(id=name) for name in ["Inflame", "Inflame", "SpotWeakness", "Flex",
        "DemonForm", "DarkEmbrace", "FeelNoPain", "Corruption", "LimitBreak",
        "SwordBoomerang", "Warcry", "AscendersBane"]]
    offers = [dict(id=name) for name in ["Inflame", "DarkEmbrace", "FeelNoPain"]]
    recipes = [dict(loadout=_recipe(deck), floor=1, offers=offers) for deck in [cards, cards[::-1]]]
    env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps(recipes))
    batch = env.decision_batch(semantic=True)
    fields = ["DeckStrengthOnPlayCardCount", "DeckStrengthConditionalCardCount",
              "DeckTemporaryStrengthCardCount", "DeckStrengthPerTurnCardCount",
              "DeckExhaustDrawPowerCardCount", "DeckExhaustBlockPowerCardCount"]
    for run in _tokens(batch, schema, "Run"):
        facts = _facts(batch, schema, run, "scalar")
        assert [facts[field] for field in fields] == [2, 1, 1, 1, 1, 1]
    audit = json.loads(env.strategic_decision_audit_json(0))
    ordinal = next(i for i, c in enumerate(audit["candidates"]) if c.get("card") == "Inflame")
    env.choose(batch["decision_token_ids"], np.array([ordinal, ordinal], dtype=np.uint64))
    env.step()
    after = env.decision_batch(semantic=True)
    for run in _tokens(after, schema, "Run"):
        facts = _facts(after, schema, run, "scalar")
        assert [facts[field] for field in fields] == [3, 1, 1, 1, 1, 1]


def test_upgrade_preview_changes_request_without_adding_an_owned_source():
    schema = bridge.semantic_schema()
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([_recipe([
        dict(id="FeelNoPain"), dict(id="Inflame"), dict(id="DarkEmbrace")])]))
    batch = env.decision_batch(semantic=True)
    run, = _tokens(batch, schema, "Run")
    facts = _facts(batch, schema, run, "scalar")
    assert (facts["DeckStrengthOnPlayCardCount"], facts["DeckExhaustDrawPowerCardCount"],
            facts["DeckExhaustBlockPowerCardCount"]) == (1, 1, 1)
    assert facts["DeckStrengthConditionalCardCount"] == 0
    assert facts["DeckTemporaryStrengthCardCount"] == 0
    assert facts["DeckStrengthPerTurnCardCount"] == 0


@pytest.mark.parametrize("site", ["card_reward", "campfire"])
def test_constructed_inventory_remains_public_and_survives_card_selection(site):
    schema = bridge.semantic_schema()
    loadout = _recipe([dict(id="Strike"), dict(id="Defend"), dict(id="Bash")])
    loadout.update(relics=["BurningBlood", "PotionBelt"],
                   potions=[None, "EnergyPotion", "FirePotion"])
    if site == "card_reward":
        recipe = dict(loadout=loadout, floor=1, offers=[dict(id="Anger"), dict(id="Rage")])
        env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps([recipe]))
        action, card = "take_card", "Anger"
    else:
        env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([loadout]))
        action, card = "smith", "Bash"

    def inventory(batch):
        tokens = _tokens(batch, schema, "PotionSlot")
        assert len(tokens) == 4
        assert np.all(batch["semantic"]["token"]["scope"][tokens] == 0)
        categories = [_facts(batch, schema, token, "categorical") for token in tokens]
        slots = [_facts(batch, schema, token, "scalar")["PotionSlot"] for token in tokens]
        assert slots == [0, 1, 2, 3]
        assert [facts["PotionOccupied"] for facts in categories] == [0, 1, 1, 0]
        assert [facts.get("PotionId") for facts in categories] == [None, 7, 0, None]
        return categories

    batch = env.decision_batch(semantic=True)
    before = inventory(batch)
    audit = json.loads(env.strategic_decision_audit_json(0))
    ordinal = next(index for index, candidate in enumerate(audit["candidates"])
                   if candidate["kind"] == action and candidate.get("card") == card)
    env.choose(batch["decision_token_ids"], np.array([ordinal], dtype=np.uint64))
    env.step()
    assert inventory(env.decision_batch(semantic=True)) == before


def test_warcry_keeps_gross_draw_separate_from_hand_topdeck_request():
    schema = bridge.semantic_schema()
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([_recipe([
        dict(id="Warcry"), dict(id="Warcry", upgrades=1), dict(id="ThinkingAhead")])]))
    batch = env.decision_batch(semantic=True)
    owned = _tokens(batch, schema, "Card")
    for token, draw in zip(owned[:2], [1, 2], strict=True):
        scalars = _facts(batch, schema, token, "scalar")
        assert scalars["CardDrawOnPlayRequest"] == draw
        assert scalars["CardHandTopdeckAfterDrawRequest"] == 1
    # Thinking Ahead's use-time gate differs; it is still unreviewed here.
    assert "CardHandTopdeckAfterDrawRequest" not in _facts(batch, schema, owned[2], "scalar")
    preview, = [token for token in _tokens(batch, schema, "UpgradedCard") if
        "CardHandTopdeckAfterDrawRequest" in _facts(batch, schema, token, "scalar")]
    assert _facts(batch, schema, preview, "scalar")["CardDrawOnPlayRequest"] == 2
    assert _facts(batch, schema, preview, "scalar")["CardHandTopdeckAfterDrawRequest"] == 1
    assert batch["semantic"]["token"]["scope"][preview] > 0


@pytest.mark.parametrize("source,status,amount,destination,timing,ethereal", [
    ("WildStrike", "Wound", 1, "DrawPileRandom", "AfterAttack", 0),
    ("RecklessCharge", "Dazed", 1, "DrawPileRandom", "AfterAttack", 1),
    ("PowerThrough", "Wound", 2, "Hand", "BeforeBlock", 0),
    ("Immolate", "Burn", 1, "DiscardPile", "AfterAttack", 0),
])
def test_status_creation_keeps_amount_zone_order_and_lifecycle_in_owned_and_preview(
        source, status, amount, destination, timing, ethereal):
    schema = bridge.semantic_schema()
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([_recipe([
        dict(id=source), dict(id=source, upgrades=1), dict(id=status), dict(id="Evolve")])]))
    batch = env.decision_batch(semantic=True)
    owned = _tokens(batch, schema, "Card")
    status_id = _facts(batch, schema, owned[2], "categorical")["CardId"]
    preview, = [token for token in _tokens(batch, schema, "UpgradedCard") if
        "CardStatusCreationRequest" in _facts(batch, schema, token, "scalar")]
    for token in [*owned[:2], preview]:
        scalars = _facts(batch, schema, token, "scalar")
        categories = _facts(batch, schema, token, "categorical")
        assert scalars["CardStatusCreationRequest"] == amount
        assert categories["CardStatusCreationCardId"] == status_id
        assert categories["CardStatusCreationDestination"] == \
            schema["card_status_creation_destination"][destination]
        assert categories["CardStatusCreationTiming"] == schema["card_status_creation_timing"][timing]
        assert categories["CardStatusCreationUpgraded"] == 0
        assert categories["CardStatusCreationEthereal"] == ethereal
    assert batch["semantic"]["token"]["scope"][preview] > 0
    for token in owned[2:]:
        assert "CardStatusCreationRequest" not in _facts(batch, schema, token, "scalar")
    run, = _tokens(batch, schema, "Run")
    assert _facts(batch, schema, run, "scalar")["DeckStatusCount"] == 1


def test_strike_tag_count_tracks_owned_instances_without_offers_or_generated_status():
    schema = bridge.semantic_schema()
    cards = [dict(id=id) for id in ["Strike", "Strike", "PommelStrike", "TwinStrike",
        "WildStrike", "PerfectedStrike", "Bash", "Defend", "AscendersBane"]]
    recipes = [dict(loadout=_recipe(deck), floor=1,
        offers=[dict(id="PerfectedStrike"), dict(id="WildStrike"), dict(id="Warcry")])
        for deck in [cards, cards[::-1]]]
    env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps(recipes))
    batch = env.decision_batch(semantic=True)
    for run in _tokens(batch, schema, "Run"):
        scalars = _facts(batch, schema, run, "scalar")
        assert (scalars["DeckSize"], scalars["DeckStrikeTagCount"],
                scalars["DeckStatusCount"]) == (9, 6, 0)
    ordinals = []
    for slot in [0, 1]:
        audit = json.loads(env.strategic_decision_audit_json(slot))
        ordinals.append(next(index for index, candidate in enumerate(audit["candidates"])
            if candidate["kind"] == "take_card" and candidate["card"] == "PerfectedStrike"))
    env.choose(batch["decision_token_ids"], np.array(ordinals, dtype=np.uint64))
    env.step()
    after = env.decision_batch(semantic=True)
    for run in _tokens(after, schema, "Run"):
        scalars = _facts(after, schema, run, "scalar")
        assert (scalars["DeckSize"], scalars["DeckStrikeTagCount"],
                scalars["DeckStatusCount"]) == (10, 7, 0)
