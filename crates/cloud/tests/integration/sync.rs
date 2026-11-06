//! Sync steps of several Devices (temporary folders) against one shared
//! directory storage standing in for the cloud.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

use excalidraw_native_cloud::{Blob, DirStorage, Storage, SyncState, sync_step};
use excalidraw_native_core::json;

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
fn a_drawing_that_cant_be_parsed_is_skipped_and_left_as_it_is_on_both_sides() {
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

    // Also when only the one side is no Drawing.
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write("Fine.excalidraw", &drawing(&[element("a", 1, "a0", 0)]));
    phone.sync(&cloud);
    tablet.sync(&cloud);
    phone.write("Fine.excalidraw", &drawing(&[element("a", 2, "a0", 5)]));
    cloud.write("Fine.excalidraw", b"{broken");
    let before = phone.read("Fine.excalidraw").unwrap();
    assert!(phone.sync(&cloud).is_empty());
    assert_eq!(phone.read("Fine.excalidraw").unwrap(), before);
    assert_eq!(cloud.read("Fine.excalidraw").unwrap(), b"{broken");
    // The cloud is fine, the local file is broken.
    cloud.write("Fine.excalidraw", &before);
    tablet.write("Fine.excalidraw", b"{broken");
    tablet.sync(&cloud);
    assert_eq!(tablet.read("Fine.excalidraw").unwrap(), b"{broken");
    assert_eq!(cloud.read("Fine.excalidraw").unwrap(), before);
}

// --- Concurrent changes -----------------------------------------------------

fn element(id: &str, version: u32, index: &str, x: u32) -> String {
    format!(
        r#"{{"id":"{id}","type":"rectangle","version":{version},"versionNonce":7,"index":"{index}","isDeleted":false,"x":{x}}}"#
    )
}

fn erased(id: &str, version: u32, index: &str) -> String {
    element(id, version, index, 0).replace("false", "true")
}

fn drawing(elements: &[String]) -> Vec<u8> {
    with(elements, "{}", "{}")
}

fn with(elements: &[String], app_state: &str, files: &str) -> Vec<u8> {
    format!(
        r#"{{"type":"excalidraw","version":2,"elements":[{}],"appState":{app_state},"files":{files}}}"#,
        elements.join(",")
    )
    .into_bytes()
}

/// `(id, version, isDeleted)` of the Elements of a Drawing's bytes, in order.
fn elements_of(bytes: &[u8]) -> Vec<(String, u64, bool)> {
    let value = json::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
    value
        .get("elements")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e.get("id").unwrap().as_str().unwrap().to_owned(),
                e.get("version").unwrap().as_f64().unwrap() as u64,
                e.get("isDeleted").unwrap().as_bool().unwrap(),
            )
        })
        .collect()
}

fn e(id: &str, version: u64, deleted: bool) -> (String, u64, bool) {
    (id.into(), version, deleted)
}

/// Two Devices that Synced one Drawing of Elements `a` and `b`, then went apart.
fn two_devices_apart() -> (Cloud, Device, Device) {
    let cloud = Cloud::new();
    let (phone, tablet) = (Device::new(), Device::new());
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 1, "a0", 0), element("b", 1, "a1", 0)]),
    );
    phone.sync(&cloud);
    tablet.sync(&cloud);
    (cloud, phone, tablet)
}

