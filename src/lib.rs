pub mod cli;
pub mod collection;
pub mod config;
pub mod display;
pub mod image;
pub mod scryfall;
pub mod server;
pub mod store;

use cli::{Cli, Commands, ConfigAction};
use collection::{CONDITION_OPTIONS, Card, DEFAULT_CONDITION};
use console::style;
use dialoguer::{FuzzySelect, Input, Select, theme::ColorfulTheme};
use scryfall::ScryfallCard;
use std::io::IsTerminal;

/// Entry point for the `mtg-collection` client binary.
pub async fn run(cli: Cli) {
    let scope = Scope {
        paper_only: !cli.all_games,
        exclude_promos: !cli.all_promos,
    };

    let store = match config::load().server_url {
        Some(url) => store::Store::remote(url),
        None => store::Store::local(collection::collection_path()),
    };

    match cli.command {
        Commands::Config { action } => run_config(action),

        Commands::Search { query } => {
            let Some(q) = resolve_query(&query, "Search query") else {
                println!("{}", style("No search query.").dim());
                return;
            };

            match scryfall::search_cards(&scryfall::default_query(
                &q,
                scope.paper_only,
                scope.exclude_promos,
            ))
            .await
            {
                Ok(result) => {
                    if result.data.is_empty() {
                        println!("{}", style("No cards found.").dim());
                        return;
                    }

                    println!(
                        "{}",
                        style(format!(
                            "Found {} card(s)",
                            result.total_cards.unwrap_or(result.data.len() as u32)
                        ))
                        .bold()
                        .green()
                    );

                    for (i, card) in result.data.iter().enumerate() {
                        display::print_search_result(i + 1, card);
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", style("Error searching Scryfall:").red(), e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Add {
            query,
            count,
            finish,
            condition,
            set,
        } => {
            let Some(q) = resolve_query(&query, "Card name or search query") else {
                println!("{}", style("No card name or query.").dim());
                return;
            };

            let finish_flag = finish.map(|f| f.to_string());

            add_flow(
                &store,
                AddFlow {
                    scope,
                    query: &q,
                    set_code: set.as_deref(),
                    finish_flag: finish_flag.as_deref(),
                    count,
                    condition,
                    verbose: true,
                },
            )
            .await;
        }

        Commands::AddMany { set } => {
            println!(
                "{}",
                style("Adding session started. Search for cards, or press Enter to quit.").bold()
            );

            loop {
                let query: String = Input::new()
                    .with_prompt("Search card (empty to exit)")
                    .allow_empty(true)
                    .interact()
                    .ok()
                    .unwrap_or_default();

                let q = query.trim().to_string();
                if q.is_empty() {
                    println!("{}", style("Session ended.").dim());
                    break;
                }

                add_flow(
                    &store,
                    AddFlow {
                        scope,
                        query: &q,
                        set_code: set.as_deref(),
                        finish_flag: None,
                        count: None,
                        condition: None,
                        verbose: false,
                    },
                )
                .await;
                println!();
            }
        }

        Commands::Remove { name } => {
            let card_name = name.join(" ");
            let collection = load_collection(&store).await;
            let matches: Vec<&Card> = if card_name.is_empty() {
                if collection.cards.is_empty() {
                    println!("{}", style("Collection is empty.").dim());
                    return;
                }
                collection.cards.iter().collect()
            } else {
                collection.search(&card_name)
            };

            if matches.is_empty() {
                println!(
                    "{}",
                    style(format!("No card matching '{}' in collection.", card_name)).red()
                );
                std::process::exit(1);
            }

            let Some(card) = select_collection_card(matches, "Select card to remove") else {
                println!("{}", style("Cancelled.").dim());
                return;
            };

            let view = resolve_card_view(card.clone()).await;
            display::print_card(&view);

            if confirm("Remove one copy from your collection?", false) {
                match store.remove(&view.card).await {
                    Ok(true) => println!(
                        "{}",
                        style(format!(
                            "Removed one '{}' [{} {}] from your collection.",
                            view.card.name, view.card.finish, view.card.condition
                        ))
                        .green()
                        .bold()
                    ),
                    Ok(false) => eprintln!(
                        "{}",
                        style(format!(
                            "Card not found in collection: '{}'.",
                            view.card.name
                        ))
                        .red()
                    ),
                    Err(e) => eprintln!("{} {}", style("Error removing card:").red(), e),
                }
            } else {
                println!("{}", style("Cancelled.").dim());
            }
        }

        Commands::List => {
            let collection = load_collection(&store).await;
            let views = resolve_collection_views(collection.cards).await;
            display::print_collection(&views);
        }

        Commands::Show { name } => {
            let card_name = name.join(" ");
            let collection = load_collection(&store).await;
            let matches: Vec<&Card> = if card_name.is_empty() {
                if collection.cards.is_empty() {
                    println!("{}", style("Collection is empty.").dim());
                    return;
                }
                collection.cards.iter().collect()
            } else {
                collection.search(&card_name)
            };

            if matches.is_empty() {
                eprintln!(
                    "{} '{}'",
                    style("No card found in collection:").red(),
                    card_name
                );
                std::process::exit(1);
            }

            let Some(card) = select_collection_card(matches, "Select card") else {
                println!("{}", style("Cancelled.").dim());
                return;
            };

            let view = resolve_card_view(card.clone()).await;
            if let Some(data) = &view.data {
                image::show_card_image(data).await;
            }

            println!();
            display::print_card(&view);
            println!();
        }
    }
}

fn run_config(action: ConfigAction) {
    match action {
        ConfigAction::SetServer { url } => {
            config::set_server(&url);
            println!(
                "{}",
                style(format!("Server URL set to {}", url)).green().bold()
            );
        }
        ConfigAction::Show => match config::load().server_url {
            Some(url) => println!("{}", style(format!("Server URL: {}", url)).bold()),
            None => println!(
                "{}",
                style("No remote server configured; using the local collection file.").dim()
            ),
        },
        ConfigAction::Clear => {
            config::clear_server();
            println!(
                "{}",
                style("Server URL removed; using the local collection file.").bold()
            );
        }
    }
}

/// Fetch the whole collection, exiting with an error message on failure.
async fn load_collection(store: &store::Store) -> collection::Collection {
    match store.all().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", style("Error reading collection:").red(), e);
            std::process::exit(1);
        }
    }
}

/// Search-scoping flags derived from the global `--all-games`/`--all-promos` options.
#[derive(Clone, Copy)]
struct Scope {
    paper_only: bool,
    exclude_promos: bool,
}

/// Everything needed to add one card, shared by `add` and `add-many`.
struct AddFlow<'a> {
    scope: Scope,
    query: &'a str,
    set_code: Option<&'a str>,
    finish_flag: Option<&'a str>,
    count: Option<u32>,
    condition: Option<String>,
    verbose: bool,
}

