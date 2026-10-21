"""Validate card facts and render selective mechanics without entity lookup."""

from __future__ import annotations

from typing import Any

from .curated import _require

TARGETS = {
    "one_enemy": "单个敌人",
    "all_enemies": "全体敌人",
    "all_characters": "所有角色",
    "player_and_one_enemy": "玩家及单个敌人",
    "player": "玩家",
    "no_explicit_target": "无显式目标",
}
RESTORATION = {
    "no_scheduled_restoration": "卡牌不安排力量返还；敌方清除或阶段变化另算",
    "shackled_at_owner_turn_end": "施放时目标无 Artifact 才挂恢复；目标回合末若 Shackled 仍在才返还，提前移除不返还",
}
PLAYER_STRENGTH_DELIVERY = {
    "on_play": "出牌后请求玩家力量 +{amount}；本牌不安排回合末回收",
    "if_target_intent_base_damage_nonnegative_at_resolution": "效果结算时所选敌人的基础攻击意图伤害 ≥0（含零伤害攻击），才请求玩家力量 +{amount}；非攻击意图仍可打出但不加力，本牌不安排回收",
    "on_play_with_separate_lose_strength": "出牌后请求玩家力量 +{amount}，另请求同量 LoseStrength；后者成功挂上且保留到玩家回合末才失去力量，Artifact 阻止或提前清除另算",
    "each_player_turn_start_post_draw": "安装每个玩家回合开始抽牌后请求力量 +{amount} 的能力；打出当下不直接加力",
}
ENERGY_DELIVERY = {
    "on_play": "出牌后加费请求 +{amount}；这是总请求，未减去打出费用",
    "each_player_turn_start": "安装每个后续玩家回合开始加费 +{amount} 的能力；打出当下不加费，能力被移除则不再生效",
    "next_player_energy_recharge": "安装下次玩家能量补充时加费 +{amount} 的一次性能力；打出当下不加费，触发后移除",
    "on_exhaust": "此牌自身被消耗时加费请求 +{amount}；普通打出不触发，仍需消耗手段",
    "if_target_vulnerable_at_resolution": "效果结算时所选敌人有易伤才请求加费 +{amount}；非易伤目标仍可打出，伤害及战斗结束清队列另算",
}
STATUS_DESTINATIONS = {"draw_pile": "抽牌堆", "hand": "手牌", "discard_pile": "弃牌堆"}
STATUS_PERSISTENCE = {"persistent": "无虚无属性", "ethereal": "带虚无属性"}
BASE_BLOCK_DELIVERY = {
    "once_on_play": "出牌时请求一次基础格挡；牌面修正和 No Block 等阻止另算",
    "base_with_wrath_bonus": "请求一次基础格挡；效果结算时若处于愤怒，将额外数值加到同一次请求（额外数值未展开）",
    "if_player_block_zero_at_use": "仅出牌时玩家格挡为 0 才发出基础格挡请求",
    "if_this_draw_history_contains_skill": "本次抽牌记录含技能牌才请求基础格挡；未抽到牌不触发",
    "per_non_attack_in_hand_at_resolution": "效果结算时按手牌非攻击牌数量安排消耗与同次数格挡；消耗和格挡请求分别排队，零张则零次",
    "per_effective_x": "按结算后的 X 重复请求基础格挡；X 与能量及 Chemical X 等有关",
    "plated_armor_if_chosen": "定义格挡字段是选择 LiveForever 分支时的 PlatedArmor 层数，不是立即格挡",
    "replaced_by_discard_pile_size_plus_upgrade": "基础格挡由弃牌堆数量加升级修正替换；定义字段不是总量",
}

