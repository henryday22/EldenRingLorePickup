use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use windows::Win32::System::LibraryLoader::GetModuleHandleW;

pub static RUNTIME: OnceLock<GameRuntime> = OnceLock::new();
static LOG: OnceLock<Mutex<File>> = OnceLock::new();

const ADD_ITEM_SIG: &[u8] = &[
    0x40, 0x55, 0x56, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x48, 0x8D, 0xAC, 0x24,
];

const SEARCH_SIG: &[u8] = &[
    0x3B, 0x51, 0x10, 0x73, 0x29, 0x44, 0x3B, 0x41, 0x14, 0x73, 0x23, 0x48, 0x8B, 0x41, 0x08,
];

#[derive(Clone, Copy, Debug)]
pub struct GameRuntime {
    pub base: usize,
    pub add_item_rva: usize,
    pub fmg_repo_rva: usize,
    pub fmg_search_rva: usize,
    pub solo_param_slot: usize,
    pub label: &'static str,
}

#[derive(Clone, Copy)]
struct Candidate {
    add_item_rva: usize,
    fmg_repo_rva: usize,
    fmg_search_rva: usize,
    label: &'static str,
}

const CANDIDATES: &[Candidate] = &[
    Candidate {
        add_item_rva: 0x0056_1400,
        fmg_repo_rva: 0x03D8_1568,
        fmg_search_rva: 0x0266_FC40,
        label: "ER 2.7.1.0 / 1.17-era",
    },
    Candidate {
        add_item_rva: 0x0056_1400,
        fmg_repo_rva: 0x03D8_1568,
        fmg_search_rva: 0x0266_FBD0,
        label: "ER 2.7.0.0 / Tarnished Edition",
    },
    Candidate {
        add_item_rva: 0x0056_05B0,
        fmg_repo_rva: 0x03D7_D4F8,
        fmg_search_rva: 0x0266_D3C0,
        label: "ER 2.6.2.0",
    },
];

type SearchFn = unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> *const u16;

pub fn init_log() {
    let path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("EldenRingLorePickup.log");

    if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = LOG.set(Mutex::new(file));
        log_line("=== EldenRingLorePickup session ===");
    }
}

pub fn log_line(message: &str) {
    if let Some(log) = LOG.get() {
        if let Ok(mut file) = log.lock() {
            let _ = writeln!(file, "{message}");
            let _ = file.flush();
        }
    }
}

pub fn detect_runtime() -> Result<GameRuntime, String> {
    let base = current_module_base().ok_or("eldenring.exe module base is not available yet")?;

    for candidate in CANDIDATES {
        if signature_matches(base + candidate.add_item_rva, ADD_ITEM_SIG)
            && signature_matches(base + candidate.fmg_search_rva, SEARCH_SIG)
        {
            let found = GameRuntime {
                base,
                add_item_rva: candidate.add_item_rva,
                fmg_repo_rva: candidate.fmg_repo_rva,
                fmg_search_rva: candidate.fmg_search_rva,
                solo_param_slot: find_solo_param_slot(base).unwrap_or(0),
                label: candidate.label,
            };
            log_line(&format!(
                "LorePickup: matched {} (base={:#x}, params={:#x}).",
                found.label, found.base, found.solo_param_slot
            ));
            return Ok(found);
        }
    }

    Err("known item/message signatures not present".to_string())
}

/// Reads the current row's `iconId` from the game's live parameter repository. This keeps
/// icon selection correct for DLC and parameter mods instead of baking item names into the DLL.
pub fn lookup_icon_id(category: u32, param_id: u32) -> Option<u32> {
    let runtime = RUNTIME.get()?;
    if runtime.solo_param_slot == 0 {
        return None;
    }

    let (param_offset, icon_offset) = match category {
        0x0000_0000 => (0x88usize, 0xBEusize), // EquipParamWeapon
        0x1000_0000 => (0xD0, 0xA8),           // EquipParamProtector
        0x2000_0000 => (0x118, 0x26),          // EquipParamAccessory
        0x4000_0000 => (0x160, 0x30),          // EquipParamGoods
        0x8000_0000 => (0x2BD8, 0x04),         // EquipParamGem
        _ => return None,
    };

    unsafe {
        let repo = read_ptr(runtime.solo_param_slot)?;
        let first = read_ptr(repo.checked_add(param_offset)?)?;
        let second = read_ptr(first.checked_add(0x80)?)?;
        let param = read_ptr(second.checked_add(0x80)?)?;

        let table_end = read_i32(param.checked_add(0x30)?)?;
        if !(0x40..=0x0400_0000).contains(&table_end) {
            return None;
        }

        let row_count = (table_end as usize - 0x40) / 0x18;
        let mut low = 0usize;
        let mut high = row_count;
        while low < high {
            let mid = low + (high - low) / 2;
            let row = param.checked_add(0x40 + mid * 0x18)?;
            let row_id = read_i32(row)? as u32;
            match row_id.cmp(&param_id) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    let data_offset = read_i32(row.checked_add(0x08)?)?;
                    if !(0..=0x0800_0000).contains(&data_offset) {
                        return None;
                    }
                    let icon_addr = param
                        .checked_add(data_offset as usize)?
                        .checked_add(icon_offset)?;
                    return Some((icon_addr as *const u16).read_unaligned() as u32);
                }
            }
        }
    }
    None
}

