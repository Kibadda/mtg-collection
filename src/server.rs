use crate::collection::{Card, Collection};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Command-line arguments for the `mtg-server` binary.
#[derive(Debug, Parser)]
#[command(name = "mtg-server", about = "Host your MTG collection over HTTP")]
pub struct ServerArgs {
    /// Address to bind to. Defaults to all interfaces so the client on another
    /// PC can reach it over your LAN.
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: String,
    /// Port to listen on.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
    /// Collection file to serve. Defaults to ~/.mtg-collection/collection.json.
    #[arg(long)]
    pub collection: Option<PathBuf>,
}

/// The collection, guarded so concurrent requests serialize, plus where to
/// persist it after each mutation.
pub struct AppState {
    collection: Mutex<Collection>,
    path: PathBuf,
}

impl AppState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            collection: Mutex::new(Collection::load(&path)),
            path,
        }
    }
}

type SharedState = Arc<AppState>;

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
}

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/cards", get(list_cards).post(add_card))
        .route("/cards/search", get(search_cards))
        .route("/cards/remove", post(remove_card))
        .route("/health", get(health))
        .with_state(state)
}

/// Run the server until it is shut down.
pub async fn serve(args: ServerArgs) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    let path = args
        .collection
        .unwrap_or_else(crate::collection::collection_path);
    let state = Arc::new(AppState::new(path));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("mtg-server listening on {}", listener.local_addr()?);
    axum::serve(listener, router(state)).await?;
    Ok(())
}

async fn list_cards(State(state): State<SharedState>) -> Json<Collection> {
    let collection = state.collection.lock().await;
    Json(Collection {
        cards: collection.cards.clone(),
    })
}

async fn search_cards(
    State(state): State<SharedState>,
    Query(params): Query<SearchParams>,
) -> Json<Vec<Card>> {
    let cards = state
        .collection
        .lock()
        .await
        .search(&params.q)
        .into_iter()
        .cloned()
        .collect();
    Json(cards)
}

async fn add_card(State(state): State<SharedState>, Json(card): Json<Card>) -> StatusCode {
    let mut collection = state.collection.lock().await;
    collection.add_card(card);
    collection.save(&state.path);
    StatusCode::CREATED
}

async fn remove_card(State(state): State<SharedState>, Json(card): Json<Card>) -> StatusCode {
    let mut collection = state.collection.lock().await;
    if collection.remove_one(&card) {
        collection.save(&state.path);
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn health() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::Prices;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn card(id: &str, quantity: u32, name: &str) -> Card {
        Card {
            id: id.to_string(),
            name: name.to_string(),
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

    fn temp_collection_path() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("mtg-server-{}-{id}", std::process::id()))
    }

    async fn app() -> (Router, PathBuf) {
        let path = temp_collection_path();
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("collection.json");
        (router(Arc::new(AppState::new(file.clone()))), file)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let (app, _) = app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn add_search_remove_roundtrip() {
        let (app, file) = app().await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cards")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&card("a", 1, "Rockslide Elemental")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/cards")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let collection: Collection = serde_json::from_slice(&body).unwrap();
        assert_eq!(collection.cards.len(), 1);
        assert_eq!(collection.cards[0].name, "Rockslide Elemental");

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cards/remove")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&card("a", 1, "Rockslide Elemental")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        assert!(file.exists());
        let saved: Collection =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(saved.cards.is_empty());

        let _ = std::fs::remove_dir_all(temp_collection_path());
    }

    #[tokio::test]
    async fn remove_missing_returns_404() {
        let (app, _) = app().await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cards/remove")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&card("zzz", 1, "X")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn search_filters_by_name() {
        let (app, _) = app().await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cards")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&card("a", 1, "Lightning Bolt")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/cards/search?q=lightning")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let cards: Vec<Card> = serde_json::from_slice(&body).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name, "Lightning Bolt");
    }

    #[tokio::test]
    async fn end_to_end_over_real_http() {
        let dir = temp_collection_path();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("collection.json");
        let state = Arc::new(AppState::new(file.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(state);
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let base = format!("http://{addr}");

        assert_eq!(
            client
                .get(format!("{base}/health"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );

        let resp = client
            .post(format!("{base}/cards"))
            .json(&card("a", 2, "Lightning Bolt"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);

        let collection: Collection = client
            .get(format!("{base}/cards"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(collection.cards.len(), 1);

        let resp = client
            .post(format!("{base}/cards/remove"))
            .json(&card("a", 1, "Lightning Bolt"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let saved: Collection =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved.cards.len(), 1);
        assert_eq!(saved.cards[0].quantity, 1);

        let _ = std::fs::remove_dir_all(temp_collection_path());
    }
}
