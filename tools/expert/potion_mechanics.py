"""Candidate-bound potion use facts from Rust; never infer effects from names."""

from tools.presentation import Presentation

from .curated import _require
from .mechanics import _booleans, _integer, _keys

def candidate_potion(action):
    if action["kind"] in {"buy_potion", "use_run_potion", "discard_run_potion"}:
        return action["potion"]
    if action["kind"] == "claim_reward" and action["reward"]["kind"] == "potion":
        return action["reward"]["potion"]
    return None


def validate_potion_mechanics(value, snapshot_id, candidates):
    _keys(value, ("schema", "public_snapshot_id", "basis", "coverage", "candidates"))
    _require(
        value["schema"] == "sts-potion-acquisition-reading-v1"
        and value["public_snapshot_id"] == snapshot_id
        and value["basis"] == "simulator_definition_and_reviewed_effects"
        and value["coverage"] == "partial",
        "unsupported potion mechanics contract or snapshot",
    )
    expected = {row["candidate_id"]: potion for row in candidates
                if (potion := candidate_potion(row["action"])) is not None}
    _require(isinstance(value["candidates"], list), "invalid potion mechanics candidates")
    result = {}
    for row in value["candidates"]:
        _keys(row, ("candidate_id", "facts"))
        cid = row["candidate_id"]
        _require(cid in expected and cid not in result, "unknown or duplicate potion candidate")
        facts = row["facts"]
        _keys(facts, ("potion", "requires_target", "permits_out_of_combat_use", "reviewed_use", "card_discovery"))
        _require(facts["potion"] == expected[cid], "potion mechanics identity differs from candidate")
        _booleans(facts, ("requires_target", "permits_out_of_combat_use"))
        effect = facts["reviewed_use"]
        if effect is not None:
            _require(isinstance(effect, dict), "invalid potion use rule")
            kind = effect.get("kind")
            _require(isinstance(kind, str), "invalid potion use kind")
            field = {"heal_max_hp_percent": "percent", "apply_enemy_weak": "amount", "gain_thorns": "amount", "choose_hand_exhaust": None}
            _require(kind in field, "unknown potion use rule")
            _keys(effect, ("kind", field[kind]) if field[kind] else ("kind",))
            if field[kind]:
                _integer(effect[field[kind]], low=1)
        discovery = facts["card_discovery"]
        if discovery is not None:
            _keys(discovery, ("pool", "offered_choices", "base_selected_card_copies", "selected_card_cost_this_turn", "can_skip"))
            _require(discovery["pool"] in ("attack", "skill", "power", "colorless"), "unknown discovery pool")
            for field in ("offered_choices", "base_selected_card_copies"):
                _integer(discovery[field], low=1)
            _integer(discovery["selected_card_cost_this_turn"])
            _booleans(discovery, ("can_skip",))
        result[cid] = facts
    _require(set(result) == set(expected), "missing potion candidate mechanics")
    return result


def render_potion_mechanics(facts, action, *, locale="zh-CN"):
    view = Presentation(locale)
    text = lambda key, **values: view.definition_text("expert", "potion." + key, **values)
    if facts is None:
        return [text("unknown")]
    lines = [text(action["kind"]), text("requiresTarget" if facts["requires_target"] else "noTarget"),
             text("outsideCombat" if facts["permits_out_of_combat_use"] else "insideCombat")]
    effect = facts["reviewed_use"]
    if effect:
        kind = effect["kind"]
        if kind == "heal_max_hp_percent":
            lines.append(text("heal", percent=effect["percent"]))
        elif kind == "apply_enemy_weak":
            lines.append(text("weak", amount=effect["amount"]))
        elif kind == "gain_thorns":
            lines.append(text("thorns", amount=effect["amount"]))
        elif kind == "choose_hand_exhaust":
            lines.append(text("exhaust"))
    discovery = facts["card_discovery"]
    if discovery:
        pool = text("colorless") if discovery["pool"] == "colorless" else view.entity("cardType", discovery["pool"])
        lines.append(text("discovery", pool=pool, choices=discovery["offered_choices"], copies=discovery["base_selected_card_copies"], cost=discovery["selected_card_cost_this_turn"], skip=text("canSkip" if discovery["can_skip"] else "cannotSkip")))
    if effect is None and discovery is None:
        lines.append(text("unknownEffect"))
    return lines
