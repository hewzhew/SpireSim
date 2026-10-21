# 实机观察回归样本

这些文件是从启用 SuperFastMode、已完成自然认证的 Java 记录中逐字节复制的六个稳定边界
和一个动作收据。
[sources.json](sources.json) 保存原始来源、内容摘要、Java producer 与认证 worker 版本。
测试校验原始字节和 producer，再调用当前实现解释它们；普通测试不依赖本机
`.oracle-lab`、Java 进程或原游戏安装。

运行仓库已有测试入口即可：

```powershell
cargo test --locked -p sts_parity_lockstep_java --lib observed_regressions
```

| 样本 | 保护的事实 | 范围 |
| --- | --- | --- |
| [Flight](reduced-flight.json) | 受击后当前层数为 2，真实 storedAmount 仍为 4；缺字段不能猜测 | Java power 投影 |
| [Invincible](exhausted-invincible.json) | 剩余额度为 0 时保留 power，独立 reset maximum 为 200 | Java power 投影 |
| [Haste](time-eater-haste.json) | 原版发布 Buff 意图，没有公开治疗量 | Java 原始意图与构造输入下的 Rust 怪物计划及适配器 |
| [Heart 入口](heart-entrance.json) | 原版特殊坐标在 map、room 中指向同一 Heart 节点 | Java map/room 投影 |
| [Proceed 前](heart-proceed.json)、[后](heart-terminal.json)、[收据](heart-proceed-action.json) | 击杀后仍有独立 Proceed；后继是另一个胜利房间，只有 host 管理动作 | 相邻边界、动作适配及收据连接 |

这些是有界的观察和机制回归，不是独立完整跑局或新的整幕认证。尤其：

- Haste 的 Rust 侧使用明确的构造输入，仅比较公开意图，不冒充原局重建，也不证明治疗执行。
- 其他样本不省略证明所需的卡牌来源后再声称全域 Exact；这些测试根本不建立卡牌身份绑定。
- 原始完整 boundary 得以保留，便于后续重解释，但当前测试只保护表中列出的范围；
  完整轨迹的牌组来源、所有动作和 RNG 仍由共享轨迹 replay 负责。
- 负例只修改测试内存副本。不得改写这些原始文件来让失败消失；替换样本时应同时审查
  新来源、协议和所增加的机制覆盖。

样本不携带存档，不取得恢复、搜索、执行 Java 动作或更改 active session 的权限。
