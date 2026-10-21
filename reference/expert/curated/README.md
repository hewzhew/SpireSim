# 整理后的策略知识样本

这里保存跨来源消化后的条件经验与决策方法。日常读取只访问整理结果；原文留作离线核查。
本样本由模型阅读来源并综合，尚无人工复核或实战增益证明，也不替代游戏机制与 parity 证据。

## 铁甲战士选牌

[知识包](ironclad-card-reward.json)包含八个可单独读取的主题：近期危险、未来收益、过渡、
过牌、费用、跳过、升级竞争、消耗启动。来源与具体记录在包内固定，原文没有复制进包中。

默认阅读显示判断要点、要看的条件和可能改判的情形。`--detail reasoning` 再显示来源取舍、
推导与输入；`--detail provenance` 继续展开原始记录的位置。JSON 中始终保留完整字段。

| 字段 | 含义 |
| --- | --- |
| principle | 本条帮助分析者考虑什么 |
| check | 采用此考虑前需要确认什么 |
| unless | 哪些情况可能反转判断或限制推论 |
| disagreements | 作者之间的差别、单来源限制或综合解释范围 |
| inputs | 可观察事实与仍需分析的条件，显式分开；这些不是自动状态提取器 |
| reasoning | 整理者的推导，不能冒充作者原句 |
| assertions | 指向经阅读的来源记录及其简短转述 |

机制事实不塞进经验类别。文中的卡牌作用只是研究对象与来源讨论；需要确定的规则应向已有
机制 owner 查证。作者的单卡排名、固定金币门槛与自述胜率没有被转成动作分数。

## 铁甲战士整局规划

[路线与资源知识包](ironclad-run-planning.json)将相互关联的资源放在一起，按主题选择读取：

| 条目 ID | 关注的问题 |
| --- | --- |
| route-options | 高收益路线的准备阶段与坏结果退路 |
| readiness-horizon | 最近危险、启动缺口与未来收益的兑现时间 |
| hp-recovery | 花血、恢复、升级与后续路线的竞争 |
| potion-reserve | 药水具体用途、槽位与已知/未知补给 |
| shop-tradeoffs | 购买、删牌、留钱与经过商店的路线成本 |
| curse-cost | 移除前负担、费用、绕路与被挤掉的删位 |
| replan-after-relic | 新能力与资源限制如何改变原有计划 |
| endgame-reserve | 后段连续战斗与药水/金币储备 |

这些方法可在同一局的不同战略页面参考，不为每个页面复制一份经验。包的 V2 `scope` 使用
明确的 `character` 与 `decision_sites` 列表；唯一的范围判断在 `CuratedStrategy.supports_decision`
中。范围匹配只表示允许咨询这个包，不证明每个条目适用，也不表示该页面已有完整策略覆盖。
V1 选牌包保留单个 `decision_site`，两种范围格式不能混用。

跨来源综合保留多精英路线的风险偏好、删牌与即时购买的竞争，以及蛇眼和贤者之石评价上的
分歧。作者的金币门槛、连胜自述和 SL 做法不成为规则、训练标签或执行权限。
[对照试读案例](../../../tools/expert/tests/fixtures/run-planning-cases.json)在相同候选下改变资源与约束，展示关注点怎样变化；
另有未知补给案例。它们是分析者撰写的构造情景，没有模拟结果或最优动作标签。

## 读取

以下命令在仓库根运行，复用现有专家工具 CLI。空读取只给短目录，不要求自然语言问题。

```powershell
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --entry future-value --entry skip-density
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --entry future-value --detail reasoning
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --entry future-value --detail provenance --json
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --package reference/expert/curated/ironclad-run-planning.json
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --package reference/expert/curated/ironclad-run-planning.json --entry route-options --entry potion-reserve
```

`brief`、`reasoning`、`provenance` 是同一个核心的不同展开程度。来源展开返回固定记录位置和
整理者的转述，不自动取回原文。普通读取不加载原始攻略、Wiki 数据库或模拟器实体目录。
它只表示知识包结构有效；当前局面的适用性始终为 `unassessed`。

