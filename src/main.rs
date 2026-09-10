use clap::Parser;
use mtg_collection::cli::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    mtg_collection::run(cli).await;
}
