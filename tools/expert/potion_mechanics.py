"""Candidate-bound potion use facts from Rust; never infer effects from names."""

from .curated import _require
from .mechanics import _booleans, _integer, _keys

POTION_READING_SCOPE = "药水数值按基础药效；Sacred Bark 等修正、用药遗物、队列与战斗收尾另算，未列效果仍为 Unknown。"


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


def render_potion_mechanics(facts, action):
    if facts is None:
        return ["药水使用机制 Unknown；此导出没有相关事实，不按名称补猜。"]
    kind = action["kind"]
    context = {
        "buy_potion": "本候选是购买，效果须之后使用才发生",
        "claim_reward": "本候选是领取，效果须之后使用才发生",
        "discard_run_potion": "本候选是丢弃，不会触发使用效果；以下供判断保留价值",
        "use_run_potion": "本候选是使用；合法性与实际结果仍由当前游戏 owner 确认",
    }[kind]
    lines = [context + "。"]
    lines.append("药水使用需指定敌人目标。" if facts["requires_target"] else "药水使用无显式敌人目标。")
    lines.append(
        "基础规则允许战外使用；We Meet Again 等当前限制另算。"
        if facts["permits_out_of_combat_use"] else
        "基础规则不允许战外主动使用；战内时机、自动触发或其他限制仍需核实。"
    )
    effect = facts["reviewed_use"]
    if effect:
        kind = effect["kind"]
        if kind == "heal_max_hp_percent":
            lines.append(f"使用时按最大 HP 的 {effect['percent']}% 计算治疗请求，按游戏浮点规则取整，不保证至少 1 HP；实际治疗受缺血量、治疗修正与禁止治疗影响。")
        elif kind == "apply_enemy_weak":
            lines.append(f"对所选敌人请求虚弱 {effect['amount']} 层；Artifact 等可阻止，持续层数不等于固定减伤量。")
        elif kind == "gain_thorns":
            lines.append(f"获得荆棘 {effect['amount']} 层，受其他来源的普通攻击时请求同量反伤；被格挡的攻击仍可触发，THORNS/HP_LOSS 不触发；不立即造成伤害。")
        elif kind == "choose_hand_exhaust":
            lines.append("选择当前手牌中任意数量进行消耗，精确游戏规则允许选零张；不消耗永久牌组，也不是抽牌。取消选择不退还已用药水，自动候选可过滤无效果选择；消耗触发与用药遗物另算。")
    discovery = facts["card_discovery"]
    if discovery:
        pool = {"attack": "攻击", "skill": "技能", "power": "能力", "colorless": "无色"}[discovery["pool"]]
        lines.append(f"从{pool}池发现 {discovery['offered_choices']} 选一；选中后生成 {discovery['base_selected_card_copies']} 张副本，本回合费用设为 {discovery['selected_card_cost_this_turn']}；"
                     + ("可以跳过。" if discovery["can_skip"] else "不能跳过。"))
    if effect is None and discovery is None:
        lines.append("具体使用效果 Unknown。")
    return lines
