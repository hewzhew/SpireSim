# 缠绕的施加与回合结算

两个独立 EndTurn 片段均从真实 Java 记录逐字节复制，[来源清单](sources.json) 保存原路径、
摘要与生产者。它们之间的出牌过程不在此回归中，不拼接为连续轨迹。

- [施加前](apply-before.json) → [结束回合](apply-end-turn.json) → [施加后](apply-after.json)：
  生长增殖体施加 12 层缠绕，来源保留为该怪物。玩家 78 → 77 HP，与残暴的 1 点损血一致。
- [已有缠绕和火焰屏障](tick-before.json) → [结束回合](tick-end-turn.json) → [下一回合](tick-after.json)：
  缠绕及来源保留，玩家 77 → 76 HP；怪物 37 → 33 HP，只因其普通攻击受到一次火焰屏障反伤。
  缠绕本身不触发额外反伤。

测试用当前 Java projection 解析真实来源，再把声明的生长增殖体 occurrence 0 用作构造场景的
本地 entity 73；不比较双端裸编号，也不建立任何 Java UUID 与 Rust 卡牌 lineage 绑定。
Rust 保留观察到的玩家 HP、格挡、能力、怪物状态和 AI stream，使用空手牌与八张 scenario
打击避免抽牌副作用。比较玩家 HP/格挡/能力、怪物 HP/格挡/能力/行动，以及单次 floor.ai
调用和前后状态；能量、原牌区、其他遗物和其他 RNG 不在范围内。

原始格挡足够高，因此最终边界不能独立证明缠绕具体消耗了多少格挡，也不能检出省略或额外
执行一次仍被格挡的伤害。这里保护施加后的来源、回合边界状态和反伤行为，不能独立证明
所有结算时机；独立的[低格挡构造片段](../constricted_low_block/README.md)补充伤害量和时机
覆盖，不改写或扩大这里两个自然片段的证明范围。

缺少缠绕来源或来源不在当前怪物边界时必须拒绝投影。负例只修改内存副本。
这些局部回归不授予完整战斗、自然整幕或恢复结论，不需要启动 Java。

```powershell
cargo test --locked -p sts_parity_lockstep_java --lib recorded_constricted
```
