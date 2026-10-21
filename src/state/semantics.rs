//! Runtime state semantics ledger.
//!
//! This module is the single index for hidden or easy-to-misread runtime state
//! conventions that tend to drift across engine, sync, tools, and tests.
//!
//! Current ownership rules:
//!
//! - `combat::Power.just_applied`
//!   - runtime lifecycle flag owned by the engine/content pipeline
//!   - consumed by hook implementations such as end-of-round decay
//!   - protocol snapshots may hint at similar concepts, but they do not define
//!     the runtime meaning
//! - `combat::Power.extra_data`
//!   - runtime-only scratchpad owned by the specific power implementation
//!   - meaning must be defined in that power's content module, not in tools
//!     or sync glue
//! - owner-specific lifecycle semantics
//!   - runtime truth lives in engine scheduling plus content hook dispatch
//!   - external adapters may map observed state onto runtime fields, but must
//!     not become the semantic source of truth
//! - external snapshot fields
//!   - import adapters are outside runtime ownership
//!   - tooling and fixtures may observe those fields but should not define their
//!     meaning for runtime behavior
//!
//! Treat this module as the canonical checklist before adding new hidden state
//! or reusing an existing field for protocol convenience.
