# Try the training environment

The environment runs complete game episodes in Rust. Python supplies decisions
through public observations and legal candidates. Java and the original game
are used when collecting new comparison evidence; they are not runtime
dependencies of the training wheel.

## Install and run the full verification

Download the matching wheel and `sts-verification-0.1.0.zip` from the
[release page](https://github.com/hewzhew/sts-sim/releases) into one directory.
Prebuilt wheels target these interpreter and platform combinations:

| Platform | Python | Wheel tag |
| --- | --- | --- |
| Linux x86_64, glibc 2.35 or newer | CPython 3.12 | `cp312-cp312-manylinux_2_35_x86_64` |
| Windows x86_64 | CPython 3.14 | `cp314-cp314-win_amd64` |

On Linux:

```bash
python3.12 -m venv .venv
.venv/bin/python -m pip install ./sts_learning_bridge-0.1.0-cp312-cp312-manylinux_2_35_x86_64.whl
.venv/bin/python -m zipfile -e sts-verification-0.1.0.zip .
.venv/bin/python sts-verification/verify.py --output verification-results
```

On Windows, in PowerShell:

```powershell
py -3.14 -m venv .venv
.\.venv\Scripts\python.exe -m pip install .\sts_learning_bridge-0.1.0-cp314-cp314-win_amd64.whl
.\.venv\Scripts\python.exe -m zipfile -e sts-verification-0.1.0.zip .
.\.venv\Scripts\python.exe sts-verification/verify.py --output verification-results
```

NumPy is installed as a declared dependency. No repository checkout is needed
for these commands. Other interpreter/platform combinations can build a wheel
using the [source build instructions](../bindings/python_learning/README.md#build-a-linux-wheel).

The verifier prints one result per character and saves detailed reports below
the requested new output directory. It checks all four complete A20 Heart
trajectories from Neow through the formal victory terminal, with three slots
at different progress points. Every public choice and actual game commit is
checked, including checkpoint recovery and an intentionally rejected input
batch followed by valid continuation. These are recorded adopted decisions;
the verifier does not ask a model to choose a new strategy.

The bundle includes the original Java observations, receipts, producers and
typed parent-prefix records used by those trajectories. To repeat the current
Java/Rust comparison, build `sts_lockstep` from the matching source release:

```text
cargo build --locked --profile dev-opt -p sts_parity_lockstep_runner --bin sts_lockstep
```

Then pass that binary to the same verifier:

```text
python sts-verification/verify.py --runner PATH_TO_STS_LOCKSTEP --output raw-verification-results
```

This reprojects raw Java records and executes Rust offline before testing the
installed Python package. Original evidence files retain their bytes and
producer identities; the bundle's relative directory layout provides their
current locations. See [environment acceptance](architecture/environment-acceptance.md)
for the comparison contract.

## Connect a policy

The source release and verification bundle include `policy_loop.py`. Its
callback receives `decision_batch(semantic=True)` and `semantic_schema()`, and
returns one candidate ordinal for each current decision row:

```python
def choose(batch, schema):
    # Your model scores the current public candidate rows.
    scores = model(batch, schema)
    splits = batch["candidate_row_splits"]
    return [
        int(scores[int(start):int(end)].argmax())
        for start, end in zip(splits[:-1], splits[1:])
    ]
```

Here `model` is the caller's own model, returning a flat score array aligned
with the public candidate rows. Put the callback and model loading in an
importable module, for example `my_policy.py`, then run:

```text
python policy_loop.py --policy my_policy:choose --seeds 42 43 --character silent
```

From the source checkout the script is at
`bindings/python_learning/examples/policy_loop.py`. The callback receives no
environment object, checkpoint, search result or hidden future state. The
semantic schema names the fields and categorical values in the public NumPy
tables; the [bridge contract](../bindings/python_learning/README.md#natural-episodes-and-trajectory-acceptance)
describes the row layout.

The loop calls `choose` until every active slot is ready, then calls `step`
once. Card selection can require several choices before that commit. Terminal
slots leave future decision rows, while rewards keep their actual slot
indices. The script reports actual rewards and whether the episodes terminated
or reached an explicit choice/commit limit; it supplies no default strategy.

## Report a problem

Open an [issue](https://github.com/hewzhew/sts-sim/issues) with the package
version, wheel filename, platform, command and first failing report. A
verification failure records the first differing boundary. For a new gameplay
case, include the seed, character and actual action sequence when available.
This lets a report become a reproducible transition or trajectory.

The project's code is [MIT licensed](../LICENSE). Third-party attributions are
listed in [THIRD_PARTY_NOTICES](../THIRD_PARTY_NOTICES.md).