#[test]
fn changes_made_on_two_devices_are_merged_on_every_device() {
    let (cloud, phone, tablet) = two_devices_apart();
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
    );
    tablet.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 1, "a0", 0), element("b", 3, "a1", 30)]),
    );

    phone.sync(&cloud); // reaches the cloud first
    assert!(tablet.sync(&cloud).contains(&"Sketch".to_owned())); // merges, uploads
    assert_eq!(phone.sync(&cloud), ["Sketch"]);

    let merged = [e("a", 2, false), e("b", 3, false)];
    assert_eq!(
        elements_of(&phone.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        elements_of(&tablet.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        phone.read("Sketch.excalidraw"),
        cloud.read("Sketch.excalidraw")
    );
    assert_eq!(
        tablet.read("Sketch.excalidraw"),
        cloud.read("Sketch.excalidraw")
    );
}

#[test]
fn the_merge_is_the_last_synced_state() {
    let (cloud, phone, tablet) = two_devices_apart();
    phone.write("Sketch.excalidraw", &drawing(&[element("a", 2, "a0", 10)]));
    tablet.write("Sketch.excalidraw", &drawing(&[element("b", 2, "a1", 20)]));
    phone.sync(&cloud);
    tablet.sync(&cloud);
    phone.sync(&cloud);
    cloud.take_calls();

    assert!(tablet.sync(&cloud).is_empty());
    assert!(phone.sync(&cloud).is_empty());
    let calls = cloud.take_calls();
    assert!(
        calls.gets.is_empty() && calls.puts.is_empty(),
        "idle after merge"
    );
}

#[test]
fn an_erase_on_one_side_and_edits_on_the_other_both_survive() {
    let (cloud, phone, tablet) = two_devices_apart();
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[erased("a", 2, "a0"), element("b", 1, "a1", 0)]),
    );
    tablet.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 1, "a0", 0), element("b", 2, "a1", 5)]),
    );
    phone.sync(&cloud);
    tablet.sync(&cloud);
    phone.sync(&cloud);

    let merged = [e("a", 2, true), e("b", 2, false)];
    assert_eq!(
        elements_of(&phone.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        elements_of(&tablet.read("Sketch.excalidraw").unwrap()),
        merged
    );
}

#[test]
fn images_added_on_two_devices_are_both_kept() {
    let (cloud, phone, tablet) = two_devices_apart();
    phone.write(
        "Sketch.excalidraw",
        &with(&[element("a", 1, "a0", 0)], "{}", r#"{"f1":{"id":"f1"}}"#),
    );
    tablet.write(
        "Sketch.excalidraw",
        &with(&[element("a", 1, "a0", 0)], "{}", r#"{"f2":{"id":"f2"}}"#),
    );
    phone.sync(&cloud);
    tablet.sync(&cloud);
    phone.sync(&cloud);

    for bytes in [
        phone.read("Sketch.excalidraw").unwrap(),
        cloud.read("Sketch.excalidraw").unwrap(),
    ] {
        let value = json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert_eq!(value.get("files").unwrap().as_object().unwrap().len(), 2);
    }
}

#[test]
fn app_state_that_another_tool_stored_survives_a_device_that_never_touched_it() {
    let (cloud, phone, tablet) = two_devices_apart();
    // excalidraw.com sets the background; the phone only edits an Element.
    cloud.write(
        "Sketch.excalidraw",
        &with(
            &[element("a", 1, "a0", 0), element("b", 1, "a1", 0)],
            r##"{"viewBackgroundColor":"#000000"}"##,
            "{}",
        ),
    );
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 2, "a0", 9), element("b", 1, "a1", 0)]),
    );
    phone.sync(&cloud);
    tablet.sync(&cloud);

    for bytes in [
        phone.read("Sketch.excalidraw").unwrap(),
        tablet.read("Sketch.excalidraw").unwrap(),
    ] {
        let value = json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let state = value.get("appState").unwrap();
        assert_eq!(
            state.get("viewBackgroundColor").unwrap().as_str(),
            Some("#000000")
        );
        assert_eq!(elements_of(&bytes)[0], e("a", 2, false));
    }
}

#[test]
fn an_upload_that_another_device_overwrote_is_written_again_and_all_devices_converge() {
    let (cloud, phone, tablet) = two_devices_apart();
    // Both go to upload at the same time; the tablet's write lands last.
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
    );
    phone.sync(&cloud);
    cloud.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 1, "a0", 0), element("b", 2, "a1", 20)]),
    );

    phone.sync(&cloud); // sees the overwrite, writes its newer Element again
    tablet.sync(&cloud);
    phone.sync(&cloud);

    let merged = [e("a", 2, false), e("b", 2, false)];
    assert_eq!(
        elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        elements_of(&phone.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        elements_of(&tablet.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        phone.read("Sketch.excalidraw"),
        tablet.read("Sketch.excalidraw")
    );
}

#[test]
fn a_local_save_that_lacks_newer_versions_the_cloud_holds_does_not_take_them_back() {
    // The open Drawing kept its own version of `a` while the user dragged it
    // (or its autosave raced Sync's write), and saved over the newer one.
    let (cloud, phone, tablet) = two_devices_apart();
    tablet.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 5, "a0", 50), element("b", 1, "a1", 0)]),
    );
    tablet.sync(&cloud);
    phone.sync(&cloud);
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 2, "a0", 9), element("b", 2, "a1", 7)]),
    );

    phone.sync(&cloud);

    let merged = [e("a", 5, false), e("b", 2, false)];
    assert_eq!(
        elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
        merged
    );
    assert_eq!(
        elements_of(&phone.read("Sketch.excalidraw").unwrap()),
        merged
    );
}

