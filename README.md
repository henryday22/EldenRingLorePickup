# EldenRingLorePickup

A small Elden Ring quality-of-life mod: when Elden Ring presents its own first-acquisition item dialog, the item's actual in-game lore description appears on a non-blocking card at the right side of the screen.

The aim is simple: Elden Ring hides a huge amount of its story in item descriptions, but the normal pickup flow makes those descriptions easy to miss. LorePickup puts the description in front of you at the moment it is contextually useful, without opening the inventory or pausing play.

## Current status

**Early alpha. v0.6.3 fixes the inventory-pointer and live-capture failures found in v0.6.1 and
v0.6.2. It now follows Elden Ring's save-backed inventory pointer correctly and opens a card only
when AddItem creates a new inventory row. The card remains readable
independently of the short pickup log.**

The first implementation is now in this repository. It:

- builds as a native Windows DLL for ModEngine3;
- hooks Elden Ring's item-add path rather than polling the inventory;
- reads item names and descriptions from Elden Ring's live message repository, so it uses the game's current language;
- supports weapons, armour, talismans, goods and Ashes of War;
- searches base-game, DLC1 and DLC2 message tables;
- compares the save-backed inventory before and after AddItem and opens a card only when the game
  creates a new entry;
- leaves first-acquisition memory to the game's persistent per-character state rather than a
  per-launch mod list or an HDR-sensitive screen capture;
- uses a click-through Win32 overlay window above the game's pickup strip, with no DirectX Present hook;
- resolves each pickup's live `iconId` from the game's parameter repository, with a built-in
  base-game mapping fallback;
- fails closed when the expected Elden Ring function signatures do not match.

## Download

Download the ready-to-install package from the
[v0.6.3 GitHub release](https://github.com/henryday22/EldenRingLorePickup/releases/tag/v0.6.3).
It contains the DLL, illustrated card asset and complete compact icon folder.

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
        EldenRingLorePickup/
          EldenRingLorePickup.dll
          EldenRingLorePickup-card.png
          EldenRingLorePickup-icons/
            MENU_Knowledge_00000.png
            ...

Then add this block to the profile:

    [[natives]]
    enabled = true
    load_early = false
    path = 'natives/EldenRingLorePickup/EldenRingLorePickup.dll'

ME3 resolves native paths relative to the .me3 profile.

## Behaviour

The v0.6.3 card:

- appears only when Elden Ring creates a new inventory entry;
- remains available for up to 30 seconds instead of inheriting the one-to-two-second lifetime of
  the right-side pickup log;
- is dismissed by controller **Y**, with release-edge protection so the pickup press cannot also
  dismiss the new card immediately;
- uses a tall, hand-aged light vellum illustration with ragged transparent edges, wear, stains and
  restrained gilded ornament; dark sepia ink replaces outlined white overlay text;
- keeps the artwork translucent while its Garamond typography and inventory icon remain opaque;
- shows the item's actual inventory thumbnail in a small gilded tile, with a category medallion
  fallback if a custom/modded icon is not supplied;
- displays live weapon requirements, affinity, base attack and Ash of War; armour/talisman load
  guidance; and crafting outputs for ingredients where the recipe params expose them;
- groups only genuine paragraph breaks and flows wrapped lines as a single paragraph;
- stacks queued new-item cards visibly behind the current card;
- closes with a smooth short fade, with no pixel-grid or coarse particle dissolve;
- composes each card off-screen and presents it atomically to prevent transparency flicker;
- completely hides the overlay window when there is no lore to show;
- rejects repeat pickups before they enter the card queue.

The DLL looks for the card artwork beside itself as `EldenRingLorePickup-card.png` and thumbnails
in `EldenRingLorePickup-icons`. The release archive ships both ready to use. Icon selection prefers
a valid live parameter value, verifies that the corresponding PNG exists, and then falls back to
the bundled base-game mapping. A zero live ID cannot mask a valid mapped item icon.

Practical information is derived from live params where possible. Weapons show build tendency,
requirements, affinity, base attack and skill; ingredients list up to four known recipe outputs.
Whetblades include curated affinity guidance—for example, the Black Whetblade explains that it
unlocks Poison, Blood and Occult and how those choices alter Arcane scaling and status buildup.

## Compatibility note

LorePickup deliberately does **not** hook DirectX. The original v0.1 DX12/ImGui renderer prevented the user's ERSS/OptiScaler stack from reaching the game. The replacement is a pair of transparent, click-through Win32 overlay windows aligned to Elden Ring's client area. The lower layer controls only the illustrated card's translucency; the upper layer keeps text and icon opaque. The card observes XInput Y without consuming it, and the AddItem hook is delayed for eight seconds after startup so it does not participate in the game's native-mod initialization burst.

The log is written as `EldenRingLorePickup.log` in Elden Ring's working directory.

Use offline / with ModEngine3's normal official-online protection. Native gameplay mods should not be used on official matchmaking servers.

## Technical notes

Pickup data is based on Elden Ring's AddItem function and a before/after comparison of the game's
save-backed inventory. `PlayerGameData + 0x5D0` contains a pointer to `EquipInventoryData`; the
mod follows that pointer and accepts only an absent-to-present row. The separate `isNew` field is
logged for diagnosis but is not trusted by itself because the inventory UI can clear it. Repeat stack
pickups therefore never enter the card queue, and no HDR-sensitive screen
capture is involved. The item ID encodes its category in the high nibble. LorePickup calls the
game's own SearchStringTable against the live MsgRepository and chooses the appropriate FMG
categories:

- Goods: name 10, info 20, caption 24
- Weapons: name 11, info 21, caption 25
- Armour: name 12, info 22, caption 26
- Talismans: name 13, info 23, caption 27
- Ashes of War / gems: name 35, info 36, caption 37

DLC1 and DLC2 category fallbacks are included as well.

See THIRD_PARTY_NOTICES.md for the reverse-engineering references used to make the alpha.
