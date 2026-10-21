# Java ↔ Rust 当前架构方向

Java 与 Rust 通过共享 lockstep 核心逐动作比较；`sts_parity_lockstep_runner` 拥有原始采集、
离线复验和按流程组织的完整任务套件。高吞吐批次与 Python 训练调用分别按其实际执行范围
验收，完整任务合同见[环境验收](architecture/environment-acceptance.md)。

本文件只记录长期职责和验收边界，不记录 PairedRoot/workbench 的迁移历史、某次实机哨兵或
当前跑局进度。实际结果与生产者保存在 artifact；旧组件的独立能力不构成新的默认工作流。

## 要解决的问题

旧的顺序路线通常先让 Rust 完成很长的跑局，再由 Java 从头重放。Rust 搜索很快，但第一处
模拟器、RNG、身份或桥接错误可能直到 Java 长回放时才暴露，导致后续路线全部失去证明价值。

目标不是让 Java 参与 Rust 的每个搜索分支，而是把真实错误的发现距离缩短到下一次已提交
动作，同时保留 Rust 搜索速度、Java 原游戏画面和可复用的对齐证据。

## 目标层级与成果判定

项目的终极目标是获得一份高速、可批量分叉且对原版机制和随机性保持高可信的 Rust 模拟器，
供搜索、数据生成和后续训练使用。当前阶段的目标不是再次证明 Rust 能独自打通 A20H，而是把
Java 原版作为 oracle，高吞吐地发现、缩短并修复 Rust 的机制、状态、身份和 RNG 差异。

自然 A20H 跑局只是覆盖面较广的纵向集成载体。它可以暴露跨房间状态和长程 RNG 问题，但不能
单独证明未出现的内容已经对齐，也不是当前阶段的策略成绩。相应地：

- Rust-only 胜局只证明当前 Rust 规则下存在胜线，不产生 Java parity 结论；
- 搜索找到 replay-exact witness 只证明固定 Rust 输入上的可达性，不证明候选排序具有战略权威；
- Audit 跑到更高楼层只表示此前转换 Exact，不是独立里程碑；第一处差异才是该次开发循环的产物；
- Act Certification 只有在既有 Audit 轨迹已经闭合后才用于低频整体验收，不能承担日常找错。

任何实现若主要提升“把一条 Rust 路线搬到 Java 上跑完”的能力，却没有缩短差异发现距离、保存
可离线重解释的原始证据或提高同一 JVM 的 case 吞吐，就不属于当前主线。

## 候选主线：高吞吐提交路线差分

```text
typed corpus / 已采用的 SemanticAction
                  |
          +-------+-------+
          v               v
  resident Java oracle   Rust worker
          |               |
     raw boundary      current state
          |               |
          v               v
     generated/shared canonical projection
                  |
          per-domain digest
                  |
      Exact -> 下一步；Different -> 只展开首差异
```

“锁步”指稳定语义边界，不指渲染帧或毫秒时钟同步。Java 可以经过动画、过渡页和 action queue；
oracle 只在已经结算、可以作出下一次玩家选择的边界发布状态。一次性 runner 可以保留为集成
哨兵，但不得成为日常修复循环：一次 JVM 生命周期应执行多条用例；普通 Rust 或投影修改应先
离线复验，不应要求重新启动 Java。

Rust 可以搜索整场战斗，但 Java 只执行最终采用的动作。战斗内默认逐张牌、逐瓶药水和逐次
End Turn 比较；非战斗默认在每个真实玩家选择及其稳定后继比较。

## 统一分析者工作区与战斗规划

日常交互围绕同一来源局面与私下工作区。来源化选择、有限语义序列、分支和条件循环的
候选合同见[决策工作区与语义计划](architecture/decision-workspace.md)。短标签只属于产生
它的视图，执行时不得按新列表重新解释。计划在 Rust 私下展开为具体动作轨迹，Java 只逐步
执行已经采用的轨迹；该合同不表示现有 CLI 或 MCP 已具备完整能力。

