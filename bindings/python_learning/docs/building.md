# Build and check the Python package

[Python interface](../README.md)

## Build a Linux wheel

Run from the repository root with Python 3.12 or newer, a Rust toolchain and a C compiler/linker installed. Python must include virtual-environment support; on Debian/Ubuntu this comes from `python3-venv` or the matching versioned package, such as `python3.12-venv`.

```bash
python3 -m venv .oracle-lab/python-learning-build
.oracle-lab/python-learning-build/bin/python -m pip install 'maturin>=1.12,<2'
.oracle-lab/python-learning-build/bin/python -m maturin build --locked --release \
  --manifest-path bindings/python_learning/Cargo.toml \
  --interpreter .oracle-lab/python-learning-build/bin/python \
  --out target/python-wheels
```

Install the exact wheel path printed by Maturin with `python -m pip install PATH_TO_WHEEL`, then run `python -m pip check` using the same runtime interpreter. Maturin and the compiler are build dependencies. The installed package depends on NumPy; on Linux, the wheel's `manylinux` tag declares its minimum glibc version.

## Verify a Windows build

From the repository root, run:

```powershell
.\bindings\python_learning\verify.ps1 -Python <python-3.12-executable>
```

The script builds a wheel, installs it without dependency mutation into a fresh isolated environment, binds the target Python's existing dependency directory after that isolated wheel, and runs `tests/smoke.py` plus one narrow real Node-SL caller smoke. The complete `learning/tests` suite belongs to `learning/dev.ps1 test`/`verify` and is not repeated inside this lower-level wheel verifier. It keeps the wheel, environment, and complete logs below one fresh ignored `.oracle-lab/python-learning-bridge/` directory while a run is active. A failed run preserves that directory and reports the relevant log path; a successful run removes the disposable wheel, environment, and logs after printing its compact summary.

Passing `-InstallTarget` explicitly installs that exact wheel into the target Python with `--force-reinstall --no-deps`, but only after all isolated checks pass. Routine learning development should use `.\learning\dev.ps1 refresh-bridge`, which applies this guarded refresh to the configured training runtime and follows it with `doctor`.

## Workspace and release profiles

The binding is a standalone Cargo workspace with its own release profiles. It batches `LearningEnvPoolV3` control calls into NumPy arrays. Routine release wheels keep exact environments and downstream model/pool and NumPy adapters in separate O2 units; `release-final` selects all-O3/thin-LTO deployment builds. Dev wheels provide the functional edit loop.

The bridge `cdylib` has no Rust lib-test harness. Typed contracts are tested in their workspace owners, and the built-wheel checks cover PyO3/NumPy arrays, semantic schemas, checkpoints, combat-root artifacts and typed ordinal execution. The caller check exercises the installed Node-SL adapter. Each wheel and installation check uses a fresh isolated environment.

Complete original-game replay and installed-package acceptance use [trajectory verification](verification.md).