ATTACK_AMOUNTS = {
    "base": "每段使用基础伤害字段",
    "base_plus_strike_cards": "基础伤害加 Strike 牌数量 × {per_card}（手牌、抽牌堆、弃牌堆及自身，不含消耗堆）",
    "base_plus_mantra_gained_this_combat": "基础伤害加本场累计获得的真言",
    "player_block": "伤害由玩家当前格挡替换",
    "draw_pile_size": "伤害由当前抽牌堆张数替换",
    "frost_channeled_this_combat": "伤害为本场累计生成的冰霜球数 × {per_orb}",
}
ATTACK_HITS = {
    "fixed": "请求 {count} 段",
    "once_plus_if_target_poisoned_at_resolution": "先请求一段；后续效果结算时目标有中毒才追加一段",
    "per_effective_x": "按有效 X 请求次数；能量与 Chemical X 等另算，零次可能",
    "per_hand_card_at_resolution_before_exhaust": "效果结算时按消耗前手牌数安排同次数伤害；先排消耗再伤害，不以最终实际消耗事件数重算",
    "per_skill_in_hand_at_resolution": "效果结算时每张技能手牌请求一段，零张则零次",
    "per_filled_orb_at_resolution": "效果结算时每个非空充能球请求一段，空球位不算",
    "per_prior_attack_this_turn_at_resolution": "效果结算时按本回合已打出的其他攻击牌数请求次数，不含本次终结技",
    "per_lightning_channeled_this_combat_at_use": "出牌时按本场累计生成的闪电球数请求次数",
    "per_living_enemy_at_use": "出牌时按存活敌人数请求次数，所有段数仍打向同一目标",
    "from_instance_misc": "请求次数来自此牌实例记录；仅凭定义或升级数无法确定",
}
ATTACK_TARGETS = {
    "selected_enemy": "所选敌人",
    "each_enemy": "每个敌人分别承受这些段数，数值不是全场相加",
    "random_enemy_per_hit": "每段随机选一个敌人，不保证集中攻击，也不是全体伤害",
}
ATTACK_CONDITIONS = {
    "hand_contains_only_attacks": "手牌全为攻击牌才能打出",
    "draw_pile_empty": "抽牌堆为空才能打出",
    "no_other_attack_in_hand": "手中没有其他攻击牌才能打出",
}

COMBAT_UPGRADE_SCOPES = {
    "selected_card_in_hand": "效果结算时，从当前手牌升级一张可升级牌；零张无效果，一张自动升级，多张时选择且不能取消",
    "whole_hand": "效果结算时，升级当前手牌内全部可升级牌",
    "all_combat_zones": "效果结算时，升级当前手牌、抽牌堆、弃牌堆和消耗堆中的全部可升级牌",
}


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


def _render_damage_delivery(value):
    if value["kind"] == "strength_if_chosen":
        return "定义伤害字段是选择 BecomeAlmighty 分支时的力量数值，不发出攻击伤害请求"
    amount, hits = value["amount"], value["hits"]
    text = (
        ATTACK_AMOUNTS[amount["kind"]].format(**amount)
        + "；" + ATTACK_HITS[hits["kind"]].format(**hits)
        + "；目标：" + ATTACK_TARGETS[value["targets"]]
    )
    if value["play_condition"] is not None:
        text += "；" + ATTACK_CONDITIONS[value["play_condition"]]
    return text + "。战斗实例变化、力量等修正、格挡与目标死亡另算"


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


