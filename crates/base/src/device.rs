//! The Device ID: a random Item ID that tells this Device's History files
//! apart from every other Device's (ADR-0022).

use crate::item_id::ItemId;
use crate::random::Entropy;
use std::fs;
use std::io;
use std::path::Path;

const FILE: &str = "device-id";

/// This Device's ID, kept in `folder`, which must be local to the Device and
/// never Synced. Generated and saved on first use; the same afterwards. A
/// file that holds no Item ID is replaced.
pub fn id(folder: &Path) -> io::Result<ItemId> {
    let path = folder.join(FILE);
    if let Some(id) = fs::read_to_string(&path)
        .ok()
        .and_then(|text| ItemId::parse(text.trim()))
    {
        return Ok(id);
    }
    let id = ItemId::new(&mut Entropy::from_system());
    fs::create_dir_all(folder)?;
    fs::write(&path, id.as_str())?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        let mut entropy = Entropy::from_system();
        std::env::temp_dir().join(format!("pkm-device-{}", ItemId::new(&mut entropy)))
    }

    #[test]
    fn the_id_is_made_once_and_then_kept() {
        let folder = scratch();
        let first = id(&folder).unwrap();
        assert_eq!(id(&folder).unwrap(), first);
        drop(fs::remove_dir_all(folder));
    }

    #[test]
    fn devices_get_different_ids() {
        let (a, b) = (scratch(), scratch());
        assert_ne!(id(&a).unwrap(), id(&b).unwrap());
        drop(fs::remove_dir_all(a));
        drop(fs::remove_dir_all(b));
    }

    #[test]
    fn a_damaged_file_is_replaced() {
        let folder = scratch();
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(FILE), "not an id").unwrap();
        let made = id(&folder).unwrap();
        assert_eq!(id(&folder).unwrap(), made);
        drop(fs::remove_dir_all(folder));
    }
}
