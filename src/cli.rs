use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "mtg-collection", about = "Manage your MTG card collection")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    /// Include digital-only (Arena/MTGO) cards in searches. Defaults to paper only.
    #[arg(long, global = true)]
    pub all_games: bool,
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
    /// (pass options before the query, e.g. `add -n 4 Lightning Bolt`)
    Add {
        /// Scryfall search query to find the card
        #[arg(trailing_var_arg = true)]
        query: Vec<String>,
        /// Quantity to add
        #[arg(short = 'n', long)]
        count: Option<u32>,
        /// Finish of the card
        #[arg(short = 'f', long, value_enum)]
        finish: Option<Finish>,
        /// Condition of the card
        #[arg(short = 'c', long)]
        condition: Option<String>,
    },
    /// Start an interactive session to add many cards
    AddMany,
    /// Remove a card from your collection
    Remove {
        /// Card name to remove
        name: Vec<String>,
    },
    /// List all cards in your collection
    List,
    /// Show details of a card in your collection (fuzzy search)
    Show {
        /// Card name or partial name
        name: Vec<String>,
    },
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
pub enum Finish {
    Nonfoil,
    Foil,
}

impl std::fmt::Display for Finish {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finish::Nonfoil => write!(f, "nonfoil"),
            Finish::Foil => write!(f, "foil"),
        }
    }
}
