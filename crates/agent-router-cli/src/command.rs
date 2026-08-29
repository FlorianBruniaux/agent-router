use std::path::PathBuf;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Commands {
    Route {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Explain {
        #[arg(long)]
        decision: PathBuf,
    },
}