pub fn lookup_item_text(
    category: u32,
    param_id: u32,
) -> (Option<String>, Option<String>, Option<String>) {
    let Some(runtime) = RUNTIME.get() else {
        return (None, None, None);
    };

    let Some((name_categories, info_categories, caption_categories)) = text_categories(category)
    else {
        return (None, None, None);
    };

    let name = lookup_first(runtime, name_categories, param_id);
    let info = lookup_first(runtime, info_categories, param_id);
    let caption = lookup_first(runtime, caption_categories, param_id);

    (name, info, caption)
}

fn lookup_first(runtime: &GameRuntime, categories: &[u32], param_id: u32) -> Option<String> {
    for &category in categories {
        if let Some(text) = lookup(runtime, category, param_id) {
            if !text.trim().is_empty() && !text.starts_with('?') && text != "[ERROR]" {
                return Some(text);
            }
        }
    }
    None
}

fn lookup(runtime: &GameRuntime, category: u32, param_id: u32) -> Option<String> {
    let slot = runtime.base.checked_add(runtime.fmg_repo_rva)?;
    let repo = unsafe { (slot as *const usize).read_unaligned() };
    if !plausible(repo) {
        return None;
    }

    let search_addr = runtime.base.checked_add(runtime.fmg_search_rva)?;
    if !signature_matches(search_addr, SEARCH_SIG) {
        return None;
    }

    let search: SearchFn = unsafe { std::mem::transmute(search_addr) };
    let ptr = unsafe { search(repo as *mut c_void, 0, category, param_id) };
    read_utf16(ptr)
}

fn read_utf16(ptr: *const u16) -> Option<String> {
    if ptr.is_null() || !plausible(ptr as usize) {
        return None;
    }

    const MAX_UNITS: usize = 4096;
    let mut units = Vec::new();

    for i in 0..MAX_UNITS {
        let ch = unsafe { ptr.add(i).read_unaligned() };
        if ch == 0 {
            break;
        }
        units.push(ch);
    }

    if units.is_empty() {
        None
    } else {
        Some(String::from_utf16_lossy(&units))
    }
}

fn text_categories(category: u32) -> Option<(&'static [u32], &'static [u32], &'static [u32])> {
    match category {
        0x0000_0000 => Some((&[11, 310, 410], &[21, 311, 411], &[25, 312, 412])),
        0x1000_0000 => Some((&[12, 313, 413], &[22, 314, 414], &[26, 315, 415])),
        0x2000_0000 => Some((&[13, 316, 416], &[23, 317, 417], &[27, 318, 418])),
        0x4000_0000 => Some((&[10, 319, 419], &[20, 320, 420], &[24, 321, 421])),
        0x8000_0000 => Some((&[35, 322, 422], &[36, 323, 423], &[37, 324, 424])),
        _ => None,
    }
}

fn current_module_base() -> Option<usize> {
    let module = unsafe { GetModuleHandleW(None) }.ok()?;
    Some(module.0 as usize)
}

fn find_solo_param_slot(base: usize) -> Option<usize> {
    // 48 8B 0D ?? ?? ?? ?? 48 85 C9 0F 84 ?? ?? ?? ?? 45 33 C0 BA 8E 00 00 00
    const PATTERN: &[i16] = &[
        0x48, 0x8B, 0x0D, -1, -1, -1, -1, 0x48, 0x85, 0xC9, 0x0F, 0x84, -1, -1, -1, -1, 0x45, 0x33,
        0xC0, 0xBA, 0x8E, 0x00, 0x00, 0x00,
    ];

    let (text, size) = pe_text_section(base)?;
    let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, size) };
    let offset = bytes.windows(PATTERN.len()).position(|window| {
        window
            .iter()
            .zip(PATTERN)
            .all(|(&actual, &expected)| expected < 0 || actual == expected as u8)
    })?;
    let instruction = text.checked_add(offset)?;
    let displacement = unsafe { ((instruction + 3) as *const i32).read_unaligned() } as isize;
    (instruction + 7).checked_add_signed(displacement)
}

fn pe_text_section(base: usize) -> Option<(usize, usize)> {
    unsafe {
        if (base as *const u16).read_unaligned() != 0x5A4D {
            return None;
        }
        let pe_offset = ((base + 0x3C) as *const u32).read_unaligned() as usize;
        let nt = base.checked_add(pe_offset)?;
        if (nt as *const u32).read_unaligned() != 0x0000_4550 {
            return None;
        }
        let section_count = ((nt + 6) as *const u16).read_unaligned() as usize;
        let optional_size = ((nt + 20) as *const u16).read_unaligned() as usize;
        let sections = nt.checked_add(24 + optional_size)?;
        for index in 0..section_count {
            let section = sections.checked_add(index * 40)?;
            let name = std::slice::from_raw_parts(section as *const u8, 8);
            if name.starts_with(b".text") {
                let virtual_size = ((section + 8) as *const u32).read_unaligned() as usize;
                let virtual_address = ((section + 12) as *const u32).read_unaligned() as usize;
                return Some((base.checked_add(virtual_address)?, virtual_size));
            }
        }
    }
    None
}

unsafe fn read_ptr(address: usize) -> Option<usize> {
    if !plausible(address) {
        return None;
    }
    let value = (address as *const usize).read_unaligned();
    plausible(value).then_some(value)
}

unsafe fn read_i32(address: usize) -> Option<i32> {
    plausible(address).then(|| (address as *const i32).read_unaligned())
}

fn signature_matches(addr: usize, expected: &[u8]) -> bool {
    if !plausible(addr) {
        return false;
    }
    let actual = unsafe { std::slice::from_raw_parts(addr as *const u8, expected.len()) };
    actual == expected
}

fn plausible(ptr: usize) -> bool {
    (0x1_0000..0x0000_7FFF_FFFF_FFFF).contains(&ptr)
}
