use crate::collection::{Card, Collection};
use std::path::PathBuf;

/// Where the collection lives. `Local` is the classic `collection.json` file;
/// `Remote` talks to an `mtg-server` over HTTP.
pub enum Store {
    Local {
        path: PathBuf,
    },
    Remote {
        base: String,
        client: reqwest::Client,
    },
}

impl Store {
    pub fn local(path: PathBuf) -> Self {
        Store::Local { path }
    }

    pub fn remote(base_url: String) -> Self {
        let base = base_url.trim_end_matches('/').to_string();
        Store::Remote {
            base,
            client: reqwest::Client::new(),
        }
    }

    /// Fetch the whole collection.
    pub async fn all(&self) -> Result<Collection, Box<dyn std::error::Error>> {
        match self {
            Store::Local { path } => Ok(Collection::load(path)),
            Store::Remote { base, client } => {
                let resp = client
                    .get(format!("{base}/cards"))
                    .send()
                    .await?
                    .error_for_status()?;
                Ok(resp.json().await?)
            }
        }
    }

    /// Add a card. The server (or the local file) merges by identity.
    pub async fn add(&self, card: &Card) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Store::Local { path } => {
                let mut collection = Collection::load(path);
                collection.add_card(card.clone());
                collection.save(path);
                Ok(())
            }
            Store::Remote { base, client } => {
                client
                    .post(format!("{base}/cards"))
                    .json(card)
                    .send()
                    .await?
                    .error_for_status()?;
                Ok(())
            }
        }
    }

    /// Remove a single copy. Returns `false` when no matching card exists.
    pub async fn remove(&self, card: &Card) -> Result<bool, Box<dyn std::error::Error>> {
        match self {
            Store::Local { path } => {
                let mut collection = Collection::load(path);
                let removed = collection.remove_one(card);
                if removed {
                    collection.save(path);
                }
                Ok(removed)
            }
            Store::Remote { base, client } => {
                let resp = client
                    .post(format!("{base}/cards/remove"))
                    .json(card)
                    .send()
                    .await?;
                if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(false);
                }
                resp.error_for_status()?;
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::Prices;

    fn card(id: &str, quantity: u32) -> Card {
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
            finish: "nonfoil".to_string(),
            condition: "NM".to_string(),
            lang: "en".to_string(),
        }
    }

    #[tokio::test]
    async fn local_store_roundtrip_add_search_remove() {
        let path = std::env::temp_dir().join(format!("mtg-store-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("collection.json");
        let store = Store::local(file.clone());

        assert!(store.all().await.unwrap().cards.is_empty());

        store.add(&card("a", 2)).await.unwrap();
        store.add(&card("a", 3)).await.unwrap();
        store.add(&card("b", 1)).await.unwrap();

        let collection = store.all().await.unwrap();
        assert_eq!(collection.cards.len(), 2);
        let merged = collection.search("test card");
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].quantity, 5);

        assert!(store.remove(&card("a", 0)).await.unwrap());
        assert!(!store.remove(&card("zzz", 0)).await.unwrap());
        let _ = std::fs::remove_dir_all(&path);
    }
}
