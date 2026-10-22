"""Validate exported card facts and render them with shared display resources."""

from __future__ import annotations

from typing import Any

from tools.presentation import Presentation

from .curated import _require

TARGETS = frozenset(('one_enemy', 'all_enemies', 'all_characters', 'player_and_one_enemy', 'player', 'no_explicit_target'))
RESTORATION = frozenset(('no_scheduled_restoration', 'shackled_at_owner_turn_end'))
PLAYER_STRENGTH_DELIVERY = frozenset(('on_play', 'if_target_intent_base_damage_nonnegative_at_resolution', 'on_play_with_separate_lose_strength', 'each_player_turn_start_post_draw'))
ENERGY_DELIVERY = frozenset(('on_play', 'each_player_turn_start', 'next_player_energy_recharge', 'on_exhaust', 'if_target_vulnerable_at_resolution'))
STATUS_DESTINATIONS = frozenset(('draw_pile', 'hand', 'discard_pile'))
STATUS_PERSISTENCE = frozenset(('persistent', 'ethereal'))
BASE_BLOCK_DELIVERY = frozenset(('once_on_play', 'base_with_wrath_bonus', 'if_player_block_zero_at_use', 'if_this_draw_history_contains_skill', 'per_non_attack_in_hand_at_resolution', 'per_effective_x', 'plated_armor_if_chosen', 'replaced_by_discard_pile_size_plus_upgrade'))

ATTACK_AMOUNTS = frozenset(('base', 'base_plus_strike_cards', 'base_plus_mantra_gained_this_combat', 'player_block', 'draw_pile_size', 'frost_channeled_this_combat'))
ATTACK_HITS = frozenset(('fixed', 'once_plus_if_target_poisoned_at_resolution', 'per_effective_x', 'per_hand_card_at_resolution_before_exhaust', 'per_skill_in_hand_at_resolution', 'per_filled_orb_at_resolution', 'per_prior_attack_this_turn_at_resolution', 'per_lightning_channeled_this_combat_at_use', 'per_living_enemy_at_use', 'from_instance_misc'))
ATTACK_TARGETS = frozenset(('selected_enemy', 'each_enemy', 'random_enemy_per_hit'))
ATTACK_CONDITIONS = frozenset(('hand_contains_only_attacks', 'draw_pile_empty', 'no_other_attack_in_hand'))

COMBAT_UPGRADE_SCOPES = frozenset(('selected_card_in_hand', 'whole_hand', 'all_combat_zones'))


def _damage_delivery(value):
    _require(isinstance(value, dict), "invalid damage delivery")
    if value.get("kind") == "strength_if_chosen":
        _keys(value, ("kind",))
        return
    _keys(value, ("kind", "amount", "hits", "targets", "play_condition"))
    _require(value["kind"] == "attack", "unknown damage delivery")
    for field, allowed, parameters in (
        ("amount", ATTACK_AMOUNTS, {"base_plus_strike_cards": "per_card", "frost_channeled_this_combat": "per_orb"}),
        ("hits", ATTACK_HITS, {"fixed": "count"}),
    ):
        item = value[field]
        _require(isinstance(item, dict), "invalid damage rule")
        kind = item.get("kind")
        _require(isinstance(kind, str) and kind in allowed, "unknown damage rule")
        parameter = parameters.get(kind)
        _keys(item, ("kind", parameter) if parameter else ("kind",))
        if parameter:
            _integer(item[parameter], low=1)
    _require(isinstance(value["targets"], str) and value["targets"] in ATTACK_TARGETS, "unknown damage target scope")
    condition = value["play_condition"]
    _require(condition is None or (isinstance(condition, str) and condition in ATTACK_CONDITIONS), "unknown attack play condition")


def _render_damage_delivery(value, locale="zh-CN"):
    view = Presentation(locale)
    text = lambda key, **values: view.definition_text("expert", "card." + key, **values)
    if value["kind"] == "strength_if_chosen":
        return text("wishStrength")
    amount, hits = value["amount"], value["hits"]
    result = text("damageSummary", amount=text("attackAmount." + amount["kind"], **amount),
                  hits=text("attackHits." + hits["kind"], **hits), targets=text("attackTarget." + value["targets"]))
    if value["play_condition"] is not None:
        result += view.text("expert", "separator") + text("attackCondition." + value["play_condition"])
    return result + text("damageModifiers")


