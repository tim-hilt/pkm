//! The Sync loop: when Sync steps run. On start and on coming to the front,
//! a few seconds after the last local change, and every 10 seconds while in
//! front; never in the background. While steps fail (offline) it waits longer
//! each time. It runs on a thread of its own, and dropping it cancels the step
//! that is running.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::storage::Storage;
use crate::sync::{SyncState, sync_step};

/// How long the loop waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Quiet time after the last local change before a step runs.
    pub debounce: Duration,
    /// Time between steps while in front and nothing happens.
    pub poll: Duration,
    /// The longest wait between steps while they fail.
    pub max_backoff: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            debounce: Duration::from_secs(3),
            poll: Duration::from_secs(10),
            max_backoff: Duration::from_secs(60),
        }
    }
}

/// When the next step is due, as a pure function of what happened and when.
/// Times are offsets from any fixed start, so a test is its own clock.
#[derive(Debug)]
pub(crate) struct Schedule {
    timing: Timing,
    in_front: bool,
    /// When the next step runs; none while nothing is scheduled.
    next: Option<Duration>,
    /// Steps that failed in a row.
    failures: u32,
}

impl Schedule {
    /// In front, with a step due at once.
    pub(crate) fn new(timing: Timing, now: Duration) -> Self {
        Self {
            timing,
            in_front: true,
            next: Some(now),
            failures: 0,
        }
    }

    pub(crate) fn came_to_front(&mut self, now: Duration) {
        self.in_front = true;
        self.next = Some(now);
    }

    pub(crate) fn went_to_background(&mut self) {
        self.in_front = false;
        self.next = None;
    }

    /// Every change pushes the step back, so a burst becomes one step.
    pub(crate) fn local_change(&mut self, now: Duration) {
        if self.in_front {
            self.next = Some(now + self.timing.debounce);
        }
    }

    /// How long until the next step; none if there won't be one.
    pub(crate) fn wait(&self, now: Duration) -> Option<Duration> {
        self.next.map(|next| next.saturating_sub(now))
    }

    /// Whether a step runs now. If so, the schedule counts it as running.
    pub(crate) fn take_due(&mut self, now: Duration) -> bool {
        match self.next {
            Some(next) if next <= now => {
                self.next = None;
                true
            }
            _ => false,
        }
    }

    /// A step ended at `now`. A change or a coming to the front during the
    /// step keeps its earlier time.
    pub(crate) fn step_ended(&mut self, now: Duration, succeeded: bool) {
        self.failures = if succeeded { 0 } else { self.failures + 1 };
        if !self.in_front {
            return;
        }
        let wait = if self.failures == 0 {
            self.timing.poll
        } else {
            // Doubles with every failure.
            let doubled = self.timing.poll.saturating_mul(1 << self.failures.min(16));
            doubled.min(self.timing.max_backoff.max(self.timing.poll))
        };
        let planned = now + wait;
        self.next = Some(self.next.map_or(planned, |next| next.min(planned)));
    }
}

/// Called with the names of the Drawings a step wrote or deleted locally.
pub type OnChanged = Arc<dyn Fn(Vec<String>) + Send + Sync>;

