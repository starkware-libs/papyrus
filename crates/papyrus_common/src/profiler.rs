use std::time::{Duration, Instant};

use tracing::debug;

/// A simple profiler that logs the time between events.
/// The results are logged when the profiler is dropped.
///
/// ```(ignore)
/// let mut profiler = Profiler::start("my scenario");
/// // do something
/// profiler.log("event 1");
/// // do something else
/// profiler.log("event 2");
/// ```
pub struct Profiler {
    stop_watch: std::time::Instant,
    events: Vec<(String, Duration)>,
    scenario: String,
}

impl Profiler {
    pub fn start(scenario: &str) -> Self {
        Self { stop_watch: Instant::now(), events: vec![], scenario: scenario.to_string() }
    }

    pub fn log(&mut self, event: &str) {
        let elapsed = self.stop_watch.elapsed();
        self.events.push((event.to_string(), elapsed));
        self.stop_watch = Instant::now();
    }
}

impl Drop for Profiler {
    fn drop(&mut self) {
        debug!(
            target: "profiling",
            scenario = ?self.scenario,
            "{} profiling results: {:#?}",
            self.scenario,
            self.events
        );
    }
}