/// One add operation: search-and-pick the card, resolve quantity, condition
/// and language, then confirm and store. Finish is resolved while picking the
/// printing.
async fn add_flow(store: &store::Store, flow: AddFlow<'_>) {
    let Some(picked) =
        pick_scryfall_card(flow.query, flow.scope, flow.set_code, flow.finish_flag).await
    else {
        return;
    };

    image::show_card_image(&picked.card).await;

    let quantity = flow.count.unwrap_or_else(prompt_quantity);
    let condition = match flow.condition {
        Some(c) => c,
        None => prompt_condition(),
    };
    let lang = prompt_language(picked.card.lang.as_deref());

    let card = card_from_scryfall(
        picked.card.clone(),
        quantity,
        picked.finish.clone(),
        condition.clone(),
        lang,
    );
    println!();
    display::print_card(&display::CollectionCardView {
        card: card.clone(),
        data: Some(picked.card),
    });
    println!();

    if !confirm_add() {
        println!(
            "{}",
            style(if flow.verbose {
                "Cancelled."
            } else {
                "Skipped."
            })
            .dim()
        );
        return;
    }

    let message = if flow.verbose {
        let lang_badge = if card.lang != "en" {
            format!(" ({})", card.lang)
        } else {
            String::new()
        };
        format!(
            "Added x{} '{}' [{} {}]{} to your collection.",
            quantity, card.name, card.finish, condition, lang_badge
        )
    } else {
        "Added.".to_string()
    };
    match store.add(&card).await {
        Ok(()) => println!("{}", style(message).green().bold()),
        Err(e) => eprintln!("{} {}", style("Failed to save to collection:").red(), e),
    }
}

