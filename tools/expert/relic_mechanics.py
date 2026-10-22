"""Visible relic definitions from the typed observation and candidate surface."""

from tools.presentation import Presentation

from .curated import _require
from .mechanics import _booleans, _integer, _keys

def render_owned_relic(ordinal, observed, facts, *, locale="zh-CN"):
    view = Presentation(locale)
    lines = [f"[relic {ordinal}] {view.entity('relic', observed['relic'])}",
             view.text("expert", "relic.observed", counter=observed["counter"], amount=observed["amount"], used=view.text("expert", "yes" if observed["used_up"] else "no"))]
    lines.extend("  " + line for line in render_relic_mechanics(facts, locale=locale))
    return lines


def candidate_relic(action):
    if action["kind"] in {"buy_relic", "take_boss_relic"}:
        return action["relic"]
    if action["kind"] == "claim_reward" and action["reward"]["kind"] == "relic":
        return action["reward"]["relic"]
    return None


def context_relics(context):
    if context["kind"] == "shop":
        return [{"relic": row["relic"], "location": {"kind": "shopLocation", "index": row["slot"]}}
                for row in context.get("inventory", {}).get("relics", [])]
    if context["kind"] == "reward":
        return [{"relic": row["relic"], "location": {"kind": "rewardLocation", "index": index}}
                for index, row in enumerate(context.get("pending_rewards", [])) if row["kind"] == "relic"]
    return []


def validate_relic_mechanics(value, snapshot_id, observation, candidates):
    _keys(value, ("schema", "public_snapshot_id", "basis", "coverage", "definitions"))
    _require(value["schema"] == "sts-visible-relic-reading-v1"
             and value["public_snapshot_id"] == snapshot_id
             and value["basis"] == "simulator_definition_and_reviewed_effects"
             and value["coverage"] == "partial", "unsupported relic reading contract or snapshot")
    expected = {row["relic"] for row in observation["relics"]}
    expected.update(row["relic"] for row in context_relics(observation["context"]))
    expected.update(relic for row in candidates if (relic := candidate_relic(row["action"])) is not None)
    _require(isinstance(value["definitions"], list), "invalid relic definitions")
    result = {}
    for facts in value["definitions"]:
        _keys(facts, ("relic", "battle_start_power", "shuffle_energy", "combat_victory_healing"),
              ("energy_per_turn_delta", "campfire_restriction", "potion_acquisition_rule", "card_play_limit_per_turn",
               "additional_cards_drawn_each_turn", "pre_battle_confusion", "end_turn_discard_rule", "gold_gain_rule",
               "card_reward_choice_delta", "additional_normal_monster_card_reward_rows", "gold_gain_healing"))
        relic = facts["relic"]
        _require(relic in expected and relic not in result, "unknown or duplicate visible relic")
        if facts.get("energy_per_turn_delta") is not None:
            _integer(facts["energy_per_turn_delta"], low=1, high=255)
        restriction = facts.get("campfire_restriction")
        if restriction is not None:
            _require(isinstance(restriction, str) and restriction in {"rest", "smith"},
                     "unknown campfire restriction")
        potions = facts.get("potion_acquisition_rule")
        if potions is not None:
            _keys(potions, ("prevents_new_potions", "owned_potions_remain_usable"))
            _booleans(potions, ("prevents_new_potions", "owned_potions_remain_usable"))
        if facts.get("card_play_limit_per_turn") is not None:
            _integer(facts["card_play_limit_per_turn"], low=1, high=2**32-1)
        if facts.get("additional_cards_drawn_each_turn") is not None:
            _integer(facts["additional_cards_drawn_each_turn"], low=1)
        if facts.get("end_turn_discard_rule") is not None:
            _require(facts["end_turn_discard_rule"] == "skip_ordinary_discard",
                     "unsupported end-turn discard rule")
        if facts.get("gold_gain_rule") is not None:
            _require(facts["gold_gain_rule"] == "prevent_gold_gain", "unsupported gold-gain rule")
        if facts.get("gold_gain_healing") is not None:
            _integer(facts["gold_gain_healing"], low=1, high=2**31-1)
        if facts.get("card_reward_choice_delta") is not None:
            _integer(facts["card_reward_choice_delta"], low=-(2**31), high=2**31-1)
            _require(facts["card_reward_choice_delta"] != 0, "empty card reward choice adjustment")
        if facts.get("additional_normal_monster_card_reward_rows") is not None:
            _integer(facts["additional_normal_monster_card_reward_rows"], low=1, high=255)
        confusion = facts.get("pre_battle_confusion")
        if confusion is not None:
            _keys(confusion, ("minimum_cost", "maximum_cost", "eligible_cost", "turn_cost_update",
                              "clears_free_to_play_once"))
            _integer(confusion["minimum_cost"], high=255)
            _integer(confusion["maximum_cost"], low=confusion["minimum_cost"], high=255)
            _require(confusion["eligible_cost"] == "nonnegative_combat_cost"
                     and confusion["turn_cost_update"] == "only_when_combat_cost_changes",
                     "unsupported confusion cost rule")
            _booleans(confusion, ("clears_free_to_play_once",))
        power = facts["battle_start_power"]
        if power is not None:
            _keys(power, ("scope", "power", "amount"))
            _require(power["scope"] in ("every_combat", "elite_combat"), "unknown relic combat scope")
            _require(power["power"] in ("strength", "thorns"), "unknown relic battle-start power")
            _integer(power["amount"], low=1)
        energy = facts["shuffle_energy"]
        if energy is not None:
            _keys(energy, ("shuffle_trigger_at", "energy", "initial_counter", "counter_persists_between_combats"))
            _integer(energy["shuffle_trigger_at"], low=1)
            _integer(energy["energy"], low=1)
            _integer(energy["initial_counter"], high=energy["shuffle_trigger_at"]-1)
            _booleans(energy, ("counter_persists_between_combats",))
        heal = facts["combat_victory_healing"]
        if heal is not None:
            _keys(heal, ("base_hp", "requires_player_alive", "capped_at_max_hp", "affected_by_healing_modifiers"))
            _integer(heal["base_hp"], low=1)
            _booleans(heal, ("requires_player_alive", "capped_at_max_hp", "affected_by_healing_modifiers"))
        result[relic] = facts
    _require(set(result) == expected, "missing visible relic definitions")
    return result


