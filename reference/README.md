# 本地尖塔资料

读打法从 `expert` 的攻略原文开始；查卡牌、遗物、事件和译名用 `wiki`；核对已整理的
机制条目用 `game-knowledge`。完整抓取属于本机的可选缓存，位于
`.oracle-lab/external-research/sources/`；不随 Git 分发。目录或索引存在不代表这台机器已经有正文。
先检查目标文件是否存在；缓存缺失时使用仓库中的整理结果、短摘录和模拟器机制查询，不反复搜索
不存在的目录。Git 中保留目录、索引和少量整理结果。

| 资料 | 现在有什么 | 入口 |
| --- | --- | --- |
| 高手攻略与复盘 | 铁甲与静默的长攻略、整局复盘及专题答疑 | [来源目录](expert/sources.jsonl)、[战士对读](expert/notes/decision-questions.md)、[静默对读](expert/notes/silent-planning.json) |
| Wiki | wiki.gg 英文、灰机中文、Fandom 旧版的完整文字抓取；灰机铁甲战士页有构筑概览 | [查询方法](wiki/README.md) |
| 已整理经验 | 选牌和整局规划各一包，共十六个主题 | [条目目录](expert/curated/README.md) |
| 机制核对 | 来源断言与按版本整理的事实 | [机制目录](game-knowledge/README.md) |

## 先读哪几篇

| 来源 ID | 适合研究的问题 |
| --- | --- |
| `tieba-10072740344` | 当前卡组的攻防产出、启动速度、二层路线与 Boss 遗物取舍 |
| `tieba-7709780592` | 血量周转、攻杀与防杀、过渡牌、允许 SL 时的构筑思路 |
| `reddit-ironclad-10-streak-guide` | 路线退路、药水、选牌阶段、精英准备和 Boss 遗物 |
| `reddit-ironclad-35-of-50`、`reddit-ironclad-64-percent-sample` | 近期整局复盘与决策习惯 |
| `steam-silent-qingyu-2022` | 静默过渡、路线与特殊遗物如何改变运营 |
| `reddit-silent-tilting-module-10-streak` | 静默能力、防御、保留与 Boss 遗物的配合 |
| `reddit-silent-zakk-30-of-50` | 静默过量补攻击、第三幕资源消耗与大卡组运行 |

[静默文字资料对读](expert/notes/silent-planning.json)按八位作者组织阅读范围、适用条件与分歧，
并将项圈下的出牌额度推导单列为分析者判断。作者答疑与完整攻略分开，长期成绩未知时明确记录。

有对应缓存时，每个来源的完整正文位于
`.oracle-lab/external-research/sources/<来源 ID>/corpus/text.md`。例如：

```powershell
rg -n '二层|药水|启动' .oracle-lab/external-research/sources/tieba-10072740344/corpus/text.md
rg -n 'path|potion|ACT 2' .oracle-lab/external-research/sources/reddit-ironclad-10-streak-guide/corpus/text.md
```

需要找到具体章节时看 `expert/outlines/`；需要短主题提示时看 `expert/curated/`。
读到有分歧的判断，再打开原文上下文。`expert/notes/` 保存已有的来源笔记。
[五个决策问题](expert/notes/decision-questions.md)把几篇攻略的取舍和原文位置放在同一页。

Wiki 适合核对机制和官方中文名称。灰机的“铁甲战士”页还有单核、防战、力量、消耗等
构筑概览，可用 `.\scripts\wiki.ps1 page --source-id slay-the-spire-huiji-zh --title 铁甲战士`
阅读。wiki.gg 抓取还含二代专用命名空间；查一代文章时给 `wiki.ps1 search` 加
`--namespace 0`。资料来源、查询命令与更新流程分别见
[Wiki 目录](wiki/README.md)和[来源目录](expert/sources.jsonl)。

`entities.jsonl` 是各索引共享的游戏实体目录；`ReferenceIndex.psm1` 为离线查询提供
JSONL 与实体读取。工具实现和试验说明见 [reference_service](../reference_service/README.md)。
