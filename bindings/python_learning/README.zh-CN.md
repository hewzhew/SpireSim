# SpireSim Python 接口

[English](README.md) | 简体中文

通过 Python 批量运行《杀戮尖塔》对局，支持四名角色、进阶系统与终局碎心。Rust 执行游戏规则，策略读取公开观察并选择合法候选。

## 安装

在[发布页](https://github.com/hewzhew/SpireSim/releases)下载与平台及 Python 版本匹配的 wheel，用训练时的解释器安装：

```text
python -m pip install PATH_TO_WHEEL
```

安装包名为 `sts-learning-bridge`，导入名为 `sts_learning_bridge`，依赖 NumPy。训练运行无需 Java 或原游戏安装。

预编译版本和完整轨迹复验命令见[安装指南](../../docs/getting-started.md)，其他平台可按[构建说明](docs/building.md)生成 wheel。

## 接入策略

```python
from sts_learning_bridge import LearningBatchEnv, semantic_schema

env = LearningBatchEnv([42, 43], 20, final_act=True,
                       player_class="silent", candidate_surface="legal")
batch = env.decision_batch(semantic=True)
schema = semantic_schema()
```

实现 `choose(batch, schema)`，为每个当前决策返回一个候选序号，然后交给现有[策略循环](examples/policy_loop.py)处理选牌、提交和终局：

```text
python bindings/python_learning/examples/policy_loop.py --policy my_policy:choose --seeds 42 43 --character silent
```

候选评分和运行限额见[接入示例](../../docs/getting-started.md#connect-a-policy)。

## 按用途查阅

详细接口文档使用英语。

| 要做什么 | 文档 |
| --- | --- |
| 创建批量对局、选择动作、获取奖励与恢复状态 | [批量对局 API](docs/api.md) |
| 理解公开观察、NumPy 表格与特征覆盖 | [观察与语义表](docs/observations.md) |
| 复验原版轨迹和已安装的 Python 包 | [轨迹复验](docs/verification.md) |
| 构建 wheel 并检查安装结果 | [构建](docs/building.md) |
| 使用固定战斗、构造局面或引导搜索 | [研究接口](docs/research.md) |

目录中的 `src/` 是原生绑定，`python/` 是 Python 导出，`examples/` 提供策略接入方式，`tests/` 检查绑定和轨迹。[原生模块职责](docs/building.md#native-code-owners)列出控制、观察、状态恢复与研究接口的实现位置。优化器与训练调用方在 [learning/](../../learning/README.md)。
