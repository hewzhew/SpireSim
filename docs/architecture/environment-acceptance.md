# 环境验收

验收围绕完整任务、实际状态转换和可复用来源组织。主基线从自然新局开始，经真实决策抵达正式
终局。补充轨迹覆盖主基线未经过的情况；最小反例用于定位和快速保护具体修复。

## 共同执行与来源

原版事实由 `RecordedJavaEvidenceV1` 保存，包括 bootstrap、原始稳定观察、语义动作、收据和
生产者。当前 Java projection 与 Rust adapter 经现有离线重放核心逐步执行同一动作，比较
完整声明状态和本次 RNG 调用。通过的转换才进入覆盖汇总；首个差异沿现有比较器报告。

套件只引用证据目录，不复制原始观察，不读旧报告中的成功标记。实现位于
`crates/sts_parity_lockstep_runner/src/replay_suite.rs`，实际执行仍由该 crate 的重放核心拥有。
原始证据、当前比较结果、覆盖要求和角色分别保存，避免报告替代事实。

重放输入包含 seed、语义动作和转换时实际采到的外部输入。当前
`ExecutionInputsV1.secret_portal_playtime_bits` 保存事件筛选读取的原版游戏时间；离线重放通过
同一执行入口使用这些记录值。新运行读取自己的真实时钟，按自己的实际状态继续选择动作。
比较两次运行是否走同一条轨迹时，也核对这些输入；相同 seed 和动作序列本身不固定时间相关
事件的结果。原始输入随动作保存，不由覆盖报告补写。

## 情况与覆盖

| 维度 | 组织依据 | 验证职责 |
| --- | --- | --- |
| 完整生命周期 | 自然 Neow 入口、连续经过各幕、正式心脏胜利 | 确定主基线的起点和封闭终点 |
| 游戏流程 | 前边界、动作种类与 surface、后边界、前后幕 | 选择需要补充的选牌、购买、战后与跨幕轨迹 |
| 机制与状态 | 原始观察中的声明领域和对应机制合同 | 逐步比较规则效果、持续状态、身份与 RNG |
| 训练调用 | 公开决策、选择前缀、真实提交、观察与奖励 | 检查实际训练接口执行同一条采用轨迹 |

流程覆盖从当前精确重放产生。一行保存一种实际转换及其首次动作位置和出现次数；重复经过
同一种流程增加次数，不增加覆盖行。它不以文件数、测试函数数或房间访问量换算机制覆盖率。
具体机制仍由已有规则测试和原版证据合同验证。

一次 Python `choose`/`step` 可能只推进选牌前缀；一次语义动作也可能包含原版 UI 的中间操作。
调用次数、语义提交数与实际 Java 动作数分别记录。Python 接入必须使用其真实公开候选和提交
入口，并在真实游戏提交处与同一轨迹对照；直接调用底层 Rust adapter 不能替代 Python 路径。

## 套件合同

`sts-parity-replay-suite-v1` 显式列出 task、cases 和 required_flows。每个 case 有唯一 id、
purpose、role、evidence 路径和自身 required_flows。相对来源路径按套件文件所在目录解释。

三个角色的执行都复用同一重放核心：

- `baseline`：从 Neow introduction 开始，连续经过第一至第四幕，到击杀心脏后的正式终局；
  击杀后的 `Proceed` 在范围内。中途根、单场胜利和成功短前缀不满足该角色。
- `supplement`：为指定流程提供补充；自身要求必须由本条轨迹满足。
- `regression`：复验一个具体修复的来源；不能替代完整主基线。

套件至少包含一个 baseline。所有 case 均须满足相同角色、难度和 Heart 目标；每个 case 的
精确重放、角色范围和自身要求都通过后，才贡献套件要求的覆盖。一个 case 的错误不阻止其他
独立 case 复验。重复 id、重复来源和重复要求直接拒绝。

流程要求使用实际稳定边界及现有语义动作名称。例如，最后一次继续的形状为：

```json
{
  "before": "natural_true_victory_complete",
  "action": "proceed",
  "surface": "true_victory",
  "after": "natural_heart_victory",
  "from_act": 4,
  "to_act": 4
}
```

`select`、`submit_selection`、`proceed`、`cancel` 的 surface 必须明确；其余动作的 surface
为 null。要求应对应需要验证的情况，不按 seed 复制一套相同列表。

## 执行与结果

单条证据保留现有入口；套件使用同一个 `replay` 动词：

```powershell
.\target\dev-opt\sts_lockstep.exe replay --evidence <java-evidence>
.\target\dev-opt\sts_lockstep.exe replay --suite <suite.json> --output <new-report.json>
```

