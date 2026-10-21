//! Explicit, action-scoped environmental time for deterministic replay.
//! Ordinary standalone simulation retains its caller-supplied EventContext
//! time. An installed tape requires exact consumption and never changes the
//! persistent RunState clock. A failed private transition must be discarded.
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

thread_local! {
    static ACTIVE: RefCell<Option<Tape>> = const { RefCell::new(None) };
}

struct Tape {
    samples: Vec<f32>,
    consumed: usize,
}

pub struct EventTimeScope {
    finished: bool,
    not_send: PhantomData<Rc<()>>,
}

impl EventTimeScope {
    pub fn begin(samples: Vec<f32>) -> Result<Self, String> {
        if samples
            .iter()
            .any(|v| !v.is_finite() || v.is_sign_negative())
        {
            return Err("event time input must be finite and nonnegative".to_owned());
        }
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.is_some() {
                return Err("event time input scopes cannot nest".to_owned());
            }
            *active = Some(Tape {
                samples,
                consumed: 0,
            });
            Ok(Self {
                finished: false,
                not_send: PhantomData,
            })
        })
    }

    /// The counts are checked even if the private simulation itself failed.
    pub fn finish(mut self) -> (usize, usize) {
        let tape = ACTIVE
            .with(|active| active.borrow_mut().take())
            .expect("owned event time scope");
        self.finished = true;
        (tape.samples.len(), tape.consumed)
    }
}

impl Drop for EventTimeScope {
    fn drop(&mut self) {
        if !self.finished {
            ACTIVE.with(|active| {
                active.borrow_mut().take();
            });
        }
    }
}

/// Called only when Act 3 candidate filtering actually evaluates Secret Portal.
pub(crate) fn secret_portal_playtime(standalone_time: f32) -> f32 {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let Some(tape) = active.as_mut() else {
            return standalone_time;
        };
        let sample = tape.samples.get(tape.consumed).copied();
        tape.consumed += 1;
        // Poison an incomplete private computation; finish reports the
        // missing read and the adapter discards the entire successor.
        sample.unwrap_or(f32::NAN)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_and_failed_scopes_cannot_leak_inputs() {
        for invalid in [f32::NAN, f32::INFINITY, -1.0, -0.0] {
            assert!(EventTimeScope::begin(vec![invalid]).is_err());
        }
        let scope = EventTimeScope::begin(vec![801.0]).unwrap();
        assert!(EventTimeScope::begin(vec![802.0]).is_err());
        assert_eq!(secret_portal_playtime(0.0), 801.0);
        drop(scope);
        assert_eq!(secret_portal_playtime(77.0), 77.0);
        let scope = EventTimeScope::begin(vec![900.0]).unwrap();
        assert_eq!(scope.finish(), (1, 0));
        let _ = std::panic::catch_unwind(|| {
            let _scope = EventTimeScope::begin(vec![1000.0]).unwrap();
            panic!("private transition failed");
        });
        assert_eq!(secret_portal_playtime(78.0), 78.0);
    }
}
