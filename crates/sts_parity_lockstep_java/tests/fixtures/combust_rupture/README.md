# 燃烧、悔恨与撕裂的实机回归

这组 `constructed_validated` 记录覆盖普通/升级/叠层燃烧、独立损血事件及 Buffer、钨合金棍
的预防顺序。[来源清单](sources.json)保留 30 个原始 JSON 的来源、SHA-256、producer、
采集器与 Java/Mod 启动身份。原版私有 `CombustPower.hpLoss` 由 Mod 直接读取，对敌伤害和
自损量独立投影；缺失字段不能靠伤害值推算。

采集批次在最后一组真实差异处停止：前九组 live Exact，最后一组 Java 79 HP、Rust 77 HP。
原始 `stopped.json` 和失败报告保留，没有补造 live-complete 封存。修复后，普通离线测试
用当前 Rust 与投影重算全部十组，不信任旧结果标签，也不需要重启 Java。

所有场景从 A20 铁甲战士 80 HP、零格挡开始，正常打出撕裂+和燃烧。叠层场景打出一张普通
燃烧及一张燃烧+，得到对敌伤害 12、自损 2；其余分别为 5/1 或 7/1。准备步骤通过同一
Java UUID 关联手牌升级信息与动作，要求唯一匹配；不建立 Java/Rust 卡牌绑定。
生长增殖体首次只施加缠绕，首次 EndTurn 可隔离全部损血。

| 场景 | 燃烧伤害 / 自损 | 手牌、预防 | 后继 HP / 力量 |
| --- | --- | --- | --- |
| `single` | 5 / 1 | 空 | 79 / 2 |
| `upgraded` | 7 / 1 | 空 | 79 / 2 |
| `stacked` | 12 / 2 | 空 | 78 / 2 |
| `stacked-tungsten` | 12 / 2 | 空、钨合金棍 | 79 / 2 |
| `stacked-buffer` | 12 / 2 | 空、Buffer 1 | 80 / 0 |
| `regret` | 5 / 1 | 悔恨 | 78 / 4 |
| `regret-buffer` | 5 / 1 | 悔恨、Buffer 1 | 79 / 2 |
| `regret-tungsten` | 5 / 1 | 悔恨、钨合金棍 | 80 / 0 |
| `regret-buffer-tungsten` | 5 / 1 | 悔恨、Buffer 1、钨合金棍 | 80 / 0 |
| `regret-three-buffer` | 5 / 1 | 悔恨＋两张伤口、Buffer 1 | 79 / 2 |

最后一组区分回合末阶段：原版先结算悔恨的 3 点损血并消耗 Buffer，再结算燃烧的 1 点损血。
旧 Rust 把普通 `atEndOfTurn` 能力与 `atEndOfTurnPreEndTurnCards` 合并，先让燃烧消耗
Buffer，后续悔恨损失 3 HP。引擎现在让普通能力在卡牌队列排空后、弃牌前触发；金属化、
多层护甲、如水仍保留在卡牌前。配套引擎窄测保护前后阶段与保留卡牌选择后的单次触发。

比较范围为玩家标量/能力（包括燃烧私有状态）、怪物状态、AI/洗牌状态与完整有序 RNG
调用。空手牌场景只有一次 AI 调用；有悔恨时另有两次洗牌调用。完整牌区、遗物持久状态、
其他 RNG stream 状态不在比较范围；不宣称全部回合末能力或自然整幕认证。

```powershell
cargo test --locked -p sts_parity_lockstep_runner --lib recorded_combust
```
