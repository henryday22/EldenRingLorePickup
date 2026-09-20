# EldenRingLorePickup

Displays item lore, inventory icons and useful gameplay details in a modern side panel.

## v0.9.0: centred cards, popup palette, useful field notes

[Download v0.9.0](https://github.com/henryday22/EldenRingLorePickup/releases/tag/v0.9.0).

The v0.8.2 pickup trigger and controller-Y dismissal are preserved. The user confirmed that
version now displays cards for their item panels. This revision changes presentation and item
information, not the event filter.

- **Position:** each card and its visible stack are vertically centred using the measured text
  height, including after a window resize or advancing to a different item.
- **Appearance:** warm charcoal, restrained gold-grey double rules, ivory serif text and a ringed
  Y prompt. No parchment image, particle effect or blue-grey rounded border.
- **Font:** an installed Agmena Pro / Agmena W1G / Agmena face is preferred, then Garamond and
  Georgia. Proprietary game fonts are not bundled; the default is an approximation, not a claim
  of a pixel-identical reproduction of the game renderer.
- **Weapons:** class, affinity, upgrade level, requirements, base attack with reinforcement where
  available, leading scaling attributes, skill, base status buildup and weight. Practical notes
  explain the weapon class and the reduced Strength requirement when two-handed. Base attack is
  explicitly distinguished from player-adjusted attack rating or actual damage dealt; a casting
  tool's melee attack is not presented as spell damage. Scaling guidance compares coefficients,
  not the damage of a specific character build.
- **Other items:** practical notes for materials, grease, cookbooks, bell bearings, upgrade stones,
  spirits, spells, tears, armour, talismans and Ashes of War, alongside original game lore/effects.
  Recipes are no longer silently limited to the first four outputs. Whetblade affinity guidance
  remains available. Unknown items retain their original text without fabricated lore.
- **Long descriptions:** keep readable type and scroll with **LB/RB** when the footer says
  “read more”. The title and Y prompt remain fixed. These buttons are observed, not intercepted;
  they may also reach the game. Short cards require no scrolling.

The extra prose is authored category/use guidance selected from live item data and, for certain
consumables, English item names. It is not a unique generated story for every item. Selected weapons (Clawmark Seal, Claymore and Bloodhound's Fang) also have specific notes. Numerical
values come from game parameters rather than a bundled balance database. Non-English item
names still receive category guidance; English-name-specific notes may not appear.

## Pickup eligibility

The previous global HUD flag is not used. Per-item metadata at +0x0d or +0x0e must equal 1.
This interpretation was inferred from supplied game traces, then confirmed working by the user
in v0.8.2. Examples covered by regression tests include existing Kukri, NEW Exile Gauntlets,
Stalwart Horn Charm and the three Cracked Tears. Ordinary unflagged pickups remain excluded.

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
- Item details are read from live params; Whetblades explain the affinities they unlock.
- Missing caption text falls back to the item's short description instead of suppressing the card.

## Validation and limits

The v0.8.2 trigger has user confirmation; v0.9.0 styling and added param reads still need in-game
verification. Automated tests use the exact metadata
values from the supplied logs and exercise the three-Tear queue, repeat items, ordinary pickups,
long reading times and held Y. The Windows DLL is built in GitHub Actions. Actual GDI renderer previews cover 864p, 1080p,
1440p, a short card, a long card and its scrolled bottom. Layout checks cover up to 2160p.

The previous HUD gate is removed entirely. This release observes controller Y rather than the
actual item-panel close event. Keyboard/remapped confirm input, panels closed by death/loading,
and unusual input timing are not yet synchronized. It does not claim end-to-end validation or
support for every unobserved presentation path. The observed metadata interpretation is grounded
in the user's 2.7.1.0 / 1.17-era sessions; older runtime patterns remain compatibility candidates.

The diagnostic log is `EldenRingLorePickup.log` in the game's working directory. A v0.9.0
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