fn card_from_scryfall(
    card: ScryfallCard,
    quantity: u32,
    finish: String,
    condition: String,
    lang: String,
) -> Card {
    Card {
        name: card.name,
        set: card.set,
        collector_number: card.collector_number,
        quantity,
        finish,
        condition,
        lang,
    }
}

/// Key set + collector number so fetched display data can be matched back
/// onto stored collection entries (which may share a printing across finish
/// variants).
fn card_key(set: &str, number: &str) -> String {
    format!("{set}:{number}")
}

/// Fetch the live Scryfall data for every stored card (bulk, batching up to 75
/// printings per request) and attach it to each entry. Entries that cannot be
/// resolved get `data: None` and are rendered as a placeholder.
async fn resolve_collection_views(cards: Vec<Card>) -> Vec<display::CollectionCardView> {
    let identifiers: Vec<scryfall::CardIdentifier> = cards
        .iter()
        .filter(|c| !c.collector_number.is_empty())
        .map(|c| scryfall::CardIdentifier {
            set: c.set.clone(),
            collector_number: c.collector_number.clone(),
        })
        .collect();

    let data = match scryfall::get_cards_by_identifiers(&identifiers).await {
        Ok(result) => {
            if !result.not_found.is_empty() {
                println!(
                    "{}",
                    style(format!(
                        "{} card(s) could not be resolved on Scryfall.",
                        result.not_found.len()
                    ))
                    .yellow()
                );
            }
            result.data
        }
        Err(e) => {
            println!(
                "{}",
                style(format!("Could not fetch card details from Scryfall: {e}")).yellow()
            );
            Vec::new()
        }
    };

    let by_key: std::collections::HashMap<String, ScryfallCard> = data
        .into_iter()
        .map(|c| (card_key(&c.set, &c.collector_number), c))
        .collect();

    cards
        .into_iter()
        .map(|card| {
            let data = by_key
                .get(&card_key(&card.set, &card.collector_number))
                .cloned();
            display::CollectionCardView { card, data }
        })
        .collect()
}

async fn resolve_card_view(card: Card) -> display::CollectionCardView {
    resolve_collection_views(vec![card])
        .await
        .into_iter()
        .next()
        .expect("resolving one card yields one view")
}

async fn pick_scryfall_card(
    query: &str,
    scope: Scope,
    set_code: Option<&str>,
    finish_flag: Option<&str>,
) -> Option<PickedCard> {
    let q = scryfall::default_query(query, scope.paper_only, scope.exclude_promos);
    let card = match scryfall::search_cards(&q).await {
        Ok(result) if !result.data.is_empty() => {
            if result.data.len() == 1 {
                result.data.into_iter().next()
            } else {
                let names: Vec<String> = result
                    .data
                    .iter()
                    .map(|c| format!("{} ({})", c.name, c.set_name))
                    .collect();

                let selection = fuzzy_pick("Multiple cards found, select one", &names, 0);

                match selection {
                    Some(idx) => result.data.into_iter().nth(idx),
                    None => {
                        println!("{}", style("Cancelled.").dim());
                        None
                    }
                }
            }
        }
        Ok(_) => {
            println!("{}", style("No cards found.").dim());
            None
        }
        Err(e) => {
            eprintln!("{} {}", style("Error searching Scryfall:").red(), e);
            None
        }
    };

    match card {
        Some(c) => select_printing(c, scope, set_code, finish_flag).await,
        None => None,
    }
}

/// A resolved card: the exact printing plus the chosen finish string.
struct PickedCard {
    card: ScryfallCard,
    finish: String,
}

/// Printings deduped by set + collector number so each printing is a single
/// choice regardless of language; the `lang:en` fetch upstream already
/// collapses language variants. Promo printings are dropped when
/// `exclude_promos` is set.
fn distinct_printings(prints: &[ScryfallCard], exclude_promos: bool) -> Vec<&ScryfallCard> {
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for c in prints {
        let key = (c.set.as_str(), c.collector_number.as_str());
        if seen.insert(key) && !(exclude_promos && c.promo) {
            rows.push(c);
        }
    }
    rows
}

