use crate::collection::Card;
use crate::scryfall::{ScryfallCard, ScryfallPrices};
use console::style;

/// A stored card plus the display data fetched live from Scryfall. `data` is
/// `None` when the printing could not be resolved on Scryfall (e.g. cards
/// saved before collector numbers were tracked).
pub struct CollectionCardView {
    pub card: Card,
    pub data: Option<ScryfallCard>,
}

fn finish_badge(finish: &str) -> String {
    if finish == "foil" {
        style("FOIL").bold().magenta().to_string()
    } else {
        style("nonfoil").dim().to_string()
    }
}

fn lang_badge(lang: &str) -> String {
    if lang == "en" {
        String::new()
    } else {
        style(format!(" [{}]", lang)).dim().to_string()
    }
}

fn price_line(prices: &ScryfallPrices, finish: &str) -> String {
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

pub fn print_card(view: &CollectionCardView) {
    let card = &view.card;
    let name = style(&card.name).bold().yellow();
    let finish = finish_badge(&card.finish);
    let cond = style(&card.condition).dim().blue();
    let qty = if card.quantity > 1 {
        format!(" (x{})", card.quantity)
    } else {
        String::new()
    };

    match &view.data {
        Some(data) => {
            let set = style(&data.set_name).dim();
            let rarity = style(&data.rarity).cyan();
            let type_line = style(&data.type_line).green();

            println!(
                "  {} {} | {} | {} [{} {}]{}{}",
                name,
                set,
                rarity,
                type_line,
                finish,
                cond,
                lang_badge(&card.lang),
                qty
            );

            if let Some(ref cost) = data.mana_cost {
                println!("    Mana: {}", cost);
            }
            if let Some(ref text) = data.oracle_text {
                println!("    {}", text);
            }

            let price = price_line(&data.prices, &card.finish);
            if !price.is_empty() {
                println!("    Price: {}", price);
            }
        }
        None => {
            let set = style(format!("[{}]", card.set.to_uppercase())).dim();
            println!(
                "  {} {} [{} {}]{}{}",
                name,
                set,
                finish,
                cond,
                lang_badge(&card.lang),
                qty
            );
            println!(
                "    {}",
                style("No Scryfall details (re-add the card to restore).").dim()
            );
        }
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

pub fn print_collection(views: &[CollectionCardView]) {
    if views.is_empty() {
        println!("{}", style("Collection is empty.").dim());
        return;
    }

    println!(
        "{}",
        style(format!("Collection ({} unique cards)", views.len()))
            .bold()
            .green()
    );
    println!("{}", style("─".repeat(60)).dim());

    for view in views {
        print_card(view);
    }

    let total: u32 = views.iter().map(|v| v.card.quantity).sum();
    println!("{}", style("─".repeat(60)).dim());
    println!("{}", style(format!("Total: {} cards", total)).bold());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prices(eur: Option<&str>, eur_foil: Option<&str>) -> ScryfallPrices {
        ScryfallPrices {
            usd: Some("9.99".to_string()),
            usd_foil: Some("19.99".to_string()),
            eur: eur.map(|s| s.to_string()),
            eur_foil: eur_foil.map(|s| s.to_string()),
        }
    }

    #[test]
    fn price_line_shows_only_euro_for_nonfoil() {
        assert_eq!(price_line(&prices(Some("1.23"), None), "nonfoil"), "€1.23");
    }

    #[test]
    fn price_line_uses_foil_euro_for_foil() {
        assert_eq!(
            price_line(&prices(Some("1.23"), Some("4.56")), "foil"),
            "€4.56"
        );
    }

    #[test]
    fn price_line_empty_when_missing() {
        assert_eq!(price_line(&prices(None, None), "nonfoil"), "");
        assert_eq!(price_line(&prices(None, None), "foil"), "");
    }

    #[test]
    fn lang_badge_only_for_non_english() {
        assert_eq!(lang_badge("en"), "");
        assert!(lang_badge("de").contains("de"));
    }
}
