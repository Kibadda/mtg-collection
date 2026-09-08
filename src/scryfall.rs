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
    pub games: Vec<String>,
    pub prices: ScryfallPrices,
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

/// Build a Scryfall query. By default restricts results to paper cards by
/// appending `game:paper`, unless the query already sets an explicit
/// `game:` filter or `paper_only` is disabled.
pub fn default_query(query: &str, paper_only: bool) -> String {
    if !paper_only || query.to_ascii_lowercase().contains("game:") {
        query.to_string()
    } else {
        format!("{} game:paper", query.trim_end())
    }
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
        }
        if !resp.has_more {
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
            default_query("Lightning Bolt", true),
            "Lightning Bolt game:paper"
        );
        assert_eq!(
            default_query("Lightning Bolt game:paper", true),
            "Lightning Bolt game:paper"
        );
        assert_eq!(
            default_query("Lightning Bolt game:arena", true),
            "Lightning Bolt game:arena"
        );
        assert_eq!(default_query("Lightning Bolt", false), "Lightning Bolt");
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
        assert!(total(&default_query(name, false)) > 0);
        assert_eq!(total(&default_query(name, true)), 0);
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
