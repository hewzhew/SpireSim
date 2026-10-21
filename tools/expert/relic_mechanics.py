"""Visible relic definitions from the typed observation and candidate surface."""

from .curated import _require
from .mechanics import _booleans, _integer, _keys

RELIC_READING_SCOPE = "遗物已审阅效果仅部分覆盖；获取时效果、其他触发与相互作用未完整展开。"


def render_owned_relic(ordinal, observed, facts):
    lines = [
        f"[relic {ordinal}] {observed['relic']}",
        f"  当前观测：计数 {observed['counter']} / 数量 {observed['amount']} / "
        f"已使用标记 {'是' if observed['used_up'] else '否'}",
    ]
    lines.extend("  " + text for text in render_relic_mechanics(facts))
    return lines


def candidate_relic(action):
    if action["kind"] in {"buy_relic", "take_boss_relic"}:
        return action["relic"]
    if action["kind"] == "claim_reward" and action["reward"]["kind"] == "relic":
        return action["reward"]["relic"]
    return None


def context_relics(context):
    if context["kind"] == "shop":
        return [{"relic": row["relic"], "location": f"商店遗物槽 {row['slot']}"}
                for row in context.get("inventory", {}).get("relics", [])]
    if context["kind"] == "reward":
        return [{"relic": row["relic"], "location": f"奖励项 {index}"}
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


def render_relic_mechanics(facts):
    if facts is None:
        return ["遗物机制 Unknown；此导出没有相关事实，不按名称补猜。"]
    lines = []
    choices_delta = facts.get("card_reward_choice_delta")
    if choices_delta is not None:
        lines.append(f"每份生成的卡牌奖励，可选张数 {choices_delta:+d}；多个相关遗物的修正相加，奖励份数不因此变化。")
        lines.append("也作用于调用奖励生成流程的无色奖励、捕梦网与星系仪；不改变商店卡牌库存、发现或药水的三选一数量。")
    additional_rows = facts.get("additional_normal_monster_card_reward_rows")
    if additional_rows is not None:
        lines.append(f"普通怪物房生成卡牌奖励时，额外生成 {additional_rows} 份；精英、Boss 与事件房不适用。")
        lines.append("每份独立生成，分别选择或跳过；每份的可选张数仍受破损王冠、问号牌等修正。")
    if facts.get("gold_gain_rule") == "prevent_gold_gain":
        lines.append("阻止获得金币，包括奖励、事件、卡牌、遗物的金币收入及盗贼返还的金币。")
        lines.append("已有金币保留，仍可花费或被盗；未获得金币时不触发获得金币后的效果，例如血腥神像的治疗。")
    healing = facts.get("gold_gain_healing")
    if healing is not None:
        lines.append(f"每次实际获得正数金币时，立即请求治疗 {healing} HP；一次收入触发一次，不按金币数量乘算。")
        lines.append("奖励、事件、卡牌与遗物收入均可触发；灵体阻止收入时不触发，花费或失去金币也不触发。")
        lines.append("实际治疗受最大 HP、治疗修正与禁止治疗影响；不会预先计入尚未发生的收入。")
    if facts.get("end_turn_discard_rule") == "skip_ordinary_discard":
        lines.append("跳过通常的回合末弃牌；不赋予保留标记，也不因此触发保留效果或确立的减费。")
        lines.append("未保留的虚无牌仍会消耗；牌本身的保留与其他效果各自结算，主动弃牌仍可发生。")
        lines.append("留下的牌占用手牌容量，可能挤占下一回合抽牌空间；此效果不增加起手抽牌数。")
    draw = facts.get("additional_cards_drawn_each_turn")
    if draw is not None:
        lines.append(f"每回合起手抽牌数 +{draw}；实际抽到的数量仍受手牌上限、牌堆与禁止抽牌等限制。")
    confusion = facts.get("pre_battle_confusion")
    if confusion is not None:
        lines.append("战斗前请求施加混乱；只有混乱成功存在时才在抽牌时随机费用，能力被阻止或清除另算。"
                     + ("额外起手抽牌不依赖混乱仍在。" if draw is not None else ""))
        lines.append(f"每次实际抽到非负战斗费用的牌，抽取 {confusion['minimum_cost']} 到 {confusion['maximum_cost']}（含两端）的费用；"
                     "按费用而非卡牌类型判断，状态与诅咒不统一排除，X 与不可打出的负费用不掷。")
        lines.append("掷出费用与当前战斗费用不同时，同时更新本场费用与本回合费用；相同时保留已有本回合费用。"
                     + ("符合资格的抽牌都会清除免费打出一次标记，包括掷出相同费用时。"
                        if confusion["clears_free_to_play_once"] else "本规则不清除免费打出一次标记。"))
        lines.append("费用变化只影响本场卡牌副本，不永久改写主牌组；之后再次符合条件抽到时会重掷。")
    energy_delta = facts.get("energy_per_turn_delta")
    if energy_delta is not None:
        lines.append(f"持有时每回合基础能量 +{energy_delta}；不是获取当下立即获得可用能量，其他能量修正另算。")
    restriction = facts.get("campfire_restriction")
    if restriction == "rest":
        lines.append("禁用营火的休息选项；其他治疗来源仍按各自规则结算，不等于禁止一切恢复。")
    elif restriction == "smith":
        lines.append("禁用营火的升级选项；事件、卡牌等其他升级来源仍按各自规则结算。")
    potions = facts.get("potion_acquisition_rule")
    if potions is not None:
        lines.append(("阻止获得新药水" if potions["prevents_new_potions"] else "本规则不阻止获得新药水")
                     + ("；已有药水仍可使用，具体使用资格另算。" if potions["owned_potions_remain_usable"]
                        else "；本规则不保留已有药水的使用资格。"))
    limit = facts.get("card_play_limit_per_turn")
    if limit is not None:
        lines.append(f"每回合最多打出 {limit} 张牌，回合开始重置计数；零费牌也计数，不是手牌数量上限。")
    power = facts["battle_start_power"]
    if power:
        scope = "每场战斗开始" if power["scope"] == "every_combat" else "仅精英战斗开始（Boss 身份本身不满足条件）"
        if power["power"] == "strength":
            lines.append(f"{scope}请求力量 +{power['amount']}；不是每回合加力，也不直接造成伤害。")
        else:
            lines.append(f"{scope}请求荆棘 {power['amount']}；受其他来源的普通攻击时反伤，被格挡的攻击仍可触发；THORNS/HP_LOSS 不触发。")
    energy = facts["shuffle_energy"]
    if energy:
        lines.append(f"每次洗牌计数 +1，恰好到 {energy['shuffle_trigger_at']} 时立即归零并请求能量 +{energy['energy']}；不是每回合或购买时加费。")
        lines.append(f"定义初始计数 {energy['initial_counter']}；"
                     + ("计数跨战斗保留。" if energy["counter_persists_between_combats"] else "计数不跨战斗保留。")
                     + "实际收益取决于洗牌与能量使用时机；牌堆变空不自动等于一次洗牌。")
    heal = facts["combat_victory_healing"]
    if heal:
        lines.append(f"战斗胜利后请求治疗 {heal['base_hp']} HP"
                     + ("，需玩家存活" if heal["requires_player_alive"] else "，不要求玩家存活")
                     + ("，受最大 HP 限制" if heal["capped_at_max_hp"] else "，不受最大 HP 限制")
                     + ("，受治疗修正与禁止治疗影响。" if heal["affected_by_healing_modifiers"] else "，不受治疗修正影响。"))
    if not lines:
        lines.append("具体遗物效果 Unknown。")
    return lines
