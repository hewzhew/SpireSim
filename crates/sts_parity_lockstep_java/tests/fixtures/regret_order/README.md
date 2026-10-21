# 悔恨、灼伤与损血预防顺序的实机回归

这些 `constructed_validated` 原始记录补充[三点悔恨实验](../regret_block/README.md)无法
单独区分的结算顺序。[来源清单](sources.json)保存原路径、SHA-256、producer、采集器与
Java/Mod 启动来源；十五个原始 JSON 均逐字节保留。

五个 A20 铁甲战士场景均从 80 HP 开始，正常打出壁垒+；需要格挡的场景再打出防御。
生长增殖体首次只施加缠绕。下表的手牌是原版实际有序手牌，均为未升级牌：

| 场景 | EndTurn 前手牌 | 前置格挡 | 其他条件 | 后继 HP / 格挡 / Buffer |
| --- | --- | --- | --- | --- |
| `one-tungsten` | 悔恨 | 5 | 钨合金棍 | 80 / 5 / 无 |
| `one-buffer-tungsten` | 悔恨 | 5 | 钨合金棍、Buffer 1 | 80 / 5 / 无 |
| `mixed-block` | 灼伤、悔恨、伤口 | 5 | 无预防 | 77 / 3 / 无 |
| `mixed-buffer-a` | 悔恨、灼伤、伤口 | 0 | Buffer 1 | 78 / 0 / 无 |
| `mixed-buffer-b` | 灼伤、悔恨、伤口 | 0 | Buffer 1 | 77 / 0 / 无 |

前三个事实分别说明：钨合金棍能独自抹去 1 点损血；有 Buffer 时，它仍先消耗，不能让
钨合金棍提前减至零而保留 Buffer；灼伤消耗 2 格挡，随后悔恨仍按最初三张手牌损失 3 HP。
两组零格挡对照中，第一张触发牌消耗 Buffer，剩余损血分别来自灼伤 2 点或悔恨 3 点。
因此把手牌顺序忽略、反转，或等到悔恨执行时重新数手牌，都会改变可观察结果。

新旧批次共用受限配方、准备、录制、重放与 `replay_regret_mechanism_v1`。仅允许悔恨一张、
悔恨＋两张伤口、悔恨＋灼伤＋伤口这三种手牌构成；原版 hand ordering 必须明确。Rust
从前边界重建有序语义输入并创建独立场景身份，执行正常 EndTurn，不建立 Java UUID 绑定。
新批次只有在五组全部 Exact 且确实观察到两种灼伤/悔恨相对顺序后才封存。

比较范围为玩家标量与能力、怪物状态、AI/洗牌 stream 及完整有序 RNG 调用。每组都记录了
一次 AI 抽取与两次洗牌调用。完整牌区、遗物持久状态和其他 RNG stream 状态不在此范围；
这些证据不拥有自然跑局、整幕认证或恢复资格。

```powershell
cargo test --locked -p sts_parity_lockstep_runner --lib regret_order
```