def _keys(value, keys, optional=()):
    _require(
        isinstance(value, dict)
        and set(keys) <= set(value) <= set(keys) | set(optional),
        "invalid card mechanics fields",
    )


def _integer(value, low=0, high=None):
    _require(
        type(value) is int and value >= low and (high is None or value <= high),
        "invalid card mechanics number",
    )


def _booleans(value, names):
    for name in names:
        _require(type(value[name]) is bool, "invalid card mechanics flag")


def _definition(value):
    _keys(
        value,
        (
            "card",
            "upgrades",
            "card_type",
            "rarity",
            "base_cost",
            "play_target",
            "exhausts",
            "ethereal",
            "innate",
            "definition_numbers",
            "reviewed_effects",
        ),
    )
    _require(isinstance(value["card"], str) and value["card"], "missing card identity")
    _integer(value["upgrades"], high=255)
    _integer(value["base_cost"], low=-2, high=127)
    _require(
        value["card_type"] in {"Attack", "Skill", "Power", "Status", "Curse"},
        "unknown card type",
    )
    _require(
        isinstance(value["rarity"], str) and value["rarity"], "missing card rarity"
    )
    _require(value["play_target"] in TARGETS, "unknown play target")
    _booleans(value, ("exhausts", "ethereal", "innate"))
    numbers = value["definition_numbers"]
    _keys(numbers, ("damage", "block"))
    for amount in numbers.values():
        _integer(amount)
    effects = value["reviewed_effects"]
    _keys(
        effects,
        (
            "unconditional_draw_request",
            "enemy_strength_reduction",
            "self_hp_loss_on_play",
            "end_of_turn_block_each_turn",
        ),
        optional=(
            "base_block_delivery", "base_damage_delivery", "player_strength_gain",
            "energy_gain", "player_vulnerable_on_play", "status_injections_on_play",
            "exhaust_draw_power", "hand_topdeck_after_draw", "end_of_turn_hp_loss_and_damage",
            "enemy_debuffs_in_monster_order", "hp_loss_strength_power",
            "draw_pile_skill_to_hand", "copy_to_discard_after_damage",
            "combat_upgrade_scope", "combat_upgrade_persistence",
        ),
    )
    upgrade_scope = effects.get("combat_upgrade_scope")
    upgrade_persistence = effects.get("combat_upgrade_persistence")
    if upgrade_scope is not None or upgrade_persistence is not None:
        _require(
            isinstance(upgrade_scope, str) and upgrade_scope in COMBAT_UPGRADE_SCOPES
            and upgrade_persistence == "combat_only",
            "invalid combat upgrade scope or persistence",
        )
    damage = effects.get("base_damage_delivery")
    if damage is not None:
        _damage_delivery(damage)
    debuffs = effects.get("enemy_debuffs_in_monster_order")
    if debuffs is not None:
        _require(isinstance(debuffs, list) and bool(debuffs), "invalid ordered enemy debuffs")
        for request in debuffs:
            _keys(request, ("debuff", "amount"))
            _require(isinstance(request["debuff"], str) and request["debuff"] in {"weak", "vulnerable"}, "unknown enemy debuff")
            _integer(request["amount"], low=1)
    if effects.get("hp_loss_strength_power") is not None:
        _integer(effects["hp_loss_strength_power"], low=1)
    for field in ("draw_pile_skill_to_hand", "copy_to_discard_after_damage"):
        if effects.get(field) is not None:
            _integer(effects[field], low=1, high=1)
    delivery = effects.get("base_block_delivery")
    _require(
        delivery is None or delivery in BASE_BLOCK_DELIVERY,
        "unknown base Block delivery",
    )
    if effects["unconditional_draw_request"] is not None:
        _integer(effects["unconditional_draw_request"])
    for field in ("exhaust_draw_power", "hand_topdeck_after_draw"):
        if effects.get(field) is not None:
            _integer(effects[field], low=1)
    if effects.get("hand_topdeck_after_draw") is not None:
        _require(effects["hand_topdeck_after_draw"] == 1, "only one-card follow-up topdeck is reviewed")
    end_damage = effects.get("end_of_turn_hp_loss_and_damage")
    if end_damage is not None:
        _keys(end_damage, ("damage_per_enemy_per_application", "hp_loss_per_application"))
        for amount in end_damage.values():
            _integer(amount, low=1)
    strength = effects["enemy_strength_reduction"]
    if strength is not None:
        _keys(strength, ("amount", "restoration"))
        _integer(strength["amount"])
        _require(strength["restoration"] in RESTORATION, "unknown strength restoration")
    gain = effects.get("player_strength_gain")
    if gain is not None:
        _keys(gain, ("amount", "delivery"))
        _integer(gain["amount"], low=1)
        _require(
            isinstance(gain["delivery"], str)
            and gain["delivery"] in PLAYER_STRENGTH_DELIVERY,
            "unknown player Strength delivery",
        )
    energy = effects.get("energy_gain")
    if energy is not None:
        _keys(energy, ("amount", "delivery"))
        _integer(energy["amount"], low=1)
        _require(
            isinstance(energy["delivery"], str) and energy["delivery"] in ENERGY_DELIVERY,
            "unknown energy delivery",
        )
    vulnerable = effects.get("player_vulnerable_on_play")
    if vulnerable is not None:
        _integer(vulnerable, low=1)
    statuses = effects.get("status_injections_on_play")
    if statuses is not None:
        _require(isinstance(statuses, list) and bool(statuses), "invalid status injections")
        for status in statuses:
            _keys(status, ("card", "count", "destination", "persistence"))
            _require(isinstance(status["card"], str) and bool(status["card"]), "missing status identity")
            _integer(status["count"], low=1)
            _require(isinstance(status["destination"], str) and status["destination"] in STATUS_DESTINATIONS, "unknown status destination")
            _require(isinstance(status["persistence"], str) and status["persistence"] in STATUS_PERSISTENCE, "unknown status persistence")
    hp = effects["self_hp_loss_on_play"]
    if hp is not None:
        _keys(
            hp,
            (
                "base_hp_loss",
                "bypasses_block",
                "bypasses_intangible",
                "triggers_rupture",
            ),
        )
        _integer(hp["base_hp_loss"])
        _booleans(hp, ("bypasses_block", "bypasses_intangible", "triggers_rupture"))
    block = effects["end_of_turn_block_each_turn"]
    if block is not None:
        flags = (
            "persists_for_combat",
            "stacks_additively",
            "affected_by_dexterity",
            "affected_by_frail",
        )
        _keys(block, ("amount", *flags))
        _integer(block["amount"])
        _booleans(block, flags)


