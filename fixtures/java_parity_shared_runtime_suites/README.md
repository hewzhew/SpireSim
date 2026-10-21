# Shared-runtime Java parity suites

These manifests are deliberately separate from fresh-runtime suites. One
manifest has one launch profile and at least two distinct exact probe cases.
Every member still receives a fresh scenario UUID and typed root, while the
suite shares one isolated game process, session, connection, and storage root.

The tracked smoke suite crosses three transition shapes: a Cultist end-turn,
a targeted Strike selected by exact hand occurrence, and a Block Potion from a
fresh exact inventory. It is designed to expose reset leakage across combat,
deck, potion, and RNG state before shared-runtime execution is used for broader
regression coverage. The manifest and its plan do not themselves prove reset,
comparison, natural reachability, strategy, or training eligibility.
