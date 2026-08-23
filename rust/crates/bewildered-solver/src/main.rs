//! Bewildered solver — headless CLI for level validation and solving.

use anyhow::Result;
use bewildered_solver::{validate_level_file, validate_pack};
use clap::{Parser, Subcommand};
use serde_json::to_string_pretty;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "bewildered-solver",
    version,
    about = "Level validation and solving CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output results as JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Check a single level for solvability
    Check {
        /// Path to the level RON file
        #[arg(value_name = "LEVEL_FILE")]
        level_file: PathBuf,
    },
    /// Check an entire pack of levels
    CheckPack {
        /// Path to the pack directory
        #[arg(value_name = "PACK_DIR")]
        pack_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut all_results = Vec::new();
    let mut overall_passed = true;

    match cli.command {
        Commands::Check { level_file } => {
            let result = validate_level_file(&level_file)?;
            overall_passed &= result.passed;
            all_results.push(result);
        }
        Commands::CheckPack { pack_dir } => {
            let results = validate_pack(&pack_dir)?;
            overall_passed &= results.iter().all(|r| r.passed);
            all_results.extend(results);
        }
    }

    if cli.json {
        let output = if all_results.len() == 1 {
            to_string_pretty(&all_results[0])?
        } else {
            to_string_pretty(&all_results)?
        };
        println!("{}", output);
    } else if all_results.len() == 1 {
        // Single level - human readable already printed in check_level
    } else {
        // Pack summary
        let passed = all_results.iter().filter(|r| r.passed).count();
        println!("\n=== Pack Summary ===");
        println!("Levels checked: {}", all_results.len());
        println!("Passed: {}", passed);
        println!("Failed: {}", all_results.len() - passed);
    }

    if !overall_passed {
        std::process::exit(1);
    }

    Ok(())
}
