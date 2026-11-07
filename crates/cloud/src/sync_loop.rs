//! The Sync loop: when Sync steps run. On start and on coming to the front,
//! a few seconds after the last local change, and every 10 seconds while in
//! front; never in the background. While steps fail (offline) it waits longer
//! each time. While the Device is offline no step runs, and coming online Syncs
//! at once. Lost access blocks the loop until `reconnect`. It tells its
//! `Observer` what it does, runs on a thread of its own, and dropping it
//! cancels the step that is running.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::schedule::{Schedule, Status, Timing};
use crate::step::Progress;
use crate::storage::Storage;
use crate::sync::{SyncState, sync_step_reporting};

/// Hears what the loop does, on its thread. The status comes at the end of
/// every step, so the observer can tell when Sync last went through.
pub trait Observer: Send + Sync {
    /// The names of the Drawings a step wrote or deleted locally. A rename is
    /// the old name and the new one.
    fn changed(&self, names: Vec<String>);
    /// How far the transfers of a step are; none once it is over.
    fn progress(&self, progress: Option<Progress>);
    /// The status: when a step starts transferring, and when a step ends or
    /// the Device goes offline.
    fn status(&self, status: Status);
}

struct Shared {
    schedule: Mutex<Schedule>,
    storage: Mutex<Arc<dyn Storage>>,
    observer: Arc<dyn Observer>,
    wake: Condvar,
    cancel: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Schedule> {
        self.schedule.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn publish(&self, status: Status) {
        self.observer.status(status);
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
        observer: Arc<dyn Observer>,
    ) -> Self {
        let started = Instant::now();
        let shared = Arc::new(Shared {
            schedule: Mutex::new(Schedule::new(timing, Duration::ZERO)),
            storage: Mutex::new(storage),
            observer,
            wake: Condvar::new(),
            cancel: AtomicBool::new(false),
        });
        let worker = Arc::clone(&shared);
        let thread = thread::spawn(move || {
            run(&worker, started, &drawings, &state);
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

    /// The Device lost its connection or has it back. Offline nothing runs;
    /// online again Syncs at once.
    pub fn set_online(&self, online: bool) {
        let now = self.started.elapsed();
        let blocked = {
            let mut schedule = self.shared.lock();
            schedule.set_online(online, now);
            schedule.blocked()
        };
        if !online && !blocked {
            self.shared.publish(Status::Offline);
        }
        self.shared.wake.notify_all();
    }

    /// The user Reconnected with `storage`, which holds the new credentials:
    /// Sync resumes where it stopped. If they are rejected too, the loop is
    /// blocked again.
    pub fn reconnect(&self, storage: Arc<dyn Storage>) {
        *self
            .shared
            .storage
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = storage;
        let now = self.started.elapsed();
        self.shared.lock().reconnected(now);
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

fn run(shared: &Shared, started: Instant, drawings: &std::path::Path, state: &SyncState) {
    let observer = &*shared.observer;
    let mut schedule = shared.lock();
    loop {
        if shared.cancel.load(Relaxed) {
            return;
        }
        let now = started.elapsed();
        if schedule.take_due(now) {
            drop(schedule);
            let storage = Arc::clone(&shared.storage.lock().unwrap_or_else(|e| e.into_inner()));
            let transferring = AtomicBool::new(false);
            let outcome = sync_step_reporting(drawings, state, &*storage, &shared.cancel, &|p| {
                if !transferring.swap(true, Relaxed) {
                    shared.publish(Status::Syncing);
                }
                observer.progress(Some(p));
            });
            if transferring.load(Relaxed) {
                observer.progress(None);
            }
            if !outcome.changed.is_empty() {
                observer.changed(outcome.changed.clone());
            }
            let cancelled = shared.cancel.load(Relaxed);
            if !cancelled {
                shared.publish(Status::after(&outcome));
            }
            schedule = shared.lock();
            let now = started.elapsed();
            // Cancelling is no failure of the cloud.
            schedule.step_ended(now, outcome.error.is_none());
            if outcome.access_lost {
                schedule.access_lost();
            }
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

    /// Remembers what the loop told.
    #[derive(Default)]
    struct Told {
        changed: Mutex<Vec<String>>,
        progress: Mutex<Vec<Option<(u32, u32)>>>,
        statuses: Mutex<Vec<Status>>,
    }

    impl Observer for Told {
        fn changed(&self, names: Vec<String>) {
            self.changed.lock().unwrap().extend(names);
        }
        fn progress(&self, progress: Option<Progress>) {
            let p = progress.map(|p| (p.done, p.total));
            self.progress.lock().unwrap().push(p);
        }
        fn status(&self, status: Status) {
            self.statuses.lock().unwrap().push(status);
        }
    }

    impl Told {
        fn statuses(&self) -> Vec<Status> {
            self.statuses.lock().unwrap().clone()
        }
    }

    fn fast() -> Timing {
        Timing {
            debounce: Duration::from_millis(20),
            poll: Duration::from_secs(3600),
            max_backoff: Duration::from_secs(3600),
        }
    }

    const DRAWING: &[u8] =
        br#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#;

    #[test]
    fn the_loop_syncs_on_start_and_on_local_changes() {
        let (cloud, drawings, state) = (Scratch::new("c"), Scratch::new("d"), Scratch::new("s"));
        let storage = Arc::new(DirStorage::open(&cloud.0).unwrap());
        put_bytes(&*storage, "Sketch.excalidraw", b"one");
        let told = Arc::new(Told::default());
        let sync = SyncLoop::start(
            drawings.0.clone(),
            SyncState::open(&state.0).unwrap(),
            storage.clone(),
            fast(),
            told.clone(),
        );

        eventually(|| *told.changed.lock().unwrap() == ["Sketch"]);
        std::fs::write(drawings.0.join("New.excalidraw"), b"new").unwrap();
        sync.local_change();
        eventually(|| storage.list().unwrap().len() == 2);
    }

    #[test]
    fn the_loop_tells_progress_and_status() {
        let (cloud, drawings, state) = (Scratch::new("c"), Scratch::new("d"), Scratch::new("s"));
        let storage = Arc::new(DirStorage::open(&cloud.0).unwrap());
        put_bytes(&*storage, "A.excalidraw", DRAWING);
        let told = Arc::new(Told::default());
        let _sync = SyncLoop::start(
            drawings.0.clone(),
            SyncState::open(&state.0).unwrap(),
            storage,
            fast(),
            told.clone(),
        );

        eventually(|| told.statuses() == [Status::Syncing, Status::UpToDate]);
        assert_eq!(
            *told.progress.lock().unwrap(),
            [Some((0, 1)), Some((1, 1)), None]
        );
    }

    /// A storage that rejects or accepts, switchable.
    struct Gate {
        inner: DirStorage,
        reject: AtomicBool,
        calls: std::sync::atomic::AtomicU32,
    }

    impl Gate {
        fn check(&self) -> io::Result<()> {
            self.calls.fetch_add(1, Relaxed);
            if self.reject.load(Relaxed) {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        }
    }

    impl Storage for Gate {
        fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
            self.check()?;
            self.inner.put(name, data)
        }
        fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
            self.check()?;
            self.inner.get(name)
        }
        fn list(&self) -> io::Result<Vec<crate::Blob>> {
            self.check()?;
            self.inner.list()
        }
        fn delete(&self, name: &str) -> io::Result<()> {
            self.check()?;
            self.inner.delete(name)
        }
    }

    fn gate(cloud: &Scratch, reject: bool) -> Arc<Gate> {
        Arc::new(Gate {
            inner: DirStorage::open(&cloud.0).unwrap(),
            reject: AtomicBool::new(reject),
            calls: Default::default(),
        })
    }

    #[test]
    fn lost_access_stops_the_loop_and_a_reconnect_resumes_it() {
        let (cloud, drawings, state) = (Scratch::new("c"), Scratch::new("d"), Scratch::new("s"));
        let rejecting = gate(&cloud, true);
        std::fs::write(drawings.0.join("A.excalidraw"), DRAWING).unwrap();
        let told = Arc::new(Told::default());
        let sync = SyncLoop::start(
            drawings.0.clone(),
            SyncState::open(&state.0).unwrap(),
            rejecting.clone(),
            fast(),
            told.clone(),
        );
        eventually(|| told.statuses().last() == Some(&Status::AccessLost));
        let calls = rejecting.calls.load(Relaxed);
        sync.local_change();
        sync.came_to_front();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            rejecting.calls.load(Relaxed),
            calls,
            "nothing runs while blocked"
        );

        // Rejected again: blocked again.
        sync.reconnect(rejecting.clone());
        eventually(|| rejecting.calls.load(Relaxed) > calls);
        thread::sleep(Duration::from_millis(100));
        let calls = rejecting.calls.load(Relaxed);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(rejecting.calls.load(Relaxed), calls);
        assert_eq!(told.statuses().last(), Some(&Status::AccessLost));

        sync.reconnect(gate(&cloud, false));
        eventually(|| told.statuses().last() == Some(&Status::UpToDate));
        assert!(cloud.0.join("A.excalidraw").exists());
    }

    #[test]
    fn the_loop_runs_no_step_offline_and_syncs_at_once_when_back_online() {
        let (cloud, drawings, state) = (Scratch::new("c"), Scratch::new("d"), Scratch::new("s"));
        let storage = gate(&cloud, false);
        let told = Arc::new(Told::default());
        let sync = SyncLoop::start(
            drawings.0.clone(),
            SyncState::open(&state.0).unwrap(),
            storage.clone(),
            fast(),
            told.clone(),
        );
        eventually(|| told.statuses() == [Status::UpToDate]);

        sync.set_online(false);
        eventually(|| told.statuses().last() == Some(&Status::Offline));
        std::fs::write(drawings.0.join("A.excalidraw"), DRAWING).unwrap();
        sync.local_change();
        thread::sleep(Duration::from_millis(150));
        assert!(!cloud.0.join("A.excalidraw").exists());

        sync.set_online(true);
        eventually(|| cloud.0.join("A.excalidraw").exists());
        eventually(|| told.statuses().last() == Some(&Status::UpToDate));
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
            Arc::new(Told::default()),
        );
        eventually(|| started.load(Relaxed));
        drop(sync); // joins: returns only once the transfer stopped
        assert!(!drawings.0.join("Big.excalidraw").exists());
    }
}
