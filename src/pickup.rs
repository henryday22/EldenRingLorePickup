use std::ffi::c_void;
use std::sync::OnceLock;

use retour::GenericDetour;

use crate::{overlay, runtime};

type AddItemFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, u64) -> u64;

static HOOK: OnceLock<GenericDetour<AddItemFn>> = OnceLock::new();

const ENTRY_ID_OFFSET: usize = 0x04;
const ENTRY_QUANTITY_OFFSET: usize = 0x08;
const INVENTORY_LIST_OFFSETS: [usize; 4] = [0x0C, 0x1C, 0x2C, 0x3C];
const LIST_CAPACITY_OFFSET: usize = 0x00;
const LIST_POINTER_OFFSET: usize = 0x04;
const LIST_ENTRY_COUNT_OFFSET: usize = 0x0C;
const INVENTORY_ENTRY_SIZE: usize = 0x18;
const INVENTORY_ENTRY_ID_OFFSET: usize = 0x04;
const INVENTORY_ENTRY_IS_NEW_OFFSET: usize = 0x10;
const MAX_INVENTORY_CAPACITY: usize = 16_384;

#[derive(Clone, Copy, Debug, Default)]
struct InventoryState {
    readable: bool,
    exists: bool,
    is_new: bool,
}

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

    // Elden Ring's own inventory entry is the source of truth. In particular, its IsNew flag is
    // persistent game state; the short right-side pickup log is not a first-acquisition signal.
    let inventory_base = runtime::player_inventory();
    let before = inventory_base
        .map(|base| unsafe { inventory_state(base, raw_id) })
        .unwrap_or_default();

    let ret = HOOK
        .get()
        .map(|hook| unsafe { hook.call(inventory, entry, item_buf, r9) })
        .unwrap_or(0);

    let after = inventory_base
        .map(|base| unsafe { inventory_state(base, raw_id) })
        .unwrap_or_default();
    let genuinely_new =
        before.readable && after.readable && !before.exists && after.exists && after.is_new;

    runtime::log_line(&format!(
        "LorePickup: acquisition gate raw={raw_id:#x}, inventory={inventory_base:?}, before={before:?}, after={after:?}, accepted={genuinely_new}."
    ));

    if genuinely_new {
        let _ = std::panic::catch_unwind(|| process_pickup(raw_id, quantity));
    }

    ret
}

unsafe fn inventory_state(base: usize, raw_id: u32) -> InventoryState {
    if raw_id == 0 || !plausible_address(base) {
        return InventoryState::default();
    }

    let mut state = InventoryState {
        readable: true,
        ..InventoryState::default()
    };

    for list_offset in INVENTORY_LIST_OFFSETS {
        let list = base + list_offset;
        let capacity = ((list + LIST_CAPACITY_OFFSET) as *const u32).read_unaligned() as usize;
        let entry_count =
            ((list + LIST_ENTRY_COUNT_OFFSET) as *const u32).read_unaligned() as usize;
        let entries = ((list + LIST_POINTER_OFFSET) as *const usize).read_unaligned();

        if capacity == 0 || entry_count == 0 {
            continue;
        }
        if capacity > MAX_INVENTORY_CAPACITY
            || entry_count > capacity
            || !plausible_address(entries)
        {
            state.readable = false;
            continue;
        }

        let mut occupied = 0usize;
        for index in 0..capacity {
            let row = entries + index * INVENTORY_ENTRY_SIZE;
            let item_id = ((row + INVENTORY_ENTRY_ID_OFFSET) as *const u32).read_unaligned();
            if item_id == 0 || item_id == u32::MAX {
                continue;
            }
            occupied += 1;
            if item_id == raw_id {
                state.exists = true;
                state.is_new |=
                    ((row + INVENTORY_ENTRY_IS_NEW_OFFSET) as *const i32).read_unaligned() != 0;
            }
            if occupied >= entry_count {
                break;
            }
        }
    }

    state
}

fn plausible_address(address: usize) -> bool {
    (0x1_0000..=0x0000_7FFF_FFFF_FFFF).contains(&address)
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

    let (name, info, caption) = runtime::lookup_item_text(category, param_id);

    let Some(name) = name else {
        runtime::log_line(&format!(
            "LorePickup: no name resolved for raw item {raw_id:#x} (param {param_id})."
        ));
        return;
    };

    let description = caption.filter(|s| !s.trim().is_empty()).unwrap_or_default();

    if description.trim().is_empty() {
        return;
    }

    // Some builds return zero from the live row even though a valid static mapping exists.
    // Never let that sentinel suppress the bundled icon, and verify the PNG before accepting it.
    let live_icon = runtime::lookup_icon_id(category, param_id).filter(|&id| id != 0);
    let fallback_icon = crate::icons::fallback_icon_id(category, param_id).filter(|&id| id != 0);
    let icon_id = live_icon
        .filter(|&id| crate::icons::icon_path(id).is_some())
        .or_else(|| fallback_icon.filter(|&id| crate::icons::icon_path(id).is_some()));
    let details = runtime::lookup_item_details(category, param_id, info.as_deref());

    runtime::log_line(&format!(
        "LorePickup: icon lookup raw={raw_id:#x}, live={live_icon:?}, fallback={fallback_icon:?}, selected={icon_id:?}."
    ));

    overlay::enqueue(overlay::LoreEntry {
        raw_id,
        param_id,
        quantity,
        name,
        description,
        icon_id,
        details,
    });
}
