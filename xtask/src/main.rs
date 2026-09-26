use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::PathBuf;
use xtask::{Error, Result, hook, projection, transaction};

#[derive(Parser)]
#[command(about = "Project development harness. Never executes Agent Router.")]
struct Args {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Generate,
    Check,
    Remove,
    Rollback,
    Hook {
        #[arg(value_parser = ["codex", "claude"])]
        host: String,
    },
}

fn execute(args: Args) -> Result<()> {
    let root = args
        .root
        .canonicalize()
        .map_err(|e| Error::Io("resolve project root", e))?;
    match args.command {
        Command::Generate => {
            let count = transaction::apply(&root, projection::render(&root)?)?;
            println!("harness.generate: {count} files changed");
        }
        Command::Check => {
            projection::check(&root)?;
            println!("harness.check: parity verified; runtime activation UNKNOWN");
        }
        Command::Remove => {
            let count = transaction::apply(&root, projection::Files::new())?;
            println!("harness.remove: {count} owned files removed");
        }
        Command::Rollback => {
            let count = transaction::rollback(&root)?;
            println!("harness.rollback: {count} preimages restored");
        }
        Command::Hook { .. } => return Err(Error::Invalid("hook dispatch")),
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    // Hooks are advisory and never return a host blocking exit code.
    if let Command::Hook { ref host } = args.command {
        let mut bytes = Vec::new();
        if std::io::stdin()
            .take(hook::MAX_INPUT + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            bytes.clear();
        }
        if let Some(output) = hook::run(&args.root, host, &bytes) {
            if let Ok(text) = serde_json::to_string(&output) {
                println!("{text}");
            }
        }
        return std::process::ExitCode::SUCCESS;
    }
    match execute(args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
