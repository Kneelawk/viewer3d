#[macro_use]
extern crate tracing;

use clap::Parser;
use std::path::PathBuf;

/// Viewer for various 3d file formats
#[derive(Parser)]
struct Cli {
    /// The 3d file to view
    file: PathBuf,
}

fn main() {
    tracing_subscriber::fmt::fmt().try_init().unwrap();

    let Cli { file } = Cli::parse();

    info!("Loading {:?}...", &file);
}
