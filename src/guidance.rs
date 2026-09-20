//! Original practical notes, kept separate from the game's lore. No invented story text.

pub fn two_hand_requirement(strength: u8) -> u8 {
    (u16::from(strength) * 2).div_ceil(3) as u8
}

pub fn named_weapon_note(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase().replace('’', "'");
    if name.contains("bloodhound's fang") {
        Some("An unusual Somber weapon that still accepts weapon buffs. Its innate bleed can work alongside a compatible coating; the fixed skill trades space for a follow-up attack.")
    } else if name.contains("clawmark seal") {
        Some("Useful for a Strength/Faith caster: both attributes contribute to offensive incantation scaling, and the seal boosts bestial incantations. Its melee attack value is not a measure of how hard Lightning Spear will hit.")
    } else if name.contains("claymore") {
        Some("Its thrusting heavy attack is a useful distinction from many other greatswords: it gives you a focused poke for punishing an opening, alongside the broader swings of its light attacks.")
    } else {
        None
    }
}

// WEP_TYPE values from Smithbox's ER enum; these are class tendencies, not bespoke movesets.
pub fn weapon_class(kind: u16) -> (&'static str, &'static str) {
    match kind {
        1 => ("Dagger", "Fast, short-reach attacks reward staying close. A useful backup for critical openings; avoid trading hits with heavier weapons."),
        3 => ("Straight sword", "A flexible melee weapon: quick recovery makes it easier to attack once and dodge. Try its heavy attacks and skill before committing upgrades."),
        5 | 11 => (if kind == 5 { "Greatsword" } else { "Curved greatsword" }, "Balances reach and impact with a slower recovery than a light sword. Jumping and charged attacks help pressure enemy stance; leave stamina for the retreat."),
        7 | 41 => (if kind == 7 { "Colossal sword" } else { "Colossal weapon" }, "Heavy commitment, high impact. Look for a clear opening for a jumping or charged heavy attack rather than chasing long combos."),
        9 => ("Curved sword", "Quick slashes favour short openings and sustained close-range pressure. Test its reach before attempting another hit against a recovering enemy."),
        13 | 94 => (if kind == 13 { "Katana" } else { "Great katana" }, "A cutting weapon with useful reach. Check the individual weapon's passive effects: the class alone does not guarantee bleed or a particular skill."),
        14 => ("Twinblade", "Two-handing opens its flowing multi-hit moveset. Those longer strings need a safe opening; use shorter attacks when an enemy can retaliate."),
        15 | 16 => (if kind == 15 { "Thrusting sword" } else { "Heavy thrusting sword" }, "Precise thrusts favour spacing and a single target. Piercing attacks can gain counter damage when they catch an enemy during an attack."),
        17 | 19 => (if kind == 17 { "Axe" } else { "Greataxe" }, "Deliberate swings reward close-range openings. Charged heavy attacks offer more pressure when you have time; avoid spending all your stamina on a combo."),
        21 | 23 | 24 => (match kind { 21 => "Hammer", 23 => "Great hammer", _ => "Flail" }, "Blunt weapons are worth trying against enemies that shrug off cutting attacks. Charged heavies and guard counters can help break an enemy's stance."),
        25 | 28 => (if kind == 25 { "Spear" } else { "Great spear" }, "Long thrusting reach helps keep enemies at a distance. Watch your spacing in crowds: a narrow attack may miss enemies beside you."),
        29 => ("Halberd", "Reach is its advantage: punish an approaching enemy before they get close. Check both light and heavy attacks, since halberds differ in their mix of thrusts and sweeps."),
        31 => ("Reaper", "Sweeping attacks cover space around you. Work at the blade's reach and leave time to recover rather than trading blows at point-blank range."),
        35 | 37 | 88 | 95 => (match kind {35 => "Fist",37 => "Claw",88 => "Hand-to-hand",_ => "Beast claw"}, "Very short reach rewards getting inside an enemy's attacks. Quick combinations work best after a clear opening; test the two-handed moveset too."),
        39 => ("Whip", "Long-reaching lashes can pressure enemies from outside sword range. Keep another weapon available for critical attacks."),
        50 | 51 | 53 => (match kind {50 => "Light bow",51 => "Bow",_ => "Greatbow"}, "A ranged option for pulling one enemy out of a group. Ammunition contributes to damage and effects; check that you have the correct arrow type."),
        55 | 56 => (if kind == 55 { "Crossbow" } else { "Ballista" }, "Ranged fire without needing spell slots. Account for the reload window, and choose compatible ammunition for the enemy you are fighting."),
        57 | 61 => (if kind == 57 { "Staff" } else { "Sacred seal" }, "A casting tool: its melee attack power is not spell damage. Compare sorcery or incantation scaling in Equipment; upgrades and the spell's own rules determine the result."),
        65 | 67 | 69 | 90 => (match kind {65 => "Small shield",67 => "Medium shield",69 => "Greatshield",_ => "Thrusting shield"}, "Blocking spends stamina. Keep enough in reserve to avoid a guard break; use a heavy attack after a successful block for a guard counter."),
        81 | 83 | 85 | 86 => ("Ammunition", "Pair with a compatible bow, crossbow or ballista. The projectile contributes its own damage and any listed effects to the shot."),
        87 => ("Torch", "Provides light when held. Check the description for any special effect; it can be useful to keep one available without making it your main weapon."),
        89 => ("Perfume bottle", "A reusable weapon with a close-range spray. Test its attack coverage and recovery before relying on it against a fast enemy."),
        91 => ("Throwing blade", "A weapon built around repeatable thrown attacks. Use the distance to create safer openings and check its heavy attack and skill for different trajectories."),
        92 | 93 => (if kind == 92 { "Backhand blade" } else { "Light greatsword" }, "A flowing melee moveset rewards controlled combinations. Try its light-to-heavy transitions and skill to learn which openings it can safely exploit."),
        _ => ("Weapon", "Check the requirements before equipping it, then try its light attack, heavy attack and skill. Upgrades often make a larger early difference than a few extra damage-stat levels."),
    }
}

