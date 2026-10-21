# 缠绕的低格挡实机回归

此构造实验补充[高格挡自然片段](../constricted/README.md)无法直接揭示的伤害量与结算时机。
原始边界和动作收据逐字节保留，[来源清单](sources.json)记录路径、摘要与生产者。

两个 A20 生长增殖体场景各保留两次 EndTurn：

| 转换 | 前置事实 | 实机后继 |
| --- | --- | --- |
| 首次施加（两个场景） | 80 HP、无格挡、尚无缠绕 | 仍为 80 HP，获得来源为该怪物的 12 层缠绕 |
| 零格挡结算 | 缠绕 12，玩家易伤 1，怪物力量 -26 | 68 HP，格挡 0 |
| 5 格挡结算 | 同上，额外打出防御获得 5 格挡 | 73 HP，格挡 0 |

准备通过原版黑暗枷锁+、尖啸+、缴械+和狂暴+完成，怪物下次普通攻击为零。因此损血来自
缠绕，既不能将它漏掉，也不能当作受力量、易伤影响的普通攻击。5 格挡场景还保留防御牌
回抽产生的两次真实洗牌 RNG 调用。

实机采集和普通测试都调用 `replay_constricted_mechanism_v1`。Java/Rust 的正常 adapter
分别投影，共享 comparator 比较玩家状态与能力、怪物状态以及 AI/洗牌 stream；完整转换
调用账本另行逐项比较，其他 RNG owner 出现时停止。Rust 只从前帧建立受限机制场景，
不会根据后帧 HP 反推伤害。单张防御的回抽使用 Rust 自己的 starter lineage，不绑定 Java UUID。

这些是 `constructed_validated` 样本，不能当作自然跑局、完整战斗、整幕认证或恢复输入。
完整牌区、其他遗物及其他 RNG stream 状态不在比较范围内；原始记录保留以便以后重解释。

```powershell
cargo test --locked -p sts_parity_lockstep_runner --lib recorded_constricted
```
