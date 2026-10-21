//! Analysis layer.
//!
//! This layer converts stable game facts and a concrete run state into typed
//! observations. It may diagnose debt or support, but it must not own final
//! scene decisions.

pub mod card_semantics;
