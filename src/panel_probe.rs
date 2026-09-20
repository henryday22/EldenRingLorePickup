//! Frontend probe logic, isolated from Windows so invalid-address handling is testable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelProbe {
    Unavailable,
    WrongObject { actual_vtable: usize },
    InvalidState(u8),
    Hud(u8),
}

impl PanelProbe {
    pub fn visible(self) -> Option<bool> {
        match self {
            Self::Hud(2) => Some(true),
            Self::Hud(0 | 1 | 3) => Some(false),
            _ => None,
        }
    }
}

pub fn probe(
    slot: usize,
    expected_vtable: usize,
    read_ptr: impl Fn(usize) -> Option<usize>,
    read_byte: impl Fn(usize) -> Option<u8>,
) -> PanelProbe {
    let Some(object) = read_ptr(slot).filter(|&p| p >= 0x10000) else {
        return PanelProbe::Unavailable;
    };
    let Some(vtable) = read_ptr(object) else {
        return PanelProbe::Unavailable;
    };
    if vtable != expected_vtable {
        return PanelProbe::WrongObject { actual_vtable: vtable };
    }
    let Some(state) = object.checked_add(0x78).and_then(read_byte) else {
        return PanelProbe::Unavailable;
    };
    match state {
        0..=3 => PanelProbe::Hud(state),
        _ => PanelProbe::InvalidState(state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: usize = 0x140000000;
    const OLD: usize = BASE + 0x3d6b880;
    const NEW: usize = BASE + 0x3d6f8f0;
    const OBJECT: usize = 0x200000;
    const VTABLE: usize = BASE + 0x2aa0a08;

    fn memory(address: usize) -> Option<usize> {
        match address {
            NEW => Some(OBJECT),
            OLD => Some(0),
            OBJECT => Some(VTABLE),
            _ => None,
        }
    }

    #[test]
    fn stale_116_address_cannot_confirm_117_popup() {
        assert_eq!(probe(OLD, VTABLE, memory, |_| Some(2)).visible(), None);
        assert_eq!(probe(NEW, VTABLE, memory, |_| Some(2)).visible(), Some(true));
    }

    #[test]
    fn wrong_object_is_not_interpreted_as_a_hud() {
        let result = probe(NEW, VTABLE + 8, memory, |_| panic!("must not read wrong object"));
        assert!(matches!(result, PanelProbe::WrongObject { .. }));
        assert_eq!(result.visible(), None);
    }

    #[test]
    fn unreadable_object_or_state_is_unknown_not_closed() {
        assert_eq!(probe(NEW, VTABLE, |_| None, |_| None).visible(), None);
        assert_eq!(probe(NEW, VTABLE, memory, |_| None).visible(), None);
        assert_eq!(probe(NEW, VTABLE, memory, |_| Some(255)).visible(), None);
    }

    #[test]
    fn hud_open_close_and_routine_gameplay() {
        for (state, expected) in [(3, false), (2, true), (2, true), (3, false)] {
            assert_eq!(probe(NEW, VTABLE, memory, |address| {
                assert_eq!(address, OBJECT + 0x78);
                Some(state)
            }).visible(), Some(expected));
        }
    }
}