/// After a card name has been chosen:
/// 1. pick which exact printing to add (prompt, or filtered by `set_code`),
/// 2. resolve the finish (flag, or prompt from that printing's own finishes),
/// 3. return that printing plus the chosen finish.
async fn select_printing(
    card: ScryfallCard,
    scope: Scope,
    set_code: Option<&str>,
    finish_flag: Option<&str>,
) -> Option<PickedCard> {
    let prints = match scryfall::get_all_printings(&card, scope.paper_only).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{} {} {}",
                style("Error fetching printings:").red(),
                e,
                style("(using the selected card anyway)").dim()
            );
            let finish = finish_flag
                .map(|f| f.to_string())
                .unwrap_or_else(|| prompt_finish(&card.finishes));
            return Some(PickedCard { card, finish });
        }
    };

    // One row per printing (set + collector number); language is not part of
    // the choice, it is resolved separately as metadata.
    let rows = distinct_printings(&prints, scope.exclude_promos);

    // Narrow to the forced set if `--set <CODE>` was given; fall back to all
    // printings (with a warning) when the card has none in that set.
    let mut candidates: Vec<&ScryfallCard> = rows.clone();
    if let Some(code) = set_code {
        let narrowed: Vec<&ScryfallCard> = rows
            .iter()
            .filter(|c| c.set.eq_ignore_ascii_case(code))
            .copied()
            .collect();
        if narrowed.is_empty() {
            if rows.len() > 1 {
                println!(
                    "{}",
                    style(format!("Card not printed in set '{}'.", code)).yellow()
                );
            }
        } else {
            candidates = narrowed;
        }
    }

    let chosen: Option<&ScryfallCard> = if candidates.is_empty() {
        None
    } else if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        let labels: Vec<String> = candidates.iter().map(|c| printing_label(c)).collect();

        // Pick a sensible default row: an explicitly requested finish first, then
        // the already-searched card's exact printing, then a nonfoil printing.
        let default_idx = finish_flag
            .and_then(|f| {
                candidates
                    .iter()
                    .position(|c| !c.finishes.is_empty() && c.finishes.iter().all(|x| x == f))
            })
            .or_else(|| candidates.iter().position(|c| c.id == card.id))
            .or_else(|| {
                candidates
                    .iter()
                    .position(|c| c.finishes.iter().any(|x| x == "nonfoil"))
            })
            .unwrap_or(0);

        let selection = fuzzy_pick(
            &format!("Select printing for '{}'", card.name),
            &labels,
            default_idx,
        );

        match selection {
            Some(idx) => candidates.into_iter().nth(idx),
            None => {
                println!("{}", style("Cancelled.").dim());
                return None;
            }
        }
    };

    let Some(chosen_print) = chosen else {
        let finish = finish_flag
            .map(|f| f.to_string())
            .unwrap_or_else(|| prompt_finish(&card.finishes));
        return Some(PickedCard { card, finish });
    };

    let finish = match finish_flag {
        Some(f) => {
            if !chosen_print.finishes.iter().any(|x| x == f) {
                println!(
                    "{}",
                    style(format!("Finish '{}' not printed for this card.", f)).yellow()
                );
            }
            f.to_string()
        }
        None => prompt_finish(&chosen_print.finishes),
    };

    Some(PickedCard {
        card: chosen_print.clone(),
        finish,
    })
}

fn printing_label(c: &ScryfallCard) -> String {
    let date = c.released_at.as_deref().unwrap_or("?");
    let number = if c.collector_number.is_empty() {
        String::new()
    } else {
        format!(" {}", c.collector_number)
    };
    let finishes = if c.finishes.is_empty() {
        String::new()
    } else {
        format!(" [{}]", c.finishes.join("/"))
    };
    format!(
        "{} ({}{}) — {} — {}{}",
        c.set_name,
        c.set.to_uppercase(),
        number,
        c.rarity,
        date,
        finishes
    )
}

