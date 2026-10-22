# 展示语言与共用名称

这个模块提供中英文游戏名称、原版卡牌文本和消息格式化。它不读取游戏存档、执行动作或修改模型请求。`replay/` 拥有离线演示的模板、样式和渲染代码；实际模型记录、素材整理和构建结果保存在 artifact。dsh 地图与工作台使用宿主语言服务，领域名称消费共用词典；Rust 卡牌与药水查询也从这里取名称、牌面与规则说明。

## 项目中的职责

项目本地化覆盖游戏内容、界面和模型工具读数。共用的是领域名称与术语，各功能拥有自己的消息，各运行端使用适合其环境的格式化入口。当前消费者是离线演示、dsh 地图和工作台界面，其余消费者按下面的合同接入。

| 层 | 负责什么 | 接入约定 |
| --- | --- | --- |
| Rust 游戏核心 | 稳定身份、规则数值、公开状态、合法动作 | 沿用 [identity](../../src/content/identity.rs) 的稳定 ID，不从译名构造身份；语言选择不参与 RNG、游戏状态、动作身份、数值训练词表或 parity 判定 |
| 领域资源 | 卡牌、遗物、敌人、遭遇、状态、房间等名称与共用术语 | 按实体种类和稳定 ID 取词；原版名称从安装包导入，各端消费同一份生成资源；工具自己的术语和消息独立维护 |
| 功能界面 | 导航、按钮、空状态、可访问性文案 | 各功能使用自己的命名空间；演示的 `replay` 不能成为地图、工作台或工具消息的公共容器 |
| 自然语言读数 | 卡牌机制、当前局面、合法动作说明、错误消息 | 沿 Rust/Node 的实际渲染 owner 使用结构化事实和具名参数，不翻译最终字符串后再解析；数值与效果顺序由规则 owner 提供 |
| Python 训练接口 | 结构化观察、动作、奖励及可选文字读数 | 稳定 ID 和数值特征不随语言变化；文字读数接现有渲染 owner，避免在 Python 再维护一套规则说明 |
| 原始记录 | 实际模型请求、输出和工具返回 | 保留产生时的原文；展示可从有来源关系的公开事实重绘，不用译文覆盖原始记录 |

`i18next` 是 JavaScript 消费者的格式化实现。原生 `sts_presentation` crate 位于同一目录，使用相同 JSON，编译时嵌入资源；Cargo 包包含所需源码和词典。Rust 和 Python 的传递构建因此不依赖 Node、浏览器、本机游戏安装或运行目录。原生格式化目前支持具名参数插值，不自动选择 i18next 的复数形式；新增复数读数时沿对应 renderer 提供数量选择。

## 展示语言与模型语言

展示语言决定用户看到的名称和界面。模型语言决定后续请求中的工具读数、规则说明和相关提示。二者分别由对应边界拥有：中文界面可以查看英文模型对局，界面切换只重绘，不改写已运行会话的输入或历史。工作台的 `agentLocale` 默认 `en`，支持 `zh-CN`，原生聊天在绑定中保存该选择，重连时保持。当前接通药水机制查询，以及卡牌定义查询的原版牌面与基础属性；卡牌机制详解仍保留英文，其他读数按实际 owner 迁移。Python 的文字语言参数还未接入。

dsh 地图使用宿主的语言偏好、订阅和 slot 翻译入口，离线演示使用自己的语言选择。宿主内置 `zh` 在资源边界映射到 `zh-CN`。宿主的 `{name}` 模板与 i18next 的 `{{name}}` 及复数规则不同：地图的界面资源使用宿主格式，领域资源只抽取无模板的名称，效果模板保留在对应格式化入口。

## 后续消费者

地图服务端返回房间种类、Boss 身份、坐标和状态，由客户端组合图例和悬停文字，局部中文名称表已删除。工作台控制与日志界面也沿宿主语言设置呈现，原始日志保持原文。复制提示等异步状态保存消息键，在渲染时取词；语言切换不重新读取记录或执行控制操作。返回点由 Rust 提供药水稳定 ID 与结构化终局结果，客户端分别通过共用名称和工作台消息呈现，机制或协议诊断保留原文。

模型工具沿实际 renderer 接入：程序消费者先摆脱英文标题解析，再让相同公开事实产生中英文读数。药水查询由 Rust `mechanics_info` 提供，基础药效、稀有度与目标要求仍来自药水定义；42 条已审阅规则维护在 `game.potionRule`，原有英语消费者从同一资源取词，旧规则文案副本已删除。`mechanics.potion` 拥有查询句子和未知药水错误，查询始终接受原有英文身份，显示明确的稳定 ID。其他机制逐类迁移，检查条件、升级、目标与效果顺序。语言覆盖按消费者实际使用的内容验收，不能将词典中存在一个名称当作对应机制已经完成本地化。

