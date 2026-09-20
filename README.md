# EldenRingLorePickup

Displays item lore, inventory icons and useful gameplay details in a modern side panel.

## v0.8.2: remove the incorrect HUD-state gate

[Download v0.8.2](https://github.com/henryday22/EldenRingLorePickup/releases/tag/v0.8.2).

The v0.8.1 session log showed a valid `Hud(3)` read throughout the Stalwart Horn Charm and
three Cracked Tear panels. The general frontend HUD flag does not indicate whether these
Y/OK item panels are open. It is no longer read or used for card eligibility or dismissal.

The pickup entry already includes per-item presentation metadata. In the supplied 1.17 traces:

| Pickup | Metadata word at +0x0c | Result |
|---|---|---|
| Existing Kukri | `0x00010000` | Queue a card |
| NEW Exile Gauntlets | `0xcc000100` | Queue a card |
| Stalwart Horn Charm | `0x00010100` | Queue a card |
| Each of the three Cracked Tears | `0x00010100` | Queue three ordered cards |
| Ordinary Rowa Fruit / Erdleaf Flower | `0x00000000` | No card |
| Ordinary Mushroom / Budding Horn | `0xcc000000` | No card |

Only byte +0x0d (observed NEW flag) and byte +0x0e (observed forced-panel flag) determine the
classification; a value of 1 in either qualifies. Non-boolean values fail closed and are logged.
The unrelated bytes, including the often-dirty `0xcc` padding, cannot trigger a card.
This is an inference from captured game events, not a fully reverse-engineered struct contract.

## Installation

Close Elden Ring, replace `EldenRingLorePickup.dll`, and relaunch. Keep the existing icon folder
and `.me3` entry. The package layout is:

    profiles/
      eldenring-default.me3
      natives/
        EldenRingLorePickup/
          EldenRingLorePickup.dll
          EldenRingLorePickup-icons/

The ME3 profile entry is:

    [[natives]]
    enabled = true
    load_early = false
    path = 'natives/EldenRingLorePickup/EldenRingLorePickup.dll'

The old `EldenRingLorePickup-card.png` is unused. No background artwork is required.

## Behavior

- No item-ownership history or first-time-only filter.
- Qualifying events queue immediately, without the old 1.5-second candidate expiration.
- Multiple cards retain pickup order and display an offset stack with a remaining-card count.
- Cards remain until controller Y is pressed; the small right-side log timer has no effect.
- The Y press used to collect an item cannot dismiss its card while held. Release Y, then press
  again to dismiss; each subsequent card also waits for a release before accepting another press.
- Dismissal is a 160 ms fade, with no particle effect.
- Lore text and icons remain opaque over a translucent charcoal panel.
- Weapons prioritise affinity, requirements, base damage and skill. Crafting materials list recipe
  outputs where live params provide them; Whetblades explain the affinities they unlock.
- Missing caption text falls back to the item's short description instead of suppressing the card.

## Validation and limits

This is a **prerelease requiring in-game verification**. Automated tests use the exact metadata
values from the supplied logs and exercise the three-Tear queue, repeat items, ordinary pickups,
long reading times and held Y. The Windows DLL is built in GitHub Actions.

The previous HUD gate is removed entirely. This release observes controller Y rather than the
actual item-panel close event. Keyboard/remapped confirm input, panels closed by death/loading,
and unusual input timing are not yet synchronized. It does not claim end-to-end validation or
support for every unobserved presentation path. The observed metadata interpretation is grounded
in the user's 2.7.1.0 / 1.17-era sessions; older runtime patterns remain compatibility candidates.

The diagnostic log is `EldenRingLorePickup.log` in the game's working directory. A v0.8.2
session logs `metadata=`, its `presentation decision`, and the queued item names. It should not
produce `panel probe` or `candidate expired` entries.

## Building

On Windows with Rust stable and Visual Studio C++ Build Tools:

    cargo test --target x86_64-pc-windows-msvc
    cargo build --release --target x86_64-pc-windows-msvc

The metadata classifier can also be tested without Windows:

    rustc --edition 2021 --test src/presentation.rs -o presentation-tests
    ./presentation-tests

The mod uses click-through Win32 overlay windows, not DirectX hooks. Runtime item/message
signatures are checked before installing the item hook. Use ME3's normal official-online
protection and do not load native gameplay mods on official matchmaking servers.

See `THIRD_PARTY_NOTICES.md` for reverse-engineering references. No game text is bundled.
