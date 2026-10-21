# 暴走在双发重放中的成长

[前帧](before.json) → [打牌收据](play.json) → [后帧](after.json) 逐字节保留一条第一幕
认证记录中的单次玩家动作；[来源清单](sources.json) 保存原始路径、摘要与生产者版本。

玩家有一层双发，打出基础伤害 8 的未升级暴走。蓝衣奴隶贩从 52 血降至 31 血；玩家能量
从 1 降至 0，双发消失，同一个 Java 暴走对象以基础伤害 18 留在弃牌堆，且没有 RNG 调用。

原版 Rampage 在每次伤害后排入 ModifyDamageAction；DoubleTapPower 先创建保留 UUID 的
重放副本，将其放入 limbo；ModifyDamageAction 更新这一 UUID 的所有战斗实例。因此重放
应使用第一次增长后的伤害。前后帧观察到的是总伤害 21 和基础伤害 8→18，**没有逐击 Java
伤害记录**；8、13 的顺序由原版代码解释，不能伪称为已录制的逐击事件。

Rust 回归用一张 scenario 暴走、双发和一个存活的蓝衣奴隶贩实际执行打牌到稳定边界，比较
目标 HP/格挡、玩家 HP/格挡/能量、双发消耗及最终牌的数量、位置、费用与持续伤害。
Java UUID 和 Rust lineage 分别只检查各自端内的连续性，不建立跨端自然卡牌绑定。

原局的其他手牌、遗物、死去的史莱姆未复建。RNG 检查的是各端无调用且状态不变，不声明
构造输入与 Java 的初始 RNG 相同。这是受限机制回归，不是完整战斗重建或新的整幕认证。

```powershell
cargo test --locked -p sts_parity_lockstep_java --lib recorded_rampage
```
