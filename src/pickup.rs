use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use retour::GenericDetour;

use crate::{overlay, runtime};

type AddItemFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, u64) -> u64;

static HOOK: OnceLock<GenericDetour<AddItemFn>> = OnceLock::new();
static SEEN: OnceLock<Mutex<HashSet<(u32, u32)>>> = OnceLock::new();

const ENTRY_ID_OFFSET: usize = 0x04;
const ENTRY_QUANTITY_OFFSET: usize = 0x08;

pub fn install() -> Result<(), String> {
    if HOOK.get().is_some() {
        return Ok(());
    }

    let runtime = runtime::RUNTIME
        .get()
        .ok_or_else(|| "runtime not initialized".to_string())?;

    let target_addr = runtime
        .base
        .checked_add(runtime.add_item_rva)
        .ok_or_else(|| "AddItem address overflow".to_string())?;

    let target: AddItemFn = unsafe { std::mem::transmute(target_addr) };
    let detour = unsafe {
        GenericDetour::<AddItemFn>::new(target, add_item_detour)
            .map_err(|e| format!("failed to create AddItem detour: {e}"))?
    };

    HOOK.set(detour)
        .map_err(|_| "AddItem hook initialized twice".to_string())?;

    unsafe {
        HOOK.get()
            .expect("hook was just stored")
            .enable()
            .map_err(|e| format!("failed to enable AddItem detour: {e}"))?;
    }

    let _ = SEEN.set(Mutex::new(HashSet::new()));
    Ok(())
}

unsafe extern "C" fn add_item_detour(
    inventory: *mut c_void,
    entry: *mut c_void,
    item_buf: *mut c_void,
    r9: u64,
) -> u64 {
    let raw_id = if entry.is_null() {
        0
    } else {
        unsafe { ((entry as *const u8).add(ENTRY_ID_OFFSET) as *const u32).read_unaligned() }
    };

    let quantity = if entry.is_null() {
        1
    } else {
        unsafe { ((entry as *const u8).add(ENTRY_QUANTITY_OFFSET) as *const i32).read_unaligned() }
            .max(1)
    };

    let ret = HOOK
        .get()
        .map(|hook| unsafe { hook.call(inventory, entry, item_buf, r9) })
        .unwrap_or(0);

    let _ = std::panic::catch_unwind(|| process_pickup(raw_id, quantity));

    ret
}

fn process_pickup(raw_id: u32, quantity: i32) {
    if raw_id == 0 {
        return;
    }

    let category = raw_id & 0xF000_0000;
    let param_id = raw_id & 0x0FFF_FFFF;

    if !matches!(
        category,
        0x0000_0000 | 0x1000_0000 | 0x2000_0000 | 0x4000_0000 | 0x8000_0000
    ) {
        return;
    }

    let key = (category, param_id);
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));

    if let Ok(mut guard) = seen.lock() {
        if !guard.insert(key) {
            return;
        }
    } else {
        return;
    }

    let (name, _info, caption) = runtime::lookup_item_text(category, param_id);

    let Some(name) = name else {
        runtime::log_line(&format!(
            "LorePickup: no name resolved for raw item {raw_id:#x} (param {param_id})."
        ));
        return;
    };

    // Only the caption is lore. The short "info" string is the mechanical summary and is
    // deliberately excluded from the card.
    let description = caption.filter(|s| !s.trim().is_empty()).unwrap_or_default();

    if description.trim().is_empty() {
        return;
    }

    let icon_id = runtime::lookup_icon_id(category, param_id)
        .or_else(|| crate::icons::fallback_icon_id(category, param_id));

    overlay::enqueue(overlay::LoreEntry {
        raw_id,
        param_id,
        quantity,
        name,
        description,
        icon_id,
    });
}