库接口 `CuratedStrategy.read(ids, detail=..., known=...)` 可以批量读取明确的条目。
只有调用方明确表示某次阅读仍在上下文中，才通过 `known` 提交阅读回执以省去重复正文；未提交
回执总会返回内容。展开程度、知识内容、所引来源说明或适用范围变化都会使回执失效。
包在进程中修改后，下次读取会自动加载新内容。上下文压缩后不要仅凭旧回执假定内容还在。

## 从公开战略局面读取

Rust `run-play` 的现有 `show` 可以把当前战略决策导出一次，之后展开牌组、地图与知识均不再
打开游戏控制器。传入一个尚不存在的输出文件；原会话不会因资料读取推进。

```powershell
cargo run --locked -p sts_run_play --bin run_play -- show --session <session> --public-output <decision.json>
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json>
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view deck
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view context
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view candidates --candidate 0
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view mechanics --candidate 0 --candidate 2
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view mechanics --card 0 --card 3
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view relics
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view mechanics --relic 0 --relic 2
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view map --entry near-threat
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --entry future-value --entry skip-density
```

V3 导出支持战略决策的公开读取，包括选牌、奖励、地图、事件与 Neow、商店、篝火、Boss 遗物、
宝箱和牌组选择。默认给资源、页面概要与全部 owner 候选；`deck/map/relics/history` 按需展开。
`context` 展开奖励内容或完整公开商店库存，包括当前买不起或被阻止的商品；可见商品不自动成为
可执行候选。`candidates --candidate N` 展开选定候选的详细效果或选择范围，保留原序号与身份。
`mechanics` 按候选展开卡牌、药水或遗物的已审阅机制；其他动作不据此获得机制全覆盖。
`relics` 提供已持有遗物的完整短目录；`mechanics --relic N` 按指定顺序展开其当前观测状态与
部分机制，真实计数与定义初始值分别保留。`context` 也能展开公开库存或奖励中的遗物，包括
买不起的商品。可见性不授予购买或领取动作。

候选、牌组与已持有遗物使用各自的阅读序号，`--candidate`、`--card`、`--relic` 同次调用
不能混用；序号只绑定本次导出文件，不是跨局面身份或执行句柄。文件变化后须重新读取目录。
派生视图使用 `sts-public-decision-reading-v2`，其中遗物目录仅含序号和实体；旧派生视图需从
原始公开导出重新读取，不作迁移。原始导出合同保持独立。

地图路线的短说明给出沿公开普通连线首先可能遇到的路标：营火、精英、商店、宝箱、Boss、
钥匙或记录端点。展开某个路线候选时，Rust 将该目标可达的图压成连续路段，保留全部连线、
经过顺序、分叉、合流与节点坐标；同坐标的路段相接，不能把不同分支的收益拼成一条路线。
这避免枚举所有完整路径，也能用于开局入口和当前飞行候选。后续飞行、问号结果、战损和能否
休息不从连线推定；记录中未展开的端点保持 Unknown，不能当作 Boss 或通关。
原始 `map` 视图继续提供全量公开拓扑。旧导出的说明保持原样，不用当前实现为其补造新说明。

公开拓扑表达导航关系：普通幕的末排连接到 Rust 动作使用的 Boss 节点，未连接的矩阵空位不
成为可访问房间；Act 4 保留特殊地图自身的 Boss 位置。抵达胜利页由真实流程决定，不补造
Boss 到胜利页的地图边。用于跨端比较的原始房间与绘图连线另有独立输入，不能因阅读方便而
被过滤或改写。旧公开导出仍保留其记录时的地图与来源，不用于恢复当前执行。

```powershell
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --view candidates --candidate 0
```

选牌默认附送选牌包的短目录，其他页面默认只读公开事实；整局规划包由分析者显式选择。
需要知识时提供相符的 `--package` 与条目，不把选牌知识自动套到其他决策类型。
只请求知识条目时不重复局面；显式展开局面或机制时不再附送知识目录，仍可用 `--entry` 组合读取。
不请求知识的视图不依赖知识包存在。
角色与决策类型检查只确定知识包范围，条目适用性始终待分析，不自动选条目、评分或执行。
地图只呈现公开节点与连线；未知 Boss、未知战损记录保持 Unknown，不推测未来遭遇或基础费用。

