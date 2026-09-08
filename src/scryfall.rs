use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT as USER_AGENT_HEADER};
use serde::Deserialize;

const SCRYFALL_API: &str = "https://api.scryfall.com";
const APP_USER_AGENT: &str = "mtg-collection/0.1.0 (personal collection tool)";

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .default_headers({
            let mut headers = HeaderMap::new();
            headers.insert(USER_AGENT_HEADER, HeaderValue::from_static(APP_USER_AGENT));
            headers.insert(
                ACCEPT,
                reqwest::header::HeaderValue::from_static("application/json;q=0.9,*/*;q=0.8"),
            );
            headers
        })
        .build()
        .expect("Failed to build HTTP client")
}

#[derive(Debug, Deserialize)]
pub struct ScryfallSearchResult {
    pub total_cards: Option<u32>,
    pub data: Vec<ScryfallCard>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ScryfallCard {
    pub id: String,
    pub name: String,
    pub set: String,
    pub set_name: String,
    pub rarity: String,
    pub type_line: String,
    pub mana_cost: Option<String>,
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub finishes: Vec<String>,
    #[serde(default)]
    pub prints_search_uri: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub released_at: Option<String>,
    #[serde(default)]
    pub collector_number: String,
    #[serde(default)]
    pub promo: bool,
    #[serde(default)]
    pub games: Vec<String>,
    #[serde(default)]
    pub image_uris: Option<ImageUris>,
    #[serde(default)]
    pub card_faces: Vec<CardFace>,
    pub prices: ScryfallPrices,
}

impl ScryfallCard {
    /// Best available full-card image URL for the front face.
    pub fn image_url(&self) -> Option<&str> {
        fn best(uris: &ImageUris) -> Option<&str> {
            uris.large
                .as_deref()
                .or(uris.normal.as_deref())
                .or(uris.small.as_deref())
                .or(uris.png.as_deref())
        }
        self.image_uris
            .as_ref()
            .and_then(best)
            .or_else(|| {
                self.card_faces
                    .iter()
                    .find_map(|face| face.image_uris.as_ref().and_then(best))
            })
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ImageUris {
    #[serde(default)]
    pub png: Option<String>,
    #[serde(default)]
    pub large: Option<String>,
    #[serde(default)]
    pub normal: Option<String>,
    #[serde(default)]
    pub small: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CardFace {
    #[serde(default)]
    pub image_uris: Option<ImageUris>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ScryfallPrices {
    #[serde(default)]
    pub usd: Option<String>,
    #[serde(default)]
    pub usd_foil: Option<String>,
    #[serde(default)]
    pub eur: Option<String>,
    #[serde(default)]
    pub eur_foil: Option<String>,
}

impl From<ScryfallPrices> for crate::collection::Prices {
    fn from(p: ScryfallPrices) -> Self {
        Self {
            usd: p.usd,
            usd_foil: p.usd_foil,
            eur: p.eur,
            eur_foil: p.eur_foil,
        }
    }
}

pub async fn search_cards(query: &str) -> Result<ScryfallSearchResult, Box<dyn std::error::Error>> {
    let url = format!("{}/cards/search?q={}", SCRYFALL_API, query);
    let resp = client().get(&url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(ScryfallSearchResult {
            total_cards: Some(0),
            data: Vec::new(),
        });
    }
    let resp = resp
        .error_for_status()?
        .json::<ScryfallSearchResult>()
        .await?;
    Ok(resp)
}

/// Fetch a single card by its Scryfall id, e.g. to look up image URLs for a
/// card already stored in the collection.
pub async fn get_card(id: &str) -> Result<ScryfallCard, Box<dyn std::error::Error>> {
    let url = format!("{}/cards/{}", SCRYFALL_API, id);
    let resp = client()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<ScryfallCard>()
        .await?;
    Ok(resp)
}

/// Download a URL (e.g. a Scryfall image) into memory.
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let bytes = client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    Ok(bytes)
}

/// Build a Scryfall query. By default restricts results to paper cards by
/// appending `game:paper`, and (when `exclude_promos`) appends `not:promo`.
/// If the query already sets an explicit `game:` or `promo` filter, the
/// corresponding default is not appended.
pub fn default_query(query: &str, paper_only: bool, exclude_promos: bool) -> String {
    let mut q = query.trim_end().to_string();
    if paper_only && !q.to_ascii_lowercase().contains("game:") {
        q.push_str(" game:paper");
    }
    if exclude_promos && !q.to_ascii_lowercase().contains("promo") {
        q.push_str(" not:promo");
    }
    q
}

#[derive(Debug, Deserialize)]
pub struct ScryfallList {
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub next_page: Option<String>,
    pub data: Vec<ScryfallCard>,
}

/// Fetch all printings (all sets) of a card via its prints_search_uri,
/// following pagination. When `paper_only` is set, digital-only prints are
/// dropped.
pub async fn get_all_printings(
    card: &ScryfallCard,
    paper_only: bool,
) -> Result<Vec<ScryfallCard>, Box<dyn std::error::Error>> {
    let Some(uri) = card.prints_search_uri.clone() else {
        return Ok(if paper_only && !card.games.iter().any(|g| g == "paper") {
            Vec::new()
        } else {
            vec![card.clone()]
        });
    };

    let mut cards = Vec::new();
    let mut next: Option<String> = Some(uri);
    for _ in 0..200 {
        let Some(url) = next.take() else {
            break;
        };
        let resp = client()
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<ScryfallList>()
            .await?;
        let mut data: Vec<ScryfallCard> = resp
            .data
            .into_iter()
            .filter(|c| !paper_only || c.games.iter().any(|g| g == "paper"))
            .collect();
        cards.append(&mut data);
        if resp.has_more {
            next = resp.next_page;
        } else {
            break;
        }
    }
    Ok(cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set_count(card: &ScryfallCard) -> usize {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let prints = rt.block_on(get_all_printings(card, true)).unwrap();
        let mut sets = HashSet::new();
        for p in &prints {
            sets.insert(&p.set);
        }
        sets.len()
    }

    #[test]
    fn default_query_appends_paper_filter() {
        assert_eq!(
            default_query("Lightning Bolt", true, true),
            "Lightning Bolt game:paper not:promo"
        );
        assert_eq!(
            default_query("Lightning Bolt", true, false),
            "Lightning Bolt game:paper"
        );
        assert_eq!(
            default_query("Lightning Bolt game:paper", true, true),
            "Lightning Bolt game:paper not:promo"
        );
        assert_eq!(
            default_query("Lightning Bolt game:arena", true, true),
            "Lightning Bolt game:arena not:promo"
        );
        assert_eq!(
            default_query("Lightning Bolt not:promo", true, true),
            "Lightning Bolt not:promo game:paper"
        );
        assert_eq!(
            default_query("Lightning Bolt", false, true),
            "Lightning Bolt not:promo"
        );
        assert_eq!(
            default_query("Lightning Bolt", false, false),
            "Lightning Bolt"
        );
    }

    #[test]
    fn paper_search_excludes_digital_only_cards() {
        fn total(query: &str) -> u32 {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(search_cards(query))
                .expect("search failed")
                .total_cards
                .unwrap_or(0)
        }

        let name = "!\"Advanced Floral Invocations\"";
        assert!(total(&default_query(name, false, false)) > 0);
        assert_eq!(total(&default_query(name, true, true)), 0);
        assert_eq!(total(&default_query(name, true, false)), 0);
    }

    #[test]
    fn image_url_prefers_large_then_falls_back_to_small_png_and_faces() {
        let uris = |large, png| ImageUris {
            png,
            large,
            normal: None,
            small: None,
        };

        let mut card = ScryfallCard {
            id: "a".to_string(),
            name: "Test".to_string(),
            set: "a".to_string(),
            set_name: "Set".to_string(),
            rarity: "rare".to_string(),
            type_line: "Creature".to_string(),
            mana_cost: None,
            oracle_text: None,
            finishes: vec![],
            prints_search_uri: None,
            lang: None,
            released_at: None,
            collector_number: String::new(),
            promo: false,
            games: vec![],
            image_uris: Some(uris(Some("https://large".to_string()), None)),
            card_faces: vec![],
            prices: ScryfallPrices {
                usd: None,
                usd_foil: None,
                eur: None,
                eur_foil: None,
            },
        };
        assert_eq!(card.image_url(), Some("https://large"));

        card.image_uris = Some(uris(None, Some("https://png".to_string())));
        assert_eq!(card.image_url(), Some("https://png"));

        card.image_uris = None;
        assert_eq!(card.image_url(), None);

        card.card_faces = vec![CardFace {
            image_uris: Some(uris(Some("https://face".to_string()), None)),
        }];
        assert_eq!(card.image_url(), Some("https://face"));
    }

    #[test]
    fn printings_multi_set() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(search_cards("!Counterspell")).unwrap();
        assert!(set_count(&result.data[0]) > 1);
    }

    #[test]
    fn printings_single_set() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt
            .block_on(search_cards("!\"Samwise the Stouthearted\""))
            .unwrap();
        assert_eq!(set_count(&result.data[0]), 1);
    }
}
