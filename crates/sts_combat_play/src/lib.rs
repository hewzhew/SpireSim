//! Reusable exact-combat play sessions and their command-line workbench.
//!
//! Session persistence and replay are public library capabilities. The CLI is
//! an adapter over those capabilities; callers do not need to parse its text
//! output or spawn the binary to validate an exact session.

mod app;
pub mod resident_audit;
pub mod session;

pub use app::main_entry;
pub use session::*;
