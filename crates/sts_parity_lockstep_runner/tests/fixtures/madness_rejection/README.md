# Madness 原始拒绝抽样反例

这里逐字节保留自然 Time Eater 战斗中一次 Madness 的 Java 前后边界与动作收据。
[sources.json](sources.json) 记录明确的来源坐标、原生产者和每个文件的 SHA-256；原文件没有
删除字段、修改版本或补造来源。

对应测试从原始手牌费用与 `cardRandomRng` 状态建立一个**构造的 Madness handler 输入**，
在打出的 Madness 离开手牌后调用现有机制 owner。它检查完整随机调用、最终 RNG 状态和
各牌的战斗/本回合费用。两张相同 Defend 的 Java 前后关联使用同一原记录的 UUID；测试中的
Rust scenario lineage 只属于构造输入，不建立自然 Java UUID → Rust lineage 绑定。

这份证据保护“整手牌拒绝抽样”与费用变化。它不导入完整战斗，不是可执行的自然重放根，
也不证明 Time Warp、消耗触发或整场 Boss 已对齐。自然转换的全域复验仍须显式指定来源，
从原始证据逐段重建。

```powershell
cargo test --locked -p sts_parity_lockstep_runner --lib observed_madness_rejection_case
```

此测试先验证原字节、producer 和动作收据连接；不依赖 `.oracle-lab` 或 Java 进程。