例如在已导出的地图上，只读取路线退路与血量规划，不重复附送整张地图：

```powershell
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m tools.expert.cli curated --decision <decision.json> --package reference/expert/curated/ironclad-run-planning.json --entry route-options --entry hp-recovery
```

导出调用现有 `TaskPublicDecisionV1` 校验；完整 public observation、candidate IDs、snapshot、
来源坐标与 producer 版本留在文件。读取校验文件完整性，不重新实现 Rust 的语义身份算法。
完整性不是来源认证，也不证明导出后 head 仍相同。普通文本明确这是离线局面；显示序号只供
阅读，不能直接作为执行许可。执行前仍须回到游戏 owner 检查当前观测与候选。

分析者提出具体方案后，现有 `run-play preview/compare` 可在当前合法运行起点上私下预演明确的
动作序列。商店方案默认展示每条序列的资源、牌组、药槽与剩余候选数量；`--detail` 展开逐步
结果、完整库存及候选。省略展示不筛选动作，也不省略保存在结构化证据中的完整后继。
预演不改变正式 head，不搜索或决定采用哪条方案；花费与已观察到的后继不是长期收益证明。

现有候选合同是 `deployable_policy`，可能窄于原游戏全部 UI 动作。读取器保留这个范围及其
提供的全部候选，不把同名不同升级牌合并，也不漏掉跳过、颂钵或丢药。V3 的短说明由 Rust 对
typed 候选生成，逐候选摘要及完整观测摘要绑定说明来源；Python 不从说明文字猜动作含义或
重建可执行动作。缺行、重复身份或说明绑定错位时明确失败。旧 V1/V2 仍按原选牌范围只读，
不会根据当前资料补造它们缺少的说明。此接口为 Rust-only，未接入当前 Java 手动研究会话。

### 机制的范围

V2 公开导出附带 `sts-card-acquisition-reading-v1`，与同一个 public snapshot 和每个拿牌动作的
candidate ID 绑定；读取器还检查具体卡牌 ID 与升级次数。它调用共享的
`card_acquisition_mechanics_facts_v1`，Python 不维护另一份卡牌效果表，也不解释策略分数。
V1 导出仍可作为离线局面读取，但不补猜机制，不能据此恢复执行。

机制视图是 **部分覆盖**：卡牌定义提供费用、目标、消耗/虚无/固有与基础伤害/格挡字段；已审阅
效果单独提供伤害用途与请求范围、无条件抽牌请求、基础格挡的生效方式、敌方减力与返还安排、
出牌 HP 损失和每回合末格挡。基础字段不是实际结算：这些规则只展开已审阅的条件、动态值和
次数，触发、实例变化与获取卡牌时的遗物修正未完整展开。缺失效果为
Unknown，零值不证明没有动态或条件效果；空的策略模型缺口也不能充当机制全覆盖证明。

V3 可选的 `sts-potion-acquisition-reading-v1` 同样绑定原 snapshot 和每个相关候选，读取器核对
药水 ID。购买、领取、丢弃与主动使用保留不同含义，药水说明不把购买或领取当作已触发效果。
当前可展开血药的最大生命百分比治疗、弱化药的目标虚弱、液态铜的荆棘与灵药的手牌消耗，
并复用已有发现药水的卡池、选择、生成副本和本回合费用合同。数值按基础药效，遗物修正、
使用边界和后续队列另算；精确游戏规则可接受的零张选择也不保证自动候选会保留。战外基础
使用资格不代替当前合法性检查。旧导出缺此表面时保持 Unknown，不按药水名称补写；其他
物品的机制覆盖独立声明。

V3 可选的 `sts-visible-relic-reading-v1` 汇集已持有遗物、明确候选、商店库存和奖励中的可见
遗物定义，与原 snapshot 绑定；定义按 typed RelicId 连接，保留各处原有候选、价格、阻止原因
和实例计数。当前展开铜鳞的开局荆棘、Sling 的精英战开局力量、日晷的洗牌加费，以及燃烧之血
与黑暗之血的战胜治疗。触发条件和数值来自共享 owner，策略分数不进入读出。日晷的定义初始
计数与已观察到的实例计数分开，跨战斗保留进度，不因读取而重置或补猜。缺失定义或未审阅的
效果保持 Unknown；这不是遗物全部效果、获取时副作用或未来战斗收益的保证。