保存计划的完整 ID、比较引用和复盘分页命令由 Rust `combat_reading` 生成；Node 从候选的 `plan_id` 绑定 `planId`，直接展示报告原文。工作台不依赖保存计划的标题文字，也不在显示普通读数时追加计划查询。

选项行、营火升级与奖励标签、单选和多选提示由 Rust `decision` 渲染；命令来自 typed choice 与 selector，说明文字不参与命令识别。Node 原样输出，战斗外显示不再补取观察。预演保留假设选择及规则，当前读数提供可复制命令。读数的 `summary` 与正文共用源端公开状态渲染器，预演直接展示执行前的摘要，不再按英文标题截取；`board` 的当前格挡、来袭伤害与合法命令使用同一份 typed 公开观察。

工作台将本次实际展示的最后一份局面作为 `currentReading` 返回，原生和脚本工具通过官方日志 metadata 保存，模型消息仍只包含原来渲染的文字。日志摘要与脚本玩家的压力压缩使用这份记录，不从中英文标题猜边界；失败或尚未返回的调用不会使旧局面成为压缩后的实际进度。主动上下文刷新记录正文的字符区间，由输入诊断直接读取。

战略观察的 `preparation_reading` 由 Rust 从同一份公开决策生成，`public_snapshot_id` 绑定当前帧。选项文字按完整命令绑定；库存卡牌按稳定 ID 和升级等级绑定，遗物按稳定 ID 绑定；商店商品还包含类型、槽位、价格和可用命令。卡牌规则的 `available` 表示源端是否提供了机制说明。脚本准备、选牌和购物比较消费这些块，不按英文标题、显示名或文本顺序拆分，不再为取得规则额外请求 `read, inventory effects`。库存全文与分块共用原有渲染 owner；文字保持完整，不由 Node 裁剪规则。准备读数目前仍是英文，后续翻译沿这些 Rust renderer 接入；`read/options` 正文和脚本策略消息尚未接入语言选择。

## 运行

需要 Node.js 22 或更新版本。在本目录执行：

```powershell
npm ci
npm test
```

`i18next` 负责消息插值、英语复数和语言回退，`Intl.NumberFormat` 负责展示数字。`esbuild` 是离线页面的构建依赖；生成的页面不需要联网或安装 Node。

## 使用

```javascript
import { createPresentation } from './index.mjs';

const view = await createPresentation({ locale: 'zh-CN' });
view.entity('card', 'ShrugItOff'); // 耸肩无视
view.text('game:effect.draw', { count: 2 }); // 抽 2 张牌
await view.setLocale('en');
view.entity('card', 'ShrugItOff'); // Shrug It Off
view.text('game:effect.draw', { count: 2 }); // Draw 2 cards.
```

每个展示端创建独立实例。`setLocale` 只改变该实例的呈现语言，调用者拥有重绘和偏好保存。不要用它替换 dsh 的宿主 locale provider；插件应沿其公开 locale/slot API 接入，游戏名称使用这里的共用资源。

只消费资源或运行构建检查的 JavaScript 端可导入 `@sts-simulator/presentation/catalogs`，该入口不加载 i18next。JSON 资源也可通过 `@sts-simulator/presentation/locales/en.json` 等导出路径直接读取。dsh 地图只将所需领域名称注册到宿主，客户端构建时包含实际资源。

## 资源职责

共用内容位于 `locales/en.json` 和 `zh-CN.json`：`game` 拥有名称、术语和规则，`mechanics` 拥有机制查询消息，`replay` 拥有离线演示的导航、场景和原始记录入口文案。dsh 地图和工作台自己的界面消息位于其 `locale/` 资源中，不在其中复制游戏名称。

`game.card`、`relic`、`potion`、`enemy`、`character`、`event`、`power`、`encounter` 是原版资源导入的名称区段，由 [import_original.py](import_original.py) 更新，不手工翻译。其中卡牌、遗物、药水和敌人按当前模拟器词表导入；角色覆盖四名可玩角色，事件目前取 Neow，状态目前取易伤、虚弱、脆弱，遭遇取地图使用的具名遭遇。多敌人遭遇 `DonuAndDeca` 用原版的两个敌人名与本项目的连接词组合。房间类型、界面消息和补充规则说明由项目维护。

原版资源位于安装目录的 `desktop-1.0.jar`：`localization/eng/` 为英文，`localization/zhs/` 为简体中文，`localization/zht/` 为繁体中文；本模块当前导入前两种。目录内的 `cards.json`、`relics.json`、`potions.json`、`monsters.json`、`powers.json`、`characters.json`、`events.json` 等按游戏内容分类。导入只读取 ZIP 中的 JSON，不启动游戏或 Java。

