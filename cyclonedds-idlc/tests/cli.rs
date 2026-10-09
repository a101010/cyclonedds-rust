// End-to-end check that `--include-dir` resolves an `#include` that is not next to the input.

use std::fs;
use std::process::Command;

#[test]
fn include_dir_flag_compiles_an_idl_with_includes() {
    let dir = tempfile::tempdir().unwrap();
    let inc = dir.path().join("inc");
    fs::create_dir_all(&inc).unwrap();

    let main_idl = dir.path().join("main.idl");
    fs::write(&main_idl, "#include <dep.idl>\nstruct Main { long a; };\n").unwrap();
    fs::write(inc.join("dep.idl"), "struct Dep { long b; };\n").unwrap();

    let out_dir = dir.path().join("out");
    let status = Command::new(env!("CARGO_BIN_EXE_cyclonedds-idlc"))
        .arg("--input")
        .arg(&main_idl)
        .arg("--output-dir")
        .arg(&out_dir)
        .arg("--include-dir")
        .arg(&inc)
        .arg("--no-idlc")
        .status()
        .expect("run cyclonedds-idlc");

    assert!(status.success(), "cyclonedds-idlc exited with {status}");

    let generated = fs::read_to_string(out_dir.join("main.rs")).unwrap();
    assert!(generated.contains("pub struct Main"), "{generated}");
    assert!(generated.contains("pub struct Dep"), "{generated}");
}
