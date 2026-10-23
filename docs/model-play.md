# Model play and replay

## Open the offline replay

Download `SpireSim-trial.zip` from the [release page](https://github.com/hewzhew/SpireSim/releases), extract it, and open `SpireSim/index.en.html` (English) or `SpireSim/index.html` (简体中文). Follow **Game demo** to replay the recorded opening from Neow through two fights and their rewards. Use the replay controls or Left/Right arrows to move through decisions, and the act/floor navigation or map to revisit a room.

The bundle's **Install and use** page links the published v0.1.0 Python wheels and four-character verification records. It also includes a source ZIP matching the replay interface and dsh plugin. The recording retains its original game producer; the bundle manifest identifies the presentation source separately.

## Connect a language model

Use the native DeepSeek Harness plugin to watch a model read a public game state, choose actions, and react to their actual results. This walkthrough starts a fresh Ironclad A20 Heart run and stops after its first ordinary fight and rewards.

The chat uses the Rust run-play workbench. For batched training through the installed Python package, use the [public policy callback](getting-started.md#connect-a-policy). The chat preset and the Python callback are separate ways to connect a player to the Rust simulator.

## Prepare the native plugin

This walkthrough uses Windows and PowerShell 7. Install a Rust toolchain and Node.js, clone the repository, and build the game program from its root:

```powershell
./scripts/run-play.ps1 build
```

Install DeepSeek Harness `0.2.1-rc.2` and add the local `tools/dsh_player` package through its Plugins page. The [plugin guide](../tools/dsh_player/README.md#在官方界面使用) describes installation and local server configuration. Configure the model and API key in Harness, then select this repository as the chat workspace.

| Preset | Model access |
| --- | --- |
| 杀戮尖塔 A20H · 公开信息 | Public observations and mechanics, legal actions, actual execution, and retained returns |
| 杀戮尖塔 A20H · 搜索辅助 | The same game tools plus fixed-state search, private previews, and explicit hidden-pile inspection |

Choose **杀戮尖塔 A20H · 公开信息** for this demonstration. Its game connection enforces the public manual mode. The model cannot enable search, preview, deck-lab, or hidden-pile order through that connection.

After updating an installed plugin's preset declarations, reload the plugin installation and restart the local Harness service. Source-only tool changes use the existing tool-update panel. Start a new chat for a different play mode; an existing chat keeps its declared mode and game.

## Start a short run

In a new chat, select Flash and Low reasoning effort. Configure a bounded experiment before sending the gameplay request:

```text
/sts-budget {"maxRequests":24,"maxTokensPerRequest":8192,"maxReportedTokens":30000}
```

This command configures limits without starting a game or a model turn. Requests include retries. The reported-token threshold counts uncached input plus output and is checked before the next request. [Budget details](../tools/dsh_player/README.md#限制实验运行) explain cancellation, missing usage and optional deadlines.

Send this request:

> Start this fresh run from Neow. Choose your own Neow reward and route into an ordinary monster fight. Win the first ordinary fight, handle its rewards, then stop on the map before entering a second room. Query any unclear game fact. Retained returns are allowed; report any returns, tool problems and actual resources when you stop.

The first `sts read` creates this chat's own natural run. The model chooses the rewards, route and combat actions. Every action returns its actual successor, so draws and selections lead to new observations and decisions.

## Observe the decisions

Open **对局** beside the chat title. Select a step to inspect:

| View | What it shows |
| --- | --- |
| 输入 | The request captured at SDK dispatch, expanded into system, user, model and tool messages |
| 模型输出 | The recorded response for this step |
| 完整返回 | Commands, results and errors paired by their actual call IDs |
| 当前游戏 | A separate read of the current game, including changes since an older recorded step |
| 返回点 | The exact earlier positions the game still retains |

Normal request records store a journal boundary and digest. Expanding an input reconstructs that prefix and checks the digest. A request that differs from its journal reconstruction at that seam is stored explicitly. Inspection reads run no model calls and change no game decisions.

The input view identifies SDK request records separately from provider delivery records. A historical step without either boundary displays that the original input is unavailable; it is not reconstructed from the latest chat state.

Open **地图** for the current act and position. The map and current-game reads stay outside the model prompt. Usage separates uncached input, cache reads and output; accumulated input includes history sent repeatedly.

## Pause and try a return

Click **暂停**. The game owner cancels the model and waits for any in-flight action to settle. Inspect **当前游戏** and **返回点**, choose a retained position, then click **返回这里**. The game restores that position's exact state and RNG and remains paused.

**回到刚才** restores the endpoint saved before that return. Both operations appear in the human-operation history; the model's earlier steps remain intact. Click **继续运行** when ready to deliver the checked current game and new return receipt to the player.

Retained free returns can cross the original game's save boundaries. Describe this mode as public-information play with recorded free returns. Record model returns, human returns and gameplay guidance separately when evaluating a player.

## Keep a useful record

Keep the actual model and reasoning setting, task, stopping condition, errors, returns and reported token buckets alongside the trace. Report the position reached and decisions made. For a bug, copy the selected input, command and complete return so the first differing step can be reproduced.

For an offline replay, attach the [recording observer](../tools/presentation/README.md#录制与构建回放) when opening the public game connection, then build the sealed record with `node scripts/replay.mjs build --input RECORDING --output NEW_DIRECTORY`. The observer captures both actions and source-bound choices. The viewer derives floor navigation and map visits from the recorded public boundaries. A model recording retains its actual call records; replay construction does not run a player.

The short run demonstrates model interaction, actual transitions and observation. Full-run strategy evaluation has its own recorded scope. [Original-game comparison and training acceptance](architecture/environment-acceptance.md) describe the environment verification workflow.