从仓库根目录核对原版名称；加 `--write` 更新名称区段与来源摘要：

```powershell
.\scripts\py.ps1 tools/presentation/import_original.py --jar 'C:/path/to/SlayTheSpire/desktop-1.0.jar'
.\scripts\py.ps1 tools/presentation/import_original.py --jar 'C:/path/to/SlayTheSpire/desktop-1.0.jar' --write
```

卡牌、遗物使用现有身份 owner 提供的原版 ID；本地化键与协议 ID 不同的项在导入器中显式列出，不用译名、英文显示名或模糊匹配反查。例如稳定 ID `Elixir` 对应原版当前药水键 `ElixirPotion`，原版同名 `Elixir` 条目已经废弃。来源 [original-names.json](original-names.json) 记录语言、资源文件摘要、生成名称摘要与明确遗漏；逐项来源可以用 `--report <新 artifact JSON 路径>` 保存。`Impulse` 在当前安装版的卡牌语言资源中没有条目，保持稳定 ID 回退。普通安装、构建和运行只消费已生成的 JSON，不依赖本机游戏；npm 和 Cargo 包保留来源摘要。

原版牌面与工具补充说明承担不同用途。卡牌 `DESCRIPTION` 中的 `!D!`、`!B!`、`!M!` 由游戏提供数值，`NL` 和 `#y` 等控制换行与颜色；药水和遗物的 `DESCRIPTIONS` 常是由 Java 按顺序插入数值的片段。当前 `game.potionRule` 是项目整理的完整机制解释，包括触发时点、目标、限制和原版短描述未展开的交互；它与原版名称分别维护，导入器保留它以及所有工具、界面消息。

[original-card-text.json](original-card-text.json) 保存同一次导入的卡牌 `DESCRIPTION`、`UPGRADE_DESCRIPTION` 与 `EXTENDED_DESCRIPTION` 原文和来源摘要，通过 `@sts-simulator/presentation/original-card-text` 导出，并嵌入 Rust crate。字段名不决定其用途：全身撞击的 `UPGRADE_DESCRIPTION` 是战斗伤害附加行，终结技用它显示不可打出的提示。`cards::mechanics::original_printed_card_text` 按原版实际升级方法选择定义牌面，使用同一份永久基础数值 owner 处理伤害、格挡和多次升级；`!M!` 使用当前升级态的定义数值。它不查询隐藏状态或计算当前战斗伤害。

共享格式化入口只替换已提供的数值、处理换行与关键字标记，将能量图标显示为 `[Energy]` 或 `[能量]`，保留原图标数量和 X 表达式。未知变量或缺失的已选模板返回错误。原版未收录的卡牌返回 `None`，由查询保留稳定 ID 和已有机制资料。当前 `info card <ID>[+N]` 使用这份牌面，按工具语言显示名称、类型、属性与目标，并保留完整的英文机制补充。`read/options`、准备文字块和战斗实例文本仍沿各自 owner 继续接入，不在每次读取时重复追加原版牌面。

名称按领域和**现有稳定 ID**取值，例如 `game.card.StrikeG`、`game.encounter.TheHeart`。同名的 `Strike`、`StrikeG`、`StrikeB`、`StrikeP` 保持四个条目，不能按翻译文字反查动作。名称资源的具体覆盖见来源摘要，新增消费者沿稳定 ID 使用已有原版名称，再补所需消息。缺项先回退英语，再显示原稳定 ID；`onMissing` 提供开发检查信息，不向页面增加提示条。

界面消息使用完整句子和具名参数，不能拼接翻译片段。英语复数采用 i18next 的 `_one`、`_other` 规则，调用时保留数值 `count`。`validateCatalogs` 检查两种语言的键、非空值和参数一致性；未知消息键或缺少参数会报错。测试同时核对实体键仍属于当前模拟器的稳定词表。

费用、伤害、格挡、升级、每段效果、条件和动作目标均由调用者从现有公开事实提供。模板不计算规则，也不添加当前局面未提供的信息。`number()` 只用于展示，命令中的 ID、ASCII 数值与语法保持原样。

## 离线分发

```powershell
node tools/presentation/bundle.mjs --entry <viewer.mjs> --output <demo.js>
```

上述命令从仓库根目录运行。构建入口会先检查资源，再把消息库和词典打包进浏览器脚本。原始模型记录和结构化展示数据由消费者分别保存；切换语言不翻译或覆盖原始请求、工具返回和摘要。

当前离线演示对一份固定的历史公开工具记录做构建期整理，浏览器不再解析英文游戏文本。这不是新的通用采集协议；新采集的正式展示应沿原生 owner 取得 typed 公开状态。原始文本与结构化展示必须保留来源关系。
