use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mtg-collection", about = "Manage your MTG card collection")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Search for cards using Scryfall syntax
    Search {
        /// Scryfall search query
        #[arg(trailing_var_arg = true)]
        query: Vec<String>,
    },
    /// Add a card to your collection
    Add {
        /// Scryfall search query to find the card
        #[arg(trailing_var_arg = true)]
        query: Vec<String>,
    },
    /// Remove a card from your collection
    Remove {
        /// Card name to remove
        name: Vec<String>,
    },
    /// List all cards in your collection
    List,
    /// Show details of a specific card in your collection
    Show {
        /// Card name
        name: Vec<String>,
    },
}
