"""Read exported instance facts; never look up names or reconstruct runtime cards."""
from tools.presentation import Presentation

from .curated import _digest, _require
from .mechanics import _definition, _integer, _keys, _parts, render_card_parts


def _state(value):
    _keys(value, ("definition", "persistent_cost", "persistent_damage", "persistent_block", "misc_value"))
    _definition(value["definition"])
    _integer(value["persistent_cost"], low=-2, high=2**31 - 1)
    for key in ("persistent_damage", "persistent_block", "misc_value"):
        _integer(value[key], low=-(2**31), high=2**31 - 1)


def validate_deck_mechanics(value, snapshot_id, cards):
    _keys(value, ("schema", "public_snapshot_id", "basis", "coverage", "cards"))
    _require(value["schema"] == "sts-owned-card-reading-v1"
             and value["public_snapshot_id"] == snapshot_id
             and value["basis"] == "public_instance_and_shared_card_mechanics"
             and value["coverage"] == "partial", "unsupported owned-card reading")
    expected = {}
    for card in cards:
        ordinal = card["ordinal"]
        _integer(ordinal)
        _require(ordinal not in expected, "duplicate public deck ordinal")
        expected[ordinal] = card
    _require(isinstance(value["cards"], list), "invalid owned-card rows")
    result = {}
    for row in value["cards"]:
        _keys(row, ("ordinal", "card_sha256", "facts"))
        ordinal = row["ordinal"]
        _integer(ordinal)
        _require(ordinal in expected and ordinal not in result, "foreign or duplicate owned-card row")
        card = expected[ordinal]
        _require(row["card_sha256"] == _digest(card), "owned-card reading bound to different instance")
        facts = row["facts"]
        _keys(facts, ("current", "next_upgrade"))
        _state(facts["current"])
        current = facts["current"]["definition"]
        _require(current["card"] == card["card"] and current["upgrades"] == card["upgrades"]
                 and facts["current"]["misc_value"] == card["misc_value"], "owned-card identity differs")
        if facts["next_upgrade"] is not None:
            _state(facts["next_upgrade"])
            upgraded = facts["next_upgrade"]["definition"]
            _require(upgraded["card"] == card["card"] and upgraded["upgrades"] == card["upgrades"] + 1,
                     "owned-card next upgrade identity differs")
        result[ordinal] = facts
    _require(set(result) == set(expected), "missing owned-card reading")
    return result


def _instance_parts(state, locale):
    view = Presentation(locale)
    parts = _parts(state["definition"], locale)
    cost = state["persistent_cost"]
    parts["cost"] = view.text("expert", "deck.cost", cost={-1: "X", -2: view.text("expert", "unplayable")}.get(cost, cost))
    parts["numbers"] = view.text("expert", "deck.numbers", damage=state["persistent_damage"], block=state["persistent_block"])
    if state["misc_value"]:
        parts["misc"] = view.text("expert", "deck.misc", value=state["misc_value"])
    return parts


def render_owned_card(card, facts, *, locale="zh-CN"):
    view = Presentation(locale)
    lines = [f"[deck {card['ordinal']}] {view.card_name(card['card'], card['upgrades'])}"]
    if facts is None:
        return lines + [view.text("expert", "deck.unknown")]
    current, next_state = facts["current"], facts["next_upgrade"]
    lines.extend(render_card_parts(_instance_parts(current, locale), _instance_parts(next_state, locale) if next_state else None,
                                  next_state["definition"]["upgrades"] if next_state else None, locale=locale))
    return lines


def owned_reading_scope(locale):
    view = Presentation(locale)
    return [view.text("expert", "deck.scope"), view.text("expert", "deck.upgradeScope")]
