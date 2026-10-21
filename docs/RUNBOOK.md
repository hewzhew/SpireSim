# 运行入口

Java ↔ Rust 的逐动作执行与原始采集由 `sts_parity_lockstep_runner` 拥有；完整任务用
`replay --suite` 离线复验。跨端职责见 [ARCHITECTURE](ARCHITECTURE.md)，验收范围见
[TESTING](TESTING.md) 与[环境验收](architecture/environment-acceptance.md)。

## 按任务选择

| 任务 | 入口与范围 |
| --- | --- |
| Rust 单机跑局 | `run-play.ps1`；状态、动作、分支与现存战斗搜索 |
| 对象式交互 | [tools/run_play](../tools/run_play/README.md)；直连同一 Rust 控制器 |
| dsh 原生跑局 | [dsh 插件](../tools/dsh_player/README.md)；官方插件页安装，选择“杀戮尖塔 A20H”预设 |
| 观察与介入原生 dsh 玩家 | 聊天标题旁的“对局”工作台；可暂停、返回保留点或回到刚才。维护者也可用 `node .\p observe` 汇总，`observe N` 定位调用，`observe live` 核对游戏 |
| 本机 dsh 服务 | 配置 `.oracle-lab/dsh-web.json` 后用 `dsh.ps1` 启动；前端热更新和后端重启的范围见插件维护入口；聊天标题旁的“地图”查看当前幕 |
| 脚本复核玩家 | 同一插件的诊断入口；`node .\p player` 查看，`player next` 调查，`player accept/replace` 复核后执行 |
| Java 控制或机制诊断 | `java-play.ps1`；CommunicationMod、原游戏观察与 typed 动作 |
| Run + RNG 差分 | `sts_parity_lockstep_runner`；逐动作对齐、自然轨迹采集与离线复验 |
| 定向战斗证据 | `ol.ps1`、`cs.ps1`；见 [combat evidence](runbooks/combat-evidence.md) |
| 性能测量 | [performance](runbooks/performance.md) |
| 本地学习试验 | [实验索引](../tools/guided_search/README.md)；明确构造任务与模型范围 |
| 当前卡组的离线遭遇诊断 | `p.ps1 deck-lab`；固定情景、完整状态搜索，不推进原局 |
| 机制或攻略查阅 | [资料条目](../reference/expert/curated/README.md)、[资料服务](../reference_service/README.md) |

PairedRoot 与 Rust workbench 保留为实验组件。使用前检查其当前接口和来源合同；它们的存在
不表示已经拥有跨端跑局或认证能力。Rust 私下搜索的结果也不产生 Java parity 结论。

自然公开 actor 的覆盖诊断可用 `public_actor_census --actor structured-bootstrap --ascension 20 --objective heart --seed N --output fresh.json`。
实验性的半规则玩家可把 actor 换成 `rule-search`，在同一入口、同一 seed 和预算下对照。它沿用公开战略规则，
只在直接伤害/格挡的当前手牌中搜索可见斩杀或致命攻击的解法；`rule_search` 计数记录改选次数。
默认仍为 A0／第三幕目标；`--ascension` 和 `--objective` 适用于这两个公开 actor。收据保留终局、战略候选，
以及已提交的逐战公开出牌、用药与下一画面资源；下一画面血量可能包含战后回复，战斗战损单独读取公开历史。
最新收据将地图决策的 `map_graph_id` 关联到去重的 `map_graphs`，保留当时公开的房间与连接；
旧收据可能没有这些字段。可达路线不说明问号结果、战损或整局策略质量。

## Rust 交互

```powershell
.\run-play.ps1 --help
```

`run_play new --session PATH --seed N --character silent --ascension 20 --goal heart-victory`
创建指定角色的自然 Rust 跑局；`--character` 支持 ironclad、silent、defect、watcher，默认 ironclad。
角色配置同时决定起始卡组、遗物、卡池和持久任务身份。

