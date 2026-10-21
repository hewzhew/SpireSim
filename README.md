# sts_simulator

[简体中文](README.zh-CN.md) | English

An unofficial Rust *Slay the Spire* simulator and training environment, with original-game comparison tools and optional planning and search.

The simulator supports Ironclad, Silent, Defect, and Watcher, including Ascension 20 and the Heart objective. The Python package provides batched episodes, public observations, legal decision candidates, checkpoints, rewards, and terminal results. Rust owns game state and execution; Python callers choose actions. Training runs without Java, Steam, or an original-game installation.

## Start here

Start with the [installation and full-run verification guide](docs/getting-started.md). It covers the prebuilt wheels, downloadable comparison records, and connecting your own policy to the public batch interface.

| Task | Entry point |
| --- | --- |
| Install and use the Python environment | [Python bridge](bindings/python_learning/README.md) |
| Play, inspect, or plan in Rust | [Run-play client](tools/run_play/README.md) |
| Watch a model play from public information | [Model-play walkthrough](docs/model-play.md) |
| Compare execution with the original game | [Architecture](docs/ARCHITECTURE.md), [runbook](docs/RUNBOOK.md) |
| Verify complete trajectories and the installed package | [Environment acceptance](docs/architecture/environment-acceptance.md) |
| Use source-checkout commands and local shortcuts | [Developer scripts](scripts/README.md) |
| Navigate development documentation | [Documentation index](docs/README.md) |

## Original-game comparison

`sts_parity_lockstep_runner` owns the shared Java/Rust execution and replay workflow. Both adapters resolve the same semantic actions and compare state and RNG at stable decision boundaries. Captures retain raw Java observations, action receipts, and producer versions, so current Rust and projection changes can be checked offline.

Training-interface acceptance follows the same adopted trajectory through the installed Python package's actual `choose` and `step` calls. It checks card selection prefixes, committed transitions, public observations, rewards, checkpoint recovery, and the formal game terminal. See the acceptance guide for complete-run, supplementary-coverage, and regression case contracts.

Development documentation is maintained in Chinese.

## License

Project code is available under the [MIT License](LICENSE). See [third-party notices](THIRD_PARTY_NOTICES.md) for attributed components.
