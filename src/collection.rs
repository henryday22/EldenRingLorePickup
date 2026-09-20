//! Durable collection model: unique discoveries, explicit profiles, and a defined catalogue.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub key: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub details: Vec<String>,
    pub icon_id: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub cards: BTreeMap<String, Card>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    pub schema: u32,
    pub catalogue: BTreeSet<String>,
    #[serde(default)]
    pub catalogue_ready: bool,
    pub profiles: BTreeMap<String, Profile>,
}

impl Default for Collection {
    fn default() -> Self {
        Self {
            schema: 1,
            catalogue: BTreeSet::new(),
            catalogue_ready: false,
            profiles: BTreeMap::new(),
        }
    }
}

impl Collection {
    pub fn record(
        &mut self,
        profile: &str,
        display_name: &str,
        card: Card,
    ) -> (usize, usize, bool) {
        self.catalogue.insert(card.key.clone());
        let p = self.profiles.entry(profile.into()).or_default();
        p.name = display_name.into();
        let fresh = p.cards.insert(card.key.clone(), card).is_none();
        (
            p.cards
                .keys()
                .filter(|key| self.catalogue.contains(*key))
                .count(),
            self.catalogue.len(),
            fresh,
        )
    }
}

pub fn category_name(category: u32) -> &'static str {
    match category {
        0 => "Weapons",
        0x10000000 => "Armour",
        0x20000000 => "Talismans",
        0x40000000 => "Items",
        0x80000000 => "Ashes of War",
        _ => "Other",
    }
}

/// A weapon's affinities/upgrades share a card; spirit reinforcement does too.
/// Altered armour and different talisman tiers keep their distinct item IDs.
pub fn canonical_id(category: u32, id: u32, goods_type: Option<u8>) -> u32 {
    if category == 0 && id >= 100000 {
        id / 10000 * 10000
    } else if category == 0x40000000 && matches!(goods_type, Some(7 | 8)) {
        id / 100 * 100
    } else {
        id
    }
}

pub fn key(category: u32, id: u32, goods_type: Option<u8>) -> String {
    format!(
        "{category:08x}:{:08x}",
        canonical_id(category, id, goods_type)
    )
}

pub fn html(db: &Collection) -> String {
    // Escape script-closing characters before embedding JSON. Item text is always inserted
    // with textContent; neither names nor modded descriptions are trusted HTML.
    let json = serde_json::to_string(db)
        .unwrap_or_else(|_| "{}".into())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    include_str!("../assets/journal.html").replace("/*COLLECTION_DATA*/{}", &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn card(id: u32) -> Card {
        Card {
            key: key(0, id, None),
            name: "Weapon".into(),
            category: "Weapons".into(),
            description: "Lore".into(),
            details: vec![],
            icon_id: None,
        }
    }
    #[test]
    fn duplicate_affinities_and_restarts_do_not_inflate_progress() {
        let mut db = Collection::default();
        assert_eq!(db.record("a", "A", card(1000000)), (1, 1, true));
        assert_eq!(db.record("a", "A", card(1000108)), (1, 1, false));
        let mut db: Collection =
            serde_json::from_str(&serde_json::to_string(&db).unwrap()).unwrap();
        assert_eq!(db.record("a", "A", card(1000000)), (1, 1, false));
        assert_eq!(db.record("b", "B", card(1000000)), (1, 1, true));
        assert_eq!(db.profiles.len(), 2);
    }
    #[test]
    fn armour_and_talisman_variants_stay_distinct() {
        assert_ne!(key(0x10000000, 10000, None), key(0x10000000, 10100, None));
        assert_ne!(key(0x20000000, 1000, None), key(0x20000000, 1001, None));
        assert_eq!(
            key(0x40000000, 200001, Some(7)),
            key(0x40000000, 200010, Some(7))
        );
    }
    #[test]
    fn export_journal_preview() {
        let Ok(directory)=std::env::var("LOREPICKUP_PREVIEW_DIR") else {return;};
        std::fs::create_dir_all(&directory).unwrap();
        let mut db=Collection::default();db.catalogue_ready=true;
        for i in 0..2500 {db.catalogue.insert(format!("catalogue-{i}"));}
        for (profile,name,item,category,description,details) in [
            ("strength","Strength journey","Heavy Halberd +8","Weapons","Layout fixture: a weapon with long reach and deliberate attacks.",vec!["REQUIRES  STR 14 · DEX 12","BASE ATTACK  Physical 190 (illustrative value)","YOUR BUILD  Your base attributes meet the requirements. Equipment bonuses and temporary effects are not included in this check.","IN PRACTICE  Reach is its advantage: punish an approaching enemy before they get close. Check both light and heavy attacks, since halberds differ in their mix of thrusts and sweeps."]),
            ("strength","Strength journey","Herb — recipe sample","Items","Layout fixture for an ingredient with several uses.",vec!["WHY KEEP IT  Keep Herb to craft Test Cure: alleviates poison buildup. Keep a supply for areas or enemies that inflict this condition; a cure can save a flask or a retreat. You will also need Moss ×1.","RECIPE 1  Test Cure ×1 — Alleviates poison buildup. Needs Herb ×2, Moss ×1. Unlock this recipe in Item Crafting first."]),
            ("faith","Faith journey","Example incantation","Items","A separate character's discovery.",vec!["REQUIRES  FAI 24","MEMORY  1 slot(s)"]),
        ] {db.record(profile,name,Card{key:item.into(),name:item.into(),category:category.into(),description:description.into(),details:details.into_iter().map(str::to_string).collect(),icon_id:None});}
        std::fs::write(std::path::Path::new(&directory).join("journal.html"),html(&db)).unwrap();
    }

    #[test]
    fn hostile_item_text_cannot_escape_the_journal_data_block() {
        let mut db = Collection::default();
        let mut c = card(10000);
        c.name = "</script><script>alert(1)</script>".into();
        db.record("a", "A", c);
        let page = html(&db);
        assert!(!page.contains("</script><script>alert"));
        assert!(page.contains("\\u003c/script"));
    }
}
