# 发布候选

发布输入为一个固定源码提交、各平台安装包，以及从原始来源重新导出的完整验收结果。
发布工具只编排既有 owner；游戏执行、原始比较和 Python 选择/提交仍由原实现拥有。

## 准备

1. 从干净的已跟踪源码构建 wheel；安装到独立运行环境。
2. 保留构建工具链、源码提交、wheel/native 摘要和安装日志。
3. 在构建源码路径不可用、当前目录也不是仓库的条件下执行完整复验。
4. 对所选来源运行当前原始重放，保留完整 Java 记录及 typed 采用父前缀。

`tools/release/build_candidate.py` 要求工作树干净，显式接收导出目录和 wheel：

```powershell
.\scripts\py.ps1 tools/release/build_candidate.py --study <verified-export-directory> `
  --wheel <windows-wheel> --wheel <linux-wheel> --output <new-candidate-directory>
```

该目录的 `cases.json` 指定四个角色各自的 `id/session/through`，每个角色子目录保存同次
验收的 `expected.json/report.json`。打包器核对报告、完整终局和导出摘要，原样保留导出、
原始 Java evidence 及其 `rust-root.json` 来源连接。它不包含活跃选择器、交互 head、
游戏安装、原生存档目录或整个实验工作区。原始资料中的历史路径只作为来源记录保留。

产物包括各平台 wheel、固定提交的源码 ZIP、完整复验 ZIP、`candidate.json` 和
`SHA256SUMS`。复验包内 `verify.py` 是同一发布工具的快照，调用逐字复制并保留摘要的
`tests/trajectory.py`；没有另一套动作执行或验收逻辑。

## 验收与发布

### 试用入口与离线回放

日常展示调整使用[固定预览目录](../../tools/presentation/README.md#日常预览)，修改后刷新同一页面。相关小改合并审阅与提交，阶段确认后再冻结源码、生成试用包。按变更范围验证：展示改动检查展示 owner 与回放交互，游戏或训练接口变更再运行对应的轨迹和安装验收。

面向首次使用者的试用包沿用 `tools/presentation/site` 的中英文静态页面，回放仍由已有录制构建器生成：

```powershell
./scripts/py.ps1 tools/release/build_trial.py --recording <sealed-recording-directory> --output <new-output-directory>
```

构建要求工作树干净，输出 `site/index.html` 与 `SpireSim-trial.zip`。包内附有当前固定提交的源码 ZIP，页面中的详细文档链接固定到同一提交。`manifest.json` 分别记录界面源码、原始录制生产者与 Python 发布版，`SHA256SUMS` 保存包内文件摘要。录制校验、动作与模型调用的关联和回放投影由 `scripts/replay.mjs` 的原有 owner 处理。

试用页面明确使用已发布的 v0.1.0 Python wheel，不把它标成当前源码的新构建。验收从新目录解压回放、检查本地资源与链接，并按页面安装说明下载准确版本、安装到独立解释器、执行四角色完整复验。需要更新 Python 运行时版本时，按本页的完整候选流程重新构建与验收。

### 工作台与插件

Node 工作台与 dsh 插件通过同一入口打包：

```powershell
npm --prefix tools/dsh_player run pack:distribution -- --output D:/path/to/new-distribution
```

[pack_node.mjs](../../tools/release/pack_node.mjs) 用 npm 自己的文件清单选择源码，在隔离目录安装实际依赖，再生成语言资源、工作台和插件三个包。第三方版本沿现有锁文件保留，本地依赖改为同次产物；开发目录的源码链接保持不动。直接 `npm pack` 不接受带链接的捆绑依赖，因为 npm 会为其传递依赖生成越出包根的路径。分发构建检查包内路径，并从新目录离线安装工作台和插件，分别加载中英文读数。输出的 `distribution.json` 记录源码版本、文件摘要、包摘要和实际安装检查，临时构建目录在结束后移除。

从生成的 ZIP 解压到新目录，在独立解释器安装 wheel，按[入门说明](../getting-started.md)
运行全部角色。另用 `--runner` 对打包后的原始来源重新比较，确认来源重定位后仍可复验。
源码发布 ZIP 中的文件也应能独立构建，不能依赖未跟踪的本机资料。

发布说明保存为发布 artifact，列出实际平台、完整验收结果、源码版本和反馈入口。
当前文档不累积 seed、性能数值或各次发布进度。新增实验可在发布后继续贡献补充覆盖。

外发前复核候选资产及其摘要、许可证、版本说明和首个使用命令。只有项目所有者明确授权后，
才推送对应源码并创建远端版本；本地生成候选不修改远端。
