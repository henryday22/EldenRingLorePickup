# EldenRingLorePickup

A small Elden Ring quality-of-life mod: when you acquire an item for the first time in a session, its actual in-game lore description appears in a non-blocking panel on the right side of the screen.

The aim is simple: Elden Ring hides a huge amount of its story in item descriptions, but the normal pickup flow makes those descriptions easy to miss. LorePickup puts the description in front of you at the moment it is contextually useful, without opening the inventory or pausing play.

## Current status

**Early alpha. v0.5 adds an ornate stacked-card queue, rune-dust transitions, manual dismissal,
more reliable inventory icons and live item-use details.**

The first implementation is now in this repository. It:

- builds as a native Windows DLL for ModEngine3;
- hooks Elden Ring's item-add path rather than polling the inventory;
- reads item names and descriptions from Elden Ring's live message repository, so it uses the game's current language;
- supports weapons, armour, talismans, goods and Ashes of War;
- searches base-game, DLC1 and DLC2 message tables;
- de-duplicates items during a session so repeat consumable pickups do not spam the panel;
- presents rapid multi-item pickups as a visible deck instead of making the queue invisible;
- uses a click-through Win32 overlay window above the game's pickup strip, with no DirectX Present hook;
- resolves each pickup's live `iconId` from the game's parameter repository, with a built-in
  base-game mapping fallback;
- fails closed when the expected Elden Ring function signatures do not match.

## Download

Download the ready-to-install package from the
[v0.5.0 GitHub release](https://github.com/henryday22/EldenRingLorePickup/releases/tag/v0.5.0).
It contains the DLL and the complete compact icon folder.

The runtime addresses currently cover the 2.6.2.0, 2.7.0.0 and 2.7.1.0 executable families used by the current 1.17-era modding stack. Signature checks are mandatory before either hook or message lookup is used.

## Building

Requirements for building the DLL locally:

- Windows 10/11
- Rust stable (x86_64-pc-windows-msvc)
- Visual Studio 2022 Build Tools / MSVC toolchain

Run:

    cargo build --release

The DLL is produced under target/release.

GitHub Actions also builds the DLL on every push.

## ModEngine3

Place the built DLL in a natives folder next to your ME3 profile, for example:

    profiles/
      eldenring-default.me3
      natives/
        EldenRingLorePickup.dll
        EldenRingLorePickup-icons/
          MENU_Knowledge_00000.png
          ...

Then add this block to the profile:

    [[natives]]
    path = 'natives/EldenRingLorePickup.dll'

ME3 resolves native paths relative to the .me3 profile.

## Behaviour

The v0.5 card:

- appears above the game's lower-right item-pickup notification;
- uses a narrower, taller playing-card proportion with consistent paragraph spacing;
- uses a translucent charcoal panel while keeping its typography, icon and frame fully opaque;
- scales with the game window and uses large Garamond typography, ivory text and restrained
  double-line gold edging;
- shows the item's actual inventory thumbnail in a small gilded tile, with a category medallion
  fallback if a custom/modded icon is not supplied;
- displays live weapon requirements, affinity, base attack and Ash of War; armour/talisman load
  guidance; and crafting outputs for ingredients where the recipe params expose them;
- stays visible for 14–24 seconds, shortening the wait when more cards are queued;
- shows up to three queued cards behind the current card, with a total deck count;
- dismisses the current card with **F8** and advances to the next queued pickup;
- dissolves the outgoing card into gold, rune-like dust instead of simply disappearing;
- composes each card off-screen and presents it atomically to prevent transparency flicker;
- completely hides the overlay window when there is no lore to show;
- then advances to the next queued pickup;
- ignores an exact item after it has already been shown in the current game session.

The DLL looks for thumbnails beside itself in `EldenRingLorePickup-icons`. The release archive
ships that folder ready to use. Icon selection prefers a valid live parameter value, verifies that
the corresponding PNG exists, and then falls back to the bundled base-game mapping. A zero live
ID can no longer mask a valid item icon.

## Compatibility note

LorePickup deliberately does **not** hook DirectX. The original v0.1 DX12/ImGui renderer prevented the user's ERSS/OptiScaler stack from reaching the game. The replacement is a pair of transparent, click-through Win32 overlay windows aligned to Elden Ring's client area. The lower layer controls only the panel translucency; the upper layer keeps text, edging and icon opaque. The AddItem hook is also delayed for eight seconds after startup so it does not participate in the game's native-mod initialization burst.

The log is written as `EldenRingLorePickup.log` in Elden Ring's working directory.

Use offline / with ModEngine3's normal official-online protection. Native gameplay mods should not be used on official matchmaking servers.

## Technical notes

Pickup detection is based on Elden Ring's AddItem function. The item ID encodes its category in the high nibble. LorePickup calls the game's own SearchStringTable against the live MsgRepository and chooses the appropriate FMG categories:

- Goods: name 10, info 20, caption 24
- Weapons: name 11, info 21, caption 25
- Armour: name 12, info 22, caption 26
- Talismans: name 13, info 23, caption 27
- Ashes of War / gems: name 35, info 36, caption 37

DLC1 and DLC2 category fallbacks are included as well.

See THIRD_PARTY_NOTICES.md for the reverse-engineering references used to make the alpha.
