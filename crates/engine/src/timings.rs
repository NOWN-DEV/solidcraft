//! How long each command of a script or batch took (`solidcraft-cli run --timings`, the MCP
//! `batch` tool's `timings`), and with `profile`, how much of that went to kernel booleans and
//! tessellation ([`solidcraft_kernel::capture_profile`]). See docs/scripting.md.

use serde::Serialize;
use serde_json::{Value, json};

/// One command's wall time, in milliseconds.
#[derive(Clone, Debug, Serialize)]
pub struct CommandTiming {
    /// The step's place in the script or batch, from 0.
    pub index: usize,
    pub command: String,
    pub ms: f64,
    /// Where the time went, when a profile was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Split>,
}

/// A command's time by kind of work: kernel booleans, tessellation, and everything else
/// (sketch solving, rebuilding the timeline around the kernel calls, the command itself).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Split {
    pub boolean_ms: f64,
    pub tessellate_ms: f64,
    pub other_ms: f64,
}

/// Run `f`, timed; with `profile`, its time split by [`Split`].
pub fn time<T>(profile: bool, f: impl FnOnce() -> T) -> (T, f64, Option<Split>) {
    let t = web_time::Instant::now();
    if !profile {
        let r = f();
        return (r, ms_since(t), None);
    }
    let (r, k) = solidcraft_kernel::capture_profile(f);
    let ms = ms_since(t);
    // The phases are parts of the same wall time; rounding never makes "other" negative.
    let other_ms = (ms - k.boolean_ms - k.tessellate_ms).max(0.0);
    (r, ms, Some(Split { boolean_ms: k.boolean_ms, tessellate_ms: k.tessellate_ms, other_ms }))
}

fn ms_since(t: web_time::Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// A run's start, for its `total_ms` (also in the browser, where `std::time::Instant` panics).
#[derive(Clone, Copy, Debug)]
pub struct Clock(web_time::Instant);

impl Clock {
    pub fn start() -> Self {
        Clock(web_time::Instant::now())
    }

    /// Milliseconds since [`Clock::start`].
    pub fn ms(&self) -> f64 {
        ms_since(self.0)
    }
}

/// A whole run's timings: each command's and the run's total (the commands plus the time
/// between them).
#[derive(Clone, Debug, Default, Serialize)]
pub struct Timings {
    pub commands: Vec<CommandTiming>,
    pub total_ms: f64,
}

impl Timings {
    /// As the CLI and MCP report it: `{"timings": [...], "total_ms": …}` merged into `into`.
    pub fn add_to(&self, into: &mut serde_json::Map<String, Value>) {
        into.insert("timings".into(), json!(self.commands));
        into.insert("total_ms".into(), json!(self.total_ms));
    }
}

/// What [`crate::Session::run_script_timed`] gives back: each command's result and the timings.
#[derive(Clone, Debug, Default)]
pub struct ScriptRun {
    pub results: Vec<Value>,
    pub timings: Timings,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn script() -> serde_json::Value {
        json!([
            {"command": "solid.box", "params": {"length": 40, "width": 30, "height": 20, "body_name": "Block"}},
            {"command": "solid.cylinder", "params": {"base": [20, 15, -5], "radius": 6, "height": 30, "body_name": "Pin"}},
            {"command": "solid.combine", "params": {"target": "Block", "tools": ["Pin"], "operation": "cut"}},
        ])
    }

    #[test]
    fn every_command_is_timed_and_the_commands_sum_to_within_the_total() {
        let mut s = Session::default();
        let run = s.run_script_timed(&script(), false).unwrap();
        assert_eq!(run.results.len(), 3);
        let t = &run.timings;
        let ids: Vec<&str> = t.commands.iter().map(|c| c.command.as_str()).collect();
        assert_eq!(ids, ["solid.box", "solid.cylinder", "solid.combine"]);
        assert!(t.commands.iter().enumerate().all(|(i, c)| c.index == i && c.ms.is_finite() && c.ms >= 0.0 && c.profile.is_none()), "{t:?}");
        let sum: f64 = t.commands.iter().map(|c| c.ms).sum();
        assert!(sum <= t.total_ms && t.total_ms - sum < 1.0 + 0.05 * t.total_ms, "sum {sum} total {}", t.total_ms);
        // A script run untimed gives the same results.
        let mut plain = Session::default();
        assert_eq!(plain.run_script(&script()).unwrap().len(), 3);
    }

    #[test]
    fn a_profile_splits_each_command_into_booleans_tessellation_and_the_rest() {
        let mut s = Session::default();
        let run = s.run_script_timed(&script(), true).unwrap();
        for c in &run.timings.commands {
            let p = c.profile.unwrap();
            assert!([p.boolean_ms, p.tessellate_ms, p.other_ms].iter().all(|x| x.is_finite() && *x >= 0.0), "{c:?}");
            assert!(p.boolean_ms + p.tessellate_ms + p.other_ms <= c.ms + 1e-6, "{c:?}");
        }
        let cut = &run.timings.commands[2];
        assert!(cut.profile.unwrap().boolean_ms > 0.0, "the cut is a boolean: {cut:?}");
        let v = serde_json::to_value(&run.timings.commands).unwrap();
        assert!(v[2]["profile"]["boolean_ms"].as_f64().is_some(), "{v}");
        assert!(v[0]["ms"].as_f64().is_some(), "{v}");
    }
}
