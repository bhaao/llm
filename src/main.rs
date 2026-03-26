use block_chain_with_context::cli::{Cli, CliExecutor, OutputFormat};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    
    let format = match cli.format.as_str() {
        "json" => OutputFormat::Json,
        _ => OutputFormat::Text,
    };
    
    let executor = CliExecutor::new(format, cli.verbose);
    
    if let Err(e) = executor.execute(&cli.command).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
