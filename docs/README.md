# 文档导航

按任务选读，不需要依次读完整个目录。

| 任务 | 入口 |
| --- | --- |
| 首次安装、完整复验与接入策略 | [Getting started](getting-started.md) |
| 观察模型的公开信息对局与人工返回 | [短段演示](model-play.md) |
| 整理可独立使用的发布候选 | [发布候选](runbooks/release.md) |
| 选择运行工具、构建与测试 | [RUNBOOK](RUNBOOK.md) |
| 开发脚本与本地短命令 | [scripts](../scripts/README.md) |
| Java ↔ Rust 职责与对齐设计 | [ARCHITECTURE](ARCHITECTURE.md)、[TESTING](TESTING.md) |
| 完整任务验收与覆盖组织 | [环境验收](architecture/environment-acceptance.md) |
| Python 训练环境、安装包与批量接口 | [Python bridge](../bindings/python_learning/README.md) |
| Rust 跑局与对象交互 | [管道客户端](../tools/run_play/README.md) |
| dsh 原生游戏插件与脚本复核 | [dsh 游戏插件](../tools/dsh_player/README.md) |
| 原生玩家介入与回溯的候选设计 | [对局工作台](architecture/native-player-workspace.md) |
| 战斗搜索实现 | [搜索 owner](architecture/combat-search.md) |
| 本地网络与搜索实验 | [实验索引](../tools/guided_search/README.md) |
| 学习目标与评测 | [策略研究边界](architecture/strategic-agent.md) |
| 自主玩家的信息权限与研究图谱 | [研究路线图](research-roadmap.md) |
| 阅读机制和高手资料 | [本地资料入口](../reference/README.md) |
| 性能定位 | [性能工具](runbooks/performance.md) |

具体涉及身份、恢复或工作区时，再读 [Card Lineage](architecture/card-lineage.md)、
[原生存档](architecture/original-save-boundaries.md)、[决策工作区](architecture/decision-workspace.md)
或 [战斗证据](runbooks/combat-evidence.md)。

当前文档保存职责、使用合同和验收边界。一次实验的结果、进度和来源保存在 artifact 中；
退役说明由 Git 保存，不在当前目录另建 archive。
