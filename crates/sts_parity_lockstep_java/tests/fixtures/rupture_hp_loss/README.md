# 撕裂与真实损血的实机回归

这组 `constructed_validated` 原始记录覆盖悔恨与残暴两种自损来源，原版均经
`LoseHPAction(player, player, amount)` 传递自来源 `DamageInfo`；当前 Rust 分别由通用伤害
与直接损血入口处理。[来源清单](sources.json)保存原路径、
SHA-256、producer、采集器和 Java/Mod 启动来源；24 个原始 JSON 均逐字节保留。

八个 A20 铁甲战士场景均从 80 HP、零格挡开始，正常打出撕裂+，获得 2 层撕裂。
悔恨场景手中保留悔恨一张或悔恨加两张伤口；残暴场景再正常打出残暴，手牌和牌区为空。
生长增殖体首次只施加缠绕，因此首次 EndTurn 的损血可以单独归因。

| 场景 | 损血输入 | 预防 | 后继 HP | 后继力量 |
| --- | --- | --- | --- | --- |
| `regret-one` | 悔恨 1 | 无 | 79 | 2 |
| `regret-three` | 悔恨 3 | 无 | 77 | 2 |
| `regret-buffer` | 悔恨 1 | Buffer 1 | 80 | 无 |
| `regret-tungsten` | 悔恨 1 | 钨合金棍 | 80 | 无 |
| `regret-buffer-tungsten` | 悔恨 1 | Buffer 1、钨合金棍 | 80 | 无 |
| `brutality-one` | 下回合残暴 1 | 无 | 79 | 2 |
| `brutality-buffer` | 下回合残暴 1 | Buffer 1 | 80 | 无 |
| `brutality-tungsten` | 下回合残暴 1 | 钨合金棍 | 80 | 无 |

有 Buffer 的场景在后继均已消耗它。撕裂按每次正数实际损血触发，增加自身层数对应的力量；
不会按损血点数倍增，也不会在 Buffer 拦截或钨合金棍减至零时触发。

实机与离线均复用 `replay_regret_mechanism_v1` 的封闭输入验证与共享 comparator。Rust
从前边界重建场景，执行正常 EndTurn；不读取后继来反推伤害，也不绑定 Java UUID。
比较玩家标量/能力、怪物状态、AI/洗牌状态与完整有序 RNG 调用。悔恨场景各有一次 AI
与两次洗牌调用；残暴场景只有一次 AI 调用，空牌区没有洗牌。

完整牌区、遗物持久状态、其他 RNG stream 状态不在比较范围；这些样本不能证明全部
损血能力，也不拥有自然整幕认证或恢复资格。普通离线回归：

```powershell
cargo test --locked -p sts_parity_lockstep_runner --lib recorded_rupture
```
