use std::collections::HashSet;
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
    pub item_popup_rva: usize,
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
            let Some(item_popup_rva) = find_item_popup_rva(base) else {
                continue;
            };
            let found = GameRuntime {
                base,
                item_popup_rva,
                fmg_repo_rva: candidate.fmg_repo_rva,
                fmg_search_rva: candidate.fmg_search_rva,
                solo_param_slot: find_solo_param_slot(base).unwrap_or(0),
                label: candidate.label,
            };
            log_line(&format!(
                "LorePickup: matched {} (base={:#x}, item_popup={:#x}, params={:#x}).",
                found.label,
                found.base,
                found.item_popup_rva,
                found.solo_param_slot
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

/// Compact, factual item data used by the card's lower information panel.
pub fn lookup_item_details(category: u32, param_id: u32, name: &str, info: Option<&str>) -> Vec<String> {
    let mut lines = match category {
        0x0000_0000 => weapon_details(param_id),
        0x1000_0000 => weight_details(0xD0, param_id, 0x24, "Armour", true),
        0x2000_0000 => weight_details(0x118, param_id, 0x0C, "Talisman", false),
        0x4000_0000 => goods_details(param_id, name),
        0x8000_0000 => vec!["TYPE  Ash of War".to_string()],
        _ => Vec::new(),
    };

    if let Some(summary) = info.map(str::trim).filter(|text| !text.is_empty()) {
        let summary = summary.replace(['\r', '\n'], " ");
        if !lines.iter().any(|line| line.contains(&summary)) {
            if category == 0x0000_0000 {
                lines.push(format!("EFFECT  {summary}"));
            } else {
                lines.insert(0, format!("EFFECT  {summary}"));
            }
        }
    }
    if category == 0x1000_0000 {
        lines.push("IN PRACTICE  Armour is not class-locked. Choose weight you can carry while keeping your preferred roll; staying below 70% of maximum equip load preserves a medium roll.".into());
    } else if category == 0x2000_0000 {
        lines.push("IN PRACTICE  Choose this for its listed effect rather than a character class. Talismans can be swapped to suit a boss or a weapon; you do not need to spend levels to equip one.".into());
    } else if category == 0x8000_0000 {
        lines.push("IN PRACTICE  Apply at a Site of Grace to a compatible armament. The skill and the affinity are separate choices; the available affinities also depend on your whetblades.".into());
    }
    lines
}

fn weapon_details(param_id: u32) -> Vec<String> {
    let Some(row) = param_row(0x88, param_id) else {
        return vec!["TYPE  Weapon".to_string()];
    };
    unsafe {
        let kind = read_u16(row + 0x1A6).unwrap_or(0);
        let (class, note) = crate::guidance::weapon_class(kind);
        let level = param_id % 100;
        let affinity = affinity_label(param_id);
        let weight = read_f32(row + 0x10).filter(|v| v.is_finite() && *v >= 0.0 && *v < 1000.0);
        let mut lines = vec![format!("TYPE  {class} · {affinity} · +{level}")];
        let req = [("STR",0xF2),("DEX",0xF3),("INT",0xF4),("FAI",0xF5),("ARC",0x195)]
            .iter().filter_map(|(name, offset)| read_u8(row + offset)
                .filter(|v| *v > 0).map(|v| format!("{name} {v}"))).collect::<Vec<_>>();
        if !req.is_empty() { lines.push(format!("REQUIRES  {}", req.join(" · "))); }

        // EquipParamWeapon's attack values omit reinforcement. Use its reinforceTypeId to
        // resolve the corresponding ReinforceParamWeapon row; never call raw +0 values +8.
        let reinforce = read_u16(row + 0xDA).and_then(|id| param_row(0x1A8, id as u32));
        let factors = [0, 4, 8, 12, 0x58].map(|offset| reinforce
            .and_then(|r| read_f32(r + offset)).filter(|v| v.is_finite() && *v > 0.0 && *v < 100.0));
        let damage = [("Physical",0xC8),("Magic",0xCA),("Fire",0xCC),("Lightning",0xCE),("Holy",0x18C)];
        let values = damage.iter().enumerate().filter_map(|(i,(name,offset))| {
            let base = read_u16(row + offset)?;
            if base == 0 { return None; }
            Some(format!("{name} {}", (base as f32 * factors[i].unwrap_or(1.0)).floor() as u32))
        }).collect::<Vec<_>>();
        if !values.is_empty() && !matches!(kind, 57 | 61 | 65 | 67 | 69 | 90) {
            let label = if factors.iter().all(Option::is_some) { "BASE ATTACK" } else { "RAW BASE" };
            lines.push(format!("{label}  {}", values.join(" · ")));
        }
        let corrections = [("STR",0x24,0x1C),("DEX",0x28,0x20),("INT",0x2C,0x24),("FAI",0x30,0x28),("ARC",0x19C,0x60)];
        let mut ranked: Vec<(&str,f32)> = corrections.iter().filter_map(|(name,offset,rate_offset)| {
            let value = read_f32(row + offset)?;
            let rate = reinforce.and_then(|r| read_f32(r + rate_offset)).unwrap_or(1.0);
            let value = value * rate;
            (value.is_finite() && value > 0.0 && value < 10000.0).then_some((*name,value))
        }).collect();
        ranked.sort_by(|a,b| b.1.total_cmp(&a.1));
        if let Some(&(_, best)) = ranked.first() {
            let main = ranked.iter().filter(|(_,v)| *v >= best * 0.8).map(|(name,_)| *name).collect::<Vec<_>>();
            lines.push(format!("SCALING  Mainly {} (relative weapon scaling)", main.join(" / ")));
        }
        if let Some(arts_id) = read_i32(row + 0x198).filter(|id| *id > 0) {
            if let Some(runtime) = RUNTIME.get() {
                if let Some(name) = lookup_first(runtime, &[42, 331, 431], arts_id as u32) {
                    lines.push(format!("SKILL  {name}"));
                }
            }
        }
        let mut buildup = [0i32; 6];
        for slot in 0..3 {
            let Some(effect_id) = read_i32(row + 0x48 + slot * 4).filter(|id| *id > 0) else { continue; };
            let increment = reinforce.and_then(|r| read_u8(r + 0x50 + slot)).unwrap_or(0);
            if let Some(effect) = param_row(0x4C0, effect_id as u32 + u32::from(increment)) {
                for (i, offset) in [0xCC, 0xD0, 0xD4, 0x1A8, 0x338, 0x33C].iter().enumerate() {
                    if let Some(value) = read_i32(effect + offset).filter(|v| *v > 0 && *v < 10000) {
                        buildup[i] += value;
                    }
                }
            }
        }
        let passive = ["Poison", "Scarlet rot", "Bleed", "Frost", "Sleep", "Madness"]
            .iter().zip(buildup).filter(|(_, value)| *value > 0)
            .map(|(name, value)| format!("{name} {value}")).collect::<Vec<_>>();
        if !passive.is_empty() { lines.push(format!("BASE BUILDUP  {} (before attribute bonuses)", passive.join(" · "))); }
        if let Some(weight) = weight { lines.push(format!("WEIGHT  {weight:.1}")); }
        lines.push(format!("IN PRACTICE  {note}"));
        if !matches!(kind, 50..=69 | 81..=86 | 89 | 90) {
            if let Some(strength) = read_u8(row + 0xF2).filter(|v| *v > 1) {
                lines.push(format!("TWO-HANDING  {} STR meets the {} STR requirement when two-handed; other requirements still apply.", crate::guidance::two_hand_requirement(strength), strength));
            }
        }
        if !values.is_empty() && !matches!(kind, 57 | 61 | 65 | 67 | 69 | 90) {
            lines.push("DAMAGE NOTE  Base attack excludes your attribute bonus and enemy defences; it is not the damage each hit will deal.".into());
        }
        lines
    }
}

fn weight_details(
    param_offset: usize,
    param_id: u32,
    weight_offset: usize,
    label: &str,
    classify_load: bool,
) -> Vec<String> {
    let Some(row) = param_row(param_offset, param_id) else {
        return vec![format!("TYPE  {label}")];
    };
    let weight = unsafe { read_f32(row + weight_offset) }.unwrap_or(0.0);
    let class = if weight < 4.0 {
        "Light"
    } else if weight < 8.0 {
        "Medium"
    } else {
        "Heavy"
    };
    if classify_load {
        vec![format!("LOAD  {class} {label} · {weight:.1} weight")]
    } else {
        vec![format!("BUILD  Any build · {label} · {weight:.1} weight")]
    }
}

fn goods_details(param_id: u32, name: &str) -> Vec<String> {
    let Some(row) = param_row(0x160, param_id) else {
        return crate::guidance::goods_note(255, name).map(|note| vec![format!("IN PRACTICE  {note}")]).unwrap_or_default();
    };
    let goods_type = unsafe { read_u8(row + 0x3E) }.unwrap_or(0);
    if let Some(lines) = whetblade_guidance(param_id) {
        return lines;
    }

    let mut lines = vec![format!("TYPE  {}", goods_type_label(goods_type))];
    let recipes = crafting_outputs(param_id);
    if !recipes.is_empty() {
        lines.push(format!("USED TO MAKE  {}", recipes.join(" · ")));
    }
    if let Some(note) = crate::guidance::goods_note(goods_type, name) {
        lines.push(format!("IN PRACTICE  {note}"));
    }
    lines
}

fn whetblade_guidance(param_id: u32) -> Option<Vec<String>> {
    let lines: &[&str] = match param_id {
        8590 => &[
            "USE  Apply Ashes of War to compatible armaments at a Site of Grace",
            "UNLOCKS  Standard affinity choices supplied by the selected skill",
        ],
        8970 => &[
            "UNLOCKS  Heavy · Keen · Quality affinities",
            "SCALING  Heavy favours STR; Keen favours DEX; Quality divides scaling between both",
        ],
        8971 => &[
            "UNLOCKS  Fire · Flame Art affinities",
            "SCALING  Fire favours STR; Flame Art adds fire damage that scales with FAI",
        ],
        8972 => &[
            "UNLOCKS  Lightning · Sacred affinities",
            "SCALING  Lightning favours DEX; Sacred adds holy damage that scales with FAI",
        ],
        8973 => &[
            "UNLOCKS  Magic · Cold affinities",
            "SCALING  Magic favours INT; Cold adds INT scaling and frost buildup",
        ],
        8974 => &[
            "UNLOCKS  Poison · Blood · Occult affinities",
            "SCALING  Poison/Blood add ARC scaling and status buildup; Occult shifts physical and innate-status scaling toward ARC",
        ],
        _ => return None,
    };
    Some(lines.iter().map(|line| (*line).to_string()).collect())
}

fn goods_type_label(goods_type: u8) -> &'static str {
    match goods_type {
        0 => "Item",
        1 => "Key item",
        2 => "Crafting material",
        3 => "Remembrance",
        5 => "Sorcery",
        7 => "Spirit summon",
        8 => "Great spirit summon",
        9 => "Wondrous Physick",
        10 => "Crystal tear",
        11 => "Regenerative material",
        12 => "Info item",
        14 => "Reinforcement material",
        15 => "Great Rune",
        16 => "Incantation",
        _ => "Item",
    }
}

fn affinity_label(param_id: u32) -> &'static str {
    match (param_id / 100) % 100 {
        1 => "Heavy",
        2 => "Keen",
        3 => "Quality",
        4 => "Fire",
        5 => "Flame Art",
        6 => "Lightning",
        7 => "Sacred",
        8 => "Magic",
        9 => "Cold",
        10 => "Poison",
        11 => "Blood",
        12 => "Occult",
        _ => "Standard",
    }
}

