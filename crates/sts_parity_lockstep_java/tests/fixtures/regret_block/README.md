# 悔恨绕过格挡的实机回归

四个 `constructed_validated` 场景的原始边界和动作收据逐字节保留；[来源清单](sources.json)
记录原路径、摘要、producer、采集器可执行文件及完整批次 manifest 的摘要。

A20 铁甲战士从 80 HP 开始，正常打出壁垒+、防御后保留 5 格挡，手中是悔恨与两张伤口。
生长增殖体首次行动只施加缠绕，因此这次 EndTurn 的损血来自悔恨，壁垒让格挡是否被消耗
在后继稳定边界仍可观察。四组共用同一只隔离 JVM：

| 对照 | 初始 Buffer | 后继 HP | 后继格挡 | 后继 Buffer |
| --- | --- | --- | --- | --- |
| 无预防 | 无 | 77 | 5 | 无 |
| 螺类化石 | 1 | 80 | 5 | 无 |
| 钨合金棍 | 无 | 78 | 5 | 无 |
| 螺类化石＋钨合金棍 | 1 | 80 | 5 | 无 |

这直接覆盖 HP_LOSS 绕过格挡、Buffer 消耗和钨合金棍减伤。Buffer 与钨合金棍并存的三点
输入证明该分支会消耗 Buffer；它不能单独区分两者的先后顺序，顺序另有原版源码依据。
能区分该顺序的 1 点输入与混合卡牌对照见[顺序回归](../regret_order/README.md)。
四组都保留怪物 AI 抽取和弃牌回抽产生的真实洗牌调用。

`replay_regret_mechanism_v1` 同时用于实机采集和离线回归。Rust 只从前帧验证并构造闭合输入，
使用自己的场景卡牌身份；两张伤口没有顺序相关效果，不将它们或其他牌绑定到 Java UUID。
双方 adapter 使用正常能力、怪物意图与 RNG 投影，共享 comparator 比较玩家标量/能力、
怪物状态、AI/洗牌状态，并严格核对完整有序调用账本。Rust 结算由正常 EndTurn 引擎执行，
不会从后帧 HP 推算伤害。

完整牌区、遗物持久状态及其他 RNG stream 状态不在比较范围。此样本不能当作自然跑局、
完整战斗、整幕认证或恢复输入；Buffer 的投影支持也不代表所有 Buffer 交互均已覆盖。

```powershell
cargo test --locked -p sts_parity_lockstep_runner --lib recorded_regret
```
