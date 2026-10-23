# SpireSim

[English](README.md) | 简体中文

一个使用 Rust 实现的非官方《杀戮尖塔》模拟器和训练环境，提供原版对齐工具，以及可选的规划与搜索能力。

支持铁甲战士、静默猎手、故障机器人与观者，以及进阶系统和终幕碎心。Python 接口提供批量对局、公开观察、合法候选、状态恢复、奖励与终局结果，训练运行不依赖 Java、Steam 或原游戏安装。

## 从这里开始

先看[安装与完整轨迹复验指南](docs/getting-started.md)，其中说明了预编译 wheel、可下载的比较记录，以及如何把自己的策略接到公开批量接口。

| 任务 | 入口 |
| --- | --- |
| 安装环境并接入 Python 策略 | [Python 接口](bindings/python_learning/README.zh-CN.md) |
| 在 Rust 中跑局、观察和规划 | [Run-play 客户端](tools/run_play/README.md) |
| 观察模型公开信息手打、暂停与返回 | [短段演示](docs/model-play.md) |
| 与原游戏比较执行结果 | [架构](docs/ARCHITECTURE.md)、[运行手册](docs/RUNBOOK.md) |
| 复验完整轨迹和已安装包 | [环境验收](docs/architecture/environment-acceptance.md) |
| 使用源码开发命令与本地快捷方式 | [开发脚本](scripts/README.md) |
| 查阅开发文档 | [文档导航](docs/README.md) |

## 与原游戏比较

`sts_parity_lockstep_runner` 负责共享的 Java/Rust 执行和重放流程。两端适配器解析同一语义动作，在稳定决策边界比较状态与随机数。采集记录保留原始 Java 观察、动作收据和生产者版本，使当前 Rust 实现及投影变更可以离线复查。

训练接口验收沿同一条已采用轨迹，通过已安装 Python 包实际调用 `choose` 和 `step`，检查选牌前缀、真实状态转换、公开观察、奖励、checkpoint 恢复和正式终局。完整轨迹、补充覆盖与回归用例的合同见环境验收文档。

## 许可证与游戏声明

项目代码采用 [MIT 许可证](LICENSE)，引入组件的署名见[第三方声明](THIRD_PARTY_NOTICES.md)。

这是非官方研究项目，与 Mega Crit 没有隶属或背书关系。