fn select_collection_card<'a>(cards: Vec<&'a Card>, prompt: &str) -> Option<&'a Card> {
    if cards.is_empty() {
        return None;
    }
    if cards.len() == 1 {
        return cards.into_iter().next();
    }

    let names: Vec<String> = cards
        .iter()
        .map(|c| {
            let lang = if c.lang == "en" {
                String::new()
            } else {
                format!(" ({})", c.lang)
            };
            format!(
                "{} [{} {}]{} x{} | {} #{}",
                c.name,
                c.finish,
                c.condition,
                lang,
                c.quantity,
                c.set.to_uppercase(),
                c.collector_number
            )
        })
        .collect();

    let selection = fuzzy_pick(prompt, &names, 0);

    match selection {
        Some(idx) => cards.into_iter().nth(idx),
        None => {
            println!("{}", style("Cancelled.").dim());
            None
        }
    }
}

fn prompt_quantity() -> u32 {
    Input::<u32>::with_theme(&ColorfulTheme::default())
        .with_prompt("Quantity")
        .default(1)
        .interact()
        .ok()
        .unwrap_or(1)
}

fn prompt_finish(available: &[String]) -> String {
    if available.is_empty() {
        return "nonfoil".to_string();
    }
    if available.len() == 1 {
        return available[0].clone();
    }

    let items: Vec<String> = available.to_vec();
    plain_pick("Finish", &items, 0)
        .map(|idx| available[idx].clone())
        .unwrap_or_else(|| available[0].clone())
}

fn prompt_condition() -> String {
    let items: Vec<String> = CONDITION_OPTIONS.iter().map(|s| s.to_string()).collect();
    plain_pick("Condition", &items, 0)
        .map(|idx| items[idx].clone())
        .unwrap_or_else(|| DEFAULT_CONDITION.to_string())
}

/// Languages offered when adding a card.
const LANGUAGES: [(&str, &str); 2] = [("English", "en"), ("German", "de")];

/// Ask for the language of the copy being added. `default` is the language of
/// the printing that was picked, so the common case is just hitting Enter.
fn prompt_language(default: Option<&str>) -> String {
    let items: Vec<String> = LANGUAGES.iter().map(|(name, _)| name.to_string()).collect();
    let default_idx = default
        .and_then(|l| LANGUAGES.iter().position(|(_, code)| *code == l))
        .unwrap_or(0);
    let idx = plain_pick("Language", &items, default_idx).unwrap_or(default_idx);
    LANGUAGES[idx].1.to_string()
}

fn confirm_add() -> bool {
    confirm("Add this card to your collection?", true)
}

fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
}

