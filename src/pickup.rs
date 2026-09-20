use std::ffi::c_void;
use std::sync::OnceLock;

use retour::GenericDetour;

use crate::{overlay, runtime};

type ItemPopupFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u64;

static HOOK: OnceLock<GenericDetour<ItemPopupFn>> = OnceLock::new();

const POPUP_ID_OFFSET: usize = 0x00;
const POPUP_QUANTITY_OFFSET: usize = 0x04;
const POPUP_KIND_OFFSET: usize = 0x08;
const POPUP_METADATA_OFFSET: usize = 0x0C;

pub fn install() -> Result<(), String> {
    if HOOK.get().is_some() {
        return Ok(());
    }

    let runtime = runtime::RUNTIME
        .get()
        .ok_or_else(|| "runtime not initialized".to_string())?;

    let target_addr = runtime
        .base
        .checked_add(runtime.item_popup_rva)
        .ok_or_else(|| "item-popup address overflow".to_string())?;

    let target: ItemPopupFn = unsafe { std::mem::transmute(target_addr) };
    let detour = unsafe {
        GenericDetour::<ItemPopupFn>::new(target, item_popup_detour)
            .map_err(|e| format!("failed to create item-popup detour: {e}"))?
    };

    HOOK.set(detour)
        .map_err(|_| "item-panel hook initialized twice".to_string())?;

    unsafe {
        HOOK.get()
            .expect("hook was just stored")
            .enable()
            .map_err(|e| format!("failed to enable item-panel detour: {e}"))?;
    }

    Ok(())
}

unsafe extern "C" fn item_popup_detour(manager: *mut c_void, entry: *mut c_void) -> u64 {
    let raw_id = if entry.is_null() {
        0
    } else {
        unsafe { ((entry as *const u8).add(POPUP_ID_OFFSET) as *const u32).read_unaligned() }
    };

    let quantity = if entry.is_null() {
        1
    } else {
        unsafe { ((entry as *const u8).add(POPUP_QUANTITY_OFFSET) as *const i32).read_unaligned() }
            .max(1)
    };
    let kind = if entry.is_null() {
        0
    } else {
        unsafe { ((entry as *const u8).add(POPUP_KIND_OFFSET) as *const u32).read_unaligned() }
    };
    let metadata = if entry.is_null() {
        0
    } else {
        unsafe { ((entry as *const u8).add(POPUP_METADATA_OFFSET) as *const u32).read_unaligned() }
    };

    let ret = HOOK
        .get()
        .map(|hook| unsafe { hook.call(manager, entry) })
        .unwrap_or(0);

    runtime::log_line(&format!(
        "LorePickup: item presentation candidate raw={raw_id:#x}, quantity={quantity}, kind={kind:#x}, metadata={metadata:#x}."
    ));

    let _ = std::panic::catch_unwind(|| {
        let decision = crate::presentation::classify(metadata);
        runtime::log_line(&format!("LorePickup: presentation decision raw={raw_id:#x}: {decision:?}."));
        if decision.shows_card() {
            process_popup(raw_id, quantity);
        }
    });

    ret
}

fn process_popup(raw_id: u32, quantity: i32) {
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

    let description = caption.filter(|s| !s.trim().is_empty())
        .or_else(|| info.clone().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "No lore description is provided for this item.".to_string());

    // Some builds return zero from the live row even though a valid static mapping exists.
    // Never let that sentinel suppress the bundled icon, and verify the PNG before accepting it.
    let live_icon = runtime::lookup_icon_id(category, param_id).filter(|&id| id != 0);
    let fallback_icon = crate::icons::fallback_icon_id(category, param_id).filter(|&id| id != 0);
    let icon_id = live_icon
        .filter(|&id| crate::icons::icon_path(id).is_some())
        .or_else(|| fallback_icon.filter(|&id| crate::icons::icon_path(id).is_some()));
    let details = runtime::lookup_item_details(category, param_id, &name, info.as_deref());

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
