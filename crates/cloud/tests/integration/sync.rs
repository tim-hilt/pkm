//! Sync steps of several Devices (temporary folders) against one shared
//! directory storage standing in for the cloud.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

use excalidraw_native_cloud::{Blob, DirStorage, Storage, SyncState, sync_step};

struct Scratch(PathBuf);

impl Scratch {
    fn new(prefix: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "excalidraw-native-sync-{prefix}-{}-{}",
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

/// The calls a storage got.
#[derive(Default)]
struct Calls {
    lists: u32,
    gets: Vec<String>,
    puts: Vec<String>,
    deletes: Vec<String>,
}

struct Counting {
    inner: DirStorage,
    calls: Mutex<Calls>,
}

impl Storage for Counting {
    fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
        self.calls.lock().unwrap().puts.push(name.into());
        self.inner.put(name, data)
    }
    fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
        self.calls.lock().unwrap().gets.push(name.into());
        self.inner.get(name)
    }
    fn list(&self) -> io::Result<Vec<Blob>> {
        self.calls.lock().unwrap().lists += 1;
        self.inner.list()
    }
    fn delete(&self, name: &str) -> io::Result<()> {
        self.calls.lock().unwrap().deletes.push(name.into());
        self.inner.delete(name)
    }
}

/// The shared cloud folder.
struct Cloud {
    dir: Scratch,
    storage: Counting,
}

impl Cloud {
    fn new() -> Self {
        let dir = Scratch::new("cloud");
        let storage = Counting {
            inner: DirStorage::open(&dir.0).unwrap(),
            calls: Mutex::default(),
        };
        Cloud { dir, storage }
    }

    /// Edits the cloud folder directly, as a server user would.
    fn write(&self, file: &str, bytes: &[u8]) {
        fs::write(self.dir.0.join(file), bytes).unwrap();
    }

    fn remove(&self, file: &str) {
        fs::remove_file(self.dir.0.join(file)).unwrap();
    }

    fn read(&self, file: &str) -> Option<Vec<u8>> {
        fs::read(self.dir.0.join(file)).ok()
    }

    fn take_calls(&self) -> Calls {
        std::mem::take(&mut *self.storage.calls.lock().unwrap())
    }
}

/// A Device: its Drawings folder and its Sync state folder.
struct Device {
    drawings: Scratch,
    state: Scratch,
}

impl Device {
    fn new() -> Self {
        Device {
            drawings: Scratch::new("drawings"),
            state: Scratch::new("state"),
        }
    }

    fn write(&self, file: &str, bytes: &[u8]) {
        fs::write(self.drawings.0.join(file), bytes).unwrap();
    }

    fn remove(&self, file: &str) {
        fs::remove_file(self.drawings.0.join(file)).unwrap();
    }

    fn rename(&self, from: &str, to: &str) {
        fs::rename(self.drawings.0.join(from), self.drawings.0.join(to)).unwrap();
    }

    fn read(&self, file: &str) -> Option<Vec<u8>> {
        fs::read(self.drawings.0.join(file)).ok()
    }

    fn files(&self) -> Vec<String> {
        listing(&self.drawings.0)
    }

    /// One Sync step; returns the names it changed here.
    fn sync(&self, cloud: &Cloud) -> Vec<String> {
        let state = SyncState::open(&self.state.0).unwrap();
        let outcome = sync_step(
            &self.drawings.0,
            &state,
            &cloud.storage,
            &AtomicBool::new(false),
        );
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        let mut changed = outcome.changed;
        changed.sort();
        changed
    }
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_new_drawing_reaches_the_cloud_and_other_devices() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");

    assert!(phone.sync(&cloud).is_empty());
    assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), b"one");
    assert_eq!(tablet.sync(&cloud), ["Sketch"]);
    assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"one");
}

#[test]
fn a_changed_drawing_reaches_other_devices() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);
    tablet.sync(&cloud);

    phone.write("Sketch.excalidraw", b"two!");
    phone.sync(&cloud);

    assert_eq!(tablet.sync(&cloud), ["Sketch"]);
    assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"two!");
}

#[test]
fn a_deleted_drawing_is_deleted_on_other_devices() {
    let cloud = Cloud::new();
    let (phone, tablet, laptop) = (Device::new(), Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);
    tablet.sync(&cloud);
    laptop.sync(&cloud);

    phone.remove("Sketch.excalidraw");
    phone.sync(&cloud);

    assert!(cloud.read("Sketch.excalidraw").is_none());
    assert_eq!(tablet.sync(&cloud), ["Sketch"]);
    assert_eq!(laptop.sync(&cloud), ["Sketch"]);
    assert!(tablet.files().is_empty() && laptop.files().is_empty());
}

#[test]
fn a_rename_is_a_delete_of_the_old_name_and_an_add_of_the_new_one() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Old.excalidraw", b"one");
    phone.sync(&cloud);
    tablet.sync(&cloud);

    phone.rename("Old.excalidraw", "New.excalidraw");
    phone.sync(&cloud);

    assert_eq!(tablet.sync(&cloud), ["New", "Old"]);
    assert_eq!(tablet.files(), ["New.excalidraw"]);
    assert_eq!(tablet.read("New.excalidraw").unwrap(), b"one");
}

#[test]
fn a_duplicate_is_a_new_drawing() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);
    tablet.sync(&cloud);

    phone.write("Sketch copy.excalidraw", b"one");
    phone.sync(&cloud);

    assert_eq!(tablet.sync(&cloud), ["Sketch copy"]);
    assert_eq!(
        tablet.files(),
        ["Sketch copy.excalidraw", "Sketch.excalidraw"]
    );
}

