# 展示语言与共用名称

这个模块为展示端提供中英文游戏名称和消息格式化。它不读取游戏存档、执行动作或修改模型请求。`replay/` 拥有离线演示的模板、样式和渲染代码；实际模型记录、素材整理和构建结果保存在 artifact。dsh 工作台的接入仍由其客户端模块负责。

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

## 资源职责

`locales/en.json` 和 `zh-CN.json` 是唯一人工编辑来源，分别包含 `game` 和 `replay` 命名空间。前者拥有名称、术语和效果句子模板；后者拥有离线演示的导航、场景和原始记录入口文案。

名称按领域和**现有稳定 ID**取值，例如 `game.card.StrikeG`、`game.encounter.TheHeart`。同名的 `Strike`、`StrikeG`、`StrikeB`、`StrikeP` 保持四个条目，不能按翻译文字反查动作。名称资源目前覆盖演示和地图所需的一部分实体，新增消费者按实际需要补齐。缺项先回退英语，再显示原稳定 ID；`onMissing` 提供开发检查信息，不向页面增加提示条。

界面消息使用完整句子和具名参数，不能拼接翻译片段。英语复数采用 i18next 的 `_one`、`_other` 规则，调用时保留数值 `count`。`validateCatalogs` 检查两种语言的键、非空值和参数一致性；未知消息键或缺少参数会报错。测试同时核对实体键仍属于当前模拟器的稳定词表。

费用、伤害、格挡、升级、每段效果、条件和动作目标均由调用者从现有公开事实提供。模板不计算规则，也不添加当前局面未提供的信息。`number()` 只用于展示，命令中的 ID、ASCII 数值与语法保持原样。

## 离线分发

```powershell
node tools/presentation/bundle.mjs --entry <viewer.mjs> --output <demo.js>
```

上述命令从仓库根目录运行。构建入口会先检查资源，再把消息库和词典打包进浏览器脚本。原始模型记录和结构化展示数据由消费者分别保存；切换语言不翻译或覆盖原始请求、工具返回和摘要。

当前离线演示对一份固定的历史公开工具记录做构建期整理，浏览器不再解析英文游戏文本。这不是新的通用采集协议；新采集的正式展示应沿原生 owner 取得 typed 公开状态。原始文本与结构化展示必须保留来源关系。