新建 Rust 跑局是 disposable scratch：当前楼层的决策可逐步回退；已完成的楼层保留奖励结算后的地图落点，
没有地图页的跨层转换在新层首个局面留落点。`back --floor N` 可直接回到保留的楼层落点。
楼层结束后旧的逐步操作会被裁掉；少数想比较的局面可用 `mark --name LABEL` 留在同一会话，
并用 `back --mark LABEL` 回去；不再需要时用 `mark --drop LABEL` 释放。构建源码摘要不阻止 scratch 继续；打开时若旧投影无法重建，
尝试从当前精确 checkpoint 原地更新。状态格式无法解码或精确状态无法恢复时明确报错，不把旧
轨迹认证为当前实现的证据。仅当任务涉及旧会话时读取 `overview`。
旧证据会话仍遵守原来源门禁，不作为日常跑局入口；要精选一处继续玩，使用 `adopt-current` 只提取精确状态/RNG，
生成 scratch 并可切换焦点。旧 `reroot-current`、`snapshot-current` 和 `reroot` 仍属证据诊断，
不用于日常更新。

普通选择、组合奖励、战术草稿与复盘的对象用法见 [客户端说明](../tools/run_play/README.md)。
`export-combat-case --session <path> --at n<archive-node-id> --output <fresh.json>`
可从仍保留的精确战斗落点只读导出 typed case；省略 `--at` 导出当前根，不移动会话进度。
每次返回的选项携带来源；购买、弃药、回退或推进后使用新对象，不按旧编号猜测动作。
当前公开地图用 `p.ps1 map` 看 Boss 和分组路线，`p.ps1 map full` 看 Boss 与全部公开节点及连接。
地图页默认选择后也附相同路线。按首个精英前营火/商店有无、绿钥匙精英是否经过与终点是否公开分组，
各组列出最少/最多精英的一条真实路径；同数量按坐标稳定选择。
普通房间用 `MM?` 连写，关键房间和岔口选中的出口标坐标；坐标用圆括号，路线分支用 `[A | B]`。
分支前的分叉房间与分支后的合流房间均标坐标，方括号表示两条路线的实际分叉和合流，
分歧段包含多只精英或只在 Boss 汇合时分行显示完整路线。
较长的共同后路可按一个真实合流房间拆成 `Approaches to` 与 `After`，各入口的精英、钥匙、商店
和首精英前准备概览仍遍历全部普通路径。每条列出的前路可接同一房间后的任意一条路线，
绕过该房间的路径单列；只在输出更短时采用，不删除原先保留的完整路径。
每条列出的路线均沿真实房间连接；其他连接可用 `map full` 查看，问号房间类型在进入时揭晓。
多个入口共用显示序列时，各入口都有真实连线支持；它们未必在省略坐标的普通房间已合流。
`p.ps1 map elites` 比较下一岔口各分支沿普通连线
的精英/绿钥匙数量范围及是否必经。精英最多/最少的路线各自标明首精英时点、此前普通战/问号数与前后营火，
并列出该精英后各路线首先到达的营火、下一精英或终点，以及通往各点的一条实际连线，
据此比较每个恢复点的实际走法和经过岔口后的可达范围。
`p.ps1 map route X Y` 显示从指定房间离开后的完整路线与后续岔口取舍。
查询起点仅作定位，不计作未来房间；单出口走廊仍属于未来路线，保留在样例及精英数量中。
每个出口显示全部普通后路的精英数量范围、绿钥匙是否必经和商店可达性；
后续岔口按全部后路的精英数量、绿钥匙、首精英前准备类型或商店可达性的变化列出。
同类路线中的其他连接用 `map full` 查看，未知后路保留 Unknown；查询不推进会话。
默认 `map` 从当前房间的下一节点起算，不把当前营火或商店重复计为未来准备机会。
地图选项保留原动作绑定；列出的路线从实际目的地开始，展示普通连线；Wing Boots 飞行另按当页实际选项处理。
商店可达不表示路上安全或必经，绿钥匙可达也不表示已经赢下该精英并领取钥匙。
`read full` 和 `map elites` 展开沿普通连接可达的已公开 Boss 坐标及首精英前的准备路径，
遇到 Boss 或公开/未公开端点也停止。商店/营火/本幕奖励的 `read full` 同样附这些示例。
每个终点与营火/商店有无组合列出一条连通路线，逐条给出普通战/问号数量、同路营火/商店坐标和完整路径；
同一摘要里分别可达的机会按实际连线组合。路线从各分支的选定节点开始，问号房间类型在进入时揭晓。
商店/营火/本幕奖励默认准备读数从当前房间的下一节点起算；幕间奖励按即将进入的幕判断，
`map route` 的显式起点按上述查询合同解释。
路线摘要用 `Elites` 标明后续精英数量，包含绿钥匙精英；`Burning elites` 单独标明其中的绿钥匙精英数量。
`burning elite unavoidable on this branch` 表示该分支的普通后续路径都经过该精英。Heart 钥匙需在第三幕结束前集齐，
已取得的钥匙跨幕保留；绿钥匙仍缺失时，第一至第三幕各自标记一个绿钥匙精英，后续幕仍提供获取机会。
路线里的 `map yN` 与网格坐标使用本幕零基行号；状态里的 `F` 保留整局楼层号。
未公开地图尽头标为 Unknown，不推断战斗损血、问号结果或未来飞行；完整连接表仍在诊断用的 `show`。

