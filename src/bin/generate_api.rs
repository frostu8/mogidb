use std::{fs, path::PathBuf};

use clap::Parser;
use utoipa::OpenApi as _;

#[derive(Default, Parser)]
struct Args {
    /// The file to output the generated spec to.
    #[arg(short, long)]
    pub output_file: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    let output_file = args
        .output_file
        .unwrap_or_else(|| PathBuf::from("./openapi/openapi.yaml"));

    // Make sure parent directory exists
    if let Some(dir) = output_file.parent() {
        fs::create_dir_all(dir).expect("make output directory");
    }

    // Generate doc
    let doc = mogidb::docs::ApiDoc::openapi();
    let yaml = serde_norway::to_string(&doc).expect("generate yaml");
    fs::write(output_file, &yaml).expect("commit openapi to file");
}
