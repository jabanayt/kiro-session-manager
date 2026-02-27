use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    env_logger::init();
    let cli = ksm::cli::Cli::parse();
    ksm::cli::run(cli)?;
    Ok(())
}
