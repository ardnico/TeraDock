use clap::{CommandFactory, Parser};

#[derive(Debug, Parser)]
#[command(author, version, about = "TeraDock CLI (skeleton)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Parser)]
enum Commands {}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => {
            // No subcommands yet; show help for discoverability.
            Cli::command().print_help().unwrap();
            println!();
        }
    }
}
