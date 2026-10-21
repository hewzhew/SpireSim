"""Read exported instance facts; never look up names or reconstruct runtime cards."""
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


def _instance_parts(state):
    parts = _parts(state["definition"])
    cost = state["persistent_cost"]
    parts["cost"] = f"当前持久费用 { {-1: 'X', -2: '不可打出'}.get(cost, cost)}"
    parts["numbers"] = f"实例字段：伤害 {state['persistent_damage']} / 格挡 {state['persistent_block']}"
    if state["misc_value"]:
        parts["misc"] = f"实例记录 misc={state['misc_value']}；含义依卡牌机制，不能通用地加到收益上"
    return parts


def render_owned_card(card, facts):
    lines = [f"[deck {card['ordinal']}] {card['card']}+{card['upgrades']}"]
    if facts is None:
        return lines + ["  此导出没有该实例的机制（Unknown）；未按卡名补造。"]
    current, next_state = facts["current"], facts["next_upgrade"]
    lines.extend(render_card_parts(
        _instance_parts(current),
        _instance_parts(next_state) if next_state else None,
        next_state["definition"]["upgrades"] if next_state else None,
    ))
    return lines


OWNED_READING_SCOPE = (
    "牌组实例机制：公开持久数值与共享规则，部分覆盖；未列效果为 Unknown。",
    "实例字段不是结算总伤害/格挡；升级预览只含卡牌自身变化，营火/遗物副作用与当前行动资格另算。",
)
