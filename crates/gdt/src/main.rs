use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use godot_dev_tools::{Dirs, EngineFeatures};

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
    /// Show where gdt keeps its files
    Paths,
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
        Command::Paths => {
            let dirs = Dirs::detect()?;
            println!("data:     {}", dirs.data_dir().display());
            println!("config:   {}", dirs.config_file().display());
            println!("versions: {}", dirs.versions_dir().display());
            println!("cache:    {}", dirs.cache_dir().display());
            println!("bin:      {}", dirs.bin_dir().display());
        }
    }
    Ok(())
}
