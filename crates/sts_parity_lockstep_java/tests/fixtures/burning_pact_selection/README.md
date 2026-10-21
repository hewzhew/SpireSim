# Burning Pact selection

Three unchanged Java boundaries and two action receipts cover playing Burning Pact,
pausing to exhaust one hand card, and drawing after that choice. [sources.json](sources.json)
records the original locations, byte digests and Java producer identities.

The offline regression checks receipt adjacency, the suspended card, paid energy,
unchanged creatures, hand order, the selected exhaust and the two subsequent draws.
Its card aliases identify Java objects across these frames only; they do not create
Java-to-Rust card bindings. The live trajectory owns the separate lockstep evidence.
This fixture does not authorize restoration or certify an Act.
