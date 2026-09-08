use crate::scryfall::ScryfallCard;
use console::{Term, style};
use std::io::{Cursor, IsTerminal};

/// Print a card's front art inline via the Kitty graphics protocol.
///
/// Renders using the Kitty protocol when the terminal supports it (viuer
/// probes the terminal for it), falling back to half-block rendering. It is a
/// silent no-op when stdout is not a terminal or the image can't be fetched.
pub async fn show_card_image(card: &ScryfallCard) {
    if !std::io::stdout().is_terminal() {
        return;
    }

    let Some(url) = card.image_url() else {
        return;
    };

    let Ok(data) = crate::scryfall::fetch_bytes(url).await else {
        return;
    };

    let reader = image::ImageReader::new(Cursor::new(data));
    let Ok(reader) = reader.with_guessed_format() else {
        return;
    };
    let Ok(img) = reader.decode() else {
        return;
    };

    let cols = Term::stdout().size().0 as u32;
    let width = (cols / 2).clamp(16, 32);

    let config = viuer::Config {
        // Keep the image at the cursor's current position.
        absolute_offset: false,
        width: Some(width),
        ..Default::default()
    };

    if let Err(e) = viuer::print(&img, &config) {
        eprintln!("{}", style(format!("Could not render image: {e}")).dim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_card_image_fetches_and_decodes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt
            .block_on(crate::scryfall::search_cards(
                &crate::scryfall::default_query("!Counterspell", true, false),
            ))
            .unwrap();
        let url = result.data[0]
            .image_url()
            .expect("Counterspell has an image");
        let bytes = rt.block_on(crate::scryfall::fetch_bytes(url)).unwrap();
        let reader = image::ImageReader::new(Cursor::new(bytes));
        let reader = reader.with_guessed_format().unwrap();
        let img = reader.decode().unwrap();
        assert!(img.width() >= 200 && img.height() >= 200);
    }
}
