mod cli;
mod collection;
mod display;
mod scryfall;

use clap::Parser;
use cli::{Cli, Commands};
use collection::{Card, Collection, Prices};
use console::style;
use dialoguer::Confirm;
use dialoguer::FuzzySelect;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let path = collection::collection_path();

    match cli.command {
        Commands::Search { query } => {
            let q = query.join(" ");
            if q.is_empty() {
                eprintln!("{}", style("Error: please provide a search query").red());
                std::process::exit(1);
            }

            match scryfall::search_cards(&q).await {
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

        Commands::Add { query } => {
            let q = query.join(" ");
            if q.is_empty() {
                eprintln!(
                    "{}",
                    style("Error: please provide a card name or search query").red()
                );
                std::process::exit(1);
            }

            let mut collection = Collection::load(&path);

            match scryfall::search_cards(&q).await {
                Ok(result) => {
                    if result.data.is_empty() {
                        println!("{}", style("No cards found.").dim());
                        return;
                    }

                    let card = if result.data.len() == 1 {
                        result.data.into_iter().next().unwrap()
                    } else {
                        let names: Vec<String> = result
                            .data
                            .iter()
                            .map(|c| format!("{} ({})", c.name, c.set_name))
                            .collect();

                        let selection =
                            FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                                .with_prompt("Multiple cards found, select one")
                                .items(&names)
                                .default(0)
                                .interact_opt();

                        match selection {
                            Ok(Some(idx)) => result.data.into_iter().nth(idx).unwrap(),
                            _ => {
                                println!("{}", style("Cancelled.").dim());
                                return;
                            }
                        }
                    };

                    println!();
                    display::print_search_result(0, &card);
                    println!();

                    let confirmed = Confirm::new()
                        .with_prompt("Add this card to your collection?")
                        .default(true)
                        .interact_opt()
                        .unwrap_or(Some(true));
                    if confirmed.unwrap_or(false) {
                        let collection_card = Card {
                            id: card.id,
                            name: card.name.clone(),
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
                            quantity: 1,
                        };

                        collection.add_card(collection_card);
                        collection.save(&path);
                        println!(
                            "{}",
                            style(format!("Added '{}' to your collection.", card.name))
                                .green()
                                .bold()
                        );
                    } else {
                        println!("{}", style("Cancelled.").dim());
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", style("Error searching Scryfall:").red(), e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Remove { name } => {
            let card_name = name.join(" ");
            if card_name.is_empty() {
                eprintln!("{}", style("Error: please provide a card name").red());
                std::process::exit(1);
            }

            let mut collection = Collection::load(&path);

            if let Some(card) = collection.find_card(&card_name) {
                let card_clone = card.clone();
                display::print_card(&card_clone);

                if Confirm::new()
                    .with_prompt("Remove this card from your collection?")
                    .default(false)
                    .interact_opt()
                    .unwrap_or(Some(false))
                    .unwrap_or(false)
                {
                    collection.remove_card(&card_name);
                    collection.save(&path);
                    println!(
                        "{}",
                        style(format!("Removed '{}' from your collection.", card_name))
                            .green()
                            .bold()
                    );
                } else {
                    println!("{}", style("Cancelled.").dim());
                }
            } else {
                eprintln!(
                    "{} '{}'",
                    style("Card not found in collection:").red(),
                    card_name
                );
                std::process::exit(1);
            }
        }

        Commands::List => {
            let collection = Collection::load(&path);
            display::print_collection(&collection.cards);
        }

        Commands::Show { name } => {
            let card_name = name.join(" ");
            let collection = Collection::load(&path);

            match collection.find_card(&card_name) {
                Some(card) => {
                    println!();
                    display::print_card(card);
                    println!();
                }
                None => {
                    eprintln!(
                        "{} '{}'",
                        style("Card not found in collection:").red(),
                        card_name
                    );
                    std::process::exit(1);
                }
            }
        }
    }
}
