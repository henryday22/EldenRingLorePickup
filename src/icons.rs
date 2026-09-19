use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ICON_IDS: OnceLock<HashMap<(u32, u32), u32>> = OnceLock::new();
static ICON_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
static CARD_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Static fallback for game builds where the live parameter repository cannot be resolved.
/// The preferred path is `runtime::lookup_icon_id`, which also covers DLC and modded params.
pub fn fallback_icon_id(category: u32, param_id: u32) -> Option<u32> {
    ICON_IDS
        .get_or_init(|| {
            include_str!("../assets/icon-map.csv")
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split(',');
                    let category = u32::from_str_radix(fields.next()?, 16).ok()?;
                    let param_id = fields.next()?.parse().ok()?;
                    let icon_id = fields.next()?.parse().ok()?;
                    Some(((category, param_id), icon_id))
                })
                .collect()
        })
        .get(&(category, param_id))
        .copied()
}

pub fn icon_path(icon_id: u32) -> Option<PathBuf> {
    if icon_id == 0 {
        return None;
    }

    let root = ICON_ROOT.get_or_init(find_icon_root).as_ref()?;
    let path = root.join(format!("MENU_Knowledge_{icon_id:05}.png"));
    path.is_file().then_some(path)
}

pub fn card_path() -> Option<PathBuf> {
    CARD_PATH
        .get_or_init(|| {
            let filename = "EldenRingLorePickup-card.png";
            let module_dir = crate::module_dir();
            let mut candidates = vec![module_dir.join(filename)];
            if let Ok(current) = std::env::current_dir() {
                candidates.push(current.join(filename));
                candidates.push(current.join("natives").join(filename));
            }
            if let Some(parent) = module_dir.parent() {
                candidates.push(parent.join(filename));
            }
            candidates.into_iter().find(|path| path.is_file())
        })
        .clone()
}

fn find_icon_root() -> Option<PathBuf> {
    let folder = "EldenRingLorePickup-icons";
    let module_dir = crate::module_dir();
    let mut candidates = vec![module_dir.join(folder)];

    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join(folder));
        candidates.push(current.join("natives").join(folder));
    }
    if let Some(parent) = module_dir.parent() {
        candidates.push(parent.join(folder));
    }

    let found = candidates
        .into_iter()
        .find(|path| icon_bundle_present(path));
    match &found {
        Some(path) => crate::runtime::log_line(&format!(
            "LorePickup: using icon bundle at {}.",
            path.display()
        )),
        None => crate::runtime::log_line(
            "LorePickup: icon bundle not found; category medallions will be used.",
        ),
    }
    found
}

fn icon_bundle_present(path: &Path) -> bool {
    path.is_dir()
        && (path.join("MENU_Knowledge_00001.png").is_file()
            || path.join("MENU_Knowledge_01000.png").is_file()
            || path.join("MENU_Knowledge_02000.png").is_file())
}
