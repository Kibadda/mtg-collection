use crate::collection::{Card, Collection};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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
    inner: Mutex<AppStateInner>,
    path: PathBuf,
}

struct AppStateInner {
    collection: Collection,
    /// Stamp of the file the in-memory collection was last loaded from. When
    /// the file on disk differs (e.g. the CLI wrote it in local mode), a read
    /// reloads so externally added cards show up without a restart.
    last_seen: Option<FileStamp>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    mtime: std::time::SystemTime,
    len: u64,
}

impl AppState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            inner: Mutex::new(AppStateInner {
                collection: Collection::load(&path),
                last_seen: None,
            }),
            path,
        }
    }
}

/// Re-read the collection file when it changed on disk since it was last
/// loaded or saved. A file that cannot be read or parsed (e.g. a torn mid-write
/// file) is ignored and the in-memory state is kept.
async fn reload_if_changed(state: &AppState) {
    let Some(stamp) = current_stamp(&state.path) else {
        return;
    };
    let mut inner = state.inner.lock().await;
    if inner.last_seen == Some(stamp) {
        return;
    }
    if let Some(collection) = std::fs::read_to_string(&state.path)
        .ok()
        .and_then(|data| serde_json::from_str::<Collection>(&data).ok())
    {
        inner.collection = collection;
        inner.last_seen = Some(stamp);
    }
}

/// Save the in-memory collection and remember the stamp the file now has.
fn persist(state: &AppState, inner: &mut AppStateInner) {
    inner.collection.save(&state.path);
    inner.last_seen = current_stamp(&state.path);
}

fn current_stamp(path: &Path) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        mtime: meta.modified().ok()?,
        len: meta.len(),
    })
}

type SharedState = Arc<AppState>;

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
}

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/", get(index))
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
    reload_if_changed(&state).await;
    let inner = state.inner.lock().await;
    Json(Collection {
        cards: inner.collection.cards.clone(),
    })
}

async fn search_cards(
    State(state): State<SharedState>,
    Query(params): Query<SearchParams>,
) -> Json<Vec<Card>> {
    reload_if_changed(&state).await;
    let inner = state.inner.lock().await;
    let cards = inner
        .collection
        .search(&params.q)
        .into_iter()
        .cloned()
        .collect();
    Json(cards)
}

async fn add_card(State(state): State<SharedState>, Json(card): Json<Card>) -> StatusCode {
    reload_if_changed(&state).await;
    let mut inner = state.inner.lock().await;
    inner.collection.add_card(card);
    persist(&state, &mut inner);
    StatusCode::CREATED
}

async fn remove_card(State(state): State<SharedState>, Json(card): Json<Card>) -> StatusCode {
    reload_if_changed(&state).await;
    let mut inner = state.inner.lock().await;
    if inner.collection.remove_one(&card) {
        persist(&state, &mut inner);
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn health() -> &'static str {
    "ok"
}

/// The read-only web dashboard, embedded so the server stays a single binary.
async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn card(id: &str, quantity: u32, name: &str) -> Card {
        Card {
            name: name.to_string(),
            set: "3ed".to_string(),
            collector_number: id.to_string(),
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
    async fn index_serves_embedded_web_app() {
        let (app, _) = app().await;
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("scryfall.com/cards/collection"));
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
    async fn picks_up_externally_modified_collection_file() {
        let (app, file) = app().await;

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

        // Simulate the CLI writing to the same file while the server runs.
        let mut collection = Collection { cards: vec![] };
        collection.add_card(card("x", 1, "Sol Ring"));
        collection.add_card(card("y", 2, "Counterspell"));
        collection.save(&file);

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
        assert_eq!(collection.cards.len(), 2);
        assert_eq!(collection.cards[0].name, "Sol Ring");

        // Mutations merge onto the freshly reloaded state.
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cards")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&card("x", 5, "Sol Ring")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let saved: Collection =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let sol = saved
            .cards
            .iter()
            .find(|c| c.collector_number == "x")
            .unwrap();
        assert_eq!(sol.quantity, 6);
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
