# EldenRingLorePickup

A small Elden Ring quality-of-life mod: when you acquire an item for the first time in a session, its actual in-game lore description appears in a non-blocking panel on the right side of the screen.

The aim is simple: Elden Ring hides a huge amount of its story in item descriptions, but the normal pickup flow makes those descriptions easy to miss. LorePickup puts the description in front of you at the moment it is contextually useful, without opening the inventory or pausing play.

## Current status

**Early alpha. Runtime testing is still required.**

The first implementation is now in this repository. It:

- builds as a native Windows DLL for ModEngine3;
- hooks Elden Ring's item-add path rather than polling the inventory;
- reads item names and descriptions from Elden Ring's live message repository, so it uses the game's current language;
- supports weapons, armour, talismans, goods and Ashes of War;
- searches base-game, DLC1 and DLC2 message tables;
- de-duplicates items during a session so repeat consumable pickups do not spam the panel;
- queues rapid multi-item pickups instead of overwriting them;
- uses a non-interactive DirectX 12 / ImGui panel on the right side of the screen;
- fails closed when the expected Elden Ring function signatures do not match.

The runtime addresses currently cover the 2.6.2.0, 2.7.0.0 and 2.7.1.0 executable families used by the current 1.17-era modding stack. Signature checks are mandatory before either hook or message lookup is used.

## Building

Requirements:

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

Then add this block to the profile:

    [[natives]]
    path = 'natives/EldenRingLorePickup.dll'

ME3 resolves native paths relative to the .me3 profile.

## Behaviour

The default panel:

- appears near the top-right;
- shows the item type, name, short info text (when available), and the full lore caption;
- stays visible for 12 seconds;
- then advances to the next queued pickup;
- ignores an exact item after it has already been shown in the current game session.

The next iteration will add a small configuration file for duration, width, font scale, screen position, and de-duplication behaviour.

## Important compatibility note

The overlay is currently implemented with hudhook's DirectX 12 backend. Other software that also hooks DX12 Present can conflict with overlay hooks. In particular, this needs to be tested alongside ERSS / OptiScaler before calling the alpha safe for the user's normal mod stack. The item-pickup hook and lore lookup are independent of the overlay renderer, so if a conflict appears we can swap the presentation layer without throwing away the hard part.

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
