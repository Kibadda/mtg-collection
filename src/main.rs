mod cli;
mod collection;
mod display;
mod scryfall;

use clap::Parser;
use cli::{Cli, Commands};
use collection::{CONDITION_OPTIONS, Card, DEFAULT_CONDITION, Prices};
use console::style;
use dialoguer::{FuzzySelect, Input, Select, theme::ColorfulTheme};
use scryfall::ScryfallCard;
use std::io::IsTerminal;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let path = collection::collection_path();
    let paper_only = !cli.all_games;

    match cli.command {
        Commands::Search { query } => {
            let q = query.join(" ");
            if q.is_empty() {
                eprintln!("{}", style("Error: please provide a search query").red());
                std::process::exit(1);
            }

            match scryfall::search_cards(&scryfall::default_query(&q, paper_only)).await {
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
        } => {
            let q = query.join(" ");
            if q.is_empty() {
                eprintln!(
                    "{}",
                    style("Error: please provide a card name or search query").red()
                );
                std::process::exit(1);
            }

            let mut collection = collection::Collection::load(&path);

            if let Some(scard) = pick_scryfall_card(&q, paper_only).await {
                let quantity = count.unwrap_or_else(prompt_quantity);
                let finish = match finish {
                    Some(f) => f.to_string(),
                    None => prompt_finish(&scard.finishes),
                };
                let card_condition = match condition {
                    Some(c) => c,
                    None => prompt_condition(),
                };

                let card = card_from_scryfall(scard, quantity, finish, card_condition.clone());
                println!();
                display::print_card(&card);
                println!();

                if confirm_add() {
                    let name_clone = card.name.clone();
                    let finish_clone = card.finish.clone();
                    collection.add_card(card);
                    collection.save(&path);
                    println!(
                        "{}",
                        style(format!(
                            "Added x{} '{}' [{} {}] to your collection.",
                            quantity, name_clone, finish_clone, card_condition
                        ))
                        .green()
                        .bold()
                    );
                } else {
                    println!("{}", style("Cancelled.").dim());
                }
            }
        }

        Commands::AddMany => {
            let mut collection = collection::Collection::load(&path);

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

                if let Some(scard) = pick_scryfall_card(&q, paper_only).await {
                    let quantity = prompt_quantity();
                    let finish = prompt_finish(&scard.finishes);
                    let condition = prompt_condition();

                    let card = card_from_scryfall(scard, quantity, finish, condition.clone());
                    println!();
                    display::print_card(&card);
                    println!();

                    if confirm_add() {
                        collection.add_card(card);
                        collection.save(&path);
                        println!("{}", style("Added.").green().bold());
                    } else {
                        println!("{}", style("Skipped.").dim());
                    }
                }
                println!();
            }
        }

        Commands::Remove { name } => {
            let card_name = name.join(" ");
            if card_name.is_empty() {
                eprintln!("{}", style("Error: please provide a card name").red());
                std::process::exit(1);
            }

            let mut collection = collection::Collection::load(&path);
            let matches = collection.search(&card_name);

            let Some(card) = select_collection_card(matches, "Select card to remove") else {
                println!(
                    "{}",
                    style(format!("No card matching '{}' in collection.", card_name)).red()
                );
                std::process::exit(1);
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
            if card_name.is_empty() {
                eprintln!("{}", style("Error: please provide a card name").red());
                std::process::exit(1);
            }

            let collection = collection::Collection::load(&path);
            let matches = collection.search(&card_name);

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

            println!();
            display::print_card(card);
            println!();
        }
    }
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
        prices: Prices {
            usd: card.prices.usd,
            usd_foil: card.prices.usd_foil,
            eur: card.prices.eur,
            eur_foil: card.prices.eur_foil,
        },
        quantity,
        finish,
        condition,
    }
}

async fn pick_scryfall_card(query: &str, paper_only: bool) -> Option<ScryfallCard> {
    let q = scryfall::default_query(query, paper_only);
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
        Some(c) => select_printing(c, paper_only).await,
        None => None,
    }
}

/// After a card name has been chosen, let the user pick which set/printing
/// to add if the card exists in multiple sets.
async fn select_printing(card: ScryfallCard, paper_only: bool) -> Option<ScryfallCard> {
    let prints = match scryfall::get_all_printings(&card, paper_only).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{} {} {}",
                style("Error fetching printings:").red(),
                e,
                style("(using the selected card anyway)").dim()
            );
            return Some(card);
        }
    };

    // Keep English printings, deduped per set (first print per set wins).
    let mut seen = std::collections::HashSet::new();
    let mut sets: Vec<&ScryfallCard> = Vec::new();
    for c in &prints {
        if c.lang.as_deref().unwrap_or("en") != "en" {
            continue;
        }
        if seen.insert(&c.set) {
            sets.push(c);
        }
    }

    if sets.is_empty() {
        return Some(card);
    }
    if sets.len() == 1 {
        return sets.into_iter().next().cloned();
    }

    let labels: Vec<String> = sets
        .iter()
        .map(|c| {
            let date = c.released_at.as_deref().unwrap_or("?");
            format!(
                "{} ({}) — {} — {}",
                c.set_name,
                c.set.to_uppercase(),
                c.rarity,
                date
            )
        })
        .collect();

    // Default to the already-selected card's set (respects `set:` in the query).
    let default_idx = sets.iter().position(|c| c.set == card.set).unwrap_or(0);

    let selection = if is_interactive() {
        fuzzy_pick(
            &format!("Select set for '{}'", card.name),
            &labels,
            default_idx,
        )
    } else {
        Some(default_idx)
    };

    match selection {
        Some(idx) => sets.into_iter().nth(idx).cloned(),
        None => {
            println!("{}", style("Cancelled.").dim());
            None
        }
    }
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
            format!(
                "{} [{} {}] x{} | {}",
                c.name, c.finish, c.condition, c.quantity, c.set_name
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

fn confirm_add() -> bool {
    confirm("Add this card to your collection?", true)
}

fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
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
