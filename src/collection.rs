use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const COLLECTION_FILE: &str = "collection.json";

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
    pub fn load(path: &PathBuf) -> Self {
        if path.exists() {
            let data = fs::read_to_string(path).expect("Failed to read collection file");
            serde_json::from_str(&data).unwrap_or(Collection { cards: vec![] })
        } else {
            Collection { cards: vec![] }
        }
    }

    pub fn save(&self, path: &PathBuf) {
        let data = serde_json::to_string_pretty(self).expect("Failed to serialize collection");
        fs::write(path, data).expect("Failed to write collection file");
    }

    pub fn find_card(&self, name: &str) -> Option<&Card> {
        self.cards
            .iter()
            .find(|c| c.name.to_lowercase() == name.to_lowercase())
    }

    pub fn add_card(&mut self, card: Card) {
        if let Some(existing) = self.cards.iter_mut().find(|c| c.id == card.id) {
            existing.quantity += 1;
        } else {
            self.cards.push(card);
        }
    }

    pub fn remove_card(&mut self, name: &str) -> bool {
        if let Some(pos) = self
            .cards
            .iter()
            .position(|c| c.name.to_lowercase() == name.to_lowercase())
        {
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