def _parts(facts):
    cost = facts["base_cost"]
    parts = {
        "cost": f"基础费用 { {-1: 'X', -2: '不可打出'}.get(cost, cost)}",
        "type": f"{facts['card_type']} / {facts['rarity']}",
        "target": f"出牌目标：{TARGETS[facts['play_target']]}",
        "flags": "消耗/虚无/固有："
        + "/".join(
            "是" if facts[k] else "否" for k in ("exhausts", "ethereal", "innate")
        ),
        "numbers": f"定义字段：伤害 {facts['definition_numbers']['damage']} / 格挡 {facts['definition_numbers']['block']}",
    }
    effects = facts["reviewed_effects"]
    upgrade_scope = effects.get("combat_upgrade_scope")
    if upgrade_scope is not None:
        parts["combat_upgrade"] = COMBAT_UPGRADE_SCOPES[upgrade_scope]
        parts["combat_upgrade_rules"] = (
            "范围内每张牌升级一次；普通已升级牌、诅咒和状态牌不升级，Searing Blow 可继续升级。"
            "只作用于本场战斗实例，不永久升级主牌组；之后才生成或进入范围的牌不自动获得升级"
        )
    debuffs = effects.get("enemy_debuffs_in_monster_order")
    if debuffs is not None:
        names = {"weak": "虚弱", "vulnerable": "易伤"}
        parts["debuffs"] = "按敌人列表顺序逐个处理，每个敌人依次请求：" + " → ".join(
            f"{names[row['debuff']]} {row['amount']} 层" for row in debuffs
        )
        parts["debuff_rules"] = "每次施加独立接受 Artifact 等阻止；一层 Artifact 阻止第一个减益后，后一个仍可生效。死亡、逃离与队列中断由运行时处理"
    rupture = effects.get("hp_loss_strength_power")
    if rupture is not None:
        parts["hp_strength"] = f"安装失血加力能力 {rupture} 层；多次施加层数相加，打出当下不加力"
        parts["hp_strength_rules"] = (
            "之后每次符合 Rupture 资格且实际损失 HP 的事件，请求等于当前能力层数的力量；收益不乘失血量。"
            "卡牌及 Combust 等产生的合资格失血可触发，敌人伤害不触发；失血被完全阻止则不触发。"
            "多份 Combust 合并为一次失血事件；能力移除、死亡与队列中断另算"
        )
    skill = effects.get("draw_pile_skill_to_hand")
    if skill is not None:
        parts["skill_to_hand"] = (
            f"从抽牌堆直接取 {skill} 张现有技能牌入手；打出时抽牌堆必须有技能牌。效果结算时重新检查：没有符合牌则无效果，仅一张自动取，多张时选择且不能取消。"
            "不属于抽牌，No Draw 不阻止；移动时满手则放入弃牌堆，取得牌保留当时费用，不自动免费"
        )
    copies = effects.get("copy_to_discard_after_damage")
    if copies is not None:
        parts["copy_discard"] = (
            f"伤害之后排队生成 {copies} 张自身副本到弃牌堆，仅在本场战斗内增加牌量。"
            "按实例复制，保留升级及持久数值/费用修改，不复制临时显示伤害；Master Reality 等生成修正另算。"
            "战斗结束清队列可能阻止复制结算"
        )
    damage = effects.get("base_damage_delivery")
    if damage is not None:
        parts["damage_delivery"] = _render_damage_delivery(damage)
    delivery = effects.get("base_block_delivery")
    if delivery is not None:
        parts["block_delivery"] = BASE_BLOCK_DELIVERY[delivery]
    draw = effects["unconditional_draw_request"]
    if draw is not None:
        parts["draw"] = f"成功出牌后抽牌请求 {draw}；No Draw、手牌上限及后续弃牌等另算"
    topdeck = effects.get("hand_topdeck_after_draw")
    if topdeck is not None:
        parts["topdeck"] = (
            f"抽牌动作之后还须将 {topdeck} 张手牌放回抽牌堆顶；No Draw 阻止抽牌也不免除此步骤。"
            "手牌为空则跳过，仅一张时自动放回，多张时从全部手牌选择，不能取消"
        )
    exhaust_draw = effects.get("exhaust_draw_power")
    if exhaust_draw is not None:
        parts["exhaust_draw"] = (
            f"安装消耗抽牌能力：每有一张牌被消耗，抽牌请求 {exhaust_draw}；多次施加层数相加，打出当下不直接抽牌。"
            "能力须仍在且战斗未进入收尾；No Draw、手牌上限及回合结束的队列处理另算"
        )
    end_damage = effects.get("end_of_turn_hp_loss_and_damage")
    if end_damage is not None:
        parts["end_damage"] = (
            f"安装持续回合末效果：每次施加增加自失血 {end_damage['hp_loss_per_application']}、每个敌人伤害 {end_damage['damage_per_enemy_per_application']}"
        )
        parts["end_damage_rules"] = (
            "两种数值分别叠加，每个玩家回合末先请求合计失血，再请求全体非攻击伤害；打出当下不结算。"
            "自失血绕过格挡、受无实体限制，实际失血可触发 Rupture；伤害不按玩家力量/虚弱调整，敌人格挡等另算。"
            "战斗进入敌人收尾时不再触发，死亡与队列中断另算"
        )
    strength = effects["enemy_strength_reduction"]
    if strength is not None:
        parts["strength"] = (
            f"每个受影响敌人的减力请求 {strength['amount']}；尚未计算 Artifact 等阻止"
        )
        parts["restoration"] = RESTORATION[strength["restoration"]]
    gain = effects.get("player_strength_gain")
    if gain is not None:
        parts["player_strength"] = PLAYER_STRENGTH_DELIVERY[gain["delivery"]].format(**gain)
    energy = effects.get("energy_gain")
    if energy is not None:
        parts["energy"] = ENERGY_DELIVERY[energy["delivery"]].format(**energy)
    vulnerable = effects.get("player_vulnerable_on_play")
    if vulnerable is not None:
        parts["player_vulnerable"] = f"出牌后对玩家施加易伤 {vulnerable} 层；Artifact 等阻止另算"
    statuses = effects.get("status_injections_on_play")
    if statuses:
        parts["statuses"] = "出牌后生成本战状态牌：" + "；".join(
            f"{status['card']} ×{status['count']} → {STATUS_DESTINATIONS[status['destination']]}（{STATUS_PERSISTENCE[status['persistence']]}）"
            for status in statuses
        )
    hp = effects["self_hp_loss_on_play"]
    if hp is not None:
        parts["hp"] = (
            f"出牌 HP 损失请求 {hp['base_hp_loss']}；"
            + ("绕过格挡" if hp["bypasses_block"] else "受格挡影响")
            + ("，绕过无实体" if hp["bypasses_intangible"] else "，受无实体影响")
            + (
                "；实际失血触发 Rupture"
                if hp["triggers_rupture"]
                else "；不触发 Rupture"
            )
        )
    block = effects["end_of_turn_block_each_turn"]
    if block is not None:
        parts["end_block"] = (
            f"每个玩家回合末格挡 {block['amount']}；"
            + ("持续本场战斗" if block["persists_for_combat"] else "非整场持续")
            + ("，层数相加" if block["stacks_additively"] else "，层数不相加")
            + ("，受敏捷影响" if block["affected_by_dexterity"] else "，不受敏捷影响")
            + ("，受脆弱影响" if block["affected_by_frail"] else "，不受脆弱影响")
        )
    return parts


