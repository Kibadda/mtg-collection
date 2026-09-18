use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const COLLECTION_FILE: &str = "collection.json";

pub const CONDITION_OPTIONS: [&str; 5] = ["NM", "LP", "MP", "HP", "D"];
pub const DEFAULT_CONDITION: &str = "NM";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Card {
    pub name: String,
    /// Scryfall set code, e.g. "3ed". Together with `collector_number` this
    /// uniquely identifies a printing.
    pub set: String,
    /// Collector number within the set.
    #[serde(default)]
    pub collector_number: String,
    #[serde(default)]
    pub quantity: u32,
    #[serde(default = "default_finish")]
    pub finish: String,
    #[serde(default = "default_condition")]
    pub condition: String,
    #[serde(default = "default_lang")]
    pub lang: String,
}

impl Card {
    /// Two cards are the same physical copy to merge: the base identity
    /// (set + collector number) plus the user-chosen finish, condition and
    /// language.
    pub fn same_printing(&self, other: &Card) -> bool {
        self.set == other.set
            && self.collector_number == other.collector_number
            && self.finish == other.finish
            && self.condition == other.condition
            && self.lang == other.lang
    }
}

fn default_finish() -> String {
    "nonfoil".to_string()
}

fn default_condition() -> String {
    DEFAULT_CONDITION.to_string()
}

fn default_lang() -> String {
    "en".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Collection {
    pub cards: Vec<Card>,
}

impl Collection {
    pub fn load(path: &Path) -> Self {
        if path.exists() {
            let data = fs::read_to_string(path).expect("Failed to read collection file");
            match serde_json::from_str(&data) {
                Ok(c) => c,
                Err(_) => Collection { cards: vec![] },
            }
        } else {
            Collection { cards: vec![] }
        }
    }

    pub fn save(&self, path: &Path) {
        let data = serde_json::to_string_pretty(self).expect("Failed to serialize collection");
        fs::write(path, data).expect("Failed to write collection file");
    }

    /// Find cards in the collection whose name contains the query (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<&Card> {
        let q = query.to_lowercase();
        self.cards
            .iter()
            .filter(|c| c.name.to_lowercase().contains(&q))
            .collect()
    }

    pub fn add_card(&mut self, card: Card) {
        if let Some(existing) = self.cards.iter_mut().find(|c| c.same_printing(&card)) {
            existing.quantity += card.quantity;
        } else {
            self.cards.push(card);
        }
    }

    pub fn remove_one(&mut self, card: &Card) -> bool {
        if let Some(pos) = self.cards.iter().position(|c| c.same_printing(card)) {
            if self.cards[pos].quantity > 1 {
                self.cards[pos].quantity -= 1;
            } else {
                self.cards.remove(pos);
            }
            true
        } else {
            false
        }
    }
}

pub fn collection_path() -> PathBuf {
    let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(".mtg-collection");
    fs::create_dir_all(&path).ok();
    path.push(COLLECTION_FILE);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(set: &str, number: &str, finish: &str, condition: &str, quantity: u32) -> Card {
        Card {
            name: "Test Card".to_string(),
            set: set.to_string(),
            collector_number: number.to_string(),
            quantity,
            finish: finish.to_string(),
            condition: condition.to_string(),
            lang: "en".to_string(),
        }
    }

    #[test]
    fn add_merges_same_identity() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("3ed", "44", "nonfoil", "NM", 2));
        collection.add_card(card("3ed", "44", "nonfoil", "NM", 3));

        assert_eq!(collection.cards.len(), 1);
        assert_eq!(collection.cards[0].quantity, 5);
    }

    #[test]
    fn add_separates_different_printings() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("3ed", "44", "nonfoil", "NM", 2));
        collection.add_card(card("3ed", "45", "nonfoil", "NM", 1));
        collection.add_card(card("4ed", "44", "nonfoil", "NM", 1));

        assert_eq!(collection.cards.len(), 3);
    }

    #[test]
    fn add_separates_different_finish() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("3ed", "44", "nonfoil", "NM", 2));
        collection.add_card(card("3ed", "44", "foil", "NM", 1));

        assert_eq!(collection.cards.len(), 2);
        assert_eq!(collection.cards[0].quantity, 2);
        assert_eq!(collection.cards[1].quantity, 1);
    }

    #[test]
    fn add_separates_different_condition() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("3ed", "44", "nonfoil", "NM", 1));
        collection.add_card(card("3ed", "44", "nonfoil", "LP", 1));

        assert_eq!(collection.cards.len(), 2);
    }

    #[test]
    fn add_separates_different_language() {
        let mut collection = Collection { cards: vec![] };
        let mut en = card("3ed", "44", "nonfoil", "NM", 1);
        let mut de = en.clone();
        de.lang = "de".to_string();
        en.quantity = 2;

        collection.add_card(en);
        collection.add_card(de);

        assert_eq!(collection.cards.len(), 2);
        assert_eq!(collection.cards[0].lang, "en");
        assert_eq!(collection.cards[0].quantity, 2);
        assert_eq!(collection.cards[1].lang, "de");
        assert_eq!(collection.cards[1].quantity, 1);
    }

    #[test]
    fn remove_decrements_then_removes() {
        let mut collection = Collection { cards: vec![] };
        let c = card("3ed", "44", "nonfoil", "NM", 2);
        collection.add_card(c.clone());

        assert!(collection.remove_one(&c));
        assert_eq!(collection.cards[0].quantity, 1);
        assert!(collection.remove_one(&c));
        assert!(collection.cards.is_empty());
        assert!(!collection.remove_one(&c));
    }

    #[test]
    fn search_is_case_insensitive_substring() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("3ed", "44", "nonfoil", "NM", 1));
        collection.add_card(card("3ed", "45", "foil", "NM", 1));

        assert_eq!(collection.search("test card").len(), 2);
        assert_eq!(collection.search("TEST").len(), 2);
        assert_eq!(collection.search("zzz").len(), 0);
    }

    #[test]
    fn old_format_still_loads_with_defaults() {
        let json = r#"{"id":"a","name":"X","set":"3ed","set_name":"Set","rarity":"rare","type_line":"Instant","mana_cost":"{R}","oracle_text":"T","prices":{"usd":null,"usd_foil":null,"eur":null,"eur_foil":null},"quantity":2}"#;
        let card: Card = serde_json::from_str(json).unwrap();
        assert_eq!(card.lang, "en");
        assert_eq!(card.condition, DEFAULT_CONDITION);
        assert_eq!(card.finish, "nonfoil");
        assert!(card.collector_number.is_empty());
        assert_eq!(card.quantity, 2);
    }

    #[test]
    fn serialization_roundtrip() {
        let c = card("3ed", "44", "foil", "LP", 4);
        let json = serde_json::to_string(&c).unwrap();
        let back: Card = serde_json::from_str(&json).unwrap();
        assert!(c.same_printing(&back));
        assert_eq!(back.quantity, 4);
    }
}
