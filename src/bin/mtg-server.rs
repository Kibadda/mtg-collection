use clap::Parser;
use mtg_collection::server::{self, ServerArgs};

#[tokio::main]
async fn main() {
    let args = ServerArgs::parse();
    if let Err(e) = server::serve(args).await {
        eprintln!("mtg-server error: {e}");
        std::process::exit(1);
    }
}
