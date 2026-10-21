# 头槌选择页与背击标记

这里保留从正式第四幕认证中逐字节复制的三个相邻 Java 边界及两份动作收据：
[打牌前](before.json) → [头槌](play.json) → [弃牌选择页](pending.json) →
[提交选择](select.json) → [队列结算后](settled.json)。
[sources.json](sources.json) 保存来源、字节摘要和原生产者。

原记录在头槌指向盾卫时立即朝左，但选择页中的 BackAttack 标记仍在盾卫身上；选择完成、
队列结算后，标记才移到长矛。测试用当前 Java 投影读取怪物位置和标记，用引擎实际的
打牌与选择流程验证一个最小构造场景的相同时序。

构造场景只含存活的盾卫、长矛、Surrounded、头槌及两张可选择的弃牌。它比较朝向、怪物左右、
背击标记及选择生命周期；不复制原局牌组、遗物、伤害数值、HP 或 RNG。Rust scenario lineage
不与原记录的 Java UUID 建立绑定。这不是完整战斗重建、可恢复起点或新的自然认证。

测试先检查原字节、producer、稳定边界与收据连接；负例只修改内存副本。

```powershell
cargo test --locked -p sts_parity_lockstep_java --lib recorded_headbutt
```
