//! Generates Rust types from the IDL fixtures under `tests/idl/codegen/`.
//!
//! Each `*.idl` there is compiled with `cyclonedds-build` into `$OUT_DIR/gen/<stem>.rs` and
//! `include!`d by the matching integration test. `tests/idl/ops_reference.idl` is not here:
//! it is a hand-transcribed reference for the ops differential tests, not a codegen fixture.

use std::fs;
use std::path::PathBuf;

use cyclonedds_build::{compile_idl_with_options, CompileOptions};

const CODEGEN_DIR: &str = "tests/idl/codegen";

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set")).join("gen");
    fs::create_dir_all(&out_dir).expect("create generated output directory");
    println!("cargo:rerun-if-changed={}", CODEGEN_DIR);

    let mut idl_files: Vec<PathBuf> = fs::read_dir(CODEGEN_DIR)
        .unwrap_or_else(|e| panic!("failed to read {CODEGEN_DIR}: {e}"))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("idl"))
        .collect();
    idl_files.sort();

    for idl in idl_files {
        println!("cargo:rerun-if-changed={}", idl.display());
        let stem = idl
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("IDL file has a UTF-8 stem")
            .to_string();
        let options = CompileOptions {
            output_dir: Some(out_dir.clone()),
            try_idlc: false,
            module_name: Some(stem),
            ..CompileOptions::default()
        };
        compile_idl_with_options(&idl, &options)
            .unwrap_or_else(|e| panic!("codegen failed for {}: {e}", idl.display()));
    }
}