#[test]
fn a_file_that_cant_be_parsed_never_replaces_a_drawing_on_the_other_side() {
    let (cloud, phone, tablet) = two_devices_apart();
    let before = phone.read("Sketch.excalidraw").unwrap();
    cloud.write("Sketch.excalidraw", b"{broken");

    assert!(phone.sync(&cloud).is_empty());
    assert_eq!(phone.read("Sketch.excalidraw").unwrap(), before);
    assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), b"{broken");

    // The other way round: broken here, the cloud unchanged.
    cloud.write("Sketch.excalidraw", &before);
    phone.sync(&cloud);
    tablet.sync(&cloud);
    tablet.write("Sketch.excalidraw", b"{broken");
    tablet.sync(&cloud);
    assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), before);
    assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"{broken");
}

#[test]
fn a_one_sided_change_stays_byte_for_byte_as_the_other_tool_wrote_it() {
    let (cloud, phone, _) = two_devices_apart();
    let theirs = br#"{ "type": "excalidraw", "elements": [ {"id":"a","version":5,"versionNonce":1,"index":"a0","isDeleted":false},{"id":"b","type":"rectangle","version":1,"versionNonce":7,"index":"a1","isDeleted":false,"x":0} ], "appState": {"z":1,"a":2}, "files": {} }"#;
    cloud.write("Sketch.excalidraw", theirs);
    phone.sync(&cloud);
    assert_eq!(phone.read("Sketch.excalidraw").unwrap(), theirs);
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

// --- First Connect, same names, deletes -------------------------------------

fn ids(bytes: &[u8]) -> Vec<String> {
    elements_of(bytes).into_iter().map(|e| e.0).collect()
}

#[test]
fn first_connect_copies_drawings_only_one_side_has() {
    let cloud = Cloud::new();
    let phone = Device::new();
    cloud.write("Cloudy.excalidraw", &drawing(&[element("c", 1, "a0", 0)]));
    phone.write("Local.excalidraw", &drawing(&[element("l", 1, "a0", 0)]));
    assert_eq!(phone.sync(&cloud), ["Cloudy"]);
    assert_eq!(phone.files(), ["Cloudy.excalidraw", "Local.excalidraw"]);
    assert!(cloud.read("Local.excalidraw").is_some());
}

#[test]
fn same_named_related_drawings_are_merged_without_a_base() {
    let cloud = Cloud::new();
    let phone = Device::new();
    cloud.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 1, "a0", 0), element("b", 3, "a1", 0)]),
    );
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 2, "a0", 5), element("c", 1, "a2", 0)]),
    );
    phone.sync(&cloud);
    let expected = vec![e("a", 2, false), e("b", 3, false), e("c", 1, false)];
    assert_eq!(
        elements_of(&phone.read("Sketch.excalidraw").unwrap()),
        expected
    );
    assert_eq!(
        elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
        expected
    );
    assert_eq!(phone.files(), ["Sketch.excalidraw"]);
}

