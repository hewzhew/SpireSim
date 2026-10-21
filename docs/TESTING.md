# 测试与验收原则

测试保护游戏语义、确定性和权限边界，不保护旧命令名称、迁移步骤、报告措辞或偶然文件布局。

完整任务、补充覆盖和最小反例的组织合同见[环境验收](architecture/environment-acceptance.md)。
完整局基线与流程覆盖由实际连续重放产生，局部成功按其在套件中的职责归位。

## 验证阶梯

1. 机制与 RNG 的 deterministic unit/property test；
2. Java/Rust adapter 对共同 `SemanticAction` 和 `ParitySnapshot` 的离线合同；
3. 一条 constructed 或固定根的双端相邻转换；
4. 当前 projection 对原始 Java boundary 的离线重投影，以及 Rust 对同一 typed trajectory 的
   逐步离线重放；
5. 一次自然 Act 的 lockstep Audit；
6. 从幕入口到下一幕入口的严格 Act Certification。

局部机制不绑定长 seed。自然整幕也不能代替机制测试：它是纵向集成证据，不证明未出现的牌、
遗物、事件或敌人。

验证阶梯不是每次修改都要从 1 跑到 6 的固定清单。普通 Rust 机制或派生投影修改停在能够回答
问题的最低层；常驻 Java 批次只用于补充或重录真实 oracle 证据；自然 Act Audit 低频运行，Act
Certification 极低频运行。不得把 JVM 冷启动、整幕重放或从 Neow 开始变成普通 Rust 提交的
默认成本。

## Audit 与 Certification 的不同门禁

Audit 允许执行以前没有认证、但满足以下条件的动作：

- 两侧都能从当前动作空间明确解析同一个语义动作；
- 动作结算后的关键确定性状态可在两侧生成同一投影；
- 运行保持 natural/probe 来源，不移动正式选择器。

Audit 遇到真实 mismatch、unsupported 或关键 unobserved 时停止，并保存第一处转换的最小反例。
不能用宽松比较吞掉差异，也不能因为 `uncertified` 预先禁止发现新机制。

Certification 输入必须是封闭 typed trajectory。声明范围内任一步 Unknown、跳过、人工状态编辑、
动作重绑定或版本不兼容都使整幕验收失败。只有抵达下一幕入口且全部转换 Exact，才发布 Act 级
通过；途中收据不单独冒充项目里程碑。

Act 4 没有下一幕入口，验收终点是击杀 Heart 后显式 Proceed 到达原版 TrueVictoryRoom 的
胜利终局；击杀后的 Proceed 选择页仍在验收范围内。幕入口的原生恢复准备必须单独记录，
不得算入连续认证，也不得用其局部通过替代完整幕范围。

## 轨迹回归

每条可复验 trajectory 应保存原始 Java 稳定 boundary、语义动作与收据、Rust 重建输入、两侧
派生后继和版本合同。Rust 或 projection 实现变化后首先离线重投影/重放，不启动 Java；在第一处
不同或非法动作停止。只有 Java 游戏、Mod、原始观察合同变化，或已有 Java 记录缺少新的关键字段
时，才要求重新录制对应 Java 范围。只保存旧版 `ParitySnapshot`、无法重新解释原始 Java frame
的 artifact 不满足新 corpus 资格。

修复应尽量落在最小机制测试，同时保留触发它的真实 trajectory transition。corpus 只保留增加
机制/分支覆盖或代表既有 bug 最短反例的 case；不得为每个 seed 复制整份长期 fixture，也不得把
单个实机例子外推为全内容覆盖。

## RNG 验证

当前至少比较声明的 RNG stream 状态和计数。引入 RNG 调用账本后，还要比较每次 committed
transition 内的 owner、调用形状、参数、结果与 ordinal；正常路径可以比较滚动 digest，差异时
必须展开第一条不同调用。测试必须能区分：

- 初始 stream 状态不同；
- 调用次数或顺序不同；
- 参数不同；
- 相同调用产生不同结果；
- 结果相同但后续状态不同。

## 身份验证

- Java UUID 与 Rust CardLineage 不做裸值比较；
- 重复卡牌必须通过来源、birth 或当前绑定关系对应；
- Java runtime-local 身份不得进入稳定 trajectory identity；
- reload 后重新建立 Java instance binding，不能沿用旧 epoch；
- 无法证明的一一对应是 Unsupported/Unknown，不按位置或名称猜测。

详细技术合同见 [Card Lineage Identity](architecture/card-lineage.md)。

## 前端与工具测试

CLI 和测试 harness 应调用同一个核心。前端测试只保护：

- 短输入能够无歧义映射到当前 typed action；
- 默认输出不包含大段 JSON、SHA、路径、PID 或重复双端 Exact 状态；
- 详细 artifact 可按明确诊断请求读取；
- 前端断开不会成为唯一状态丢失或错误动作提交；
- experimental/retired 工具不会被默认帮助宣称为权威 parity runner。

不要测试整段人类措辞、固定行数、私有模块路径或旧工具是否仍保持过去的实现形状。

## 搜索结论的测试边界

complete-turn generator、atomic search 和 resident witness producer 即使报告 exact、complete 或
replay-verified，也只证明其声明的合法动作、结算与固定输入可达性。测试不得把以下事实固化为
合同：

- 默认候选集合包含战略上最好的回合；
- 候选顺序是可信的资源偏好；
- 一个短窗口或 turn segment 足以解决长战斗；
- 未在预算内找到胜线等价于该分支较差或不可胜。

战略质量需要单独的假设、跨回合比较和资源 Pareto 证据。搜索输出在这些能力实现并验收以前是
proposal/evidence，不是猫猫必须服从的决策 owner，也不是训练标签。

## 应保留的既有测试

- 卡牌、遗物、药水、敌人、事件和 RNG 机制；
- exact transition、合法动作和稳定身份；
- checkpoint round trip 与原版存档范围；
- 搜索终止、预算、frontier、replay 与 Unknown；
- Java protocol、稳定观察和动作执行；
- 第一处分歧和失败不污染正式状态；
- 有清楚来源与结论范围的最小 regression case。

仅保护已经退役工作流、旧 schema 迁移或旧命令名字的测试应随 owner 退役，不得为了全绿复活
旧架构。

## 提交前

owner 验证使用完整的 `cargo test -p <owner>`，包含该包注册的 integration targets。
若为定位问题先使用 `--lib`、`--bins` 或名称过滤，提交前仍须补齐受影响 owner 的独立集成测试；
这些过滤结果不能代替完整 owner 验证。标记 `ignored` 的实机测试按其声明范围另行安排。

代码改动按真实风险覆盖 owner，并执行：

```powershell
cargo fmt --all -- --check
cargo lint-workspace
git diff --check
```

纯文档改动至少运行 `git diff --check`、检查 Markdown 相对链接，并确认退役文档只能通过 Git
历史取得。大日志写入 ignored artifact，交接只报告结论和短失败原因。