fn crafting_outputs(material_id: u32) -> Vec<String> {
    let Some(recipe_param) = param_base(0x868) else {
        return Vec::new();
    };
    let Some(material_param) = param_base(0x748) else {
        return Vec::new();
    };
    let Some(runtime) = RUNTIME.get() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = HashSet::new();

    for recipe_row in param_rows(recipe_param) {
        unsafe {
            let Some(material_set_id) = read_i32(recipe_row + 0x08).filter(|&id| id >= 0) else {
                continue;
            };
            let Some(material_row) = find_row(material_param, material_set_id as u32) else {
                continue;
            };
            let used = (0..6).any(|slot| {
                read_i32(material_row + slot * 4) == Some(material_id as i32)
                    && read_u8(material_row + 0x28 + slot) == Some(4)
            });
            if !used {
                continue;
            }

            let equip_type = read_u8(recipe_row + 0x17).unwrap_or(3);
            let Some(output_id) = read_i32(recipe_row)
                .filter(|&id| id >= 0)
                .map(|id| id as u32)
            else {
                continue;
            };
            let categories: &[u32] = match equip_type {
                0 => &[11, 310, 410],
                1 => &[12, 313, 413],
                2 => &[13, 316, 416],
                4 => &[35, 322, 422],
                _ => &[10, 319, 419],
            };
            if seen.insert((equip_type, output_id)) {
                if let Some(name) = lookup_first(runtime, categories, output_id) {
                    names.push(name);
                }
            }
        }
    }
    names
}

