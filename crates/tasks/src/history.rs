//! The History (ADR-0022): when Tasks were created, made Active, moved to the
//! Backlog, by Item ID and time only.

use crate::files::Files;
use pkm_base::clock::iso8601;
use pkm_base::item_id::ItemId;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

/// What happened to a Task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    Created,
    MadeActive,
    MovedToBacklog,
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::MadeActive => "made-active",
            Self::MovedToBacklog => "moved-to-backlog",
        }
    }
}

/// This Device's History files: `<folder>/<device>-<yyyy-mm>.log`. No other
/// Device writes them, so Sync never merges them.
#[derive(Debug)]
pub(crate) struct History {
    files: Arc<dyn Files>,
    folder: PathBuf,
    device: ItemId,
}

impl History {
    pub(crate) fn new(files: Arc<dyn Files>, folder: PathBuf, device: ItemId) -> Self {
        Self {
            files,
            folder,
            device,
        }
    }

    /// Appends `<time> <event> <id>` to the file of the month `at_ms` is in.
    pub(crate) fn record(&self, event: Event, id: &ItemId, at_ms: f64) -> io::Result<()> {
        let time = iso8601(at_ms);
        // `2023-11-14T22:13:20Z`: the month is its first seven characters.
        let file = self
            .folder
            .join(format!("{}-{}.log", self.device, &time[..7]));
        self.files
            .append(&file, &format!("{time} {} {id}\n", event.name()))
    }

    /// When each Task was created, by Item ID, from every Device's files; the
    /// earliest line wins.
    pub(crate) fn created_times(&self) -> HashMap<String, String> {
        let mut times: HashMap<String, String> = HashMap::new();
        let listed = self.files.list(&self.folder).unwrap_or_default();
        for text in (listed.iter()).filter_map(|l| self.files.read(&self.folder.join(&l.name)).ok())
        {
            for line in text.lines() {
                let mut parts = line.split(' ');
                if let (Some(time), Some("created"), Some(id)) =
                    (parts.next(), parts.next(), parts.next())
                {
                    let earlier = times.get(id).is_none_or(|t| time < t.as_str());
                    if earlier {
                        times.insert(id.to_owned(), time.to_owned());
                    }
                }
            }
        }
        times
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::DiskFiles;
    use std::fs;

    #[test]
    fn lines_go_to_the_file_of_their_month() {
        let folder = std::env::temp_dir().join(format!(
            "pkm-history-{}",
            ItemId::new(&mut pkm_base::random::Entropy::from_system())
        ));
        let device = ItemId::parse("aaaaaaaaaaaa").unwrap();
        let id = ItemId::parse("bbbbbbbbbbbb").unwrap();
        let history = History::new(Arc::new(DiskFiles), folder.clone(), device);
        history
            .record(Event::Created, &id, 1_700_000_000_000.0)
            .unwrap(); // 2023-11-14
        history
            .record(Event::MadeActive, &id, 1_701_388_800_000.0)
            .unwrap(); // 2023-12-01
        history
            .record(Event::MovedToBacklog, &id, 1_701_388_801_000.0)
            .unwrap();
        let read = |name: &str| fs::read_to_string(folder.join(name)).unwrap();
        assert_eq!(
            read("aaaaaaaaaaaa-2023-11.log"),
            "2023-11-14T22:13:20Z created bbbbbbbbbbbb\n"
        );
        assert_eq!(
            read("aaaaaaaaaaaa-2023-12.log"),
            "2023-12-01T00:00:00Z made-active bbbbbbbbbbbb\n2023-12-01T00:00:01Z moved-to-backlog bbbbbbbbbbbb\n"
        );
        drop(fs::remove_dir_all(folder));
    }
}
