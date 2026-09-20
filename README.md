# EldenRingLorePickup

A small Elden Ring quality-of-life mod. When Elden Ring opens its large item panel—the one that
waits for **Y/OK**—LorePickup shows that item's in-game description and useful gameplay information
in a clean panel at the right of the screen.

The normal pickup flow makes Elden Ring's item lore easy to miss. LorePickup presents it at the
moment it is useful without opening the inventory or pausing play.

## Current release

**v0.8.1 fixes a confirmed frontend-address error in v0.8.0.** Version 0.8.0 used the
1.16 frontend singleton address as its fallback for 1.17. The supplied pickup log captured Kukri
and Exile Gauntlets successfully but discarded both because that popup probe never resolved.
The 1.17 address is now corrected and its object identity is checked before the HUD state is read.

[Download v0.8.1](https://github.com/henryday22/EldenRingLorePickup/releases/tag/v0.8.1).
Replace the DLL, retain the icon folder and existing `.me3` configuration, then restart the game.

**Validation:** automated memory-probe and queue tests plus a Windows build. This is a prerelease
pending an in-game check. The association of HUD state 2 with all Y-dismissable item panels has
not been verified in the user's game. This release fixes the proven address bug; it does not claim
that a direct item-panel hook has been implemented or that all popup behavior is confirmed.

## Features

- Uses a validated frontend HUD-state probe to gate item candidates.
- Does not filter by item ownership or first-time pickup history.
- Suppresses candidates when the frontend probe does not confirm a popup.
- Reads names and lore from the game's live message repository in the game's current language.
- Supports weapons, armour, talismans, goods and Ashes of War across base-game and DLC tables.
- Resolves each item's live inventory icon, with a bundled mapping fallback.
- Shows weapon affinity, attribute requirements, base damage, skill and likely build use.
- Shows ingredient recipe outputs where the live recipe params expose them.
- Explains the affinities and stat effects unlocked by each Whetblade.
- Displays queued cards as a visible stack and advances them one Y press at a time.
- Uses a short, clean fade with no simulated particle effect.
- Uses no DirectX hook and hides its click-through overlay completely while idle.
- Fails closed if the expected executable signatures do not match.

The runtime addresses currently cover the 2.6.2.0, 2.7.0.0 and 2.7.1.0 executable families used
by the current 1.17-era modding stack. Signatures are checked before any hook or message lookup.

## Installation with ModEngine3

Extract the release so the DLL and icon folder sit together:

    profiles/
      eldenring-default.me3
      natives/
        EldenRingLorePickup/
          EldenRingLorePickup.dll
          EldenRingLorePickup-icons/
            MENU_Knowledge_00000.png
            ...

Add this block to the `.me3` profile if it is not already present:

    [[natives]]
    enabled = true
    load_early = false
    path = 'natives/EldenRingLorePickup/EldenRingLorePickup.dll'

If v0.7.0 is already installed, replace only `EldenRingLorePickup.dll`. Keep the icon folder and
leave the `.me3` entry unchanged. `EldenRingLorePickup-card.png` is no longer read and may be
deleted.

ME3 resolves native paths relative to the `.me3` profile.

## Behaviour

The panel is intended to behave as follows:

- opens only when an item presentation candidate coincides with Elden Ring's large blocking popup;
- remains paired with that game panel instead of inheriting the short right-side log lifetime;
- observes the same controller **Y** press that dismisses the game panel without consuming it;
- waits for Y to be released before arming the next queued card, preventing one held press from
  skipping a stack;
- shows up to two offset rear panels plus an exact remaining-card count when several items queue;
- uses an opaque icon and text over a restrained translucent charcoal panel;
- formats the original lore into consistent paragraphs and separates factual details into aligned
  label/value rows;
- fades in over 120 ms and out over 160 ms, with no low-resolution particle overlay;
- has a 90-second safety timeout if the underlying game panel becomes stuck.

Weapon information deliberately prioritises affinity, requirements, damage and skill. Defensive
resistances are not shown. Armour and talisman information remains limited to concise weight/use
guidance. Ingredients list up to four known crafting outputs.

## Compatibility and log

LorePickup deliberately does **not** hook DirectX. It uses two transparent, click-through Win32
overlay windows aligned to Elden Ring's client area. The lower layer provides the translucent panel;
the upper layer keeps text and icons opaque. The item hook is delayed for eight seconds so it does
not join the native-mod initialisation burst.

The diagnostic log is `EldenRingLorePickup.log` in Elden Ring's working directory.

Use offline or with ModEngine3's normal official-online protection. Native gameplay mods should not
be used on official matchmaking servers.

## Building

Requirements:

- Windows 10/11
- Rust stable with `x86_64-pc-windows-msvc`
- Visual Studio 2022 Build Tools / MSVC

Build with:

    cargo build --release --target x86_64-pc-windows-msvc

GitHub Actions also builds and packages the DLL on every push.

## Technical notes

The item presentation hook supplies a raw item ID and quantity, but that signal alone is too broad:
the same path can be reached for routine pickups. LorePickup therefore stages the item for 1.5
seconds and promotes it only while the validated `CSFeManImp` reports `CSFeManHudState::PopupMenu` (`2`). Normal
gameplay and small pickup logs remain in `Default` (`3`).

The item ID encodes its category in the high nibble. LorePickup queries the game's live
`MsgRepository`/`SearchStringTable` using these FMG categories:

- Goods: name 10, info 20, caption 24
- Weapons: name 11, info 21, caption 25
- Armour: name 12, info 22, caption 26
- Talismans: name 13, info 23, caption 27
- Ashes of War / gems: name 35, info 36, caption 37

DLC1 and DLC2 fallbacks are included. See `THIRD_PARTY_NOTICES.md` for the public
reverse-engineering references used by the project.

### Frontend probe correction

The known 2.7.0.0/2.7.1.0 executable signatures select singleton RVA `0x3D6F8F0` and
CSFeManImp vtable RVA `0x2AA0A08`. The 2.6.2.0 signatures select `0x3D6B880` and
`0x2A9D988`. An unverified broad signature scan no longer overrides this mapping.
Reads use `ReadProcessMemory` and reject null, unreadable and mismatched objects. The log
records every probe change, including the first unavailable result, without logging each frame.
`None` in candidate-expiry messages means the probe was unavailable, not that the pickup was routine.

To check in-game: compare an existing-item Y/OK panel, a first-time-item Y/OK panel, and an
ordinary small-log pickup; also dismiss with Y and check a multiple-item pickup. Inspect the
new session's `panel probe` entries if any case still fails.