/// Prompt for a query interactively. Returns `None` when running with a
/// non-terminal stdin or when the user enters nothing.
fn prompt_query(label: &str) -> Option<String> {
    if !is_interactive() {
        return None;
    }
    let input = Input::<String>::with_theme(&ColorfulTheme::default())
        .with_prompt(label)
        .allow_empty(true)
        .interact()
        .ok()?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Combine trailing-argument words into a query, prompting interactively when
/// nothing was given on the command line. Returns `None` when no query could
/// be resolved.
fn resolve_query(words: &[String], label: &str) -> Option<String> {
    let joined = words.join(" ").trim().to_string();
    if !joined.is_empty() {
        return Some(joined);
    }
    prompt_query(label)
}

fn fuzzy_pick(prompt: &str, items: &[String], default: usize) -> Option<usize> {
    if !is_interactive() {
        return Some(default);
    }
    FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact_opt()
        .ok()
        .flatten()
}

fn plain_pick(prompt: &str, items: &[String], default: usize) -> Option<usize> {
    if !is_interactive() {
        return Some(default);
    }
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact_opt()
        .ok()
        .flatten()
}

fn confirm(prompt: &str, default: bool) -> bool {
    if !is_interactive() {
        return default;
    }
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact_opt()
        .ok()
        .flatten()
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scard(id: &str, set: &str, num: &str, finishes: &[&str], promo: bool) -> ScryfallCard {
        ScryfallCard {
            id: id.to_string(),
            name: "Test Card".to_string(),
            set: set.to_string(),
            set_name: format!("Set {}", set.to_uppercase()),
            rarity: "rare".to_string(),
            type_line: "Creature".to_string(),
            mana_cost: Some("{R}".to_string()),
            oracle_text: Some("Ability".to_string()),
            finishes: finishes.iter().map(|s| s.to_string()).collect(),
            prints_search_uri: None,
            lang: Some("en".to_string()),
            released_at: Some("2024-01-01".to_string()),
            collector_number: num.to_string(),
            promo,
            games: vec!["paper".to_string()],
            image_uris: None,
            card_faces: vec![],
            prices: scryfall::ScryfallPrices {
                usd: Some("1.00".to_string()),
                usd_foil: None,
                eur: Some("0.80".to_string()),
                eur_foil: Some("2.50".to_string()),
            },
        }
    }

    fn non_english(id: &str) -> ScryfallCard {
        let mut c = scard(id, "hoc", "1", &["nonfoil"], false);
        c.lang = Some("de".to_string());
        c
    }

    #[test]
    fn distinct_printings_keeps_one_row_per_printing_not_per_language() {
        let prints = vec![
            scard("en-19", "hoc", "19", &["nonfoil", "foil"], false),
            scard("de-19", "hoc", "19", &["nonfoil", "foil"], false),
            scard("jp-19", "hoc", "19", &["nonfoil", "foil"], false),
            scard("de-59", "hoc", "59", &["foil"], false),
        ];
        let rows = distinct_printings(&prints, false);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].collector_number, "19");
        assert_eq!(rows[1].collector_number, "59");
    }

    #[test]
    fn distinct_printings_excludes_promos_by_default() {
        let prints = vec![
            scard("a", "ltr", "103", &["nonfoil", "foil"], false),
            scard("b", "pltr", "103s", &["foil"], true),
        ];
        let rows = distinct_printings(&prints, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].set, "ltr");

        let all = distinct_printings(&prints, false);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn printing_label_includes_set_code_number_and_finishes() {
        let label = printing_label(&scard("a", "hoc", "59", &["foil"], false));
        assert!(label.contains("(HOC 59)"));
        assert!(label.contains("[foil]"));
        assert!(!label.contains("(en)"));

        let bare = printing_label(&scard("b", "ltr", "", &[], false));
        assert!(bare.contains("(LTR)"));
        assert!(!bare.contains('['));
    }

    #[test]
    fn card_from_scryfall_stores_only_identifying_fields() {
        let card = card_from_scryfall(
            scard("id-1", "3ed", "44", &["nonfoil", "foil"], false),
            2,
            "foil".to_string(),
            "LP".to_string(),
            "en".to_string(),
        );

        assert_eq!(card.name, "Test Card");
        assert_eq!(card.set, "3ed");
        assert_eq!(card.collector_number, "44");
        assert_eq!(card.quantity, 2);
        assert_eq!(card.finish, "foil");
        assert_eq!(card.condition, "LP");
        assert_eq!(card.lang, "en");
    }

    #[test]
    fn card_from_scryfall_stores_prompted_language() {
        let card = card_from_scryfall(
            non_english("id-9"),
            1,
            "nonfoil".to_string(),
            "NM".to_string(),
            "de".to_string(),
        );
        assert_eq!(card.lang, "de");

        let english_forced = card_from_scryfall(
            non_english("id-9"),
            1,
            "nonfoil".to_string(),
            "NM".to_string(),
            "en".to_string(),
        );
        assert_eq!(english_forced.lang, "en");
    }

    #[test]
    fn card_key_matches_by_set_and_number() {
        assert_eq!(card_key("3ed", "44"), "3ed:44");
        assert_eq!(card_key("ltr", "103s"), "ltr:103s");
    }

    #[test]
    fn prompt_finish_defaults_deterministically() {
        let empty: Vec<String> = vec![];
        assert_eq!(prompt_finish(&empty), "nonfoil");
        assert_eq!(prompt_finish(&["foil".to_string()]), "foil");
    }

    #[test]
    fn prompt_language_defaults_to_english_when_picking_non_german() {
        assert_eq!(prompt_language(None), "en");
        assert_eq!(prompt_language(Some("en")), "en");
    }

    #[test]
    fn prompt_language_defaults_to_german_for_german_printing() {
        assert_eq!(prompt_language(Some("de")), "de");
    }

    #[test]
    fn resolve_query_joins_words_variadic_style() {
        assert_eq!(
            resolve_query(&["Lightning".to_string(), "Bolt".to_string()], "Search"),
            Some("Lightning Bolt".to_string())
        );
    }

    #[test]
    fn resolve_query_requires_prompt_for_empty_words() {
        assert_eq!(resolve_query(&[], "Search"), None);
    }
}
