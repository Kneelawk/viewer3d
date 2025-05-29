use clap::Parser;
use std::path::PathBuf;

/// Viewer for various 3d file formats
#[derive(Parser)]
struct Cli {
    /// The 3d file to view
    file: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let args = viewer3d::StartupArgs {
        file: cli.file,
    };

    viewer3d::run_impl(args)
}
