mod directories;
mod error;
mod process_rpc;
mod util;

pub use error::{Error, Result};

use directories::Directory;
use process_rpc::OutputFileContent;

static ROOT_DIR: &str = "/home/eran/code/early_medical";
static TYPES_DIR: &str = "/home/eran/code/early_medical/frontend/src/lib/types";
static CLIENT_DIR: &str = "/home/eran/code/early_medical/frontend/src/lib/api/client";

fn main() -> Result<()> {
    run_typeshare();
    let starting_dir = Directory::new(ROOT_DIR)?;

    let output_file_content = OutputFileContent::new(&starting_dir)?;

    // println!("{output_file_content:#?}");

    output_file_content.write_to_file(CLIENT_DIR, TYPES_DIR);

    Ok(())
}
use std::process::Command;

fn run_typeshare() {
    println!("Running typeshare!");
    let status = Command::new("typeshare")
        .args(&[
            "--lang",
            "typescript",
            "--output-file",
            format!("{TYPES_DIR}/bindings.ts").as_str(),
            format!("{ROOT_DIR}").as_str(),
        ])
        .status()
        .expect("Failed to execute command");

    if !status.success() {
        panic!("Command executed with failing error code");
    }
}
