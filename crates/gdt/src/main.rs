use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use godot_dev_tools::EngineFeatures;

/// Godot Development Tools: version manager, addon manager and test runner for Godot.
#[derive(Parser)]
#[command(name = "gdt", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the engine requirements recorded in a project.godot
    Inspect {
        /// Path to a project.godot file
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Inspect { path } => {
            let features = EngineFeatures::from_file(&path)?;
            match features.engine {
                Some((major, minor)) => println!("engine: {major}.{minor}"),
                None => println!("engine: not recorded"),
            }
            println!("dotnet: {}", features.dotnet);
        }
    }
    Ok(())
}