struct Shared {
    schedule: Mutex<Schedule>,
    wake: Condvar,
    cancel: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Schedule> {
        self.schedule.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Runs Sync steps between the Drawings folder and a storage until dropped.
pub struct SyncLoop {
    shared: Arc<Shared>,
    started: Instant,
    thread: Option<JoinHandle<()>>,
}

impl SyncLoop {
    /// Starts Syncing `drawings` with `storage`, remembering in the state
    /// folder `state`. The first step runs at once.
    pub fn start(
        drawings: PathBuf,
        state: SyncState,
        storage: Arc<dyn Storage>,
        timing: Timing,
        on_changed: OnChanged,
    ) -> Self {
        let started = Instant::now();
        let shared = Arc::new(Shared {
            schedule: Mutex::new(Schedule::new(timing, Duration::ZERO)),
            wake: Condvar::new(),
            cancel: AtomicBool::new(false),
        });
        let worker = Arc::clone(&shared);
        let thread = thread::spawn(move || {
            run(&worker, started, &drawings, &state, &*storage, &*on_changed);
        });
        Self {
            shared,
            started,
            thread: Some(thread),
        }
    }

    /// The app came to the front: Sync at once.
    pub fn came_to_front(&self) {
        let now = self.started.elapsed();
        self.shared.lock().came_to_front(now);
        self.shared.wake.notify_all();
    }

    /// The app went to the background: nothing runs until it is back.
    pub fn went_to_background(&self) {
        self.shared.lock().went_to_background();
        self.shared.wake.notify_all();
    }

    /// A Drawing changed on this Device.
    pub fn local_change(&self) {
        let now = self.started.elapsed();
        self.shared.lock().local_change(now);
        self.shared.wake.notify_all();
    }
}

impl Drop for SyncLoop {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Relaxed);
        self.shared.wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    shared: &Shared,
    started: Instant,
    drawings: &std::path::Path,
    state: &SyncState,
    storage: &dyn Storage,
    on_changed: &(dyn Fn(Vec<String>) + Send + Sync),
) {
    let mut schedule = shared.lock();
    loop {
        if shared.cancel.load(Relaxed) {
            return;
        }
        let now = started.elapsed();
        if schedule.take_due(now) {
            drop(schedule);
            let outcome = sync_step(drawings, state, storage, &shared.cancel);
            if !outcome.changed.is_empty() {
                on_changed(outcome.changed);
            }
            schedule = shared.lock();
            let now = started.elapsed();
            // Cancelling is no failure of the cloud.
            schedule.step_ended(now, outcome.error.is_none());
        } else {
            schedule = match schedule.wait(now) {
                Some(wait) => {
                    shared
                        .wake
                        .wait_timeout(schedule, wait)
                        .unwrap_or_else(|e| e.into_inner())
                        .0
                }
                None => shared
                    .wake
                    .wait(schedule)
                    .unwrap_or_else(|e| e.into_inner()),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DirStorage;
    use crate::storage::contract::{Scratch, put_bytes};
    use std::io::{self, Read};

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// Whether a step runs at `now`, counting it as run (and succeeded) if so.
    fn runs(schedule: &mut Schedule, now: u64) -> bool {
        let due = schedule.take_due(secs(now));
        if due {
            schedule.step_ended(secs(now), true);
        }
        due
    }

    fn schedule() -> Schedule {
        Schedule::new(Timing::default(), secs(0))
    }

    #[test]
    fn syncs_at_once_on_start_and_then_polls_every_10_seconds() {
        let mut s = schedule();
        assert!(runs(&mut s, 0));
        assert!(!runs(&mut s, 9));
        assert!(runs(&mut s, 10));
        assert!(!runs(&mut s, 19));
        assert!(runs(&mut s, 20));
    }

    #[test]
    fn coming_to_front_syncs_at_once() {
        let mut s = schedule();
        runs(&mut s, 0);
        s.went_to_background();
        s.came_to_front(secs(4));
        assert!(runs(&mut s, 4));
    }

    #[test]
    fn nothing_runs_in_the_background() {
        let mut s = schedule();
        runs(&mut s, 0);
        s.went_to_background();
        s.local_change(secs(5));
        assert_eq!(s.wait(secs(5)), None);
        assert!(!runs(&mut s, 1000));
    }

    #[test]
    fn a_burst_of_local_changes_is_one_step_after_the_debounce() {
        let mut s = schedule();
        runs(&mut s, 0);
        for t in [1, 2, 3, 4] {
            s.local_change(secs(t));
        }
        assert!(!runs(&mut s, 6)); // 3 s after the first change, but not the last
        assert!(runs(&mut s, 7));
        assert!(!runs(&mut s, 8));
    }

    #[test]
    fn a_change_during_a_step_is_not_lost() {
        let mut s = schedule();
        assert!(s.take_due(secs(0)));
        s.local_change(secs(1));
        s.step_ended(secs(2), true);
        assert!(runs(&mut s, 4));
    }

    #[test]
    fn failing_steps_back_off_up_to_a_limit_and_success_resets_it() {
        let mut s = schedule();
        let mut now = 0;
        let mut waits = vec![];
        for _ in 0..5 {
            assert!(s.take_due(secs(now)));
            s.step_ended(secs(now), false);
            let wait = s.wait(secs(now)).unwrap().as_secs();
            waits.push(wait);
            now += wait;
        }
        assert_eq!(waits, [20, 40, 60, 60, 60]);
        assert!(s.take_due(secs(now)));
        s.step_ended(secs(now), true);
        assert_eq!(s.wait(secs(now)), Some(secs(10)));
    }

    /// Waits up to 5 s for `done`.
    fn eventually(done: impl Fn() -> bool) {
        for _ in 0..500 {
            if done() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("did not happen in time");
    }

    #[test]
    fn the_loop_syncs_on_start_and_on_local_changes() {
        let (cloud, drawings, state) = (Scratch::new("c"), Scratch::new("d"), Scratch::new("s"));
        let storage = Arc::new(DirStorage::open(&cloud.0).unwrap());
        put_bytes(&*storage, "Sketch.excalidraw", b"one");
        let changed = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&changed);
        let timing = Timing {
            debounce: Duration::from_millis(20),
            poll: Duration::from_secs(3600),
            max_backoff: Duration::from_secs(3600),
        };
        let sync = SyncLoop::start(
            drawings.0.clone(),
            SyncState::open(&state.0).unwrap(),
            storage.clone(),
            timing,
            Arc::new(move |names| seen.lock().unwrap().extend(names)),
        );

        eventually(|| *changed.lock().unwrap() == ["Sketch"]);
        std::fs::write(drawings.0.join("New.excalidraw"), b"new").unwrap();
        sync.local_change();
        eventually(|| storage.list().unwrap().len() == 2);
    }

    /// A storage whose downloads never end until cancelled.
    struct Endless {
        started: Arc<AtomicBool>,
    }

    struct Trickle(Arc<AtomicBool>);

    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.store(true, Relaxed);
            thread::sleep(Duration::from_millis(5));
            buf[0] = b'x';
            Ok(1)
        }
    }

    impl Storage for Endless {
        fn put(&self, _: &str, _: &mut dyn Read) -> io::Result<()> {
            Ok(())
        }
        fn get(&self, _: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
            Ok(Some(Box::new(Trickle(Arc::clone(&self.started)))))
        }
        fn list(&self) -> io::Result<Vec<crate::Blob>> {
            Ok(vec![crate::Blob {
                name: "Big.excalidraw".into(),
                marker: "1".into(),
            }])
        }
        fn delete(&self, _: &str) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn dropping_the_loop_cancels_a_running_transfer() {
        let (drawings, state) = (Scratch::new("d"), Scratch::new("s"));
        let started = Arc::new(AtomicBool::new(false));
        let sync = SyncLoop::start(
            drawings.0.clone(),
            SyncState::open(&state.0).unwrap(),
            Arc::new(Endless {
                started: Arc::clone(&started),
            }),
            Timing::default(),
            Arc::new(|_| {}),
        );
        eventually(|| started.load(Relaxed));
        drop(sync); // joins: returns only once the transfer stopped
        assert!(!drawings.0.join("Big.excalidraw").exists());
    }
}
