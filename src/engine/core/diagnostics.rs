use crate::runtime::combat::CombatState;
use crate::state::selection::EngineDiagnostic;
use std::cell::Cell;

thread_local! {
    static SUPPRESS_ENGINE_WARNINGS_DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn engine_warnings_enabled() -> bool {
    SUPPRESS_ENGINE_WARNINGS_DEPTH.with(|depth| depth.get() == 0)
}

pub(super) fn record_engine_diagnostic(
    combat_state: &mut CombatState,
    diagnostic: EngineDiagnostic,
) {
    if engine_warnings_enabled() {
        combat_state.emit_diagnostic(diagnostic);
    }
}

pub(crate) fn with_suppressed_engine_warnings<T>(f: impl FnOnce() -> T) -> T {
    SUPPRESS_ENGINE_WARNINGS_DEPTH.with(|depth| {
        depth.set(depth.get() + 1);
        let result = f();
        depth.set(depth.get().saturating_sub(1));
        result
    })
}