遗物资源读取也展开共享的每回合基础能量增量，以及咖啡杯的禁休息、融合之锤的禁营火升级、
添水的禁获新药和天鹅绒项圈的每回合出牌上限。限制保留作用范围：其他恢复与升级来源独立，
已有药水的使用资格独立，零费牌仍计入出牌数。候选效果不成为当前已持有能力，也不自动选择
遗物；可组合 `replan-after-relic`、`hp-recovery` 或 `potion-reserve` 按当前资源判断。旧导出缺少
这些字段时仍为 Unknown，不按遗物名称补写。

血腥神像的治疗数值与战斗、整局金币路径共用定义：每次实际获得正数金币时立即请求一次治疗，
不按收入金额乘算。灵体阻止收入时不触发；最大 HP、治疗修正与禁止治疗仍限制实际恢复。
读取不预估未来收入，也不把这些潜在治疗提前加入当前 HP。

蛇眼的额外起手抽牌与战前混乱请求分别表达。混乱费用规则来自同一运行时 owner：按当前战斗
费用非负判断资格，状态与诅咒不被统一排除；掷出相同战斗费用时保留已有本回合费用，符合资格
的抽牌都会清除一次免费标记。额外抽牌不保证实际抽到固定张数，混乱被阻止或清除后也不能继续
套用费用随机。它们都是机制事实，不代表蛇眼对当前牌组的策略收益。

格挡生效方式区分一次出牌请求、抽牌/当前格挡条件、按非攻击手牌或 X 重复、可选多层护甲、
弃牌堆动态数值与愤怒加成。普通牌面修正和 No Block 等阻止仍由运行时处理。旧机制导出缺少
这一可选字段时保持 Unknown，不默认成一次格挡；策略的稳定防御与牌组统计也使用同一分类。
条件效果保留独立理由，具体牌组的燃料等前提仍由相应分析判断，不能从有这个机制推导出必然收益。

伤害规则区分单目标、每敌人和逐段随机目标，固定次数与 X、手牌、充能球、历史及实例记录的
依赖分别保留；Wish 的定义伤害字段明确表示力量选项。策略使用已审阅的基础伤害贡献，不能把
随机分散伤害充作集中攻击，或把未知次数默认为一段。有条件的伤害能力仍可参与配合分析，
但不凭空获得固定总量。旧导出缺少伤害规则时保持 Unknown。

力量读取保留生效条件与时序：燃烧的出牌加力、观察弱点在效果结算时对攻击意图的检查、活动
肌肉另挂回合末失力，以及恶魔形态在后续回合开始抽牌后加力，不能互换成即时无条件收益。
观察弱点的条件允许零伤害攻击，并非限制打出卡牌；意图不可见时仍需保留信息限制。升级数值
来自同一机制 owner。突破极限的倍增仍未展开，旧导出缺字段时不按牌名补猜。

加费读取区分出牌请求、狂暴的后续回合开始、抢占先机的下次能量补充、哨卫自身被消耗，
以及飞踢结算时目标易伤的条件。数字是总加费请求，不是减去打出费用后的净收益；献祭的
失血与抽牌、狂暴的自易伤、涡轮增压生成的 Void 等已展开代价与加费一起保留。运行时与
资料事实复用同一份数值规则，肾上腺素的加费升级不被误当作抽牌升级。
状态牌说明保留生成数量、去向与虚无属性，不将它们加入永久牌组。倍增、X、弃牌和充能球
依赖的加费仍为 Unknown；这些读取不是完整的费用预测或卡牌效果说明。旧导出缺字段时不补猜。

延迟效果单独保留代价和后续义务：自燃每次施加的回合末伤害与失血分别累加，先请求合计
失血，再请求全敌非攻击伤害；黑暗之拥安装消耗触发的抽牌能力，不能当成出牌即抽牌。
战吼的抽牌与放回一张手牌到牌堆顶是前后独立动作，No Draw 阻止抽牌也不免除后者；空手
跳过、一张自动、多张不可取消地选择。此规则不覆盖深谋远虑的不同前置条件。数值与运行时
共用机制 owner，升级保留伤害、抽牌或费用各自的变化；战斗收尾、抽牌限制与队列中断仍由
运行时决定。这些是已审阅的局部效果，旧导出缺字段时保持 Unknown，不按卡名补造。

