use crate::collection::{Card, Prices};
use crate::scryfall::ScryfallCard;
use console::style;

fn finish_badge(finish: &str) -> String {
    if finish == "foil" {
        style("FOIL").bold().magenta().to_string()
    } else {
        style("nonfoil").dim().to_string()
    }
}

fn price_line(prices: &Prices, finish: &str) -> String {
    let eur = if finish == "foil" {
        prices.eur_foil.as_deref()
    } else {
        prices.eur.as_deref()
    };

    match eur {
        Some(e) => format!("€{}", e),
        None => String::new(),
    }
}

pub fn print_card(card: &Card) {
    let name = style(&card.name).bold().yellow();
    let set = style(&card.set_name).dim();
    let rarity = style(&card.rarity).cyan();
    let type_line = style(&card.type_line).green();
    let qty = if card.quantity > 1 {
        format!(" (x{})", card.quantity)
    } else {
        String::new()
    };
    let finish = finish_badge(&card.finish);
    let cond = style(&card.condition).dim().blue();

    println!(
        "  {} {} | {} | {} [{} {}]{}",
        name, set, rarity, type_line, finish, cond, qty
    );

    if let Some(ref cost) = card.mana_cost {
        println!("    Mana: {}", cost);
    }
    if let Some(ref text) = card.oracle_text {
        println!("    {}", text);
    }

    let prices = price_line(&card.prices, &card.finish);
    if !prices.is_empty() {
        println!("    Price: {}", prices);
    }
}

pub fn print_search_result(index: usize, card: &ScryfallCard) {
    let name = style(&card.name).bold().yellow();
    let set = style(&card.set_name).dim();
    let rarity = style(&card.rarity).cyan();
    let type_line = style(&card.type_line).green();

    println!(
        "  {}. {} {} | {} | {}",
        style(index).bold(),
        name,
        set,
        rarity,
        type_line
    );

    if let Some(ref cost) = card.mana_cost {
        println!("      Mana: {}", cost);
    }
    if let Some(ref text) = card.oracle_text {
        println!("      {}", text);
    }

    if !card.finishes.is_empty() {
        let finishes = style(card.finishes.join(", ")).dim();
        println!("      Finishes: {}", finishes);
    }

    if let Some(ref eur) = card.prices.eur {
        println!("      Price: €{}", eur);
    }
}

pub fn print_collection(cards: &[Card]) {
    if cards.is_empty() {
        println!("{}", style("Collection is empty.").dim());
        return;
    }

    println!(
        "{}",
        style(format!("Collection ({} unique cards)", cards.len()))
            .bold()
            .green()
    );
    println!("{}", style("─".repeat(60)).dim());

    for card in cards {
        print_card(card);
    }

    let total: u32 = cards.iter().map(|c| c.quantity).sum();
    println!("{}", style("─".repeat(60)).dim());
    println!("{}", style(format!("Total: {} cards", total)).bold());
}
