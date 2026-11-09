//! What a Sync step reports: progress through the transfers, and the two
//! problems that block Sync: lost access and a full cloud.

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

use pkm_cloud::{Blob, DirStorage, Progress, StepOutcome, Storage, SyncState, sync_step_reporting};

struct Scratch(PathBuf);

impl Scratch {
    fn new(prefix: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pkm-problems-{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A directory storage that can be made to fail.
struct Flaky {
    inner: DirStorage,
    /// What `put` fails with, while set.
    put_fails: Mutex<Option<io::ErrorKind>>,
    /// What every call fails with, while set.
    all_fail: Mutex<Option<io::ErrorKind>>,
}

impl Flaky {
    fn check(&self, slot: &Mutex<Option<io::ErrorKind>>) -> io::Result<()> {
        match *slot.lock().unwrap() {
            Some(kind) => Err(io::Error::from(kind)),
            None => Ok(()),
        }
    }
}

impl Storage for Flaky {
    fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
        self.check(&self.all_fail)?;
        self.check(&self.put_fails)?;
        self.inner.put(name, data)
    }
    fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
        self.check(&self.all_fail)?;
        self.inner.get(name)
    }
    fn list(&self) -> io::Result<Vec<Blob>> {
        self.check(&self.all_fail)?;
        self.inner.list()
    }
    fn delete(&self, name: &str) -> io::Result<()> {
        self.check(&self.all_fail)?;
        self.inner.delete(name)
    }
}

struct World {
    cloud: Scratch,
    drawings: Scratch,
    state: Scratch,
    storage: Flaky,
}

impl World {
    fn new() -> Self {
        let cloud = Scratch::new("cloud");
        let storage = Flaky {
            inner: DirStorage::open(&cloud.0).unwrap(),
            put_fails: Mutex::default(),
            all_fail: Mutex::default(),
        };
        World {
            cloud,
            drawings: Scratch::new("drawings"),
            state: Scratch::new("state"),
            storage,
        }
    }

    fn local(&self, file: &str, bytes: &[u8]) {
        fs::write(self.drawings.0.join(file), bytes).unwrap();
    }

    fn cloud(&self, file: &str, bytes: &[u8]) {
        fs::write(self.cloud.0.join(file), bytes).unwrap();
    }

    fn step(&self) -> (StepOutcome, Vec<(u32, u32)>) {
        let reports = Mutex::new(Vec::new());
        let outcome = sync_step_reporting(
            &self.drawings.0,
            &SyncState::open(&self.state.0).unwrap(),
            &self.storage,
            &AtomicBool::new(false),
            &|p: Progress| reports.lock().unwrap().push((p.done, p.total)),
        );
        (outcome, reports.into_inner().unwrap())
    }
}

/// A Drawing that merges fine: the smallest valid file.
const DRAWING: &[u8] =
    br#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#;

#[test]
fn progress_counts_the_drawings_done_of_all_to_transfer() {
    let w = World::new();
    w.local("Up.excalidraw", DRAWING);
    w.cloud("Down.excalidraw", DRAWING);

    let (outcome, reports) = w.step();

    assert!(outcome.error.is_none());
    assert_eq!(reports, [(0, 2), (1, 2), (2, 2)]);
}

#[test]
fn nothing_to_transfer_reports_no_progress() {
    let w = World::new();
    w.local("A.excalidraw", DRAWING);
    w.step();

    let (_, reports) = w.step();

    assert_eq!(reports, []);
}

#[test]
fn deletes_are_exchanges_of_state_and_report_no_progress() {
    let w = World::new();
    w.local("A.excalidraw", DRAWING);
    w.step();
    fs::remove_file(w.drawings.0.join("A.excalidraw")).unwrap();

    let (_, reports) = w.step();

    assert_eq!(reports, []);
    assert!(!w.cloud.0.join("A.excalidraw").exists());
}

#[test]
fn rejected_credentials_are_access_lost() {
    let w = World::new();
    w.local("A.excalidraw", DRAWING);
    *w.storage.all_fail.lock().unwrap() = Some(io::ErrorKind::PermissionDenied);

    let (outcome, _) = w.step();

    assert!(outcome.access_lost);
}

#[test]
fn a_full_cloud_blocks_uploads_but_not_downloads() {
    let w = World::new();
    w.local("Up.excalidraw", DRAWING);
    w.cloud("Down.excalidraw", DRAWING);
    *w.storage.put_fails.lock().unwrap() = Some(io::ErrorKind::StorageFull);

    let (outcome, _) = w.step();

    assert!(outcome.storage_full);
    assert!(!outcome.access_lost);
    assert!(outcome.error.is_none(), "full is not an offline failure");
    assert!(w.drawings.0.join("Down.excalidraw").exists());
    assert!(!w.cloud.0.join("Up.excalidraw").exists());
}

#[test]
fn a_full_cloud_clears_itself_once_an_upload_succeeds() {
    let w = World::new();
    w.local("Up.excalidraw", DRAWING);
    *w.storage.put_fails.lock().unwrap() = Some(io::ErrorKind::StorageFull);
    assert!(w.step().0.storage_full);

    *w.storage.put_fails.lock().unwrap() = None;
    let (outcome, _) = w.step();

    assert!(!outcome.storage_full);
    assert!(w.cloud.0.join("Up.excalidraw").exists());
}

#[test]
fn other_failures_are_errors_and_no_problem() {
    let w = World::new();
    *w.storage.all_fail.lock().unwrap() = Some(io::ErrorKind::ConnectionRefused);

    let (outcome, _) = w.step();

    assert!(outcome.error.is_some());
    assert!(!outcome.access_lost && !outcome.storage_full);
}