战斗搜索的新候选方向见[模型引导搜索重设计](architecture/model-guided-search.md)。
新核心从共享模拟器的真实合法选择和稳定后继出发，允许模型参与动作提议与中间局面评估，
独立设计计算分配。不把现有评分、lane、药水阶段、完整回合生成器或旧搜索结果作为基础。
现有实现由[搜索 ownership](architecture/combat-search.md)说明，其内部算法不约束替代方案。

模型可以提出单个动作、有限前缀或跨回合打法。完整回合可以成为有用的展示与试验单位，
但不能强制模型先接受搜索筛选过的完整回合列表。每个中间选牌边界仍有真实状态和合法选择，
组合输入按需组装，不能为了开始探索先枚举全部回合或多选排列。长战斗的探索效率和模型
调用成本需要独立实验，不能从“分层”“树搜索”等算法名称推断已经解决。

对外保留少量长期语义，具体 CLI 或 MCP 拼写不属于架构合同：

- **draft/resume**：从当前来源继续单步、选牌或有限计划的私下试验；
- **inspect**：展开具体候选、分支差异与真实后继；
- **choose/undo**：采用明确选择或回到父节点，不改写其他分支；
- **certify/adopt**：复验明确采用的完整战斗结算；胜利与烟雾弹逃脱分别验证，逃脱不计为胜局。

普通输出以局面、决定和观察结果为中心。完整状态、搜索统计、hash 与 checkpoint 留在磁盘，
需要诊断时按来源句柄展开。前端不拥有第二份游戏状态或轨迹；搜索保留哪些进程内工作由
具体核心明示，不因旧实现曾有缓存就要求新方案兼容或迁移。

搜索可以决定私下探索哪些分支，预算和目标分开声明。模型建议及中间评价影响计算分配，
不取得合法性、结算或提交权限。实际死亡只证明该后缀失败；局部收益、较深前缀和预算耗尽
均不能证明完整打法成立。固定根的某条胜线有执行价值，但不证明其他分支较差或整局价值。

采用时从相同来源完整重放，保留实际资源与持久状态。战斗方案须到达当前代码验证的完整
胜局，或合法烟雾弹逃脱的存活空奖励页；后者同时核对房间 smoked 标记和无奖励，严格胜局
验证继续拒绝它。下一回合 segment 不取得 Java 提交授权；普通战斗如显式允许较短范围，
也不能冒充整场已解决。不得通过向 Java 逐回合试错掩盖搜索能力不足。

新策略实验只产生 Rust 结论。Java 继续通过共享 lockstep 核心逐动作执行已采用轨迹，保存
原始后继并比较；一个新搜索原型、一次策略提升或一个模型胜局均不能提升 parity 验收资格。

## 四档验证频率

日常开发必须优先使用成本最低、结论足够的循环：

1. **高频离线复验**：对已记录的原始 Java boundary 重新运行当前 Java projection，并从 seed、初始
   checkpoint 与 typed actions 重放当前 Rust。它回答 Rust、canonical schema、normalizer 或
   comparator 修改是否修复既有反例，不启动 Java。
2. **中频常驻 oracle 批次**：Java/Mod、观察能力或已有记录真正不足时，启动一次隔离 JVM，在同一
   owner 下执行多条 seed/constructed case。每条 case 独立结算和记录，单个 mismatch 不销毁
   整个 oracle。
3. **低频自然 Act Audit**：前两层覆盖稳定后才执行纵向集成，从 Act 入口沿一条自然轨迹发现
   第一处真实差异。它不因经过若干房间而发布中途成果。
4. **极低频 Act Certification**：只重放已经由 Audit 闭合的完整 typed trajectory，从一个 Act
   入口严格到下一个 Act 入口。它是里程碑验收，不是发现字段映射和机制 bug 的主要方法。

