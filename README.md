# EldenRingLorePickup

Displays item lore, inventory icons and useful gameplay details in a modern side panel.

## v0.10.0: recipes, character guidance and a collection journal

[Download v0.10.0](https://github.com/henryday22/EldenRingLorePickup/releases/tag/v0.10.0).

The confirmed v0.8.2 pickup filter, Y dismissal, stacked deck and v0.9.0 centred styling are retained.
Every qualifying repeat pickup still shows its card. Collection history never suppresses a popup.

- **Ingredients:** a “Why keep it” paragraph connects an actual recipe to its effect and practical
  use. Every resolved recipe includes output quantity, all material names/quantities, the output's
  effect and a recipe-unlock reminder. Pots/perfumes also identify their reusable container through
  the live pot-group data. Container cards list the recipes that use their capacity.
- **Crafted consumables:** also show how to make another, with the same requirements. The mod does
  not claim you own the other ingredients or have unlocked the cookbook. Exact cookbook names and
  unlock status are not resolved in this version; the Item Crafting menu remains authoritative.
- **Weapons:** retain affinity, requirements, reinforcement-adjusted base attack, leading scaling,
  skill, base status buildup, weight and class/selected-weapon practical notes. Coating compatibility
  is read from the current weapon row. “Your build” checks your character's base attributes and
  explains deficits or two-handed eligibility. This excludes equipment bonuses and temporary buffs;
  base attack is not final attack rating or damage dealt to an enemy.
- **Spells:** requirements, initial FP cost, memory slots, casting-tool guidance and a personal
  requirement check. Sustained/follow-up casting can cost more. Casting-tool melee damage is never
  presented as spell damage. Spirit ashes show their FP/HP summon cost where provided.
- **Other item types:** armour focuses on weight/roll implications; talismans and consumables connect
  their effect to a practical use; cookbooks, bell bearings, upgrade materials, tears, remembrances,
  Great Runes and Ashes of War explain how they are used. Whetblades explain affinity/scaling choices.
- **Collection:** a fixed footer shows **X/Y collected** and marks new cards. Each character has an
  independent collection. Repeat pickups update its saved note without adding a duplicate; weapon
  upgrades/affinities and spirit upgrades share a card. Armour alterations and talisman tiers remain distinct.
- **Journal:** `Open Lore Journal.cmd` opens your local, searchable collection in a browser. Select a
  character, filter by item type, search names/effects/materials, and reread original lore and field notes.
  Reload the page to see discoveries made while it was open. Saved build checks are pickup-time snapshots.

Recipe facts, item effects, names and numerical values come from the installed game's parameters
and messages. Practical prose uses authored rules, not an online AI service or invented story text.
English-name/effect-specific advice may not match other languages; numerical data and original game
text still follow the installation. Unresolved data is labelled rather than filled in with guessed facts.

## Collection profiles and storage

The default `profile=auto` in `LorePickup.ini` beside the DLL reads the supported 1.17-era character's
save slot, saved character ID and name. Attribute/level sanity checks must pass. No game save is modified.
If the runtime or character cannot be identified, cards still work and the footer asks you to choose a
profile; collection data is not silently mixed with another character's records.

For unsupported runtimes, renamed/recreated characters or copied saves, edit the generated INI to a
unique profile, for example `profile=Strength Journey`, then restart the game. Use a different value
for each character; reuse the same value to continue that collection. An explicit profile names a
separate collection; it does not migrate automatic-profile records.

Progress begins with item panels observed after installing this version; older inventory is not
backfilled. **Y is the bundled supported catalogue whose names resolve in your game, plus recorded
additional items**, not a claim that every entry is obtainable in your current playthrough. DLC
names can be present without access to the DLC. It is not a game achievement or vanilla completion total.

Data lives in `%LOCALAPPDATA%\EldenRingLorePickup\collection.json`; the browser journal is `index.html`.
The previous JSON is retained as `collection.backup.json`. Writes use a background worker, a flushed
temporary file and atomic replacement. An unreadable or unsupported existing file is preserved and
collection writes are disabled for that session; check the log rather than deleting your history.
Keep a copy of the JSON if moving to another PC. No cloud account or network service is needed.

## Presentation

Cards centre vertically using their measured height and visible deck. The background is translucent
warm charcoal with gold-grey rules and opaque ivory text/icons. No parchment or particle animation.
An installed Agmena face is preferred, then Garamond/Georgia; no proprietary game font is bundled.
The appearance approximates the game's popup, rather than using its actual UI renderer.
Long text scrolls with **LB/RB**, while title, collection count and Y prompt remain fixed. These
buttons are observed, not intercepted, so the game may also receive them.

## Pickup eligibility

The previous global HUD flag is not used. Per-item metadata at +0x0d or +0x0e must equal 1.
This interpretation was inferred from supplied game traces, then confirmed working by the user
in v0.8.2. Examples covered by regression tests include existing Kukri, NEW Exile Gauntlets,
Stalwart Horn Charm and the three Cracked Tears. Ordinary unflagged pickups remain excluded.

## Installation

Close Elden Ring, replace `EldenRingLorePickup.dll`, and relaunch. Keep the existing icon folder
and `.me3` entry. Put `Open Lore Journal.cmd` beside the DLL (or anywhere convenient).
`LorePickup.ini` is created on first launch and preserved on subsequent starts. The package layout is:

    profiles/
      eldenring-default.me3
      natives/
        EldenRingLorePickup/
          EldenRingLorePickup.dll
          EldenRingLorePickup-icons/
          Open Lore Journal.cmd
          LorePickup.ini  (created on first launch)

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

The v0.8.2 trigger has user confirmation. The new live recipe, character and collection features
still require in-game verification; automated tests do not substitute for it. Automated tests use the exact metadata
values from the supplied logs and exercise the three-Tear queue, repeat items, ordinary pickups,
long reading times and held Y. The Windows DLL is built in GitHub Actions. Actual GDI renderer previews cover 864p, 1080p,
1440p, short and long cards, recipe lists, collection footers and scrolled content. Collection tests cover
profile isolation, repeat discoveries, serialization/restart, atomic file replacement and unsafe text escaping. Layout checks cover up to 2160p.

The previous HUD gate is removed entirely. This release observes controller Y rather than the
actual item-panel close event. Keyboard/remapped confirm input, panels closed by death/loading,
and unusual input timing are not yet synchronized. It does not claim end-to-end validation or
support for every unobserved presentation path. The observed metadata interpretation is grounded
in the user's 2.7.1.0 / 1.17-era sessions; older runtime patterns remain compatibility candidates.

The diagnostic log is `EldenRingLorePickup.log` in the game's working directory. A v0.10.0
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
