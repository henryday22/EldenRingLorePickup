//! Optional read-only character context. Failure never blocks or filters a pickup card.
use crate::runtime::RUNTIME;
use windows_sys::Win32::System::{
    Diagnostics::Debug::ReadProcessMemory, Threading::GetCurrentProcess,
};

#[derive(Clone, Debug)]
pub struct Player {
    pub profile: String,
    pub name: String,
    pub attributes: [u32; 5],
}

fn read<const N: usize>(address: usize) -> Option<[u8; N]> {
    if address < 0x10000 {
        return None;
    }
    let mut bytes = [0u8; N];
    let mut got = 0usize;
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            address as *const _,
            bytes.as_mut_ptr() as *mut _,
            N,
            &mut got,
        )
    };
    (ok != 0 && got == N).then_some(bytes)
}
fn ptr(address: usize) -> Option<usize> {
    Some(usize::from_le_bytes(read::<8>(address)?))
}

pub fn current() -> Option<Player> {
    let runtime = RUNTIME.get()?;
    // The global translation is witnessed by 642 GameDataMan / 142 GameMan references
    // in er-mods' 1.16.2 -> 1.17 ledger. Do not apply it to an unrecognised version.
    if !runtime.label.contains("1.17-era") {
        return None;
    }
    let data = ptr(runtime.base + 0x3D61F98)?;
    let game = ptr(runtime.base + 0x3D6D988)?;
    let slot = i32::from_le_bytes(read::<4>(game.checked_add(0xAC0)?)?);
    if !(0..10).contains(&slot) {
        return None;
    }
    let pgd = ptr(data.checked_add(8)?)?;
    let bytes = read::<0x100>(pgd)?;
    parse(&bytes, slot)
}

fn parse(bytes: &[u8; 0x100], slot: i32) -> Option<Player> {
    let get = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let level = get(0x68);
    let all: [u32; 8] = std::array::from_fn(|i| get(0x3C + i * 4));
    if !(1..=713).contains(&level) || all.iter().any(|v| !(1..=99).contains(v)) {
        return None;
    }
    // Level and the eight base stats obey the same relationship for all starting classes.
    if all.iter().sum::<u32>() != level + 79 {
        return None;
    }
    let units: Vec<u16> = bytes[0x9C..0xBE]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|v| *v != 0)
        .collect();
    let name = String::from_utf16(&units).ok()?.trim().to_string();
    if name.is_empty() || name.chars().any(char::is_control) {
        return None;
    }
    // Slot + saved character ID + name separates ordinary characters. A manual profile in
    // LorePickup.ini covers renamed characters or recreated/duplicated saves.
    let profile = format!("slot-{slot}-{:08x}-{name}", get(0xEC));
    Some(Player {
        profile,
        name,
        attributes: [all[3], all[4], all[5], all[6], all[7]],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validated_character_data_separates_slots_and_rejects_unready_memory() {
        let mut bytes = [0u8; 0x100];
        for i in 0..8 {
            bytes[0x3C + i * 4..0x40 + i * 4].copy_from_slice(&10u32.to_le_bytes());
        }
        bytes[0x68..0x6C].copy_from_slice(&1u32.to_le_bytes());
        for (i, v) in "Test".encode_utf16().enumerate() {
            bytes[0x9C + i * 2..0x9E + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        let a = parse(&bytes, 0).unwrap();
        let b = parse(&bytes, 1).unwrap();
        assert_ne!(a.profile, b.profile);
        assert_eq!(a.attributes, [10; 5]);
        bytes[0x68] = 2;
        assert!(parse(&bytes, 0).is_none());
        assert!(parse(&[0; 0x100], 0).is_none());
    }
}
