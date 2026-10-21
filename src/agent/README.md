# Rust 公开 Agent 边界

状态：**当前实现级技术说明**。

`src/agent` 与对应 workspace crate 负责从私有 exact state 投影公开观察、完整合法候选和
typed choice，并把当前根绑定的选择交给私有 runtime 解析。它不拥有游戏机制、搜索目标、
策略赢家或 checkpoint 文件格式。

## 稳定边界

- public snapshot identity 绑定一份完整候选面；
- display text 只用于阅读，runtime 不从文字反解析动作；
- candidate ordinal/handle 只在绑定的 snapshot 内有意义；
- hidden RNG、私有 checkpoint、Java-only UUID 和未公开牌序不进入普通 actor view；
- exact successor、模型提案、搜索 witness 和 committed action 是不同证据；
- deterministic/routine follower 只能处理协议明确证明唯一的动作，不能代替卡奖、路线、
  药水、事件或其他高自主决策。

当前实现仍包含历史 controller、belief 和学习适配代码。它们存在不表示训练或低成本模型是
当前项目目标。跨 Java/Rust 的未来 controller 只能作为
[锁步差分运行](../../docs/ARCHITECTURE.md)的决策前端，不能自行拥有 parity 状态或证明。

## 修改要求

- 新公开字段必须来自 typed owner，不能从渲染文本补造；
- compact 与 detail view 必须投影同一对象；
- 协议破坏性变更使用新 schema/root，不兼容旧 artifact 时诚实拒绝；
- 测试候选完整性、身份绑定和权限，不测试摘要措辞。