#[test]
fn same_named_unrelated_drawings_are_both_kept_and_the_local_one_renamed() {
    let cloud = Cloud::new();
    let phone = Device::new();
    cloud.write("Sketch.excalidraw", &drawing(&[element("r", 1, "a0", 0)]));
    phone.write("Sketch.excalidraw", &drawing(&[element("l", 1, "a0", 0)]));
    phone.write("Sketch 2.excalidraw", &drawing(&[element("m", 1, "a0", 0)]));
    assert_eq!(phone.sync(&cloud), ["Sketch", "Sketch 3"]);
    assert_eq!(
        phone.files(),
        [
            "Sketch 2.excalidraw",
            "Sketch 3.excalidraw",
            "Sketch.excalidraw"
        ]
    );
    assert_eq!(ids(&phone.read("Sketch.excalidraw").unwrap()), ["r"]);
    assert_eq!(ids(&phone.read("Sketch 3.excalidraw").unwrap()), ["l"]);
    assert_eq!(ids(&cloud.read("Sketch.excalidraw").unwrap()), ["r"]);
    assert_eq!(ids(&cloud.read("Sketch 3.excalidraw").unwrap()), ["l"]);
    assert_eq!(phone.sync(&cloud), Vec::<String>::new());
}

#[test]
fn the_next_free_name_also_avoids_names_only_the_cloud_has() {
    let cloud = Cloud::new();
    let phone = Device::new();
    cloud.write("Sketch.excalidraw", &drawing(&[element("r", 1, "a0", 0)]));
    cloud.write("Sketch 2.excalidraw", &drawing(&[element("s", 1, "a0", 0)]));
    phone.write("Sketch.excalidraw", &drawing(&[element("l", 1, "a0", 0)]));
    phone.sync(&cloud);
    assert_eq!(ids(&phone.read("Sketch 3.excalidraw").unwrap()), ["l"]);
    assert_eq!(ids(&phone.read("Sketch 2.excalidraw").unwrap()), ["s"]);
}

#[test]
fn two_devices_creating_the_same_name_offline_follow_the_same_rule() {
    let cloud = Cloud::new();
    let (phone, tablet, mac) = (Device::new(), Device::new(), Device::new());
    for (device, id) in [(&phone, "p"), (&tablet, "t")] {
        device.write("Untitled.excalidraw", &drawing(&[element(id, 1, "a0", 0)]));
    }
    mac.write("Untitled.excalidraw", &drawing(&[]));
    phone.sync(&cloud);
    tablet.sync(&cloud);
    assert_eq!(ids(&tablet.read("Untitled.excalidraw").unwrap()), ["p"]);
    assert_eq!(ids(&tablet.read("Untitled 2.excalidraw").unwrap()), ["t"]);
    phone.sync(&cloud);
    assert_eq!(phone.files(), tablet.files());

    // Empty on one side and drawn on the other are unrelated; no pointless copy
    // when both are empty.
    let (a, b) = (Device::new(), Device::new());
    let cloud = Cloud::new();
    a.write("Blank.excalidraw", &drawing(&[]));
    b.write("Blank.excalidraw", &drawing(&[]));
    a.sync(&cloud);
    b.sync(&cloud);
    assert_eq!(b.files(), ["Blank.excalidraw"]);
    mac.sync(&cloud);
}

#[test]
fn connecting_again_merges_without_duplicates_also_after_both_sides_changed() {
    let (cloud, phone, tablet) = two_devices_apart();
    SyncState::forget(&phone.state.0).unwrap();
    phone.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
    );
    tablet.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 1, "a0", 0), element("b", 2, "a1", 9)]),
    );
    tablet.sync(&cloud);
    phone.sync(&cloud);
    tablet.sync(&cloud);
    for side in [
        phone.read("Sketch.excalidraw"),
        tablet.read("Sketch.excalidraw"),
    ] {
        assert_eq!(
            elements_of(&side.unwrap()),
            [e("a", 2, false), e("b", 2, false)]
        );
    }
    assert_eq!(phone.files(), ["Sketch.excalidraw"]);
    assert_eq!(tablet.files(), ["Sketch.excalidraw"]);
}

#[test]
fn switching_cloud_provider_merges_without_duplicates() {
    let (old_cloud, phone, _tablet) = two_devices_apart();
    // A second directory storage with the same Drawing, uploaded by another Device.
    let new_cloud = Cloud::new();
    let other = Device::new();
    other.sync(&old_cloud);
    other.write(
        "Sketch.excalidraw",
        &drawing(&[element("a", 1, "a0", 0), element("b", 4, "a1", 0)]),
    );
    other.sync(&new_cloud);

    SyncState::forget(&phone.state.0).unwrap();
    phone.sync(&new_cloud);
    assert_eq!(phone.files(), ["Sketch.excalidraw"]);
    assert_eq!(
        elements_of(&phone.read("Sketch.excalidraw").unwrap()),
        [e("a", 1, false), e("b", 4, false)]
    );
    assert!(new_cloud.read("Sketch 2.excalidraw").is_none());
}