触发频率由证据变化决定，而不是由“代码刚改过”决定。纯 Rust 机制、投影、normalizer、比较器
或显示修改默认只进入第 1 档；只有原始 Java 记录缺字段、Java adapter/Mod 行为改变或需要新的
机制覆盖时才进入第 2 档；第 3、4 档必须由覆盖门禁明确触发，不能成为每次提交后的固定回归。

不能从原始 Java 记录离线重建的新字段属于明确的 evidence gap，只重录受影响 corpus；不得迁移
PairedRoot 长链或修改历史标签来填补。

## 原始记录与派生投影

Java oracle 的耐久事实是原始稳定 boundary、可用动作、已执行的 typed action、动作收据以及
producer/协议身份。`ParitySnapshot`、domain digest、首差异和显示文本都是可由当前代码重新
计算的派生物。

trajectory/corpus 因此必须保留：

- 原始 Java frame 与稳定边界身份；
- 前一边界绑定的 `SemanticAction` 与 Java receipt；
- Rust 的自然重建输入或最小 checkpoint；
- 游戏、Mod、schema 与输入生产者版本。

只保存旧版投影而丢弃原始 frame，会迫使 projection 小改也重新启动 Java；这种 artifact 不得
作为新 parity lab 的主要回归格式。

## 单一 canonical schema 与 digest-first

Run、RNG 等 canonical 状态的字段、类型、排序、稳定身份规则和 criticality 应由一份声明式
schema 拥有。优先生成或共享：

- Rust canonical 类型与序列化；
- Java capture DTO/字段合同或其机器校验器；
- per-domain canonical encoding、digest 与首差异路径；
- schema 版本和完整性测试。

endpoint-specific adapter 仍负责从各自真实对象图提取语义，生成代码不能假装消除这部分差异。
但字段名、JSON 外壳、排序、版本、hash 和 diff 不得在 Java adapter、Rust adapter、CLI、MCP
分别手写。

正常步骤只比较短 domain digest vector。digest 不同才加载该 domain 的完整 canonical value，
报告一条有界路径和值；原始 JSON 和完整 dump 留在 artifact。digest 是加速索引，不取代可重算
的完整证据。

## 共同动作

`SemanticAction` 表达玩家意图，不携带 Java 裸 UUID、Java 界面坐标、Rust 私有整数或显示文字。
两侧 adapter 分别把同一个动作解析到自己的当前对象：

- 事件：稳定事件 ID、当前页面和选项身份；
- 地图：稳定节点身份；
- 卡牌：来源可证明的 lineage/实例引用和目标；
- 药水、奖励、商店与篝火：稳定实体身份、槽位或页面内 occurrence；
- 系统自动转换：记录为 settlement，不伪装成玩家选择。

动作是否可执行、后继是否可比较、过去是否已经认证是三个不同事实。探索运行不能因为
“以前没认证过这种奖励”就拒绝一个两侧都能明确解析且关键后状态可观察的动作。

## 共同状态与 RNG

Java 与 Rust 不比较内部对象图，而各自生成同一版本的 `ParitySnapshot`。至少包含会影响确定性
继续运行的领域：

- run scalar、幕、楼层、角色、钥匙和资源；
- 完整地图、当前房间和公开动作空间；
- 主牌组、遗物与药水；
- 战斗牌区、能量、HP/Block、敌人、powers 与公开意图；
- 已声明的 RNG streams、状态和计数；
- 当前机制需要的持久对象身份与来源关系。

意图类别以原版为准；吸血等具体招式效果由执行机制负责，规则辅助独立解释。辅助从公开
观察能够确定的招式推导，并保留结构化来源，不加入 parity 状态或语义动作记录。隐藏意图
不能经由辅助说明泄露；试探所得或特权信息需要独立来源，不能标成原版可见事实。

