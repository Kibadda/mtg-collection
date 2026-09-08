mod cli;
mod collection;
mod display;
mod image;
mod scryfall;

use clap::Parser;
use cli::{Cli, Commands};
use collection::{CONDITION_OPTIONS, Card, DEFAULT_CONDITION};
use console::style;
use dialoguer::{FuzzySelect, Input, Select, theme::ColorfulTheme};
use scryfall::ScryfallCard;
use std::io::IsTerminal;
use std::path::Path;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let path = collection::collection_path();
    let scope = Scope {
        paper_only: !cli.all_games,
        exclude_promos: !cli.all_promos,
    };

    match cli.command {
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

            let mut collection = collection::Collection::load(&path);
            let finish_flag = finish.map(|f| f.to_string());
            let lang = prompt_language();

            add_flow(
                &mut collection,
                &path,
                AddFlow {
                    scope,
                    query: &q,
                    lang,
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
            let mut collection = collection::Collection::load(&path);
            let lang = prompt_language();

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
                    &mut collection,
                    &path,
                    AddFlow {
                        scope,
                        query: &q,
                        lang: lang.clone(),
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

            let mut collection = collection::Collection::load(&path);
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

            let card_clone = card.clone();
            display::print_card(&card_clone);

            if confirm("Remove one copy from your collection?", false) {
                collection.remove_one(&card_clone);
                collection.save(&path);
                println!(
                    "{}",
                    style(format!(
                        "Removed one '{}' [{} {}] from your collection.",
                        card_clone.name, card_clone.finish, card_clone.condition
                    ))
                    .green()
                    .bold()
                );
            } else {
                println!("{}", style("Cancelled.").dim());
            }
        }

        Commands::List => {
            let collection = collection::Collection::load(&path);
            display::print_collection(&collection.cards);
        }

        Commands::Show { name } => {
            let card_name = name.join(" ");

            let collection = collection::Collection::load(&path);
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

            if let Ok(scard) = scryfall::get_card(&card.id).await {
                image::show_card_image(&scard).await;
            }

            println!();
            display::print_card(card);
            println!();
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
    lang: String,
    set_code: Option<&'a str>,
    finish_flag: Option<&'a str>,
    count: Option<u32>,
    condition: Option<String>,
    verbose: bool,
}

/// One add operation: search-and-pick the card, resolve quantity, finish and
/// condition, then confirm and store.
async fn add_flow(collection: &mut collection::Collection, path: &Path, flow: AddFlow<'_>) {
    let Some(picked) = pick_scryfall_card(
        flow.query,
        flow.scope,
        &flow.lang,
        flow.set_code,
        flow.finish_flag,
    )
    .await
    else {
        return;
    };

    image::show_card_image(&picked.card).await;

    let quantity = flow.count.unwrap_or_else(prompt_quantity);
    let condition = match flow.condition {
        Some(c) => c,
        None => prompt_condition(),
    };

    let card = card_from_scryfall(picked.card, quantity, picked.finish, condition.clone());
    println!();
    display::print_card(&card);
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
    collection.add_card(card);
    collection.save(path);
    println!("{}", style(message).green().bold());
}

fn card_from_scryfall(
    card: ScryfallCard,
    quantity: u32,
    finish: String,
    condition: String,
) -> Card {
    Card {
        id: card.id,
        name: card.name,
        set: card.set,
        set_name: card.set_name,
        rarity: card.rarity,
        type_line: card.type_line,
        mana_cost: card.mana_cost,
        oracle_text: card.oracle_text,
        prices: card.prices.into(),
        quantity,
        finish,
        condition,
        lang: card.lang.unwrap_or_else(|| "en".to_string()),
    }
}

async fn pick_scryfall_card(
    query: &str,
    scope: Scope,
    lang: &str,
    set_code: Option<&str>,
    finish_flag: Option<&str>,
) -> Option<PickedCard> {
    let mut q = scryfall::default_query(query, scope.paper_only, scope.exclude_promos);
    if !q.to_ascii_lowercase().contains("lang:") {
        q.push_str(&format!(" lang:{lang}"));
    }
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
        Some(c) => select_printing(c, scope, lang, set_code, finish_flag).await,
        None => None,
    }
}

/// A resolved card: the exact printing plus the chosen finish string.
struct PickedCard {
    card: ScryfallCard,
    finish: String,
}

fn in_language(card: &ScryfallCard, lang: &str) -> bool {
    card.lang.as_deref().unwrap_or("en") == lang
}

/// Printings in the requested language, deduped by card id (each printing is
/// its own choice). Promo printings are dropped when `exclude_promos` is set.
fn distinct_printings<'a>(
    prints: &'a [ScryfallCard],
    lang: &str,
    exclude_promos: bool,
) -> Vec<&'a ScryfallCard> {
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for c in prints {
        if in_language(c, lang) && seen.insert(&c.id) && !(exclude_promos && c.promo) {
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
    lang: &str,
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

    // Printings in the requested language, deduped by card id.
    let rows = distinct_printings(&prints, lang, scope.exclude_promos);

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
                "{} [{} {}]{} x{} | {}",
                c.name, c.finish, c.condition, lang, c.quantity, c.set_name
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

/// Languages offered when adding a card. The first entry is the default.
const LANGUAGES: [(&str, &str); 2] = [("English", "en"), ("German", "de")];

fn prompt_language() -> String {
    let items: Vec<String> = LANGUAGES.iter().map(|(name, _)| name.to_string()).collect();
    let idx = plain_pick("Language", &items, 0).unwrap_or(0);
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
    fn distinct_printings_dedupes_by_id_and_skips_other_languages() {
        let prints = vec![
            scard("a", "hoc", "19", &["nonfoil", "foil"], false),
            scard("b", "hoc", "59", &["foil"], false),
            scard("a", "hoc", "19", &["nonfoil", "foil"], false),
            non_english("c"),
        ];
        let rows = distinct_printings(&prints, "en", false);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].collector_number, "19");
        assert_eq!(rows[1].collector_number, "59");
    }

    #[test]
    fn distinct_printings_keeps_requested_language() {
        let prints = vec![
            scard("a", "hoc", "1", &["nonfoil"], false),
            non_english("c"),
        ];
        let de = distinct_printings(&prints, "de", false);
        assert_eq!(de.len(), 1);
        assert_eq!(de[0].lang.as_deref(), Some("de"));
        assert_eq!(de[0].id, "c");
    }

    #[test]
    fn distinct_printings_excludes_promos_by_default() {
        let prints = vec![
            scard("a", "ltr", "103", &["nonfoil", "foil"], false),
            scard("b", "pltr", "103s", &["foil"], true),
        ];
        let rows = distinct_printings(&prints, "en", true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].set, "ltr");

        let all = distinct_printings(&prints, "en", false);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn printing_label_includes_set_code_number_and_finishes() {
        let label = printing_label(&scard("a", "hoc", "59", &["foil"], false));
        assert!(label.contains("(HOC 59)"));
        assert!(label.contains("[foil]"));

        let bare = printing_label(&scard("b", "ltr", "", &[], false));
        assert!(bare.contains("(LTR)"));
        assert!(!bare.contains('['));
    }

    #[test]
    fn card_from_scryfall_maps_fields_and_prices() {
        let card = card_from_scryfall(
            scard("id-1", "3ed", "44", &["nonfoil", "foil"], false),
            2,
            "foil".to_string(),
            "LP".to_string(),
        );

        assert_eq!(card.id, "id-1");
        assert_eq!(card.name, "Test Card");
        assert_eq!(card.set, "3ed");
        assert_eq!(card.quantity, 2);
        assert_eq!(card.finish, "foil");
        assert_eq!(card.condition, "LP");
        assert_eq!(card.lang, "en");
        assert_eq!(card.prices.usd.as_deref(), Some("1.00"));
        assert_eq!(card.prices.eur_foil.as_deref(), Some("2.50"));
    }

    #[test]
    fn card_from_scryfall_stores_language() {
        let card = card_from_scryfall(
            non_english("id-9"),
            1,
            "nonfoil".to_string(),
            "NM".to_string(),
        );
        assert_eq!(card.lang, "de");
    }

    #[test]
    fn prompt_finish_defaults_deterministically() {
        let empty: Vec<String> = vec![];
        assert_eq!(prompt_finish(&empty), "nonfoil");
        assert_eq!(prompt_finish(&["foil".to_string()]), "foil");
    }

    #[test]
    fn prompt_language_defaults_to_english() {
        assert_eq!(prompt_language(), "en");
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
