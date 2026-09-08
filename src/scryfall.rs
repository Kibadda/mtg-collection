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
    let resp = client()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<ScryfallSearchResult>()
        .await?;
    Ok(resp)
}
