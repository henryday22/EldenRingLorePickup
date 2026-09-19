use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

static ICON_IDS: OnceLock<HashMap<(u32, u32), u32>> = OnceLock::new();

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

pub fn icon_path(icon_id: u32) -> PathBuf {
    crate::module_dir()
        .join("EldenRingLorePickup-icons")
        .join(format!("MENU_Knowledge_{icon_id:05}.png"))
}
