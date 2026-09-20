# Third-party notices

This alpha relies on public reverse-engineering work from the Elden Ring modding community.

## The Grand Archives Elden Ring table

Repository: https://github.com/The-Grand-Archives/Elden-Ring-CT-TGA

The public `ItemPopup_code.cea` script was used as the reverse-engineering reference for the item
presentation function signature, its `0x14` entry offset and the entry's item ID, quantity and gem
fields. No Cheat Engine script or binary code is included in LorePickup.

## from-software-archipelago-clients

Repository: https://github.com/4laric/from-software-archipelago-clients

The current Elden Ring Archipelago client was used as a reference for the modern AddItem function signature/RVAs, the live MsgRepository/SearchStringTable RVAs, and the 1.17-era executable split. That project is MIT licensed.

MIT License

Copyright (c) its contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE.

## fromsoftware-rs

Repository: https://github.com/vswarte/fromsoftware-rs

The public Elden Ring frontend-manager definitions were used as the reference for
`CSFeManImp::hud_state` at offset `0x78` and the `CSFeManHudState` values: `PopupMenu` (`2`) and
`Default` (`3`). No source or binary code from that project is included in LorePickup.

## FMG category mapping

FMG IDs were cross-checked against established Elden Ring tooling including Soulstruct / Smithbox / Erd-Tools mappings. LorePickup reads the game's own runtime strings; no game text is redistributed in this repository.

## Smithbox / Erd-Tools parameter references

Repositories: https://github.com/vawser/Smithbox and
https://github.com/Nordgaren/Elden-Ring-Debug-Tool

Their public parameter definitions and runtime pointer patterns were used to identify the five
`iconId` fields read by LorePickup. Erd-Tools' public CSFeMan signature was also used to resolve the
frontend-manager singleton without relying solely on a fixed RVA. Smithbox is MIT licensed.
Erd-Tools is GPL-3.0 licensed and was used only as a reverse-engineering reference; no Erd-Tools
source or binary code is included.

The source repository contains only numeric item-to-icon fallback mappings. Game artwork is not
committed to this source tree.

## Version-specific frontend addresses

The numerical 1.16.2-to-1.17 mappings were cross-checked against Banon-Labs/er-mods-rs,
commit d0f41fd44020c77f2c2809800e3a965e0353f656:

- `docs/recon/rva-map-1162-to-1170.data.tsv`: CSFEMAN_SINGLETON_RVA 0x3D6B880 -> 0x3D6F8F0 (125/125 references).
- `docs/recon/npc-possess-1170-address-table.md`: CSFeManImp vtable 0x2A9D988 -> 0x2AA0A08.

Repository: https://github.com/Banon-Labs/er-mods-rs

Only numerical addresses were used; no implementation code from that project was copied.

## v0.8.2 presentation metadata

The HUD-state probe and its version-specific address reads were removed after the user's live
v0.8.1 log showed Hud(3) throughout item panels. The two flag-byte interpretations in
`src/presentation.rs` are inferred from user-supplied logs and screenshots (Kukri, Exile Gauntlets,
Stalwart Horn Charm and the three Cracked Tears). They are not attributed to the older
Grand Archives ItemPopup script, which labelled the metadata word differently.


## v0.9.0 item data and type references

Added weapon field offsets and reinforcement/status-effect layouts were checked against
[Paramdex ER definitions](https://github.com/soulsmods/Paramdex/tree/master/ER/Defs) and
[the fromsoftware-rs Elden Ring structs](https://github.com/vswarte/fromsoftware-rs).
Weapon-type numeric mappings were checked against
[Smithbox WEP_TYPE](https://github.com/vawser/Smithbox/blob/main/src/Smithbox.Data/Assets/PARAM/ER/Param%20Enums/WEP_TYPE.json).
Practical notes are original category-level guidance, not copied wiki articles or invented lore.
Original lore is read from the user's game. No proprietary font or new background image is bundled.

Selected weapon-note checks: [Claymore](https://eldenring.wiki.fextralife.com/Claymore),
[Bloodhound's Fang buff discussion](https://steamcommunity.com/app/1245620/discussions/0/3820780544828221988/),
[Clawmark Seal scaling discussion](https://www.reddit.com/r/Eldenring/comments/t2n8l8/about_the_clawmark_seal/).
Notes paraphrase mechanics rather than reproducing source descriptions.
