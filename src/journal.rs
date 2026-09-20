use crate::{
    collection::{Card, Collection},
    overlay::LoreEntry,
    player::Player,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{mpsc, Mutex, OnceLock},
    thread,
};

struct Journal {
    data: Mutex<Collection>,
    sender: mpsc::Sender<()>,
    profile: Option<String>,
    writable: bool,
}
static JOURNAL: OnceLock<Journal> = OnceLock::new();

fn root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(crate::module_dir)
        .join("EldenRingLorePickup")
}

fn load(path: &Path) -> Result<Collection, String> {
    match fs::read(path) {
        Ok(bytes) => {
            let db: Collection = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if db.schema != 1 {
                return Err(format!("unsupported journal schema {}", db.schema));
            }
            Ok(db)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Collection::default()),
        Err(e) => Err(e.to_string()),
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let temporary = path.with_extension("tmp");
    let mut f = fs::File::create(&temporary).map_err(|e| e.to_string())?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    let a: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            a.as_ptr(),
            b.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

pub fn start() {
    let root = root();
    let result = fs::create_dir_all(&root);
    let path = root.join("collection.json");
    let loaded = load(&path);
    let writable = result.is_ok() && loaded.is_ok();
    if let Err(error) = &loaded {
        crate::runtime::log_line(&format!(
            "LorePickup: journal preserved without overwrite: {error}"
        ));
    }
    let config_path = crate::module_dir().join("LorePickup.ini");
    if let Ok(mut file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config_path)
    {
        let _=file.write_all(b"# auto separates supported characters by save slot, saved ID and name.\r\n# Use an explicit unique name for unsupported builds, renamed characters or copied saves.\r\nprofile=auto\r\n");
    }
    let profile = fs::read_to_string(config_path).ok().and_then(|s| {
        s.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "profile" && !value.trim().is_empty() && value.trim() != "auto")
                .then(|| value.trim().to_string())
        })
    });
    let (sender, receiver) = mpsc::channel();
    if JOURNAL
        .set(Journal {
            data: Mutex::new(loaded.unwrap_or_default()),
            sender,
            profile,
            writable,
        })
        .is_err()
    {
        return;
    }
    if let Err(error) = thread::Builder::new()
        .name("LorePickupJournal".into())
        .spawn(move || {
            while receiver.recv().is_ok() {
                while receiver.try_recv().is_ok() {}
                let Some(journal) = JOURNAL.get() else {
                    break;
                };
                if !journal.writable {
                    continue;
                }
                let Ok(data) = journal.data.lock().map(|d| d.clone()) else {
                    continue;
                };
                let save = (|| {
                    let json = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
                    if path.exists() {
                        fs::copy(&path, root.join("collection.backup.json"))
                            .map_err(|e| e.to_string())?;
                    }
                    atomic_write(&path, &json)?;
                    atomic_write(
                        &root.join("index.html"),
                        crate::collection::html(&data).as_bytes(),
                    )
                })();
                if let Err(error) = save {
                    crate::runtime::log_line(&format!(
                        "LorePickup: could not save journal: {error}"
                    ));
                }
            }
        })
    {
        crate::runtime::log_line(&format!(
            "LorePickup: journal worker could not start: {error}"
        ));
    }
}

pub fn record(entry: &LoreEntry, player: Option<&Player>) -> String {
    let Some(journal) = JOURNAL.get() else {
        return "Collection unavailable".into();
    };
    if !journal.writable {
        return "Collection unavailable — existing file preserved".into();
    }
    let (profile, name) = if let Some(manual) = journal.profile.as_ref() {
        (format!("manual:{manual}"), manual.as_str())
    } else if let Some(player) = player {
        (player.profile.clone(), player.name.as_str())
    } else {
        return "Collection paused — choose a profile in LorePickup.ini".into();
    };
    let category = entry.raw_id & 0xF0000000;
    let goods_type = if category == 0x40000000 {
        crate::runtime::goods_type(entry.param_id)
    } else {
        None
    };
    let key = crate::collection::key(category, entry.param_id, goods_type);
    let catalogue = crate::runtime::lore_catalogue();
    let Ok(mut db) = journal.data.lock() else {
        return "Collection unavailable".into();
    };
    if !catalogue.is_empty() {
        db.catalogue.extend(catalogue.iter().cloned());
        db.catalogue_ready = true;
    }
    let card = Card {
        key,
        name: entry.name.clone(),
        category: crate::collection::category_name(category).into(),
        description: entry.description.clone(),
        details: entry.details.clone(),
        icon_id: entry.icon_id,
    };
    let (count, total, fresh) = db.record(&profile, name, card);
    let catalogue_ready = db.catalogue_ready;
    drop(db);
    if journal.sender.send(()).is_err() {
        return "Collection could not be saved — check the log".into();
    }
    if catalogue_ready {
        format!(
            "{count}/{total} collected{}",
            if fresh { " · NEW CARD" } else { "" }
        )
    } else {
        format!("{count} collected · catalogue unavailable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_replacement_survives_restart_and_corrupt_files_are_rejected() {
        let root = std::env::temp_dir().join(format!("lore-journal-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("collection.json");
        atomic_write(
            &path,
            serde_json::to_string(&Collection::default())
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        let mut db = load(&path).unwrap();
        db.catalogue.insert("test".into());
        atomic_write(&path, &serde_json::to_vec(&db).unwrap()).unwrap();
        assert!(load(&path).unwrap().catalogue.contains("test"));
        fs::write(&path, b"broken original").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken original");
        fs::remove_dir_all(root).unwrap();
    }
}
