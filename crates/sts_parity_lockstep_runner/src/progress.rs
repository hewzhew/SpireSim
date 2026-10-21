//! Observational progress only. Callbacks cannot grant execution or verification
//! authority, and are shared by CLI and explicitly requested diagnostic probes.
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum LockstepProgressV1 {
    Started(&'static str),
    Finished {
        phase: &'static str,
        elapsed: Duration,
        succeeded: bool,
    },
    Replay {
        scope: usize,
        scopes: usize,
        completed: usize,
        total: usize,
    },
    NativeSuffix {
        completed: usize,
        total: usize,
    },
}

pub(crate) fn progress_phase<T>(
    observer: &mut dyn FnMut(LockstepProgressV1),
    phase: &'static str,
    operation: impl FnOnce(&mut dyn FnMut(LockstepProgressV1)) -> Result<T, String>,
) -> Result<T, String> {
    observer(LockstepProgressV1::Started(phase));
    let started = Instant::now();
    let result = operation(observer);
    observer(LockstepProgressV1::Finished {
        phase,
        elapsed: started.elapsed(),
        succeeded: result.is_ok(),
    });
    result
}
