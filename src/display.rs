use crate::collection::Card;
use crate::scryfall::ScryfallCard;
use console::style;

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

    println!("  {} {} | {} | {}{}", name, set, rarity, type_line, qty);

    if let Some(ref cost) = card.mana_cost {
        println!("    Mana: {}", cost);
    }
    if let Some(ref text) = card.oracle_text {
        println!("    {}", text);
    }

    let mut prices = Vec::new();
    if let Some(ref usd) = card.prices.usd {
        prices.push(format!("${}", usd));
    }
    if let Some(ref eur) = card.prices.eur {
        prices.push(format!("€{}", eur));
    }
    if !prices.is_empty() {
        println!("    Price: {}", prices.join(" | "));
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

    let mut prices = Vec::new();
    if let Some(ref usd) = card.prices.usd {
        prices.push(format!("${}", usd));
    }
    if let Some(ref eur) = card.prices.eur {
        prices.push(format!("€{}", eur));
    }
    if !prices.is_empty() {
        println!("      Price: {}", prices.join(" | "));
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
