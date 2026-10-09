//! Minimal IDL preprocessor: expands `#include`/`import` directives.
//!
//! OMG IDL 4.1 §7.2.5 specifies that IDL is preprocessed per ISO/IEC 14882:2003
//! (C++03), so both `#include "..."` and `#include <...>` exist; CycloneDDS `idlc`
//! delegates to the `mcpp` C preprocessor, which supports both. This module implements
//! the include subset the built-in parser needs. It does not expand macros.

use std::fs;
use std::path::{Path, PathBuf};

/// The result of expanding a file's `#include`/`import` directives.
#[derive(Debug, Clone)]
pub struct Preprocessed {
    /// Combined source with includes inlined and directives removed.
    pub source: String,
    /// Every file read (root first, then includes in read order).
    pub files: Vec<PathBuf>,
}

/// Expand `#include`/`import` directives in `root`, resolving against `include_dirs`.
///
/// Quoted includes (`"file"`) resolve against the including file's directory first, then
/// `include_dirs`; angle includes (`<file>`) resolve against `include_dirs` only.
pub fn preprocess(root: &Path, include_dirs: &[PathBuf]) -> Result<Preprocessed, String> {
    let mut files = Vec::new();
    let mut stack = Vec::new();
    let source = expand(root, include_dirs, &mut stack, &mut files)?;
    Ok(Preprocessed { source, files })
}

/// A parsed include/import target.
struct IncludeSpec {
    name: String,
    quoted: bool,
}

fn expand(
    path: &Path,
    include_dirs: &[PathBuf],
    stack: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<String, String> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if stack.iter().any(|seen| seen == &canonical) {
        let mut chain: Vec<String> = stack.iter().map(|p| p.display().to_string()).collect();
        chain.push(canonical.display().to_string());
        return Err(format!("include cycle: {}", chain.join(" -> ")));
    }

    let text = fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

    stack.push(canonical);
    files.push(path.to_path_buf());

    let including_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(spec) = parse_include_line(trimmed).or_else(|| parse_import_line(trimmed)) {
            let resolved = resolve(&spec, including_dir, include_dirs)?;
            out.push_str(&expand(&resolved, include_dirs, stack, files)?);
            out.push('\n');
        } else if trimmed.starts_with('#') {
            // Other directives (#pragma, ...) are dropped.
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    stack.pop();
    Ok(out)
}

fn resolve(
    spec: &IncludeSpec,
    including_dir: &Path,
    include_dirs: &[PathBuf],
) -> Result<PathBuf, String> {
    if spec.quoted {
        let candidate = including_dir.join(&spec.name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    for dir in include_dirs {
        let candidate = dir.join(&spec.name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("included file not found: {}", spec.name))
}

fn parse_include_line(line: &str) -> Option<IncludeSpec> {
    let rest = line.strip_prefix("#include")?;
    if !rest.starts_with(|c: char| c.is_whitespace() || c == '"' || c == '<') {
        return None;
    }
    parse_spec(rest)
}

fn parse_import_line(line: &str) -> Option<IncludeSpec> {
    let rest = line.strip_prefix("import")?;
    if !rest.starts_with(|c: char| c.is_whitespace() || c == '"' || c == '<') {
        return None;
    }
    parse_spec(rest)
}

fn parse_spec(rest: &str) -> Option<IncludeSpec> {
    let rest = rest.trim_start();
    if let Some(inner) = rest.strip_prefix('"') {
        let name = inner.split('"').next()?;
        return Some(IncludeSpec {
            name: name.to_string(),
            quoted: true,
        });
    }
    if let Some(inner) = rest.strip_prefix('<') {
        let name = inner.split('>').next()?;
        return Some(IncludeSpec {
            name: name.to_string(),
            quoted: false,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn includes_same_directory() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("b.idl"), "struct B { long y; };\n");
        let a = dir.path().join("a.idl");
        write(&a, "#include \"b.idl\"\nstruct A { long x; };\n");

        let pre = preprocess(&a, &[]).unwrap();
        assert!(pre.source.contains("struct A"));
        assert!(pre.source.contains("struct B"));
        assert_eq!(pre.files.len(), 2);
    }

    #[test]
    fn includes_via_include_dir() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("inc");
        write(&inc.join("b.idl"), "struct B { long y; };\n");
        let a = dir.path().join("a.idl");
        write(&a, "#include <b.idl>\nstruct A { long x; };\n");

        let pre = preprocess(&a, &[inc]).unwrap();
        assert!(pre.source.contains("struct B"));
    }

    #[test]
    fn quoted_prefers_including_dir() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("inc");
        write(&inc.join("b.idl"), "struct FromInclude { long y; };\n");
        write(
            &dir.path().join("b.idl"),
            "struct FromSibling { long z; };\n",
        );
        let a = dir.path().join("a.idl");
        write(&a, "#include \"b.idl\"\nstruct A { long x; };\n");

        let pre = preprocess(&a, &[inc]).unwrap();
        assert!(pre.source.contains("FromSibling"));
        assert!(!pre.source.contains("FromInclude"));
    }

    #[test]
    fn cycle_is_error() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("a.idl"),
            "#include \"b.idl\"\nstruct A { long x; };\n",
        );
        write(
            &dir.path().join("b.idl"),
            "#include \"a.idl\"\nstruct B { long y; };\n",
        );

        let err = preprocess(&dir.path().join("a.idl"), &[]).unwrap_err();
        assert!(err.contains("cycle"), "{err}");
    }

    #[test]
    fn import_is_inlined() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("b.idl"), "struct B { long y; };\n");
        let a = dir.path().join("a.idl");
        write(&a, "import \"b.idl\";\nstruct A { long x; };\n");

        let pre = preprocess(&a, &[]).unwrap();
        assert!(pre.source.contains("struct B"));
    }

    #[test]
    fn files_lists_every_file_read() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("b.idl"), "struct B { long y; };\n");
        let a = dir.path().join("a.idl");
        write(&a, "#include \"b.idl\"\n");

        let pre = preprocess(&a, &[]).unwrap();
        assert!(pre.files.iter().any(|p| p.ends_with("a.idl")));
        assert!(pre.files.iter().any(|p| p.ends_with("b.idl")));
    }
}