决策阅读的公开导航图与地图比较输入具有不同职责。前者可将绘图端点规范为动作使用的 Boss
节点，并省去不可见的矩阵空位；后者必须保留声明范围内的原始房间与绘图连线。比较器不能
借用导航图，让阅读层的过滤或坐标改写掩盖地图生成差异。

每次转换还应增加 RNG 调用账本：owner、操作形状、参数、结果与 ordinal。平时只比较每个 owner
的滚动 digest；不同时展开第一条调用。只比较转换后的 RNG 状态可能较晚才能说明是哪一次调用
错位。RNG 只能由模拟逻辑推进，观察、显示和诊断不得消费它。

原版机制实际读取的外部环境量属于转换输入，不能混入状态投影或从结果反推。第三幕神秘传送门
的时间门槛在原版读取 `CardCrawlGame.playtime` 的位置捕获精确浮点位，绑定当前已执行动作及
相邻原始边界。共享核心将这份输入交给 Rust 的独立事件筛选流程，严格核对消费次数；原版时钟
照常运行。可能消费时间的转换只在准备阶段解析合法动作，收到输入后才计算 Rust 后继。
缺失输入是 `Unobserved`，明确未消费与未观察不能混同；输入不全或多读、少读都停止提交。
Live Audit、原始证据离线重放和连续整幕预检共用输入合同。Certification 重新观察本次原版
真实输入；若时间门槛改变路线，应停止，不能重放旧时间来强制原版匹配。

活跃 RNG 合同按后续是否仍可消费来确定生命周期。普通怪物与精英的战后奖励页已结束战斗，
下一战必经换房并重建 floor streams；因此只保留仍可能供奖励遗物使用的 `floor.misc` 与所有
run streams。事件奖励和 Boss 保守保留全部 floor streams，以覆盖同房间再次战斗。双端各自
从真实房间提取上下文，再调用共享生命周期规则；逐转换 RNG 调用账本不因此过滤。

自然跑局取得绿钥匙后，当前地图 RNG 的游标也结束生命周期：唯一的燃烧精英已走过，下一幕
会重新构造地图 RNG。原生读档重建地图时不再抽取绿钥匙位置，也不重放此前的精英强化抽取，
所以旧游标不属于可恢复的持续状态。绿钥匙取得前仍严格比较它；地图与所有逐转换 RNG 调用
继续比较，不能用游标退役掩盖地图生成或精英强化的调用差异。

结果至少区分：

- `Exact`：双方观察到的声明领域相同；
- `Mismatch`：双方都观察到但值不同，立即停止；
- `Unsupported`：动作或关键机制在一侧无法表示，立即停止；
- `Unobserved`：合同尚未观察某个领域。关键确定性领域为停止线，非关键领域只能形成覆盖缺口。

`Uncertified` 只表示此前没有正式验收，不能与 `Unsupported` 混为一谈。

## 双层运行：Audit 与 Certification

`Lockstep Audit` 用于日常 parity 开发：

- 允许第一次遇到、但两侧可执行且关键后继可比较的机制；
- 每个 committed action 后立即比较；
- 第一处真实 mismatch、unsupported 或关键 unobserved 产生最小反例并停止；
- 不移动正式选择器，也不因为跑得远而声称获得正式整幕证明。

`Act Certification` 用于正式验收：

- 输入是已经由 Audit 走通的封闭 typed trajectory；
- 从幕入口开始连续重放到下一幕入口；
- 声明范围内不允许 Unknown、跳步或人工改档；
- 全部转换 Exact 后只发布一枚 Act 级结论，逐步收据留作内部诊断。

途中事件、房间和战斗根是恢复与定位坐标，不是对外阶段终点。自然 Act 1 的验收终点是 Act 2
入口；单个事件或四层前缀不能冒充整幕验收。

最后一幕 Act 4 的对应终点是原版心脏胜利终局，包含击杀后进入 TrueVictoryRoom 的显式
Proceed。原生读档可以准备某幕入口，但准备证据必须在连续认证范围之外单独记录。

