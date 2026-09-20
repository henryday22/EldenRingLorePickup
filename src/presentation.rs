//! Per-item presentation metadata at ItemPopupEntry+0x0c on the observed 1.17 build.
//!
//! Trace-backed interpretation, not a fully reverse-engineered game struct:
//! +0x0d is 1 on NEW-item panels (Exile Gauntlets); +0x0e is 1 on forced
//! panels including existing items (Kukri). Both are 1 on the three Cracked Tears.
//! Ordinary resource pickups have neither. The upper byte is often uninitialized
//! 0xcc; treating the entire word as a boolean would show cards for those too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    Routine,
    Panel { new_item: bool, forced: bool },
    Unknown { new_byte: u8, forced_byte: u8 },
}

impl Presentation {
    pub fn shows_card(self) -> bool {
        matches!(self, Self::Panel { .. })
    }
}

pub fn classify(metadata: u32) -> Presentation {
    let [_, new_byte, forced_byte, _] = metadata.to_le_bytes();
    if new_byte > 1 || forced_byte > 1 {
        return Presentation::Unknown { new_byte, forced_byte };
    }
    if new_byte == 1 || forced_byte == 1 {
        Presentation::Panel { new_item: new_byte == 1, forced: forced_byte == 1 }
    } else {
        Presentation::Routine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_backed_pickups_are_not_rejected() {
        for (name, flags) in [
            ("Kukri (already owned)", 0x00010000),
            ("Exile Gauntlets (NEW)", 0xcc000100),
            ("Stalwart Horn Charm", 0x00010100),
            ("Magic-Shrouding Cracked Tear", 0x00010100),
            ("Lightning-Shrouding Cracked Tear", 0x00010100),
            ("Holy-Shrouding Cracked Tear", 0x00010100),
        ] {
            assert!(classify(flags).shows_card(), "{name}");
        }
    }

    #[test]
    fn ordinary_trace_values_do_not_qualify_even_with_dirty_padding() {
        for (name, flags) in [
            ("Rowa Fruit", 0), ("Erdleaf Flower", 0),
            ("Mushroom", 0xcc000000), ("Budding Horn", 0xcc000000),
        ] {
            assert_eq!(classify(flags), Presentation::Routine, "{name}");
        }
    }

    #[test]
    fn unrelated_bytes_cannot_change_the_decision() {
        for low in 0..=255u32 {
            for high in [0, 1, 0xcc, 0xff] {
                let padding = low | (high << 24);
                assert!(!classify(padding).shows_card());
                assert!(classify(padding | 0x100).shows_card());
                assert!(classify(padding | 0x10000).shows_card());
            }
        }
    }

    #[test]
    fn malformed_flag_bytes_fail_closed() {
        for flags in [0xffff_ffff, 0x0000_0200, 0x0002_0000, 0x0001_cc00] {
            assert!(matches!(classify(flags), Presentation::Unknown { .. }));
        }
    }
}