def validate_card_mechanics(value: dict[str, Any], snapshot_id: str, candidates):
    _keys(value, ("schema", "public_snapshot_id", "basis", "coverage", "candidates"))
    _require(
        value["schema"] == "sts-card-acquisition-reading-v1"
        and value["public_snapshot_id"] == snapshot_id
        and value["basis"] == "simulator_definition_and_reviewed_effects"
        and value["coverage"] == "partial",
        "unsupported card mechanics contract or snapshot",
    )
    expected = {
        row["candidate_id"]: row["action"]
        for row in candidates
        if row["action"]["kind"] in {"take_card", "buy_card"}
    }
    _require(isinstance(value["candidates"], list), "invalid mechanics candidates")
    result = {}
    for row in value["candidates"]:
        _keys(row, ("candidate_id", "facts"))
        cid = row["candidate_id"]
        _require(
            cid in expected and cid not in result,
            "unknown or duplicate mechanics candidate",
        )
        facts = row["facts"]
        _keys(facts, ("current", "next_upgrade"))
        current = facts["current"]
        _definition(current)
        action = expected[cid]
        _require(
            current["card"] == action["card"]
            and current["upgrades"] == action["upgrades"],
            "mechanics identity differs from candidate",
        )
        upgraded = facts["next_upgrade"]
        if upgraded is not None:
            _definition(upgraded)
            _require(
                upgraded["card"] == current["card"]
                and upgraded["upgrades"] == current["upgrades"] + 1,
                "mechanics next upgrade identity differs",
            )
        result[cid] = facts
    _require(set(result) == set(expected), "missing card candidate mechanics")
    return result