## Corpus、离线复验与恢复

parity lab 维护 append-only raw trajectory，并从中提炼最小 typed corpus。每个转换至少保存：

- 前一稳定状态身份；
- `SemanticAction`；
- Java 前后原始稳定 boundary，以及可重算的 `ParitySnapshot`；
- Rust 前后 `ParitySnapshot`；
- RNG 调用账本或当前可用的 RNG 状态证据；
- 比较结果与生产者版本。

修复 Rust 后，优先在不启动 Java 的情况下，从初始状态或最近 Rust checkpoint 重放 journal，
并用当前 projection 重新解释原始 Java boundary。Java 游戏、Mod 和原始观察合同未变化时，不应
只因 Rust、normalizer、schema 显示或 comparator 代码修改就重新录制整幕 Java。

corpus 不按跑局数量无限增长。保留项至少满足一项：增加机制/分支覆盖、代表新的 canonical
identity 族、或是既有 bug 的最短反例。相同覆盖的长轨迹应缩短或淘汰；corpus case 是测试程序，
不是可迁移 head、官方世界线或游戏存档收藏。

房间入口是首选耐久恢复点：保存原版 Java autosave、Rust checkpoint 和本房间 typed suffix。
正常运行保持 Java resident；若战斗中需要重启，则从最近真实 room-entry 恢复并只重放本房间
短后缀。原版存档没有保存的状态不能通过人工字段编辑伪装成 native recovery。

恢复 owner 必须区分真实 autosave generation 与当前页面。`POST_COMBAT`、`POST_NEOW` 和
`ENTER_ROOM` 根据原版保存语义关联到已记录的入口；异步写入尚未完成时不得封存上一代字节。
同一 generation 下的奖励、宝箱选择及跨幕动作形成已采用后缀，不额外获得原生保存点。
恢复前离线复验来源，Continue 后重绑身份并比较完整入口，再逐步锁步执行后缀；全部 Exact
才发布新的研究会话。必要的封闭界面准备动作保留原始收据，不能代替玩家决策或冒充连续整幕认证。

研究跑局的 `seeded checkpoint` 能力与 Human SL 分开：任一已经由 Java 与 Rust 共同到达并
`Exact` 的战斗入口，都可以封存当时真实的原版 autosave 与 Rust typed checkpoint，之后从
任意较晚稳定边界回到该入口并选择不同后缀。它不受原版 Save & Quit UI 的回溯范围限制，也不
宣称是原版玩家能力；恢复后必须重新绑定运行时身份并完整 `Exact`，才能继续提交动作。分支产生
的新后缀还要继续捕获后续战斗入口，不能退化成只允许第一次 retry 的一次性机制。

如果后来扩张观察合同才发现较早证据缺少关键状态，受影响前缀仍可能需要重验。该架构缩短并
明确失效范围，但不承诺任何旧证据永久有效。

原版不保存 `shrineList`，读档会重建本幕的初始神龛列表。因此不能假设任意战斗存档都能保留
已消耗神龛后的后续状态。标准第一、二幕的 Boss 遗物选择边界是一个明确例外：选择或跳过之后
只能前往下一幕，`dungeonTransitionSetup` 会先清空该列表，再由新幕重新生成。双端在这两个
边界结束旧神龛列表的比较，原始记录仍保留真实列表；正常房间、下一幕入口、一次性事件池及
其他运行字段照常比较。这个生命周期规则不授予其他缺失状态恢复资格。

## 组件与工具位置

