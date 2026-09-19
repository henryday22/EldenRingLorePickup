use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use windows::Win32::System::LibraryLoader::GetModuleHandleW;

pub static RUNTIME: OnceLock<GameRuntime> = OnceLock::new();
static LOG: OnceLock<Mutex<File>> = OnceLock::new();

const ADD_ITEM_SIG: &[u8] = &[
    0x40, 0x55, 0x56, 0x57, 0x41, 0x54, 0x41, 0x55,
    0x41, 0x56, 0x41, 0x57, 0x48, 0x8D, 0xAC, 0x24,
];

const SEARCH_SIG: &[u8] = &[
    0x3B, 0x51, 0x10, 0x73, 0x29, 0x44, 0x3B, 0x41,
    0x14, 0x73, 0x23, 0x48, 0x8B, 0x41, 0x08,
];

#[derive(Clone, Copy, Debug)]
pub struct GameRuntime {
    pub base: usize,
    pub add_item_rva: usize,
    pub fmg_repo_rva: usize,
    pub fmg_search_rva: usize,
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
    let path = hudhook::util::get_dll_path().map(|mut p| {
        p.set_extension("log");
        p
    });

    let Some(path) = path else {
        return;
    };

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
                label: candidate.label,
            };
            log_line(&format!(
                "LorePickup: matched {} (base={:#x}).",
                found.label, found.base
            ));
            return Ok(found);
        }
    }

    Err("known item/message signatures not present".to_string())
}

pub fn lookup_item_text(
    category: u32,
    param_id: u32,
) -> (Option<String>, Option<String>, Option<String>) {
    let Some(runtime) = RUNTIME.get() else {
        return (None, None, None);
    };

    let Some((name_categories, info_categories, caption_categories)) = text_categories(category) else {
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

fn text_categories(
    category: u32,
) -> Option<(&'static [u32], &'static [u32], &'static [u32])> {
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
