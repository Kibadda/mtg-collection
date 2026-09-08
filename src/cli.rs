use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "mtg-collection", about = "Manage your MTG card collection")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    /// Include digital-only (Arena/MTGO) cards in searches. Defaults to paper only.
    #[arg(long, global = true)]
    pub all_games: bool,
    /// Include promo printings (e.g. from promo sets) in searches. Default excludes them.
    #[arg(long, global = true)]
    pub all_promos: bool,
}

#[derive(Debug, Subcommand)]
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
        /// Force a specific set code (e.g. 3ed), skipping the set picker
        #[arg(long)]
        set: Option<String>,
    },
    /// Start an interactive session to add many cards
    AddMany {
        /// Force a specific set code (e.g. 3ed) for the whole session, skipping the set picker
        #[arg(long)]
        set: Option<String>,
    },
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

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("mtg-collection").chain(args.iter().copied()))
            .unwrap_or_else(|e| panic!("failed to parse {args:?}: {e}"))
    }

    #[test]
    fn add_parses_query_and_flags() {
        let Cli {
            command,
            all_games,
            all_promos,
        } = parse(&[
            "add",
            "--set",
            "3ed",
            "-n",
            "2",
            "-f",
            "foil",
            "-c",
            "LP",
            "Counterspell",
            "set:3ed",
        ]);
        assert!(!all_games);
        assert!(!all_promos);
        match command {
            Commands::Add {
                query,
                count,
                finish,
                condition,
                set,
            } => {
                assert_eq!(query, ["Counterspell", "set:3ed"]);
                assert_eq!(count, Some(2));
                assert_eq!(finish, Some(Finish::Foil));
                assert_eq!(condition.as_deref(), Some("LP"));
                assert_eq!(set.as_deref(), Some("3ed"));
            }
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn add_defaults() {
        let Cli { command, .. } = parse(&["add", "Lightning", "Bolt"]);
        match command {
            Commands::Add {
                query,
                count,
                finish,
                condition,
                set,
            } => {
                assert_eq!(query, ["Lightning", "Bolt"]);
                assert_eq!(count, None);
                assert_eq!(finish, None);
                assert_eq!(condition, None);
                assert_eq!(set, None);
            }
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn add_many_parses_set() {
        let Cli { command, .. } = parse(&["add-many", "--set", "hoc"]);
        match command {
            Commands::AddMany { set } => assert_eq!(set.as_deref(), Some("hoc")),
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn global_flags_work_after_subcommand() {
        let Cli {
            command,
            all_games,
            all_promos,
        } = parse(&["search", "--all-games", "--all-promos", "Sol", "Ring"]);
        assert!(all_games);
        assert!(all_promos);
        match command {
            Commands::Search { query } => assert_eq!(query, ["Sol", "Ring"]),
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn finish_display() {
        assert_eq!(Finish::Foil.to_string(), "foil");
        assert_eq!(Finish::Nonfoil.to_string(), "nonfoil");
    }
}