#[test]
fn a_drawing_deleted_on_one_side_and_changed_on_the_other_comes_back_with_the_change() {
    let (cloud, phone, tablet) = two_devices_apart();
    phone.remove("Sketch.excalidraw");
    let edited = drawing(&[element("a", 2, "a0", 3), element("b", 1, "a1", 0)]);
    tablet.write("Sketch.excalidraw", &edited);
    phone.sync(&cloud);
    tablet.sync(&cloud);
    phone.sync(&cloud);
    assert_eq!(phone.read("Sketch.excalidraw").unwrap(), edited);
    assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), edited);
    assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), edited);
}

#[test]
fn a_drawing_deleted_on_one_side_and_unchanged_on_the_other_is_deleted_there() {
    let (cloud, phone, tablet) = two_devices_apart();
    phone.remove("Sketch.excalidraw");
    phone.sync(&cloud);
    assert!(cloud.read("Sketch.excalidraw").is_none());
    assert_eq!(tablet.sync(&cloud), ["Sketch"]);
    assert!(tablet.files().is_empty());
}

#[test]
fn a_rename_crossing_an_edit_leaves_both_drawings_everywhere() {
    let (cloud, phone, tablet) = two_devices_apart();
    phone.rename("Sketch.excalidraw", "Renamed.excalidraw");
    let edited = drawing(&[element("a", 2, "a0", 3), element("b", 1, "a1", 0)]);
    tablet.write("Sketch.excalidraw", &edited);
    phone.sync(&cloud);
    tablet.sync(&cloud);
    phone.sync(&cloud);
    for device in [&phone, &tablet] {
        assert_eq!(device.files(), ["Renamed.excalidraw", "Sketch.excalidraw"]);
        assert_eq!(device.read("Sketch.excalidraw").unwrap(), edited);
    }
}

#[test]
fn a_drawing_deleted_while_disconnected_comes_back_on_the_next_connect() {
    let (cloud, phone, _tablet) = two_devices_apart();
    SyncState::forget(&phone.state.0).unwrap();
    phone.remove("Sketch.excalidraw");
    assert_eq!(phone.sync(&cloud), ["Sketch"]);
    assert_eq!(phone.files(), ["Sketch.excalidraw"]);
}

#[test]
fn an_icloud_placeholder_is_absent_until_downloaded_and_a_later_step_finds_the_file() {
    let cloud_dir = Scratch::new("icloud");
    let asked = std::sync::Arc::new(Mutex::new(Vec::new()));
    let log = std::sync::Arc::clone(&asked);
    let storage = DirStorage::open_downloading(&cloud_dir.0, move |path| {
        log.lock().unwrap().push(path.to_path_buf());
    })
    .unwrap();
    let device = Device::new();
    let step = |device: &Device| {
        let state = SyncState::open(&device.state.0).unwrap();
        let outcome = sync_step(
            &device.drawings.0,
            &state,
            &storage,
            &AtomicBool::new(false),
        );
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        outcome.changed
    };
    fs::write(cloud_dir.0.join(".Sketch.excalidraw.icloud"), b"stub").unwrap();

    assert!(step(&device).is_empty());
    assert!(device.files().is_empty(), "not copied, not deleted");
    assert!(
        asked
            .lock()
            .unwrap()
            .contains(&cloud_dir.0.join("Sketch.excalidraw"))
    );

    // The system has downloaded it.
    fs::remove_file(cloud_dir.0.join(".Sketch.excalidraw.icloud")).unwrap();
    fs::write(cloud_dir.0.join("Sketch.excalidraw"), b"one").unwrap();

    assert_eq!(step(&device), ["Sketch"]);
    assert_eq!(device.read("Sketch.excalidraw").unwrap(), b"one");
}
