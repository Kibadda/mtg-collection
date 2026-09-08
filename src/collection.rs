use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const COLLECTION_FILE: &str = "collection.json";

pub const CONDITION_OPTIONS: [&str; 5] = ["NM", "LP", "MP", "HP", "D"];
pub const DEFAULT_CONDITION: &str = "NM";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub set: String,
    pub set_name: String,
    pub rarity: String,
    pub type_line: String,
    pub mana_cost: Option<String>,
    pub oracle_text: Option<String>,
    pub prices: Prices,
    #[serde(default)]
    pub quantity: u32,
    #[serde(default = "default_finish")]
    pub finish: String,
    #[serde(default = "default_condition")]
    pub condition: String,
    #[serde(default = "default_lang")]
    pub lang: String,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Prices {
    pub usd: Option<String>,
    pub usd_foil: Option<String>,
    pub eur: Option<String>,
    pub eur_foil: Option<String>,
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
        if let Some(existing) = self
            .cards
            .iter_mut()
            .find(|c| c.id == card.id && c.finish == card.finish && c.condition == card.condition)
        {
            existing.quantity += card.quantity;
        } else {
            self.cards.push(card);
        }
    }

    pub fn remove_one(&mut self, card: &Card) -> bool {
        if let Some(pos) = self.cards.iter().position(|c| {
            c.id == card.id && c.finish == card.finish && c.condition == card.condition
        }) {
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

    fn card(id: &str, finish: &str, condition: &str, quantity: u32) -> Card {
        Card {
            id: id.to_string(),
            name: "Test Card".to_string(),
            set: "test".to_string(),
            set_name: "Test Set".to_string(),
            rarity: "common".to_string(),
            type_line: "Instant".to_string(),
            mana_cost: Some("{R}".to_string()),
            oracle_text: Some("Text".to_string()),
            prices: Prices {
                usd: None,
                usd_foil: None,
                eur: None,
                eur_foil: None,
            },
            quantity,
            finish: finish.to_string(),
            condition: condition.to_string(),
            lang: "en".to_string(),
        }
    }

    #[test]
    fn add_merges_same_identity() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("a", "nonfoil", "NM", 2));
        collection.add_card(card("a", "nonfoil", "NM", 3));

        assert_eq!(collection.cards.len(), 1);
        assert_eq!(collection.cards[0].quantity, 5);
    }

    #[test]
    fn add_separates_different_finish() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("a", "nonfoil", "NM", 2));
        collection.add_card(card("a", "foil", "NM", 1));

        assert_eq!(collection.cards.len(), 2);
        assert_eq!(collection.cards[0].quantity, 2);
        assert_eq!(collection.cards[1].quantity, 1);
    }

    #[test]
    fn add_separates_different_condition() {
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("a", "nonfoil", "NM", 1));
        collection.add_card(card("a", "nonfoil", "LP", 1));

        assert_eq!(collection.cards.len(), 2);
    }

    #[test]
    fn remove_decrements_then_removes() {
        let mut collection = Collection { cards: vec![] };
        let c = card("a", "nonfoil", "NM", 2);
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
        collection.add_card(card("a", "nonfoil", "NM", 1));
        collection.add_card(card("b", "foil", "NM", 1));

        assert_eq!(collection.search("test card").len(), 2);
        assert_eq!(collection.search("TEST").len(), 2);
        assert_eq!(collection.search("zzz").len(), 0);
    }

    #[test]
    fn lang_defaults_to_english_when_missing() {
        let json = r#"{"id":"a","name":"X","set":"3ed","set_name":"Set","rarity":"rare","type_line":"Instant","prices":{"usd":null,"usd_foil":null,"eur":null,"eur_foil":null}}"#;
        let card: Card = serde_json::from_str(json).unwrap();
        assert_eq!(card.lang, "en");
        assert_eq!(card.condition, DEFAULT_CONDITION);
        assert_eq!(card.finish, "nonfoil");
    }
}
