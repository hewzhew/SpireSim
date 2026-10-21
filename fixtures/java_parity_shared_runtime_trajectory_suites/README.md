# Shared-runtime Java parity trajectory suites

These manifests batch complete tracked trajectories in one isolated Java
runtime. They are intentionally separate from the single-transition
`java_parity_shared_runtime_suites` schema: a trajectory member is bound by its
entire ordered selector list, every action receipt and successor frame, and its
final frame rather than one probe transition.

Every member receives a fresh scenario UUID compiled before launch. The suite
may reset to the next member only from the fully captured final successor, and
that successor must still be a typed constructed combat decision. The final
report binds one session, process, connection, isolation directory, launch
plan, `N-1` reset reports, every V2 comparison attempt, and the managed
lifecycle. A member attempt preserves its first failure stage and matched
prefix; string-only comparator errors remain Unknown. The suite keeps natural
reachability, strategy evidence, and training eligibility false.

```powershell
.\java-play.ps1 scenario shared-trajectory-suites
.\java-play.ps1 scenario shared-trajectory-suite --suite cultist-shared-runtime-trajectories
```

The tracked suite pairs the two-step duplicate-Strike sentinel with the
three-step Corruption, Power Through, and Second Wind chain. It exists because
measured cold-start reuse is material; it is not a reason to add trajectories
whose ordering is not itself a mechanics question.