后续效果保留事件与顺序：震荡波按敌人列表顺序，对每个敌人先请求虚弱、再请求易伤，分别
接受 Artifact 等阻止；撕裂安装按合资格实际失血事件加力的能力，不按失血量乘收益，也不因
敌人伤害触发。多份自燃合并成一次失血事件，完全阻止失血则不触发撕裂。
Secret Technique 打出时要求抽牌堆含技能，排队效果在结算时重新检查抽牌堆：零张跳过、一张
自动、多张不可取消地选择。此前触发的抽牌可能改变待取牌；不能在出牌时固定单张身份。No Draw
不阻止直接移动，满手放入弃牌堆，保留费用。Anger 在伤害之后排队复制自身到弃牌堆，保留
升级与实例修改并接受生成修正，只增加本场牌量；战斗结束清队列可能阻止复制。上述已审阅
请求与运行时共用定义，选择和生成仍由现有 owner 结算；它们不表示整张卡已完整覆盖。

下一升级沿用模拟器的真实升级资格及可表达次数，包括灼热攻击的重复升级。只展示已投影字段
的变化，不暗示整张卡的其他效果都不变。机制来源、结构与局部测试不构成 Java parity 或策略
收益认证。完整数值和身份留在导出文件，普通阅读只输出选定候选的短说明。

### 已有卡牌与升级比较

V3 的可选 `sts-owned-card-reading-v1` 从公开牌组实例生成机制与下一次卡牌自身升级的读取。
先用 `deck` 查看牌组序号，再用 `mechanics --card N` 选择一张或数张；无需把卡名或修改值写进
自然语言问题，也不会默认展开整副牌。明确的 smith、toke、remove-card 候选会连接到其公开
牌组实例；`--candidate` 与 `--card` 分别使用候选序号和牌组序号，同一次调用不能混用。

实例数值与普通定义分别保留。运行时和读取共享持久伤害、格挡、misc 记录、费用以及升级规则，
不编造 lineage，不使用战斗中的临时显示数值或免费状态。同名卡的不同永久修改不会合并：
例如降至 0 费的 Corruption 自身升级会设为 2 费，不能套用普通卡的 3→2；永久改成 20 基础伤害
的 Strike 升级后为 23。灼热攻击保留再次升级，仪式匕首和基因算法保留已成长的实例数值。

升级读取不是营火或事件完整转移预演，不包含相关遗物、房间副作用，也不授予当前升级、移除
或选择权限。伤害/格挡字段仍按已展开的次数、条件和动态替换解释；没有展开的效果为 Unknown。
记录绑定当次 snapshot、公开牌组序号和完整实例摘要；旧导出没有该记录时保持 Unknown，不按
卡名重建。读取任意牌组实例也不增加 owner 的候选集合。

## 阅读检查

```powershell
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m pytest tools/expert/tests/test_curated.py -q
.\scripts\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m pytest tools/expert/tests/test_run_planning_curated.py -q
```

原文或整理条目更新后，直接重读相关段落和条目。原始资料不在 Git 中时，整理条目仍可阅读；
需要引用原文的判断，应打开当前本地正文核对。

[合成试读案例](../../../tools/expert/tests/fixtures/card-reward-cases.json)覆盖同组奖励在不同资源条件下的关注点、缺费与缺访问、
后期过渡与跳过、升级竞争、消耗启动和信息缺失。案例中的阅读选择与应用说明由分析者撰写，
不是自动推理结果，也不是最优动作标签。长度检查只验证阅读预算和限制信息保留，不证明决策提升。

样本通过后的扩展仍以具体问题为单位：补齐来源、澄清分歧，检查压缩是否丢失会改变决策的条件。
不要为了支持新主题重新引入从问题猜候选身份的接口，也不要直接把全部原文换一种序列化格式搬进上下文。