fn param_base(param_offset: usize) -> Option<usize> {
    let runtime = RUNTIME.get()?;
    if runtime.solo_param_slot == 0 {
        return None;
    }
    unsafe {
        let repo = read_ptr(runtime.solo_param_slot)?;
        let first = read_ptr(repo.checked_add(param_offset)?)?;
        let second = read_ptr(first.checked_add(0x80)?)?;
        read_ptr(second.checked_add(0x80)?)
    }
}

fn param_row(param_offset: usize, row_id: u32) -> Option<usize> {
    find_row(param_base(param_offset)?, row_id)
}

fn find_row(param: usize, row_id: u32) -> Option<usize> {
    unsafe {
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
            match (read_i32(row)? as u32).cmp(&row_id) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    let offset = read_i32(row + 0x08)?;
                    return (0..=0x0800_0000)
                        .contains(&offset)
                        .then(|| param + offset as usize);
                }
            }
        }
    }
    None
}

fn param_rows(param: usize) -> Vec<usize> {
    unsafe {
        let Some(table_end) = read_i32(param + 0x30) else {
            return Vec::new();
        };
        if !(0x40..=0x0400_0000).contains(&table_end) {
            return Vec::new();
        }
        let count = (table_end as usize - 0x40) / 0x18;
        (0..count)
            .filter_map(|index| {
                let entry = param + 0x40 + index * 0x18;
                let offset = read_i32(entry + 0x08)?;
                (0..=0x0800_0000)
                    .contains(&offset)
                    .then(|| param + offset as usize)
            })
            .collect()
    }
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

fn find_item_popup_rva(base: usize) -> Option<usize> {
    // The game's large item-acquisition panel calls this function with (MapItemMan + 0xA0,
    // ItemPopupEntry*). The Grand Archives signature begins 0x14 bytes into the function:
    // ?? 8B FA ?? 8B D9 ?? 8B 81 A8 00 00 00
    const PATTERN: &[i16] = &[
        -1, 0x8B, 0xFA, -1, 0x8B, 0xD9, -1, 0x8B, 0x81, 0xA8, 0x00, 0x00, 0x00,
    ];
    let (text, size) = pe_text_section(base)?;
    let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, size) };
    let mut matches = bytes
        .windows(PATTERN.len())
        .enumerate()
        .filter_map(|(offset, window)| {
            window
                .iter()
                .zip(PATTERN)
                .all(|(&actual, &expected)| expected < 0 || actual == expected as u8)
                .then_some(offset)
        });
    let offset = matches.next()?;
    if matches.next().is_some() || offset < 0x14 {
        return None;
    }
    text.checked_add(offset - 0x14)?.checked_sub(base)
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

unsafe fn read_u8(address: usize) -> Option<u8> {
    plausible(address).then(|| (address as *const u8).read_unaligned())
}

unsafe fn read_u16(address: usize) -> Option<u16> {
    plausible(address).then(|| (address as *const u16).read_unaligned())
}

unsafe fn read_f32(address: usize) -> Option<f32> {
    let value = plausible(address).then(|| (address as *const f32).read_unaligned())?;
    value.is_finite().then_some(value)
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