def render_mechanics(content):
    from .deck_mechanics import OWNED_READING_SCOPE, render_owned_card
    from .relic_mechanics import (
        RELIC_READING_SCOPE, candidate_relic, render_owned_relic, render_relic_mechanics,
    )
    from .potion_mechanics import (
        POTION_READING_SCOPE, candidate_potion, render_potion_mechanics,
    )

    if "cards" in content:
        lines = list(OWNED_READING_SCOPE)
        for row in content["cards"]:
            lines.extend(render_owned_card(row["card"], row["facts"]))
        return lines
    if "relics" in content:
        lines = [RELIC_READING_SCOPE]
        for row in content["relics"]:
            lines.extend(render_owned_relic(row["ordinal"], row["observed"], row["facts"]))
        return lines
    lines = [
        "机制来源：Rust 共享语义；部分覆盖，未列效果为 Unknown，不表示不存在。",
    ]
    if any(row["facts"] is not None for row in content["candidates"]):
        lines.append("定义字段不等于结算伤害/格挡；条件、动态替换、次数与触发未完整展开，获取卡牌时的遗物修正另算。")
    if any(row.get("potion_facts") is not None for row in content["candidates"]):
        lines.append(POTION_READING_SCOPE)
    if any(row.get("relic_facts") is not None for row in content["candidates"]):
        lines.append(RELIC_READING_SCOPE)
    if any(row.get("owned_card") is not None for row in content["candidates"]):
        lines.extend(OWNED_READING_SCOPE)
    for row in content["candidates"]:
        lines.append(f"[{row['ordinal']}] {row['label']}")
        facts = row["facts"]
        if facts is None:
            if row.get("owned_card") is not None:
                owned = row["owned_card"]
                lines.extend("  " + text for text in render_owned_card(owned["card"], owned["facts"]))
                continue
            if candidate_potion(row["action"]) is not None:
                lines.extend("  " + text for text in render_potion_mechanics(row.get("potion_facts"), row["action"]))
                continue
            if candidate_relic(row["action"]) is not None:
                lines.extend("  " + text for text in render_relic_mechanics(row.get("relic_facts")))
                continue
            lines.append("  此候选的机制未展开（Unknown）。")
            continue
        upgraded = facts["next_upgrade"]
        lines.extend(render_card_parts(_parts(facts["current"]),
                     _parts(upgraded) if upgraded else None,
                     upgraded["upgrades"] if upgraded else None))
    return lines


def render_card_parts(parts, next_parts, next_upgrades):
    attribute_keys = ("cost", "type", "target", "flags")
    lines = ["  " + " · ".join(parts[key] for key in attribute_keys)]
    lines.extend(f"  {text}" for key, text in parts.items() if key not in attribute_keys)
    if next_parts is None:
        lines.append("  无下一合法卡牌升级（模拟器可表达范围内）。")
    else:
        changes = [next_parts.get(key, f"{key}：Unknown")
                   for key in dict.fromkeys((*parts, *next_parts))
                   if parts.get(key) != next_parts.get(key)]
        lines.append(f"  下次升级 +{next_upgrades}："
                     + "；".join(changes or ["已展示属性不变"])
                     + "。其余效果的升级变化未完整展开。")
    return lines