pub fn goods_note(kind: u8, name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    if name.contains("grease") {
        return Some("A temporary weapon coating, not a permanent affinity change. Apply it before a fight to a weapon that accepts buffs; many special or elemental weapons cannot be coated.");
    }
    if name.contains("cookbook") {
        return Some("Unlocks recipes in Item Crafting. Learning the recipes does not consume the book; you still need the listed ingredients and any required crafting container.");
    }
    if name.contains("bell bearing") {
        return Some("Offer this to the Twin Maiden Husks at Roundtable Hold to add its associated stock to their shop.");
    }
    if name.contains("smithing stone") {
        return Some("Used by a smith to improve armaments. Regular and Somber Smithing Stones serve different upgrade paths; the number on the stone is its tier, not the weapon's upgrade level.");
    }
    if name.contains("glovewort") {
        return Some("Take this to Roderika for spirit tuning once her service is unlocked. Grave Glovewort upgrades ordinary spirits; Ghost Glovewort upgrades renowned spirits such as the Mimic Tear.");
    }
    match kind {
        2 | 11 => Some("A crafting ingredient. The recipe list shows what uses it; you may still need a cookbook to unlock a recipe. Gathering a material does not by itself unlock everything it can make."),
        3 => Some("A boss remembrance can be exchanged with Enia at Roundtable Hold for a reward. Consuming it for runes spends it; inspect the reward choices first."),
        5 => Some("Memorise this at a Site of Grace and cast it with a staff. Meeting the spell's requirements lets you use it; damage also depends on the staff's sorcery scaling and upgrades."),
        7 | 8 => Some("Equip this spirit ash and summon where the rebirth-monument symbol appears. Roderika can strengthen it; summons are limited by the area's rules and your available FP or HP."),
        9 | 10 => Some("Mix Crystal Tears into the Flask of Wondrous Physick at a Site of Grace. Two tear effects can share one flask; the tears are reusable and the flask refills when you rest."),
        15 => Some("Check this rune's description for activation requirements. Equippable Great Runes are selected at a Site of Grace; a Rune Arc activates the equipped rune's benefit until death."),
        16 => Some("Memorise this at a Site of Grace and cast it with a sacred seal. For offensive incantations, seal upgrades and incantation scaling matter as well as meeting the spell's requirements."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_hand_requirement_obeys_rounding() {
        for requirement in 1..=99 {
            let minimum = two_hand_requirement(requirement);
            assert!(minimum as u16 * 3 / 2 >= requirement as u16);
            assert!((minimum as u16 - 1) * 3 / 2 < requirement as u16);
        }
    }

    #[test]
    fn specific_use_notes_take_priority_over_generic_category() {
        assert!(goods_note(0, "Magic Grease").unwrap().contains("not a permanent"));
        assert!(goods_note(1, "Nomadic Warrior's Cookbook [1]").unwrap().contains("recipes"));
        assert!(goods_note(2, "Rowa Fruit").unwrap().contains("ingredient"));
        assert!(goods_note(255, "Unknown").is_none());
        assert!(named_weapon_note("Heavy Claymore +8").unwrap().contains("thrusting"));
        assert!(named_weapon_note("Bloodhound’s Fang +5").unwrap().contains("buffs"));
    }
}
