//! Where a stretch of work spends its time in the kernel: in booleans and in tessellation
//! ([`capture_profile`]), for script profiles (`solidcraft-cli run --profile`, MCP `batch` with
//! `profile`). Off unless a capture is running on the thread, so it costs a thread-local check.
//!
//! Wall time on the capturing thread, outermost phase only: a tessellation inside a boolean
//! counts as boolean time, and a boolean that waits on worker threads counts the wait. Work
//! a kernel call hands to worker threads outside a timed phase is not split out.

use std::cell::Cell;

use serde::Serialize;

/// Milliseconds spent in each timed kernel phase during a [`capture_profile`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct KernelProfile {
    /// Booleans (union, cut, intersect, and joins of bodies apart).
    pub boolean_ms: f64,
    /// Triangulating bodies (measuring, display and export meshes).
    pub tessellate_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Boolean,
    Tessellate,
}

#[derive(Clone, Copy, Default)]
struct State {
    capturing: bool,
    /// Inside a timed phase: inner phases are not timed again.
    busy: bool,
    profile: KernelProfile,
}

thread_local! {
    static STATE: Cell<State> = const { Cell::new(State { capturing: false, busy: false, profile: KernelProfile { boolean_ms: 0.0, tessellate_ms: 0.0 } }) };
}

/// Run `f` and report the time it spent in kernel booleans and tessellation on this thread.
/// Captures nest: an outer capture also counts what an inner one reports.
pub fn capture_profile<T>(f: impl FnOnce() -> T) -> (T, KernelProfile) {
    let outer = STATE.with(Cell::get);
    STATE.with(|s| s.set(State { capturing: true, busy: outer.busy, profile: KernelProfile::default() }));
    // Restores the outer capture also when `f` unwinds.
    struct Restore(State);
    impl Drop for Restore {
        fn drop(&mut self) {
            let inner = STATE.with(Cell::get).profile;
            let mut outer = self.0;
            outer.profile.boolean_ms += inner.boolean_ms;
            outer.profile.tessellate_ms += inner.tessellate_ms;
            STATE.with(|s| s.set(outer));
        }
    }
    let restore = Restore(outer);
    let r = f();
    let profile = STATE.with(Cell::get).profile;
    drop(restore);
    (r, profile)
}

/// Run `f` as kernel work of `phase`: timed when a capture runs on this thread and no phase
/// is already being timed.
pub(crate) fn timed<T>(phase: Phase, f: impl FnOnce() -> T) -> T {
    let state = STATE.with(Cell::get);
    if !state.capturing || state.busy {
        return f();
    }
    STATE.with(|s| s.set(State { busy: true, ..state }));
    struct Done(web_time::Instant, Phase);
    impl Drop for Done {
        fn drop(&mut self) {
            let ms = self.0.elapsed().as_secs_f64() * 1000.0;
            STATE.with(|s| {
                let mut st = s.get();
                st.busy = false;
                match self.1 {
                    Phase::Boolean => st.profile.boolean_ms += ms,
                    Phase::Tessellate => st.profile.tessellate_ms += ms,
                }
                s.set(st);
            });
        }
    }
    let _done = Done(web_time::Instant::now(), phase);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spin(ms: u64) {
        let t = web_time::Instant::now();
        while t.elapsed() < std::time::Duration::from_millis(ms) {}
    }

    #[test]
    fn nothing_is_timed_without_a_capture() {
        timed(Phase::Boolean, || spin(2));
        let ((), p) = capture_profile(|| {});
        assert_eq!(p, KernelProfile::default());
    }

    #[test]
    fn phases_count_once_and_outer_captures_include_inner_ones() {
        let (inner, outer) = capture_profile(|| {
            timed(Phase::Boolean, || timed(Phase::Tessellate, || spin(3)));
            capture_profile(|| timed(Phase::Tessellate, || spin(3))).1
        });
        assert!(inner.tessellate_ms >= 3.0 && inner.boolean_ms == 0.0, "{inner:?}");
        assert!(outer.boolean_ms >= 3.0, "{outer:?}");
        // The tessellation inside the boolean is boolean time; only the inner capture's counts.
        assert!(outer.tessellate_ms >= inner.tessellate_ms && outer.tessellate_ms < inner.tessellate_ms + 3.0, "{outer:?} {inner:?}");
    }
}