def _parts(facts, locale="zh-CN"):
    view = Presentation(locale)
    text = lambda key, **values: view.definition_text("expert", "card." + key, **values)
    yesno = lambda value: view.text("expert", "yes" if value else "no")
    cost = facts["base_cost"]
    parts = {
        "cost": text("cost", cost={-1: "X", -2: view.text("expert", "unplayable")}.get(cost, cost)),
        "type": f"{view.entity('cardType', facts['card_type'].lower())} / {view.entity('cardRarity', facts['rarity'].lower())}",
        "target": text("playTarget", target=text("target." + facts["play_target"])),
        "flags": text("flags", exhaust=yesno(facts["exhausts"]), ethereal=yesno(facts["ethereal"]), innate=yesno(facts["innate"])),
        "numbers": text("numbers", damage=facts["definition_numbers"]["damage"], block=facts["definition_numbers"]["block"]),
    }
    effects = facts["reviewed_effects"]
    scope = effects.get("combat_upgrade_scope")
    if scope is not None:
        parts["combat_upgrade"] = text("combatUpgrade." + scope)
        parts["combat_upgrade_rules"] = text("combatUpgradeRules")
    debuffs = effects.get("enemy_debuffs_in_monster_order")
    if debuffs is not None:
        names = {"weak": "Weak", "vulnerable": "Vulnerable"}
        parts["debuffs"] = text("debuffs", effects=" → ".join(text("debuffAmount", name=view.entity("power", names[row["debuff"]]), amount=row["amount"]) for row in debuffs))
        parts["debuff_rules"] = text("debuffRules")
    rupture = effects.get("hp_loss_strength_power")
    if rupture is not None:
        parts["hp_strength"] = text("hpStrength", amount=rupture)
        parts["hp_strength_rules"] = text("hpStrengthRules")
    for field, part, key in [
        ("draw_pile_skill_to_hand", "skill_to_hand", "skillToHand"),
        ("copy_to_discard_after_damage", "copy_discard", "copyDiscard"),
    ]:
        count = effects.get(field)
        if count is not None:
            parts[part] = text(key, count=count)
    damage = effects.get("base_damage_delivery")
    if damage is not None:
        parts["damage_delivery"] = _render_damage_delivery(damage, locale)
    block_delivery = effects.get("base_block_delivery")
    if block_delivery is not None:
        parts["block_delivery"] = text("blockDelivery." + block_delivery)
    for field, part, key in [
        ("unconditional_draw_request", "draw", "draw"),
        ("hand_topdeck_after_draw", "topdeck", "topdeck"),
        ("exhaust_draw_power", "exhaust_draw", "exhaustDraw"),
    ]:
        count = effects.get(field)
        if count is not None:
            parts[part] = text(key, count=count)
    end_damage = effects.get("end_of_turn_hp_loss_and_damage")
    if end_damage is not None:
        parts["end_damage"] = text("endDamage", hp=end_damage["hp_loss_per_application"], damage=end_damage["damage_per_enemy_per_application"])
        parts["end_damage_rules"] = text("endDamageRules")
    strength = effects["enemy_strength_reduction"]
    if strength is not None:
        parts["strength"] = text("strengthReduction", amount=strength["amount"])
        parts["restoration"] = text("restoration." + strength["restoration"])
    for field, part, group in [("player_strength_gain", "player_strength", "playerStrength"), ("energy_gain", "energy", "energy")]:
        value = effects.get(field)
        if value is not None:
            parts[part] = text(group + "." + value["delivery"], **value)
    vulnerable = effects.get("player_vulnerable_on_play")
    if vulnerable is not None:
        parts["player_vulnerable"] = text("playerVulnerable", amount=vulnerable)
    statuses = effects.get("status_injections_on_play")
    if statuses:
        parts["statuses"] = text("statuses", cards=view.text("expert", "separator").join(
            text("statusRow", name=view.card_name(row["card"]), count=row["count"], destination=text("statusDestination." + row["destination"]), persistence=text("statusPersistence." + row["persistence"])) for row in statuses))
    hp = effects["self_hp_loss_on_play"]
    if hp is not None:
        parts["hp"] = text("hpLoss", amount=hp["base_hp_loss"], block=text("bypassBlock" if hp["bypasses_block"] else "affectedByBlock"),
                           intangible=text("bypassIntangible" if hp["bypasses_intangible"] else "affectedByIntangible"), rupture=text("triggersRupture" if hp["triggers_rupture"] else "noRupture"))
    block = effects["end_of_turn_block_each_turn"]
    if block is not None:
        parts["end_block"] = text("endBlock", amount=block["amount"], duration=text("combatDuration" if block["persists_for_combat"] else "shortDuration"),
                                  stacking=text("additive" if block["stacks_additively"] else "nonadditive"), dexterity=text("affectedByDexterity" if block["affected_by_dexterity"] else "ignoresDexterity"), frail=text("affectedByFrail" if block["affected_by_frail"] else "ignoresFrail"))
    return parts