def render_relic_mechanics(facts, *, locale="zh-CN"):
    view = Presentation(locale)
    text = lambda key, **values: view.definition_text("expert", "relic." + key, **values)
    if facts is None:
        return [text("unknown")]
    lines = []
    choices = facts.get("card_reward_choice_delta")
    if choices is not None:
        lines.extend([text("rewardChoices", delta=f"{choices:+d}"), text("rewardChoiceScope")])
    rows = facts.get("additional_normal_monster_card_reward_rows")
    if rows is not None:
        lines.extend([text("additionalRows", count=rows), text("rowScope")])
    if facts.get("gold_gain_rule") == "prevent_gold_gain":
        lines.extend([text("preventGold"), text("existingGold")])
    healing = facts.get("gold_gain_healing")
    if healing is not None:
        lines.extend([text("goldHealing", hp=healing), text("goldHealingTriggers"), text("healingModifiers")])
    if facts.get("end_turn_discard_rule") == "skip_ordinary_discard":
        lines.extend([text("skipDiscard"), text("ethereal"), text("handCapacity")])
    draw = facts.get("additional_cards_drawn_each_turn")
    if draw is not None:
        lines.append(text("draw", count=draw))
    confusion = facts.get("pre_battle_confusion")
    if confusion is not None:
        lines.append(text("confusion"))
        if draw is not None:
            lines.append(text("drawIndependent"))
        lines.append(text("randomCost", min=confusion["minimum_cost"], max=confusion["maximum_cost"]))
        lines.extend([text("costUpdate"), text("clearFree" if confusion["clears_free_to_play_once"] else "keepFree")])
        lines.append(text("combatCost"))
    energy = facts.get("energy_per_turn_delta")
    if energy is not None:
        lines.append(text("energy", amount=energy))
    restriction = facts.get("campfire_restriction")
    if restriction in {"rest", "smith"}:
        lines.append(text("noRest" if restriction == "rest" else "noSmith"))
    potions = facts.get("potion_acquisition_rule")
    if potions is not None:
        lines.append(text("potions", acquisition=text("preventPotions" if potions["prevents_new_potions"] else "allowPotions"), existing=text("useOwnedPotions" if potions["owned_potions_remain_usable"] else "noUseOwnedPotions")))
    limit = facts.get("card_play_limit_per_turn")
    if limit is not None:
        lines.append(text("cardLimit", count=limit))
    power = facts["battle_start_power"]
    if power:
        lines.append(text("strength" if power["power"] == "strength" else "thorns", scope=text("everyCombat" if power["scope"] == "every_combat" else "eliteCombat"), amount=power["amount"]))
    shuffle = facts["shuffle_energy"]
    if shuffle:
        lines.extend([text("shuffleEnergy", threshold=shuffle["shuffle_trigger_at"], amount=shuffle["energy"]),
                      text("shuffleCounter", initial=shuffle["initial_counter"], persistence=text("persistentCounter" if shuffle["counter_persists_between_combats"] else "transientCounter"))])
    heal = facts["combat_victory_healing"]
    if heal:
        lines.append(text("victoryHealing", hp=heal["base_hp"], alive=text("requiresAlive" if heal["requires_player_alive"] else "allowsDead"), cap=text("hpCap" if heal["capped_at_max_hp"] else "noHpCap"), modifiers=text("affectedHealing" if heal["affected_by_healing_modifiers"] else "ignoresHealing")))
    return lines or [text("unknownEffect")]