| 组件 | 长期职责 | 当前资格 |
| --- | --- | --- |
| Rust simulator/search | 机制、合法转移、快速分叉搜索 | 可用组件 |
| Java bridge/adapter | 原游戏观察、typed 动作执行、稳定边界与 RNG 旁证 | 可用组件 |
| Comparator/projection | 双端公共投影和第一处分歧 | 可复用组件 |
| Raw trajectory/corpus | 原始 Java 边界、typed input、Rust 重建与最小反例 | Run + RNG 本地 typed corpus 已可按当前证据合同去重或替换失效 case；全域 shrink 待实现 |
| Java oracle host | 跨多条 case 持有一只隔离 JVM/bridge；不拥有比较语义 | Run + RNG 常驻批次可用；尚非完整 runner |
| Rust parity worker | 当前代码重放、projection、digest/diff 与 corpus shrink | Run + RNG corpus 与逐转换调用账本可离线批量重放；全域 shrink 待实现 |
| One-shot lockstep sentinel | 同进程持有两侧的窄集成哨兵 | 实验实现，不是日常入口 |
| `run-play` | Rust 单机玩法和搜索 | 不能产生 Java parity 结论 |
| `java-play` | Java 诊断与旧 mirror | 不是默认完整验收路线 |
| PairedRoot/workbench | 局部投影、证据和生命周期实验 | 实验组件，不是主流程 |

新 lab 应复用底层 Rust library、typed protocol 和通用 supervisor，不得通过拼接多个旧
PowerShell 入口来实现。稳定 owner 与高频 worker 分进程的目的，是允许 Rust 重编译时保留
Java oracle；它不是新的用户前端。入口名称和 IPC 可以更换，raw boundary、`SemanticAction`、
canonical schema、typed corpus 和 Act 级验收才是长期边界。

## 前端边界

当前维护 CLI 前端。前端只转发 typed 请求并呈现短结果，不拥有 Java 进程、Rust run、
journal 或正式 head，不复制领域 schema，也不从人类文本反解析动作。完整结构化证据留在磁盘，
诊断时再展开。项目 MCP 前端已退役，由 Git 历史保留。

## 仓库治理

- 当前已有实验性的 transport-independent 核心、Run + RNG 常驻批次与本地离线 corpus；仍不得称为
  完整的高吞吐 parity lab。
- 默认帮助和默认文档只展示权威入口。component、diagnostic、experimental 与 retired 必须分层。
- 旧入口可以贡献实现模块，但不能因为代码还在就获得当前执行资格。
- 每增加一个重叠入口，都必须同时说明替代对象和删除条件；不得长期增加平行 production surface。
- 退役设计由 Git 保存，不在当前文档树建立可被普通搜索命中的 archive。
- 动态进度、seed、性能数字和迁移日志进入 artifact 或提交记录，不进入本文件。

## Run + RNG 纵向试片门禁

在迁移 Map、牌组或战斗以前，试片必须同时证明：

- Run 与 RNG 字段只在一个 schema owner 中声明；
- 同一 JVM 能连续处理多条独立 case，单个 mismatch 不销毁 oracle；
- 已记录的原始 Java boundary 能被当前代码重新投影；
- 普通 Rust 修复可离线复验，默认输出只有 domain 状态或一个首差异；
- RNG mismatch 能定位到 owner 与第一条不同调用，而不只是末端 state；
- corpus 能按覆盖去重并保存最短 typed reproducer；
- 整个试片不依赖 MCP、PairedRoot、`official` 或旧 mirror plan。

任何一项只能靠迁移旧长链、复制第二套 schema 或扩大人工 JSON 才能成立，都应暂停并重新评估，
不能为了沉没成本继续扩张。

## 暂不决定

- parity lab、Java oracle host 与 Rust worker 的最终 crate/binary 名称；
- host/worker 的最小本地 IPC；
- 第一版 schema 是生成双端 DTO，还是生成 Rust 类型并机器校验 Java DTO；
- 最终公开动词；
- RNG 调用账本还需要扩展到哪些不经过原版 `Random` 基础方法的来源；
- 哪些旧入口在 Run + RNG 试片和自然 Act 1 通过后可以直接删除。

这些问题应由最小锁步骨架和自然 Act 1 证据回答，不能由旧架构名称提前决定。
