//! Opt-in, thread-local diagnostics. Timings never enter snapshots, journals,
//! replay inputs, or continuation decisions. No clocks or I/O run when disabled.
use serde::Serialize;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Instant};

thread_local! {
    static ACTIVE: RefCell<Option<Rc<RefCell<State>>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct State {
    stack: Vec<Frame>,
    spans: BTreeMap<String, TimingSummaryV1>,
    observations: BTreeMap<String, TimingSummaryV1>,
    next_id: u64,
    balanced: bool,
}

struct Frame {
    id: u64,
    path: String,
    child_ns: u128,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TimingSummaryV1 {
    pub calls: u64,
    pub total_ns: u128,
    pub self_ns: u128,
    pub max_ns: u128,
}

impl TimingSummaryV1 {
    fn add(&mut self, elapsed: u128, children: u128) {
        self.calls = self.calls.saturating_add(1);
        self.total_ns = self.total_ns.saturating_add(elapsed);
        self.self_ns = self
            .self_ns
            .saturating_add(elapsed.saturating_sub(children));
        self.max_ns = self.max_ns.max(elapsed);
    }
}

#[derive(Debug, Serialize)]
pub struct TimingReportV1 {
    pub schema: &'static str,
    pub balanced: bool,
    /// Inclusive totals overlap their descendants. Sum self_ns, not total_ns.
    pub spans: BTreeMap<String, TimingSummaryV1>,
    /// Independent producer-reported intervals; never subtracted from spans.
    /// Missing observations remain absent, rather than becoming zero samples.
    pub observations: BTreeMap<String, TimingSummaryV1>,
}

/// A capture is confined to its originating thread and cannot nest. Frontends
/// own persistence; the shared execution core only contributes observations.
pub struct TimingCaptureV1(Rc<RefCell<State>>);

impl TimingCaptureV1 {
    pub fn start() -> Result<Self, String> {
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.is_some() {
                return Err("a timing capture is already active on this thread".into());
            }
            let state = Rc::new(RefCell::new(State {
                balanced: true,
                ..State::default()
            }));
            *active = Some(state.clone());
            Ok(Self(state))
        })
    }

    pub fn finish(self) -> TimingReportV1 {
        let state = self.0.borrow();
        TimingReportV1 {
            schema: "sts-lockstep-timing-v1",
            balanced: state.balanced && state.stack.is_empty(),
            spans: state.spans.clone(),
            observations: state.observations.clone(),
        }
    }
}

impl Drop for TimingCaptureV1 {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = None);
    }
}

#[must_use]
pub struct TimingSpanV1(Option<(Rc<RefCell<State>>, u64, Instant)>);

pub fn timing_span_v1(label: &'static str) -> TimingSpanV1 {
    ACTIVE.with(|active| {
        let active = active.borrow();
        let Some(shared) = active.as_ref() else {
            return TimingSpanV1(None);
        };
        let mut state = shared.borrow_mut();
        let path = match state.stack.last() {
            Some(parent) => format!("{}/{}", parent.path, label),
            None => label.to_owned(),
        };
        let id = state.next_id;
        state.next_id += 1;
        state.stack.push(Frame {
            id,
            path,
            child_ns: 0,
        });
        TimingSpanV1(Some((shared.clone(), id, Instant::now())))
    })
}

impl Drop for TimingSpanV1 {
    fn drop(&mut self) {
        let Some((shared, id, started)) = &self.0 else {
            return;
        };
        let elapsed = started.elapsed().as_nanos();
        let mut state = shared.borrow_mut();
        if state.stack.last().map(|frame| frame.id) != Some(*id) {
            // Diagnostic misuse must not panic in a gameplay transition.
            state.balanced = false;
            return;
        }
        let frame = state.stack.pop().unwrap();
        if let Some(parent) = state.stack.last_mut() {
            parent.child_ns += elapsed;
        }
        state
            .spans
            .entry(frame.path)
            .or_default()
            .add(elapsed, frame.child_ns);
    }
}

/// For example, a bridge receipt reports send-to-stable-successor time. It is
/// not a pure Java timer and does not share the caller's monotonic clock.
pub fn record_timing_observation_v1(label: &'static str, elapsed_ns: Option<u128>) {
    let Some(elapsed) = elapsed_ns else { return };
    ACTIVE.with(|active| {
        if let Some(shared) = active.borrow().as_ref() {
            let mut state = shared.borrow_mut();
            let path = match state.stack.last() {
                Some(parent) => format!("{}/{}", parent.path, label),
                None => label.to_owned(),
            };
            state.observations.entry(path).or_default().add(elapsed, 0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_intervals_do_not_double_count_or_turn_missing_observations_into_zero() {
        let capture = TimingCaptureV1::start().unwrap();
        assert!(TimingCaptureV1::start().is_err());
        {
            let _outer = timing_span_v1("step");
            {
                let _inner = timing_span_v1("java");
            }
            record_timing_observation_v1("bridge", Some(123));
            record_timing_observation_v1("missing", None);
        }
        let report = capture.finish();
        assert!(report.balanced);
        let outer = &report.spans["step"];
        let inner = &report.spans["step/java"];
        assert_eq!(outer.total_ns, outer.self_ns + inner.total_ns);
        assert_eq!(inner.total_ns, inner.self_ns);
        assert_eq!(report.observations["step/bridge"].total_ns, 123);
        assert!(!report.observations.contains_key("step/missing"));
        let _disabled = timing_span_v1("disabled");
        assert!(TimingCaptureV1::start().unwrap().finish().spans.is_empty());
    }

    #[test]
    fn unwinding_keeps_capture_balanced_and_does_not_leak_to_other_threads() {
        let capture = TimingCaptureV1::start().unwrap();
        std::thread::spawn(|| {
            let _span = timing_span_v1("foreign");
        })
        .join()
        .unwrap();
        let _ = std::panic::catch_unwind(|| {
            let _span = timing_span_v1("failed");
            panic!("fixture failure");
        });
        let report = capture.finish();
        assert!(report.balanced);
        assert_eq!(report.spans["failed"].calls, 1);
        assert!(!report.spans.contains_key("foreign"));
    }
}