def render_mechanics(content, *, locale="zh-CN"):
    from .deck_mechanics import owned_reading_scope, render_owned_card
    from .relic_mechanics import candidate_relic, render_owned_relic, render_relic_mechanics
    from .potion_mechanics import candidate_potion, render_potion_mechanics
    from .decision import _candidate_label

    view = Presentation(locale)
    text = lambda key: view.definition_text("expert", key)
    if "cards" in content:
        lines = owned_reading_scope(locale)
        for row in content["cards"]:
            lines.extend(render_owned_card(row["card"], row["facts"], locale=locale))
        return lines
    if "relics" in content:
        lines = [text("relic.scope")]
        for row in content["relics"]:
            lines.extend(render_owned_relic(row["ordinal"], row["observed"], row["facts"], locale=locale))
        return lines
    lines = [text("card.scope")]
    if any(row["facts"] is not None for row in content["candidates"]):
        lines.append(text("card.definitionScope"))
    if any(row.get("potion_facts") is not None for row in content["candidates"]):
        lines.append(text("potion.scope"))
    if any(row.get("relic_facts") is not None for row in content["candidates"]):
        lines.append(text("relic.scope"))
    if any(row.get("owned_card") is not None for row in content["candidates"]):
        lines.extend(owned_reading_scope(locale))
    for row in content["candidates"]:
        lines.append(f"[{row['ordinal']}] {row.get('label') or _candidate_label(row['action'], locale)}")
        facts = row["facts"]
        if facts is None:
            if row.get("owned_card") is not None:
                owned = row["owned_card"]
                lines.extend("  " + line for line in render_owned_card(owned["card"], owned["facts"], locale=locale))
                continue
            if candidate_potion(row["action"]) is not None:
                lines.extend("  " + line for line in render_potion_mechanics(row.get("potion_facts"), row["action"], locale=locale))
                continue
            if candidate_relic(row["action"]) is not None:
                lines.extend("  " + line for line in render_relic_mechanics(row.get("relic_facts"), locale=locale))
                continue
            lines.append(text("card.unknownCandidate"))
            continue
        upgraded = facts["next_upgrade"]
        lines.extend(render_card_parts(_parts(facts["current"], locale), _parts(upgraded, locale) if upgraded else None, upgraded["upgrades"] if upgraded else None, locale=locale))
    return lines


def render_card_parts(parts, next_parts, next_upgrades, *, locale="zh-CN"):
    view = Presentation(locale)
    attribute_keys = ("cost", "type", "target", "flags")
    lines = ["  " + " | ".join(parts[key] for key in attribute_keys)]
    lines.extend(f"  {line}" for key, line in parts.items() if key not in attribute_keys)
    if next_parts is None:
        lines.append(view.text("expert", "card.noUpgrade"))
    else:
        changes = [next_parts.get(key, f"{key}: Unknown") for key in dict.fromkeys((*parts, *next_parts)) if parts.get(key) != next_parts.get(key)]
        lines.append(view.text("expert", "card.nextUpgrade", upgrades=next_upgrades, changes=view.text("expert", "separator").join(changes or [view.text("expert", "card.unchanged")])))
    return lines
