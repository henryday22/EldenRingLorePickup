//! Data-led explanations. Recipe facts come from the installed game's parameters/messages.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ingredient {
    pub category: u32,
    pub id: u32,
    pub name: String,
    pub quantity: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    pub effect: String,
    pub output_category: u32,
    pub output_id: u32,
    pub output_quantity: u16,
    pub ingredients: Vec<Ingredient>,
    pub containers: Vec<String>,
    pub unlock_required: bool,
    pub complete: bool,
}

impl Recipe {
    pub fn ingredients_text(&self) -> String {
        self.ingredients
            .iter()
            .map(|m| {
                if m.quantity == 0 {
                    format!("{} (required)", m.name)
                } else {
                    format!("{} ×{}", m.name, m.quantity)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn explanation(&self) -> String {
        let effect = if self.effect.trim().is_empty() {
            "Its effect description is unavailable in this game's messages".to_string()
        } else {
            self.effect.trim().trim_end_matches('.').to_string()
        };
        format!(
            "{} ×{} — {}. Needs {}. {}{}{}",
            self.name,
            self.output_quantity,
            effect,
            self.ingredients_text(),
            if self.containers.is_empty() {
                String::new()
            } else {
                format!(
                    "Also needs an available {} (reusable container). ",
                    self.containers.join(" / ")
                )
            },
            if self.unlock_required {
                "Unlock this recipe in Item Crafting first; finding a material alone does not unlock it. "
            } else {
                "Requires access to Item Crafting. "
            },
            if self.complete {
                ""
            } else {
                "Some requirements could not be resolved; check the crafting menu."
            }
        )
    }
}

/// Explain uses for different play styles without claiming to know a character's inventory.
pub fn use_context(name: &str, effect: &str) -> &'static str {
    let text = format!("{name} {effect}").to_ascii_lowercase();
    if text.contains("raises faith")
        || text.contains("raises intelligence")
        || text.contains("raises strength")
        || text.contains("raises dexterity")
        || text.contains("raises arcane")
    {
        "Can help meet an equipment or spell requirement and support scaling that uses this attribute. Removing the item may leave that equipment unusable, so recheck requirements when swapping it."
    } else if text.contains("skill") && (text.contains("enhance") || text.contains("boost")) {
        "Worth considering when your weapon skill is a major source of damage; it gives less value if you mostly use ordinary attacks."
    } else if text.contains("incantation") || text.contains("sorcer") {
        "Consider this when its listed effect matches the spells you actually cast. Requirements unlock casting, while the casting tool and upgrades also shape offensive spell damage."
    } else if text.contains("maximum hp") || text.contains("max hp") {
        "Extra health gives more room for mistakes, especially when learning an unfamiliar enemy. Check whether the listed benefit is conditional before relying on it."
    } else if text.contains("equip load") {
        "Useful if heavier equipment would push you into a slower roll. Check total equip load after changing armour or adding another weapon."
    } else if text.contains("bolus") || text.contains("boluses") || text.contains("alleviates") {
        "Keep a supply for areas or enemies that inflict this condition; a cure can save a flask or a retreat."
    } else if text.contains("grease") || text.contains("coats armament") {
        "Useful for a melee build that wants this effect temporarily without permanently changing a weapon's affinity. Check that the weapon accepts coatings."
    } else if text.contains("arrow") || text.contains("bolt") {
        "Useful for ranged play or pulling one enemy away from a group. Bring the compatible bow, crossbow or ballista."
    } else if text.contains("pot") || text.contains("throw") {
        "Gives a build a ranged consumable option without spending a spell slot. Check the target's vulnerabilities before spending limited materials."
    } else if text.contains("stamina") {
        "Useful when repeated attacks, blocking or evasive movement leave you short of stamina."
    } else if text.contains("rune") && (text.contains("acquisition") || text.contains("obtained")) {
        "Save it for a stretch of fighting where you expect substantial rune rewards."
    } else if text.contains("item discovery") || text.contains("discovery") {
        "Useful when farming a particular enemy drop; use it when you can make several attempts during its effect."
    } else if text.contains("hp") || text.contains("heal") {
        "A recovery option worth keeping for sustained exploration; check its restrictions rather than assuming it replaces your flask."
    } else if text.contains("damage negation") || text.contains("resistance") {
        "Keep it for encounters where its particular protection matches the incoming threat."
    } else if text.contains("attack") || text.contains("damage") {
        "Keep it for fights where this effect suits your weapon or the enemy; its value depends on the situation, not your starting class."
    } else {
        "Keep it when the listed effect supports the way you want to approach an encounter; it is an option rather than a permanent build commitment."
    }
}

pub fn ingredient_paragraph(name: &str, recipes: &[Recipe]) -> String {
    if recipes.is_empty() {
        return format!("Keep {name} if you want to explore crafting options. No complete recipe link could be resolved from the loaded data, so a specific use cannot be recommended yet.");
    }
    let recipe = recipes
        .iter()
        .find(|r| r.complete && !r.effect.is_empty())
        .unwrap_or(&recipes[0]);
    let companions = recipe
        .ingredients
        .iter()
        .filter(|m| !m.name.eq_ignore_ascii_case(name))
        .map(|m| {
            if m.quantity > 0 {
                format!("{} ×{}", m.name, m.quantity)
            } else {
                m.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let effect = recipe.effect.trim().trim_end_matches('.');
    format!(
        "Keep {name} to craft {}: {}. {} {}{}",
        recipe.name,
        if effect.is_empty() {
            "one of its crafting uses".to_string()
        } else {
            effect.to_string()
        },
        use_context(&recipe.name, &recipe.effect),
        if companions.is_empty() {
            "The recipe below lists the amount needed.".to_string()
        } else {
            format!("You will also need {companions}.")
        },
        if recipes.len() > 1 {
            " Other recipes below offer alternatives for different play styles."
        } else {
            ""
        }
    )
}

pub fn requirements_note(required: [u8; 5], attributes: [u32; 5], two_hand: bool) -> String {
    let names = ["STR", "DEX", "INT", "FAI", "ARC"];
    let deficits: Vec<String> = (0..5)
        .filter_map(|i| {
            let need = u32::from(required[i]);
            (need > attributes[i]).then(|| {
                format!(
                    "{} +{} ({} → {})",
                    names[i],
                    need - attributes[i],
                    attributes[i],
                    need
                )
            })
        })
        .collect();
    if deficits.is_empty() {
        return "Your base attributes meet the requirements. Equipment bonuses and temporary effects are not included in this check.".into();
    }
    let can_two_hand = two_hand
        && attributes[0] * 3 / 2 >= u32::from(required[0])
        && (1..5).all(|i| attributes[i] >= u32::from(required[i]));
    if can_two_hand {
        "Your base attributes meet the requirements when two-handed. One-handed use still needs more STR; temporary bonuses are not included.".into()
    } else {
        format!("With your base attributes, still needs {}. Equipment bonuses or temporary effects may change this.", deficits.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipe_explains_output_other_materials_effect_and_unlock() {
        let r = Recipe {
            name: "Test Cure".into(),
            effect: "Alleviates poison buildup".into(),
            output_category: 0x40000000,
            output_id: 1,
            output_quantity: 2,
            containers: vec![],
            ingredients: vec![
                Ingredient {
                    category: 4,
                    id: 1,
                    name: "Herb".into(),
                    quantity: 2,
                },
                Ingredient {
                    category: 4,
                    id: 2,
                    name: "Moss".into(),
                    quantity: 1,
                },
            ],
            unlock_required: true,
            complete: true,
        };
        let p = ingredient_paragraph("Herb", &[r.clone()]);
        assert!(p.contains("poison") && p.contains("Moss ×1"));
        let detail = r.explanation();
        assert!(
            detail.contains("Test Cure ×2")
                && detail.contains("Herb ×2")
                && detail.contains("Unlock")
        );
    }
    #[test]
    fn builds_are_evaluated_independently_without_assuming_two_hands_fixes_dex() {
        assert!(
            requirements_note([30, 12, 0, 0, 0], [20, 12, 10, 10, 10], true)
                .contains("when two-handed")
        );
        assert!(
            requirements_note([30, 12, 0, 0, 0], [20, 10, 10, 10, 10], true).contains("DEX +2")
        );
        assert!(requirements_note([0, 0, 12, 24, 0], [20, 12, 12, 24, 10], false).contains("meet"));
    }
    #[test]
    fn missing_recipe_does_not_invent_one() {
        assert!(ingredient_paragraph("Unknown", &[]).contains("could be resolved"));
    }
}
