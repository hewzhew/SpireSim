# 仓库协作约定

与项目所有者使用中文交流，区分事实、推断、候选设计和 Unknown。

## 上下文

- 进入任务时检查分支、工作树和近期提交；从 [文档导航](docs/README.md) 按任务选读。
  日常修改不需要预读全部设计。代码、测试和显式 artifact 高于聊天或压缩摘要。
- 运行入口见 [RUNBOOK](docs/RUNBOOK.md)。会话、checkpoint 或工具仍存在不代表可以恢复执行。
  旧任务 JSONL、退役设计和历史档案只在用户明确要求考古或回归时定向读取。
- 当前文档只保留职责和使用合同；实验结果、seed、性能数据与进度进入 artifact。
  退役内容由 Git 保存，不另建 archive。

## 游戏与证据

- `run-play` 是 Rust 单机玩法和策略实验；`java-play` 是 Java 控制和诊断。
  跨端主线见 [ARCHITECTURE](docs/ARCHITECTURE.md)，目前不能将实验组件拼接后宣称端到端完成。
- 新 parity 核心共享 `SemanticAction`、双端 adapter、稳定边界、`ParitySnapshot`、comparator
  和 append-only journal。CLI 与测试共用核心；前端不拥有状态、head 或证明语义。
- Rust 可私下搜索，Java 只跟随已采用轨迹。Audit、Certification 和自然 Act 验收边界见
  [TESTING](docs/TESTING.md)；单步证据不冒充整幕认证。
- Java UUID 与 Rust CardLineage 只通过 typed 来源证据绑定，不能按名称、位置或裸数值猜测。
- 跨端、认证和原始采集证据保留来源与生产者版本；日常 `run-play` 是可丢弃的可玩状态，
  只保留有界回退和主动精选的局面，不借它声称自然来源或认证结果。
- 固定输入上的搜索只证明局部可达性，预算耗尽是 Unknown。Uncertified 不等于 Unsupported；
  真实 mismatch、动作不能对应、关键状态不可观察或来源不明确才停止该跨端轨迹。
- 默认输出面向决策。完整 JSON、来源摘要与收据留在磁盘，诊断时再展开。

## 修改和验证

- 沿真实 owner 完成改动，删除被替代的副本；避免由一次故障衍生新的外壳、别名或迁移层。
- 文档、静态检查和离线测试不启动 Java。实机检查先说明问题和停止条件，尽量复用 resident JVM。
- Python 和 pytest 使用 `scripts/py.ps1`。Rust 提交前运行 `cargo fmt --all -- --check` 与
  `git diff --check`；文档检查相对链接。功能修改先跑 owner 窄测，再按风险补集成哨兵。
- 删除或递归移动前核对显式绝对路径和范围；保留无关修改与未知文件。
- 可为当前任务创建范围明确、可回退的本地提交；只有用户明确要求时才 push、开 PR 或修改远端。
  交接说明结论、实际验证、尚存限制与工作树状态。