#[test]
fn edits_made_directly_in_either_folder_are_synced_like_the_apps_own() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("A.excalidraw", b"a");
    phone.write("B.excalidraw", b"b");
    phone.sync(&cloud);
    tablet.sync(&cloud);

    // On the server: edit A, remove B, add C. In Files on the tablet: add D.
    cloud.write("A.excalidraw", b"a, edited");
    cloud.remove("B.excalidraw");
    cloud.write("C.excalidraw", b"c");
    tablet.write("D.excalidraw", b"d");
    tablet.sync(&cloud);

    assert_eq!(
        tablet.files(),
        ["A.excalidraw", "C.excalidraw", "D.excalidraw"]
    );
    assert_eq!(tablet.read("A.excalidraw").unwrap(), b"a, edited");
    phone.sync(&cloud);
    assert_eq!(phone.files(), tablet.files());
}

#[test]
fn files_that_are_no_drawings_are_ignored() {
    let cloud = Cloud::new();
    let phone = Device::new();
    phone.write("notes.txt", b"x");
    cloud.write("readme.md", b"y");
    phone.sync(&cloud);

    assert_eq!(phone.files(), ["notes.txt"]);
    assert!(cloud.read("notes.txt").is_none());
}

#[test]
fn nothing_half_written_is_left_on_either_side() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", &vec![b'x'; 1 << 20]);
    phone.sync(&cloud);
    tablet.sync(&cloud);

    assert_eq!(tablet.files(), ["Sketch.excalidraw"]);
    assert_eq!(tablet.read("Sketch.excalidraw").unwrap().len(), 1 << 20);
}

#[test]
fn unknown_content_is_preserved_byte_for_byte() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    let bytes = "{\"type\":\"excalidraw\",\"elements\":[{\"type\":\"hologram\",\"x\":1.50,\"glow\":[1,2]}],\"zzz\":\"\u{1F600}\"}\n  ";
    phone.write("Sketch.excalidraw", bytes.as_bytes());
    phone.sync(&cloud);
    tablet.sync(&cloud);

    assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), bytes.as_bytes());
}

#[test]
fn an_idle_poll_is_one_listing_and_transfers_nothing() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);
    tablet.sync(&cloud);
    cloud.take_calls();

    phone.sync(&cloud);
    tablet.sync(&cloud);

    let calls = cloud.take_calls();
    assert_eq!(calls.lists, 2);
    assert!(calls.gets.is_empty() && calls.puts.is_empty() && calls.deletes.is_empty());
}

#[test]
fn only_changed_drawings_are_transferred() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    for name in ["A", "B", "C"] {
        phone.write(&format!("{name}.excalidraw"), name.as_bytes());
    }
    phone.sync(&cloud);
    tablet.sync(&cloud);
    cloud.take_calls();

    phone.write("B.excalidraw", b"B, changed");
    phone.sync(&cloud);
    let calls = cloud.take_calls();
    assert_eq!(calls.puts, ["B.excalidraw"]);
    assert!(calls.gets.is_empty());

    tablet.sync(&cloud);
    let calls = cloud.take_calls();
    assert_eq!(calls.gets, ["B.excalidraw"]);
    assert!(calls.puts.is_empty());
}

#[test]
fn a_drawing_changed_on_both_sides_is_left_as_it_is_on_both() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);
    tablet.sync(&cloud);

    phone.write("Sketch.excalidraw", b"phone");
    cloud.write("Sketch.excalidraw", b"cloud!");
    assert!(phone.sync(&cloud).is_empty());
    assert_eq!(phone.read("Sketch.excalidraw").unwrap(), b"phone");
    assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), b"cloud!");
}

#[test]
fn an_edit_wins_over_a_delete() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);
    tablet.sync(&cloud);

    phone.remove("Sketch.excalidraw");
    tablet.write("Sketch.excalidraw", b"edited");
    phone.sync(&cloud); // the delete reaches the cloud
    tablet.sync(&cloud); // the edit brings it back
    assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), b"edited");
    assert_eq!(phone.sync(&cloud), ["Sketch"]);
    assert_eq!(phone.read("Sketch.excalidraw").unwrap(), b"edited");

    // The other way round: edited in the cloud, deleted here.
    phone.remove("Sketch.excalidraw");
    cloud.write("Sketch.excalidraw", b"edited again");
    phone.sync(&cloud);
    assert_eq!(phone.read("Sketch.excalidraw").unwrap(), b"edited again");
}

#[test]
fn sync_state_stays_out_of_the_drawings_and_is_forgotten_on_disconnect() {
    let cloud = Cloud::new();
    let phone = Device::new();
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);
    assert_eq!(phone.files(), ["Sketch.excalidraw"]);
    assert!(!listing(&phone.state.0).is_empty());

    SyncState::forget(&phone.state.0).unwrap();
    assert!(!phone.state.0.exists());

    // Without history a Drawing deleted meanwhile comes back from the cloud.
    phone.remove("Sketch.excalidraw");
    phone.sync(&cloud);
    assert_eq!(phone.files(), ["Sketch.excalidraw"]);
}

#[test]
fn a_cancelled_step_changes_nothing_and_says_so() {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Sketch.excalidraw", b"one");
    phone.sync(&cloud);

    let state = SyncState::open(&tablet.state.0).unwrap();
    let outcome = sync_step(
        &tablet.drawings.0,
        &state,
        &cloud.storage,
        &AtomicBool::new(true),
    );
    assert_eq!(outcome.error.unwrap().kind(), io::ErrorKind::Interrupted);
    assert!(tablet.files().is_empty());
}