离线执行不启动 Java，不修改原始证据或游戏 head。报告只能写入新文件。
输出保留每个 case 的真实起止位置、精确重放结果、角色检查、流程见证和首差异。
套件通过要求所有 case 通过、所有套件要求有实际见证；失败返回非零退出码。

新增 case 时说明它补充的情况或错误。完全重复的来源只引用一次；具有相同流程覆盖的轨迹
仍可能覆盖不同机制，淘汰时同时检查机制合同。长期保留项由新增覆盖或独立的快速回归价值
决定。实验结果、选用的 seed、耗时和进度进入 artifact。

## Python 的同轨迹执行

`replay --suite` 拥有原版记录与当前 Rust 的连续复验。`replay --evidence PATH
--learning-output NEW_FILE` 在同一个重放核心上，解析每次已核对的 typed 原生动作，映射到
训练接口的公开候选和选牌前缀，再由独立的学习环境执行。它检查每次游戏提交后的延续状态、
公开观察、候选顺序、奖励和终局。产物保留来源目录、原版生产者和语义动作对应关系。

含原生恢复或重试的会话使用 `replay --session ID --repository PATH --learning-output NEW_FILE`。
它复用原生恢复的来源链重放，按 typed 父前缀从 Neow 验证到当前采用终点；不从诊断 checkpoint
直接跳入训练。格式 2 保存各段生产者、逐段复验结果与全局动作偏移，连续覆盖由同一覆盖器
计算。Java 的原生读取准备计入原版执行数，不转换成训练决策或额外奖励。
`--through N` 固定所选会话本段的 N 个已采用动作，祖先段仍按 typed 父前缀截断；后续继续
推进同一会话时，已有课题可按原范围复验。上限超过已采用范围或截在未提交的打铁界面时停止。

训练的 `legal` 候选面提供完整合法选择；既有 `policy` 面保留任务筛选。验收使用 `legal`，
不替调用者改领奖顺序。卡牌选择沿 shared decoder 的 typed lineage 逆映射，不枚举组合，
不按名字或位置猜测。原版连续的“打开打铁、提交卡牌”对应一次训练游戏提交；打开界面本身
不作为 Python 游戏状态转换。其余不能一一对应的操作明确停止。

[Python 验收入口](../../bindings/python_learning/tests/trajectory.py) 每次先调用该原始重放，
再让安装后的 Python 包实际执行全部 `choose/step`。每个选择前缀核对公开 snapshot 与候选，
`choose` 期间确认游戏未推进，`step` 后核对实际动作、状态、奖励和终止列。结果保存预期轨迹、
Python 实际提交日志、包版本和 native 模块 SHA-256；失败保留首个差异。

同一入口的 `--stagger-commits` 按提交偏移建立批量恢复课程。每个 checkpoint 都由对应槽位
实际执行采用前缀产生，再按原槽位恢复；至少一个槽位从 Neow 开始。不同进度要求来源有真实
终局，已完成槽位由环境正常退出 active rows。每个决策行分别对照自身提交和选牌前缀，
实际转换的路径分段、奖励及终局列按槽位核对。准备调用与验收调用分开记录。

`--reject-extra-input-once` 在多槽位批次中给最后一个 active row 多传一项外部输入，
核对整批拒绝后的精确状态和待提交选择，再提交有效输入。来源含记录时间输入时，在该边界
执行这一检查；普通批次在首个有多个 active rows 的提交检查。

外部时间输入从原版执行收据传入同一次真实训练提交，并要求完全消费。坏输入或消费数不符
撤销整批私下执行，保留原局和待提交选择。无外部输入参数的普通训练继续使用自己的环境输入。
延续状态哈希只供验收诊断，不进入模型特征。具体调用见 [Python bridge](../../bindings/python_learning/README.md)。

## 跨平台安装包

先在原始来源可访问的一端用上述入口重新导出 `expected.json`，保留同次验收报告中的
`expected_sha256`、重放执行器摘要和原版来源。另一端安装同一份源码构建的本地 wheel 后，
使用同一 Python 验收器的 `--trajectory PATH --trajectory-sha256 SHA` 执行该冻结导出。
摘要在创建环境前核对，实际选择、提交、恢复和终局继续由相同核心检查。

接收端报告记录导出摘要、来源生产者与实际导入模块，类型为安装包对冻结轨迹的复验；
原始 Java/Rust 差分由导出端的本次重放拥有。导出文件按引用保留，不在各平台的产物中复制。
该路径允许 Windows 和 Linux 使用各自的 Python ABI 与 wheel，并共享同一条来源轨迹。