## Java 与实验组件

在非战斗决策时，`p.ps1 deck-lab` 从当前精确局面做小型遭遇实验；先用
`.\p.ps1 deck-lab --list` 查看本幕诊断遭遇及显式 Heart 试验，选择要检查的怪，再给每场转换、时间和情景数：

```powershell
.\p.ps1 deck-lab --encounter TheHeart --scenarios 3 `
  --work 200000 --ms 8000 --output .oracle-lab/strategic/<new-result>.json
```

此入口使用与玩家 `whole_combat` 相同的 `CombatSolver`，替换此前 CLI 的旧探针搜索。
`--work` 替代旧的节点预算。默认每情景 200000 转换、8000 毫秒，两者独立。

可加 `--after 'smith:PommelStrike#0'` 或 `--after 'buy:MembershipCard#0/buy:PommelStrike#0'`，
在私下分支执行当前合法准备并诊断其真实资源，原局保持不变。选择链需完成触发的卡牌选择；
旅行、推进和战斗留给普通玩法入口。随机准备只表示当前精确 RNG 下的一个结果，
准备后的战斗仍替换为固定诊断 RNG。与不带 `--after` 的诊断保持相同参数，比较准备收益。
终端显示每个情景的已重放胜线、终局与最低稳定边界血量、药水支出、工作和停止原因；
每个情景默认展开候选 0 的逐回合轨迹，显示实际血量、敌方血量、出牌和用药，
以及 End Turn 前的格挡与可见攻击量；每回合开始展示实际能量、格挡与起手各卡当时的费用。
回合起止各自列出带名字的敌方名单，动作目标也附执行前的名字。
召唤可改变 M 编号，起止名单不按编号配对计算敌方血量变化；其他候选的完整轨迹仍留在 artifact。
完整 artifact 的 `diagnostic_lines[].turns[].action_steps` 按动作顺序保留执行前后公开边界、
能量/手牌/能力/敌方名单、各牌堆数量及实际有序抽牌事件。`input_index` 对应整条线路的输入，
包括附带选择；边界同时保留回合号，结束回合后的抽牌可与当前回合区分。一次输入的记录包含
触发效果和战后回复，需按执行前后名单阅读，不能把全部变化归于单张牌；手牌数量变化也不等于抽牌量。
未找到胜线时显示一条实际失败线路，由最深已观察回合选择、同回合保留先观察到的线路，
不能据此判定全部线路失败。完整根、动作、各回合的手牌、能力和意图留在新 artifact。
目录随当前幕与公开 Boss 更新，第四幕同时支持矛盾精英与 Heart；`info rules` 展示同一目录。
默认药水支出上限为 0；用 `--max-potion-uses N` 比较当前库存，计入主动、被动消耗和弃药。
`--full-hp` 显式改为满血，不增加营火、升级、购物或跨幕回血，也不能单独隔离卡组。
当前遗物计数和药水保留；普通精英试验不继承当前位置的绿钥匙强化。
各情景使用从 1 开始的固定诊断 RNG，改变完整战斗随机性，不读取或消耗真实未来 RNG。
有 Power 的卡组先只生成种子 1..16 的起手概览，列出各 Power 起手出现与缺席的种子；
搜索情景仍是种子 1..N，预算和准备前后的种子配对保持原合同。概览只说明固定起手覆盖。
跨幕 Heart 试验只是用现有资源直接打 Heart，不模拟此前矛盾或后续发育，不推进原局。
固定情景的胜线比例不能读作自然胜率；搜索未找到胜线也不是弃局依据。

只为实机问题启动 Java，先说明要检查的机制和停止条件，尽量复用 resident JVM。
旧 mirror/checkpoint/suffix 是定向诊断表面，不承担新的 parity 主线。

`sts_parity_lockstep_runner` 记录原始 Java 稳定边界、采用动作、收据与 RNG 调用。
`replay --suite <suite.json> --output <new-report.json>` 按完整局基线、补充流程和回归的职责
复验已有来源；`replay-corpus` 复验本机 corpus。两者都不启动 Java。
套件的自然完整局范围包含 Neow 入口、各幕和心脏后的最后 Proceed；Python 训练调用另按
同一采用轨迹验收。机制脚本和原生恢复按具体问题查看 owner 的 CLI 帮助。

`probe-combat --session ID --repository PATH --through N` 从已核对的来源前缀私下搜索完整
战斗胜线。`--workspace W…` 从该来源下已保存分支的当前节点继续，核验与导出仍包含整个
战斗前缀；查询不移动工作区或 Java。与日常跑局共用整场求解器，`--work N --wall-ms N`
分别限定实际转换工作量和搜索时间；不把旧 Atomic 节点数当作转换数。
`--max-hp-loss N` 要求从搜索起点起的战损上限；未指定时接受完整胜线。
满足条件的方案优先保留较高终局血量，同血量再比较药水消耗和动作数。
只有显式 `--save-proposal` 才保存逐动作重放通过的采用方案，随后仍由原版逐步对齐。
预算耗尽保留 Unknown；来源不符或不完整的战斗不生成方案。
`--export-root-case PATH` 在搜索前另存所选来源前缀的标准精确战斗 case，拒绝覆盖已有文件；
即使搜索耗尽，该文件仍可交给[离线搜索诊断](../tools/guided_search/README.md)使用。
它保留来源坐标、生产者摘要和运行上下文；同时指定 `--workspace` 时，导出的仍是来源根，
不含私下分支前缀。诊断文件含完整内部状态，不属于公开玩家观察。

`sts_lockstep manual-start --seed <seed> --character silent` 从空闲的 resident oracle
新建指定角色的 A20H 对齐局；角色参数接受 `ironclad`、`silent`、`defect`、`watcher`，默认
`ironclad`。bootstrap、卡池和原生存档选择都绑定该角色。需要替换当前实验局时显式使用
`--replace-active`。各角色的实际机制范围由 adapter 合同和原始轨迹记录给出。

项目 [Codex 配置](../.codex/config.toml) 只关闭继承的 `runpod` MCP。
旧资料和 workbench MCP 前端已由 Git 历史保留；当前资料 CLI 与跑局 CLI 不需要重启客户端。

## 构建与检查

```powershell
cargo test --locked -p <owner-package> <filter>
cargo fmt --all -- --check
.\py.ps1 -m pytest <test-path> -q
git diff --check
```

Python 可显式指定 `-Python <python.exe>` 或 `-Environment <name>`。
学习环境的安装与 native bridge 检查见 [learning](../learning/README.md)。
按改动风险选择窄测和少量集成哨兵；文档或离线测试无需启动 Java。

## 状态与证据

真实 mismatch、动作不能对应、关键状态不可观察或来源不明确时，停止该条跨端轨迹并保留
最小反例。未认证本身不阻止 Audit；搜索预算耗尽保留 Unknown。

默认只展示游戏状态、选择和有用结果。完整 frame、RNG 账本、来源摘要和执行收据留在磁盘。
模型、数据、原始 Java 记录与唯一反例需要保留来源；`target` 下的编译缓存可以重新生成。
清缓存会增加下次构建时间，不会提高搜索策略质量。
