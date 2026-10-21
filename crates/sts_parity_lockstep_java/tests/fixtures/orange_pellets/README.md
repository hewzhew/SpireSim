# 橙色药丸的两种重置时机

这里保留两个独立原始片段，不拼接成一条自然轨迹。[来源清单](sources.json) 保存每个片段
的认证来源、原始路径、摘要和生产者。所有原始 JSON 均逐字节复制。

- 回合重置：[已有攻击与技能标志](reset-before.json) → [结束回合](end-turn.json) →
  [标志全部清空](reset-after.json) → [黑暗之拥](dark-embrace.json) →
  [只有能力标志](power-after.json)。原牌为升级黑暗之拥。
- 组合触发：[已有技能与能力标志，以及 10 力量、9 点待失去力量](cleanse-before.json) →
  [打出愤怒](anger.json) → [清除待失去力量，保留力量与黑暗之拥，清空组合标志](cleanse-after.json)。

Java 的显示计数在这些帧中一直为 -1，不能表示组合进度。测试使用当前 relic projection 读取
Mod 导出的 attack_played、skill_played、power_played；缺少任一标志都不能用显示计数补齐。

Rust 使用明确构造的场景：回合重置片段用准备强化的颚虫和固定五张抽牌，完整执行 EndTurn
及升级黑暗之拥；组合片段用存活邪教徒和记录中的三个玩家能力，实际打出愤怒。前者只比较
药丸标志，后者比较药丸标志和玩家能力，不复建原局 Boss、其他遗物、牌组、资源或 RNG。
Rust scenario lineage 不与 Java UUID 绑定；这些测试不授予完整战斗、整幕或恢复结论。

```powershell
cargo test --locked -p sts_parity_lockstep_java --lib recorded_orange_pellets
```
