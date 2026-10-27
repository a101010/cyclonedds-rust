//! Code generation: converts parsed IDL type definitions to Rust source code.
//!
//! The generated code uses the derive macros from `cyclonedds-derive`:
//! - `DdsType` for structs
//! - `DdsEnum` for enums
//! - `DdsUnion` for unions
//! - `DdsBitmask` for bitmasks

use std::collections::{HashMap, HashSet};

use crate::idl_parser::{
    Definition, IdlBitmask, IdlEnum, IdlFile, IdlModule, IdlStruct, IdlType, IdlTypeRef, IdlUnion,
    PrimitiveType,
};
use crate::CompileOptions;

/// Outer allow applied to each generated item so the file can be `include!`d.
///
/// A crate-level `#![allow(...)]` cannot be `include!`d into a module, so the
/// allow is attached to each item instead.
const ITEM_ALLOW: &str = "#[allow(dead_code, non_camel_case_types, non_snake_case, clippy::deref_addrof, clippy::needless_borrow)]\n";

/// A resolved IDL type: the snake_case Rust module path plus the type name.
struct TargetInfo {
    modules: Vec<String>,
    name: String,
}

/// A typedef definition with the scope it was written in.
struct TypedefInfo {
    scope: Vec<String>,
    target: IdlTypeRef,
}

/// Every type defined in an IDL file, keyed by its fully-qualified IDL name.
struct SymbolTable {
    types: HashMap<String, TargetInfo>,
    typedefs: HashMap<String, TypedefInfo>,
}

impl SymbolTable {
    fn from_file(file: &IdlFile) -> Self {
        let mut types = HashMap::new();
        let mut typedefs = HashMap::new();
        for scoped in file.scoped_types() {
            let modules = scoped.scope.iter().map(|s| to_snake_case(s)).collect();
            let name = scoped.ty.name().to_string();
            types.insert(scoped.qualified_name(), TargetInfo { modules, name });
            if let IdlType::Typedef(td) = scoped.ty {
                typedefs.insert(
                    scoped.qualified_name(),
                    TypedefInfo {
                        scope: scoped.scope.iter().map(|s| s.to_string()).collect(),
                        target: td.ty.clone(),
                    },
                );
            }
        }
        SymbolTable { types, typedefs }
    }

    fn lookup(&self, fq: &str) -> Option<&TargetInfo> {
        self.types.get(fq)
    }

    /// Resolve a named reference that may be a typedef.
    ///
    /// A typedef whose final target is a generated type is transparent (the
    /// alias and the target are the same Rust type), so the caller can still
    /// use the target's `DdsFieldMetadata` impl. A typedef to a primitive,
    /// string, sequence, or array has no impl of its own and must be inlined.
    fn resolve_named_ref<'a>(&'a self, ref_name: &str, scope: &[&str]) -> NamedRef<'a> {
        let original_rust_path = resolve_named(ref_name, scope, self);
        let mut name = ref_name.to_string();
        let mut scope_owned: Vec<String> = scope.iter().map(|s| s.to_string()).collect();
        let mut seen = HashSet::new();

        loop {
            if !seen.insert((scope_owned.join("::"), name.clone())) {
                return NamedRef::Generated(original_rust_path);
            }
            let scope_refs: Vec<&str> = scope_owned.iter().map(String::as_str).collect();
            let Some(info) = lookup_typedef(self, &name, &scope_refs) else {
                if name == ref_name {
                    return NamedRef::Generated(original_rust_path);
                }
                let path = resolve_fq(&name, &scope_refs, self)
                    .and_then(|fq| self.lookup(&fq))
                    .map(|target| rust_path(scope, target))
                    .unwrap_or_else(|| {
                        name.rsplit("::")
                            .next()
                            .unwrap_or(name.as_str())
                            .to_string()
                    });
                return NamedRef::Generated(path);
            };
            match &info.target {
                IdlTypeRef::Named(next) => {
                    name = next.clone();
                    scope_owned = info.scope.clone();
                }
                target => {
                    return NamedRef::Inline {
                        ty: target,
                        scope: info.scope.clone(),
                    }
                }
            }
        }
    }
}

/// A named type reference after typedef resolution.
enum NamedRef<'a> {
    /// A generated type (or an alias to one); use its `DdsFieldMetadata` impl.
    Generated(String),
    /// A typedef to a builtin/composite shape with no impl; inline this target.
    Inline {
        ty: &'a IdlTypeRef,
        scope: Vec<String>,
    },
}

fn lookup_typedef<'a>(
    symbols: &'a SymbolTable,
    ref_name: &str,
    scope: &[&str],
) -> Option<&'a TypedefInfo> {
    candidates(ref_name, scope)
        .into_iter()
        .find_map(|candidate| symbols.typedefs.get(&candidate))
}

/// Generate a complete Rust source file from a parsed IDL file.
pub fn generate_rust(idl_file: &IdlFile, module_name: &str) -> String {
    generate_rust_with_options(idl_file, module_name, &CompileOptions::default())
}

/// Generate a complete Rust source file with custom options.
pub fn generate_rust_with_options(
    idl_file: &IdlFile,
    module_name: &str,
    options: &CompileOptions,
) -> String {
    let mut output = String::new();
    let symbols = SymbolTable::from_file(idl_file);

    output.push_str("// @generated by cyclonedds-build. DO NOT EDIT.\n");
    output.push_str("// Source IDL module: ");
    output.push_str(module_name);
    output.push_str("\n\n");
    output.push_str("#[allow(unused_imports)]\n");
    output.push_str(
        "use cyclonedds::{DdsTypeDerive, DdsEnumDerive, DdsUnionDerive, DdsBitmaskDerive};\n",
    );
    output.push_str("#[allow(unused_imports)]\n");
    output.push_str("use cyclonedds::{DdsSequence, DdsBoundedSequence, DdsString};\n\n");

    // Generate top-level definitions (types and nested modules)
    for def in &idl_file.definitions {
        output.push_str(&generate_definition(def, &[], &symbols, options));
        output.push('\n');
    }

    output
}

/// Generate Rust code for a definition: a type or a nested module.
fn generate_definition(
    def: &Definition,
    scope: &[&str],
    symbols: &SymbolTable,
    options: &CompileOptions,
) -> String {
    match def {
        Definition::Type(ty) => generate_type(ty, scope, symbols, options),
        Definition::Module(m) => generate_module(m, scope, symbols, options),
    }
}

/// Generate a Rust module wrapping a set of definitions.
fn generate_module(
    m: &IdlModule,
    scope: &[&str],
    symbols: &SymbolTable,
    options: &CompileOptions,
) -> String {
    let mut output = String::new();
    output.push_str(ITEM_ALLOW);
    output.push_str("pub mod ");
    output.push_str(&to_snake_case(&m.name));
    output.push_str(" {\n");
    output.push_str("    use super::*;\n\n");

    let mut child_scope: Vec<&str> = scope.to_vec();
    child_scope.push(m.name.as_str());

    for child in &m.definitions {
        output.push_str(&indent(
            &generate_definition(child, &child_scope, symbols, options),
            4,
        ));
        output.push('\n');
    }

    output.push_str("}\n");
    output
}

/// Indent every non-empty line of `code` by `spaces` spaces.
fn indent(code: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    let mut out = String::new();
    for line in code.lines() {
        if !line.is_empty() {
            out.push_str(&pad);
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Generate Rust code for a single IDL type definition.
fn generate_type(
    ty: &IdlType,
    scope: &[&str],
    symbols: &SymbolTable,
    options: &CompileOptions,
) -> String {
    match ty {
        IdlType::Struct(s) => generate_struct(s, scope, symbols, options),
        IdlType::Enum(e) => generate_enum(e, scope),
        IdlType::Union(u) => generate_union(u, scope, symbols),
        IdlType::Bitmask(b) => generate_bitmask(b, scope),
        IdlType::Typedef(td) => generate_typedef(td, scope, symbols),
    }
}

/// Join a module scope and a type name into a fully-qualified IDL name.
fn qualified_name(scope: &[&str], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", scope.join("::"), name)
    }
}

/// Generate a Rust struct with `#[derive(DdsType)]`.
fn generate_struct(
    s: &IdlStruct,
    scope: &[&str],
    symbols: &SymbolTable,
    options: &CompileOptions,
) -> String {
    let mut output = String::new();

    output.push_str(ITEM_ALLOW);
    output.push_str("#[derive(Debug, Clone, Default, PartialEq, DdsTypeDerive)]\n");
    if options.emit_dds_typename {
        output.push_str("#[dds_typename(\"");
        output.push_str(&qualified_name(scope, &s.name));
        output.push_str("\")]\n");
    }
    output.push_str("pub struct ");
    output.push_str(&s.name);
    output.push_str(" {\n");

    for field in &s.fields {
        // Generate field annotations
        for ann in &field.annotations {
            match ann.name.as_str() {
                "key" => output.push_str("    #[key]\n"),
                "position" | "id" | "hash_id" => {
                    // These are IDL-level annotations; the derive macro handles them
                    // via the op descriptor layout. No Rust attribute needed for now.
                }
                _ => {}
            }
        }

        let is_optional = field.annotations.iter().any(|a| a.name == "optional");
        let mut rust_ty = type_ref_to_rust(&field.ty, scope, symbols);
        if is_optional {
            rust_ty = format!("Option<{}>", rust_ty);
        }

        output.push_str("    pub ");
        output.push_str(&field.name);
        output.push_str(": ");
        output.push_str(&rust_ty);
        output.push_str(",\n");
    }

    output.push_str("}\n");
    output.push_str(&field_metadata_impl_struct(s, scope, symbols));
    output
}

/// Generate a Rust enum with `#[derive(DdsEnum)]`.
fn generate_enum(e: &IdlEnum, scope: &[&str]) -> String {
    let mut output = String::new();

    output.push_str(ITEM_ALLOW);
    output.push_str("#[repr(i32)]\n");
    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, DdsEnumDerive)]\n");
    output.push_str("pub enum ");
    output.push_str(&e.name);
    output.push_str(" {\n");

    for variant in &e.variants {
        output.push_str("    ");
        output.push_str(&variant.name);
        if let Some(val) = variant.value {
            output.push_str(" = ");
            output.push_str(&val.to_string());
        }
        output.push_str(",\n");
    }

    output.push_str("}\n");
    output.push_str(&field_metadata_impl_enum(e, scope));
    output
}

/// Generate a Rust enum with `#[derive(DdsUnion)]`.
fn generate_union(u: &IdlUnion, scope: &[&str], symbols: &SymbolTable) -> String {
    let mut output = String::new();

    output.push_str(ITEM_ALLOW);
    output.push_str("#[derive(Debug, Clone, DdsUnionDerive)]\n");
    output.push_str("#[dds_discriminant(");
    output.push_str(&type_ref_to_rust(&u.discriminant_type, scope, symbols));
    output.push_str(")]\n");
    output.push_str("pub enum ");
    output.push_str(&u.name);
    output.push_str(" {\n");

    for case in &u.cases {
        for &label in &case.label_values {
            output.push_str("    #[dds_case(");
            output.push_str(&label.to_string());
            output.push_str(")]\n");
        }
        output.push_str("    ");
        output.push_str(&to_pascal_case(&case.name));
        output.push('(');
        output.push_str(&type_ref_to_rust(&case.ty, scope, symbols));
        output.push_str("),\n");
    }

    if let Some(default) = &u.default_case {
        output.push_str("    #[dds_default]\n");
        output.push_str("    ");
        output.push_str(&to_pascal_case(&default.name));
        output.push('(');
        output.push_str(&type_ref_to_rust(&default.ty, scope, symbols));
        output.push_str("),\n");
    }

    output.push_str("}\n");
    output.push_str(&field_metadata_impl_union(u, scope, symbols));
    output
}

/// Generate a Rust struct with `#[derive(DdsBitmask)]`.
fn generate_bitmask(b: &IdlBitmask, scope: &[&str]) -> String {
    let mut output = String::new();

    output.push_str(ITEM_ALLOW);
    output.push_str("#[derive(Debug, Clone, DdsBitmaskDerive)]\n");
    output.push_str("#[bit_bound(");
    output.push_str(&b.bit_bound.to_string());
    output.push_str(")]\n");
    output.push_str("pub struct ");
    output.push_str(&b.name);
    output.push_str(" {\n");

    for flag in &b.flags {
        output.push_str("    pub ");
        output.push_str(flag);
        output.push_str(": bool,\n");
    }

    output.push_str("}\n");
    output.push_str(&field_metadata_impl_bitmask(b, scope));
    output
}

/// Generate a Rust type alias for a typedef.
fn generate_typedef(
    td: &crate::idl_parser::IdlTypedef,
    scope: &[&str],
    symbols: &SymbolTable,
) -> String {
    let mut output = String::new();
    output.push_str(ITEM_ALLOW);
    output.push_str("pub type ");
    output.push_str(&td.name);
    output.push_str(" = ");
    output.push_str(&type_ref_to_rust(&td.ty, scope, symbols));
    output.push_str(";\n");
    output
}

/// Resolve an IDL named reference to a Rust path relative to `scope`.
///
/// Tries the reference as an absolute (leading `::`) or scope-relative name against the
/// symbol table; falls back to the last segment for names defined outside the file.
fn resolve_named(ref_name: &str, scope: &[&str], symbols: &SymbolTable) -> String {
    let clean = ref_name.trim_start_matches("::");

    for candidate in candidates(ref_name, scope) {
        if let Some(target) = symbols.lookup(&candidate) {
            return rust_path(scope, target);
        }
    }

    clean.rsplit("::").next().unwrap_or(clean).to_string()
}

/// The fully-qualified IDL name a reference resolves to, if it is defined here.
fn resolve_fq(ref_name: &str, scope: &[&str], symbols: &SymbolTable) -> Option<String> {
    candidates(ref_name, scope)
        .into_iter()
        .find(|candidate| symbols.lookup(candidate).is_some())
}

/// Candidate fully-qualified IDL names for a reference, innermost scope first.
fn candidates(ref_name: &str, scope: &[&str]) -> Vec<String> {
    let absolute = ref_name.starts_with("::");
    let clean = ref_name.trim_start_matches("::");

    if absolute {
        return vec![clean.to_string()];
    }

    (0..=scope.len())
        .rev()
        .map(|i| {
            if i == 0 {
                clean.to_string()
            } else {
                format!("{}::{}", scope[..i].join("::"), clean)
            }
        })
        .collect()
}

/// The Rust path to `target` as seen from `scope`, using `super::` for enclosing modules.
fn rust_path(scope: &[&str], target: &TargetInfo) -> String {
    let mut common = 0;
    while common < scope.len()
        && common < target.modules.len()
        && to_snake_case(scope[common]) == target.modules[common]
    {
        common += 1;
    }

    let mut parts: Vec<String> = Vec::new();
    for _ in common..scope.len() {
        parts.push("super".to_string());
    }
    for module in &target.modules[common..] {
        parts.push(module.clone());
    }
    parts.push(target.name.clone());
    parts.join("::")
}

/// Map an IDL type reference to a Rust type string.
fn type_ref_to_rust(ty: &IdlTypeRef, scope: &[&str], symbols: &SymbolTable) -> String {
    match ty {
        IdlTypeRef::Primitive(p) => primitive_to_rust(*p).to_string(),
        IdlTypeRef::String { bound: None } => "String".to_string(),
        IdlTypeRef::String { bound: Some(_n) } => {
            // Bounded strings use DdsString in the current derive macro
            "DdsString".to_string()
        }
        IdlTypeRef::Sequence {
            element_type,
            bound: None,
        } => {
            format!(
                "DdsSequence<{}>",
                type_ref_to_rust(element_type, scope, symbols)
            )
        }
        IdlTypeRef::Sequence {
            element_type,
            bound: Some(max),
        } => {
            format!(
                "DdsBoundedSequence<{}, {}>",
                type_ref_to_rust(element_type, scope, symbols),
                max
            )
        }
        IdlTypeRef::Array { element_type, size } => {
            format!(
                "[{}; {}]",
                type_ref_to_rust(element_type, scope, symbols),
                size
            )
        }
        IdlTypeRef::Named(name) => resolve_named(name, scope, symbols),
    }
}

/// Map an IDL primitive type to a Rust type.
fn primitive_to_rust(p: PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::Boolean => "bool",
        PrimitiveType::Octet => "u8",
        PrimitiveType::Char => "u8",
        PrimitiveType::Wchar => "u16",
        PrimitiveType::Short => "i16",
        PrimitiveType::UnsignedShort => "u16",
        PrimitiveType::Long => "i32",
        PrimitiveType::UnsignedLong => "u32",
        PrimitiveType::LongLong => "i64",
        PrimitiveType::UnsignedLongLong => "u64",
        PrimitiveType::Float => "f32",
        PrimitiveType::Double => "f64",
        PrimitiveType::LongDouble => "f64", // Rust has no f128; approximate with f64
    }
}

/// The `DynamicPrimitiveKind` variant for an IDL primitive.
fn primitive_schema_kind(p: PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::Boolean => "Boolean",
        PrimitiveType::Octet => "UInt8",
        PrimitiveType::Char => "Char8",
        PrimitiveType::Wchar => "Char16",
        PrimitiveType::Short => "Int16",
        PrimitiveType::UnsignedShort => "UInt16",
        PrimitiveType::Long => "Int32",
        PrimitiveType::UnsignedLong => "UInt32",
        PrimitiveType::LongLong => "Int64",
        PrimitiveType::UnsignedLongLong => "UInt64",
        PrimitiveType::Float => "Float32",
        PrimitiveType::Double => "Float64",
        PrimitiveType::LongDouble => "Float64",
    }
}

/// The `DynamicValue` variant for an IDL primitive.
fn primitive_dynamic_variant(p: PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::Boolean => "Bool",
        PrimitiveType::Octet => "UInt8",
        PrimitiveType::Char => "Char8",
        PrimitiveType::Wchar => "Char16",
        PrimitiveType::Short => "Int16",
        PrimitiveType::UnsignedShort => "UInt16",
        PrimitiveType::Long => "Int32",
        PrimitiveType::UnsignedLong => "UInt32",
        PrimitiveType::LongLong => "Int64",
        PrimitiveType::UnsignedLongLong => "UInt64",
        PrimitiveType::Float => "Float32",
        PrimitiveType::Double => "Float64",
        PrimitiveType::LongDouble => "Float64",
    }
}

fn scope_refs(scope: &[String]) -> Vec<&str> {
    scope.iter().map(String::as_str).collect()
}

/// Emit a `DynamicTypeSchema` construction expression for an IDL type.
fn schema_expr(ty: &IdlTypeRef, scope: &[&str], symbols: &SymbolTable) -> String {
    let mut out = String::new();
    match ty {
        IdlTypeRef::Primitive(p) => {
            out.push_str(
                "cyclonedds::DynamicTypeSchema::Primitive(cyclonedds::DynamicPrimitiveKind::",
            );
            out.push_str(primitive_schema_kind(*p));
            out.push(')');
        }
        IdlTypeRef::String { bound: Some(n) } => {
            out.push_str("cyclonedds::DynamicTypeSchema::String { bound: Some(");
            out.push_str(&n.to_string());
            out.push_str("u32) }");
        }
        IdlTypeRef::String { bound: None } => {
            out.push_str("cyclonedds::DynamicTypeSchema::String { bound: None }");
        }
        IdlTypeRef::Sequence {
            element_type,
            bound,
        } => {
            out.push_str("cyclonedds::DynamicTypeSchema::Sequence { name: ::std::string::String::new(), bound: ");
            match bound {
                Some(n) => {
                    out.push_str("Some(");
                    out.push_str(&n.to_string());
                    out.push_str("u32)");
                }
                None => out.push_str("None"),
            }
            out.push_str(", element: ::std::boxed::Box::new(");
            out.push_str(&schema_expr(element_type, scope, symbols));
            out.push_str(") }");
        }
        IdlTypeRef::Array { element_type, size } => {
            out.push_str("cyclonedds::DynamicTypeSchema::Array { name: ::std::string::String::new(), bounds: vec![");
            out.push_str(&size.to_string());
            out.push_str("u32], element: ::std::boxed::Box::new(");
            out.push_str(&schema_expr(element_type, scope, symbols));
            out.push_str(") }");
        }
        IdlTypeRef::Named(name) => match symbols.resolve_named_ref(name, scope) {
            NamedRef::Generated(path) => {
                out.push('<');
                out.push_str(&path);
                out.push_str(" as cyclonedds::DdsFieldMetadata>::field_schema().clone()");
            }
            NamedRef::Inline { ty, scope } => {
                let scope = scope_refs(&scope);
                out.push_str(&schema_expr(ty, &scope, symbols));
            }
        },
    }
    out
}

/// Emit a `DynamicValue` conversion expression for a borrowed IDL value.
fn to_expr(ty: &IdlTypeRef, value: &str, scope: &[&str], symbols: &SymbolTable) -> String {
    match ty {
        IdlTypeRef::Primitive(p) => format!(
            "cyclonedds::DynamicValue::{}(*({value}))",
            primitive_dynamic_variant(*p)
        ),
        IdlTypeRef::String { bound: None } => {
            format!("cyclonedds::DynamicValue::String(({value}).clone())")
        }
        IdlTypeRef::String { bound: Some(_) } => {
            format!("cyclonedds::DynamicValue::String(({value}).to_string_lossy())")
        }
        IdlTypeRef::Sequence { element_type, .. } => format!(
            "cyclonedds::DynamicValue::Sequence(({value}).iter().map(|__value| {}).collect())",
            to_expr(element_type, "__value", scope, symbols)
        ),
        IdlTypeRef::Array { element_type, .. } => format!(
            "cyclonedds::DynamicValue::Array(({value}).iter().map(|__value| {}).collect())",
            to_expr(element_type, "__value", scope, symbols)
        ),
        IdlTypeRef::Named(name) => match symbols.resolve_named_ref(name, scope) {
            NamedRef::Generated(path) => {
                format!("<{path} as cyclonedds::DdsFieldMetadata>::to_dynamic_value({value})")
            }
            NamedRef::Inline { ty, scope } => {
                let scope = scope_refs(&scope);
                to_expr(ty, value, &scope, symbols)
            }
        },
    }
}

/// Emit an expression of type `DdsResult<T>` that reads an IDL value.
fn from_expr(ty: &IdlTypeRef, value: &str, scope: &[&str], symbols: &SymbolTable) -> String {
    match ty {
        IdlTypeRef::Primitive(p) => {
            let variant = primitive_dynamic_variant(*p);
            format!(
                "match {value} {{ cyclonedds::DynamicValue::{variant}(__item) => Ok(*__item), \
                 __other => Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected \
                 {variant}, got {{:?}}\", __other))) }}"
            )
        }
        IdlTypeRef::String { bound: None } => format!(
            "match {value} {{ cyclonedds::DynamicValue::String(__item) => Ok(__item.clone()), \
             __other => Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected String, \
             got {{:?}}\", __other))) }}"
        ),
        IdlTypeRef::String { bound: Some(_) } => format!(
            "match {value} {{ cyclonedds::DynamicValue::String(__item) => \
             Ok(cyclonedds::DdsString::new(__item)?), __other => \
             Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected String, got \
             {{:?}}\", __other))) }}"
        ),
        IdlTypeRef::Sequence {
            element_type,
            bound,
        } => {
            let mut out = String::new();
            out.push_str("{ let __items = match ");
            out.push_str(value);
            out.push_str(" { cyclonedds::DynamicValue::Sequence(__items) => __items, __other => ");
            out.push_str("return Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected Sequence, got {:?}\", __other))) }; ");
            out.push_str("let __values: cyclonedds::DdsResult<::std::vec::Vec<_>> = __items.iter().map(|__value| ");
            out.push_str(&from_expr(element_type, "__value", scope, symbols));
            out.push_str(").collect(); ");
            let element_rust = type_ref_to_rust(element_type, scope, symbols);
            match bound {
                None => {
                    out.push_str("cyclonedds::DdsSequence::<");
                    out.push_str(&element_rust);
                    out.push_str(">::from_vec(__values?)");
                }
                Some(max) => {
                    out.push_str("cyclonedds::DdsBoundedSequence::<");
                    out.push_str(&element_rust);
                    out.push_str(", ");
                    out.push_str(&max.to_string());
                    out.push_str(">::from_vec(__values?)");
                }
            }
            out.push_str(" }");
            out
        }
        IdlTypeRef::Array { element_type, size } => {
            let mut out = String::new();
            out.push_str("{ let __items = match ");
            out.push_str(value);
            out.push_str(" { cyclonedds::DynamicValue::Array(__items) => __items, __other => ");
            out.push_str("return Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected Array, got {:?}\", __other))) }; ");
            out.push_str("let __values: cyclonedds::DdsResult<::std::vec::Vec<_>> = __items.iter().map(|__value| ");
            out.push_str(&from_expr(element_type, "__value", scope, symbols));
            out.push_str(
                ").collect(); let __values = __values?; let __len = __values.len(); let __array: ",
            );
            out.push_str(&type_ref_to_rust(ty, scope, symbols));
            out.push_str(" = ::std::convert::TryInto::try_into(__values).map_err(|_| ");
            out.push_str(
                "cyclonedds::DdsError::BadParameter(::std::format!(\"expected array of length ",
            );
            out.push_str(&size.to_string());
            out.push_str(", got {}\", __len)))?; Ok::<_, cyclonedds::DdsError>(__array) }");
            out
        }
        IdlTypeRef::Named(name) => match symbols.resolve_named_ref(name, scope) {
            NamedRef::Generated(path) => {
                format!("<{path} as cyclonedds::DdsFieldMetadata>::from_dynamic_value({value})")
            }
            NamedRef::Inline { ty, scope } => {
                let scope = scope_refs(&scope);
                from_expr(ty, value, &scope, symbols)
            }
        },
    }
}

fn field_metadata_impl_struct(s: &IdlStruct, scope: &[&str], symbols: &SymbolTable) -> String {
    let fq = qualified_name(scope, &s.name);
    let mut out = String::new();

    out.push_str(ITEM_ALLOW);
    out.push_str("impl cyclonedds::DdsFieldMetadata for ");
    out.push_str(&s.name);
    out.push_str(" {\n");

    out.push_str("    fn field_schema() -> &'static cyclonedds::DynamicTypeSchema {\n");
    out.push_str("        static SCHEMA: ::std::sync::OnceLock<cyclonedds::DynamicTypeSchema> = ::std::sync::OnceLock::new();\n");
    out.push_str("        SCHEMA.get_or_init(|| cyclonedds::DynamicTypeSchema::Struct {\n");
    out.push_str("            name: ");
    out.push_str(&format!("{:?}", fq));
    out.push_str(".to_string(),\n");
    out.push_str("            base: None,\n");
    out.push_str("            fields: vec![\n");
    for (index, field) in s.fields.iter().enumerate() {
        let optional = field.annotations.iter().any(|a| a.name == "optional");
        let key = field.annotations.iter().any(|a| a.name == "key");
        out.push_str("                cyclonedds::DynamicFieldSchema {\n");
        out.push_str("                    name: ");
        out.push_str(&format!("{:?}", field.name));
        out.push_str(".to_string(),\n");
        out.push_str("                    member_id: ");
        out.push_str(&index.to_string());
        out.push_str(",\n");
        out.push_str("                    value: ");
        out.push_str(&schema_expr(&field.ty, scope, symbols));
        out.push_str(",\n");
        out.push_str("                    optional: ");
        out.push_str(if optional { "true" } else { "false" });
        out.push_str(",\n");
        out.push_str("                    key: ");
        out.push_str(if key { "true" } else { "false" });
        out.push_str(",\n");
        out.push_str("                    external: false,\n");
        out.push_str("                    must_understand: false,\n");
        out.push_str("                    hash_id_name: None,\n");
        out.push_str("                },\n");
    }
    out.push_str("            ],\n");
    out.push_str("            extensibility: Some(cyclonedds::DynamicTypeExtensibility::Final),\n");
    out.push_str("            autoid: None,\n");
    out.push_str("            nested: false,\n");
    out.push_str("        })\n");
    out.push_str("    }\n");

    out.push_str("    fn to_dynamic_value(&self) -> cyclonedds::DynamicValue {\n");
    out.push_str("        let mut __fields = ::std::collections::BTreeMap::new();\n");
    for field in &s.fields {
        let optional = field.annotations.iter().any(|a| a.name == "optional");
        let value = format!("&self.{}", field.name);
        let expr = if optional {
            let inner = to_expr(&field.ty, "__value", scope, symbols);
            format!(
                "match {value} {{ Some(__value) => {inner}, None => cyclonedds::DynamicValue::Null }}"
            )
        } else {
            to_expr(&field.ty, &value, scope, symbols)
        };
        out.push_str("        __fields.insert(");
        out.push_str(&format!("{:?}", field.name));
        out.push_str(".to_string(), ");
        out.push_str(&expr);
        out.push_str(");\n");
    }
    out.push_str("        cyclonedds::DynamicValue::Struct(__fields)\n");
    out.push_str("    }\n");

    out.push_str("    fn from_dynamic_value(__value: &cyclonedds::DynamicValue) -> cyclonedds::DdsResult<Self> {\n");
    out.push_str("        let __fields = match __value {\n");
    out.push_str("            cyclonedds::DynamicValue::Struct(__fields) => __fields,\n");
    out.push_str("            __other => return Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected struct value for ");
    out.push_str(&fq);
    out.push_str(", got {:?}\", __other))),\n");
    out.push_str("        };\n");
    out.push_str("        Ok(Self {\n");
    for field in &s.fields {
        let optional = field.annotations.iter().any(|a| a.name == "optional");
        out.push_str("            ");
        out.push_str(&field.name);
        out.push_str(": {\n");
        out.push_str("                match __fields.get(");
        out.push_str(&format!("{:?}", field.name));
        out.push_str(") {\n");
        if optional {
            out.push_str("                    None | Some(cyclonedds::DynamicValue::Null) => Ok::<_, cyclonedds::DdsError>(None),\n");
            out.push_str("                    Some(__field) => Ok(Some(");
            out.push_str(&from_expr(&field.ty, "__field", scope, symbols));
            out.push_str("?)),\n");
        } else {
            out.push_str("                    Some(__field) => ");
            out.push_str(&from_expr(&field.ty, "__field", scope, symbols));
            out.push_str(",\n");
            out.push_str(
                "                    None => return Err(cyclonedds::DdsError::BadParameter(",
            );
            out.push_str(&format!(
                "{:?}.to_string()",
                format!("missing required field '{}' for {}", field.name, fq)
            ));
            out.push_str(")),\n");
        }
        out.push_str("                }\n");
        out.push_str("            }?,\n");
    }
    out.push_str("        })\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

fn field_metadata_impl_enum(e: &IdlEnum, scope: &[&str]) -> String {
    let fq = qualified_name(scope, &e.name);
    let mut out = String::new();

    out.push_str(ITEM_ALLOW);
    out.push_str("impl cyclonedds::DdsFieldMetadata for ");
    out.push_str(&e.name);
    out.push_str(" {\n");
    out.push_str("    fn field_schema() -> &'static cyclonedds::DynamicTypeSchema {\n");
    out.push_str("        static SCHEMA: ::std::sync::OnceLock<cyclonedds::DynamicTypeSchema> = ::std::sync::OnceLock::new();\n");
    out.push_str("        SCHEMA.get_or_init(|| cyclonedds::DynamicTypeSchema::Enum {\n");
    out.push_str("            name: ");
    out.push_str(&format!("{:?}", fq));
    out.push_str(".to_string(),\n");
    out.push_str("            literals: vec![\n");
    for (index, variant) in e.variants.iter().enumerate() {
        let value = variant.value.unwrap_or(index as i64);
        out.push_str("                cyclonedds::DynamicEnumLiteralSchema { name: ");
        out.push_str(&format!("{:?}", variant.name));
        out.push_str(".to_string(), value: ");
        out.push_str(&value.to_string());
        out.push_str(", default: false },\n");
    }
    out.push_str("            ],\n");
    out.push_str("            bit_bound: None,\n");
    out.push_str("        })\n");
    out.push_str("    }\n");
    out.push_str("    fn to_dynamic_value(&self) -> cyclonedds::DynamicValue {\n");
    out.push_str("        cyclonedds::DynamicValue::Enum { value: *self as i32 }\n");
    out.push_str("    }\n");
    out.push_str("    fn from_dynamic_value(__value: &cyclonedds::DynamicValue) -> cyclonedds::DdsResult<Self> {\n");
    out.push_str("        match __value {\n");
    out.push_str("            cyclonedds::DynamicValue::Enum { value } => match *value {\n");
    for (index, variant) in e.variants.iter().enumerate() {
        let value = variant.value.unwrap_or(index as i64);
        out.push_str("                ");
        out.push_str(&value.to_string());
        out.push_str(" => Ok(Self::");
        out.push_str(&variant.name);
        out.push_str("),\n");
    }
    out.push_str("                __other => Err(cyclonedds::DdsError::BadParameter(::std::format!(\"enum value {} is not a variant of ");
    out.push_str(&fq);
    out.push_str("\", __other))),\n");
    out.push_str("            },\n");
    out.push_str("            __other => Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected enum value for ");
    out.push_str(&fq);
    out.push_str(", got {:?}\", __other))),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

fn field_metadata_impl_bitmask(b: &IdlBitmask, scope: &[&str]) -> String {
    let fq = qualified_name(scope, &b.name);
    let mut out = String::new();

    out.push_str(ITEM_ALLOW);
    out.push_str("impl cyclonedds::DdsFieldMetadata for ");
    out.push_str(&b.name);
    out.push_str(" {\n");
    out.push_str("    fn field_schema() -> &'static cyclonedds::DynamicTypeSchema {\n");
    out.push_str("        static SCHEMA: ::std::sync::OnceLock<cyclonedds::DynamicTypeSchema> = ::std::sync::OnceLock::new();\n");
    out.push_str("        SCHEMA.get_or_init(|| cyclonedds::DynamicTypeSchema::Bitmask {\n");
    out.push_str("            name: ");
    out.push_str(&format!("{:?}", fq));
    out.push_str(".to_string(),\n");
    out.push_str("            fields: vec![\n");
    for (index, flag) in b.flags.iter().enumerate() {
        out.push_str("                cyclonedds::DynamicBitmaskFieldSchema { name: ");
        out.push_str(&format!("{:?}", flag));
        out.push_str(".to_string(), position: ");
        out.push_str(&index.to_string());
        out.push_str("u16 },\n");
    }
    out.push_str("            ],\n");
    out.push_str("            bit_bound: Some(");
    out.push_str(&b.bit_bound.to_string());
    out.push_str("u16),\n");
    out.push_str("        })\n");
    out.push_str("    }\n");
    out.push_str("    fn to_dynamic_value(&self) -> cyclonedds::DynamicValue {\n");
    out.push_str("        cyclonedds::DynamicValue::Bitmask(self.to_bits())\n");
    out.push_str("    }\n");
    out.push_str("    fn from_dynamic_value(__value: &cyclonedds::DynamicValue) -> cyclonedds::DdsResult<Self> {\n");
    out.push_str("        match __value {\n");
    out.push_str(
        "            cyclonedds::DynamicValue::Bitmask(__bits) => Ok(Self::from_bits(*__bits)),\n",
    );
    out.push_str("            __other => Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected Bitmask for ");
    out.push_str(&fq);
    out.push_str(", got {:?}\", __other))),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

fn field_metadata_impl_union(u: &IdlUnion, scope: &[&str], symbols: &SymbolTable) -> String {
    let fq = qualified_name(scope, &u.name);
    let mut all_labels: Vec<i32> = Vec::new();
    for case in &u.cases {
        for label in &case.label_values {
            all_labels.push(*label as i32);
        }
    }
    let default_discriminator = {
        let mut value = 0i32;
        while all_labels.contains(&value) {
            value += 1;
        }
        value
    };

    let mut out = String::new();
    out.push_str(ITEM_ALLOW);
    out.push_str("impl cyclonedds::DdsFieldMetadata for ");
    out.push_str(&u.name);
    out.push_str(" {\n");
    out.push_str("    fn field_schema() -> &'static cyclonedds::DynamicTypeSchema {\n");
    out.push_str("        static SCHEMA: ::std::sync::OnceLock<cyclonedds::DynamicTypeSchema> = ::std::sync::OnceLock::new();\n");
    out.push_str("        SCHEMA.get_or_init(|| cyclonedds::DynamicTypeSchema::Union {\n");
    out.push_str("            name: ");
    out.push_str(&format!("{:?}", fq));
    out.push_str(".to_string(),\n");
    out.push_str("            discriminator: ::std::boxed::Box::new(");
    out.push_str(&schema_expr(&u.discriminant_type, scope, symbols));
    out.push_str("),\n");
    out.push_str("            cases: vec![\n");
    let mut case_index = 0usize;
    for case in &u.cases {
        out.push_str("                cyclonedds::DynamicUnionCaseSchema {\n");
        out.push_str("                    name: ");
        out.push_str(&format!("{:?}", case.name));
        out.push_str(".to_string(),\n");
        out.push_str("                    member_id: ");
        out.push_str(&case_index.to_string());
        out.push_str(",\n");
        out.push_str("                    value: ");
        out.push_str(&schema_expr(&case.ty, scope, symbols));
        out.push_str(",\n");
        out.push_str("                    labels: vec![");
        for (i, label) in case.label_values.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&(*label as i32).to_string());
            out.push_str("i32");
        }
        out.push_str("],\n");
        out.push_str("                    default: false,\n");
        out.push_str("                    external: false,\n");
        out.push_str("                    hash_id_name: None,\n");
        out.push_str("                },\n");
        case_index += 1;
    }
    if let Some(default) = &u.default_case {
        out.push_str("                cyclonedds::DynamicUnionCaseSchema {\n");
        out.push_str("                    name: ");
        out.push_str(&format!("{:?}", default.name));
        out.push_str(".to_string(),\n");
        out.push_str("                    member_id: ");
        out.push_str(&case_index.to_string());
        out.push_str(",\n");
        out.push_str("                    value: ");
        out.push_str(&schema_expr(&default.ty, scope, symbols));
        out.push_str(",\n");
        out.push_str("                    labels: vec![],\n");
        out.push_str("                    default: true,\n");
        out.push_str("                    external: false,\n");
        out.push_str("                    hash_id_name: None,\n");
        out.push_str("                },\n");
    }
    out.push_str("            ],\n");
    out.push_str("            autoid: None,\n");
    out.push_str("            nested: false,\n");
    out.push_str("        })\n");
    out.push_str("    }\n");

    out.push_str("    fn to_dynamic_value(&self) -> cyclonedds::DynamicValue {\n");
    out.push_str("        match self {\n");
    for case in &u.cases {
        let disc = case.label_values.first().copied().unwrap_or(0) as i32;
        out.push_str("            Self::");
        out.push_str(&to_pascal_case(&case.name));
        out.push_str("(__value) => cyclonedds::DynamicValue::Union { discriminator: ");
        out.push_str(&disc.to_string());
        out.push_str(", field: ");
        out.push_str(&format!("{:?}", case.name));
        out.push_str(".to_string(), value: ::std::boxed::Box::new(");
        out.push_str(&to_expr(&case.ty, "__value", scope, symbols));
        out.push_str(") },\n");
    }
    if let Some(default) = &u.default_case {
        out.push_str("            Self::");
        out.push_str(&to_pascal_case(&default.name));
        out.push_str("(__value) => cyclonedds::DynamicValue::Union { discriminator: ");
        out.push_str(&default_discriminator.to_string());
        out.push_str(", field: ");
        out.push_str(&format!("{:?}", default.name));
        out.push_str(".to_string(), value: ::std::boxed::Box::new(");
        out.push_str(&to_expr(&default.ty, "__value", scope, symbols));
        out.push_str(") },\n");
    }
    out.push_str("        }\n");
    out.push_str("    }\n");

    out.push_str("    fn from_dynamic_value(__value: &cyclonedds::DynamicValue) -> cyclonedds::DdsResult<Self> {\n");
    out.push_str("        match __value {\n");
    out.push_str("            cyclonedds::DynamicValue::Union { discriminator, field, value } => match field.as_str() {\n");
    for case in &u.cases {
        out.push_str("                ");
        out.push_str(&format!("{:?}", case.name));
        if !case.label_values.is_empty() {
            out.push_str(" if [");
            for (i, label) in case.label_values.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&(*label as i32).to_string());
                out.push_str("i32");
            }
            out.push_str("].contains(discriminator)");
        }
        out.push_str(" => Ok(Self::");
        out.push_str(&to_pascal_case(&case.name));
        out.push('(');
        out.push_str(&from_expr(&case.ty, "value.as_ref()", scope, symbols));
        out.push_str("?)),\n");
    }
    if let Some(default) = &u.default_case {
        out.push_str("                ");
        out.push_str(&format!("{:?}", default.name));
        out.push_str(" => Ok(Self::");
        out.push_str(&to_pascal_case(&default.name));
        out.push('(');
        out.push_str(&from_expr(&default.ty, "value.as_ref()", scope, symbols));
        out.push_str("?)),\n");
    }
    out.push_str("                __other => Err(cyclonedds::DdsError::BadParameter(::std::format!(\"union field {} is not a case of ");
    out.push_str(&fq);
    out.push_str("\", __other))),\n");
    out.push_str("            },\n");
    out.push_str("            __other => Err(cyclonedds::DdsError::BadParameter(::std::format!(\"expected Union for ");
    out.push_str(&fq);
    out.push_str(", got {:?}\", __other))),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

/// Convert a snake_case or lowercase string to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper = c.to_uppercase().to_string();
                    format!("{}{}", upper, chars.as_str())
                }
            }
        })
        .collect()
}

/// Convert a CamelCase string to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idl_parser::{parse_idl, IdlEnumVariant, IdlField};

    fn empty_symbols() -> SymbolTable {
        SymbolTable {
            types: HashMap::new(),
            typedefs: HashMap::new(),
        }
    }

    fn gen(idl: &str) -> String {
        let file = parse_idl(idl).unwrap();
        generate_rust(&file, "types")
    }

    #[test]
    fn test_generate_simple_struct() {
        let s = IdlStruct {
            name: "Point".into(),
            fields: vec![
                IdlField {
                    name: "x".into(),
                    ty: IdlTypeRef::Primitive(PrimitiveType::Long),
                    annotations: vec![],
                },
                IdlField {
                    name: "y".into(),
                    ty: IdlTypeRef::Primitive(PrimitiveType::Double),
                    annotations: vec![],
                },
            ],
            annotations: vec![],
        };
        let code = generate_struct(&s, &[], &empty_symbols(), &CompileOptions::default());
        assert!(code.contains("#[derive(Debug, Clone, Default, PartialEq, DdsTypeDerive)]"));
        assert!(code.contains("#[dds_typename(\"Point\")]"));
        assert!(code.contains("pub struct Point"));
        assert!(code.contains("pub x: i32"));
        assert!(code.contains("pub y: f64"));
    }

    #[test]
    fn test_generate_enum() {
        let e = IdlEnum {
            name: "Color".into(),
            variants: vec![
                IdlEnumVariant {
                    name: "Red".into(),
                    value: Some(0),
                },
                IdlEnumVariant {
                    name: "Green".into(),
                    value: Some(1),
                },
                IdlEnumVariant {
                    name: "Blue".into(),
                    value: Some(2),
                },
            ],
        };
        let code = generate_enum(&e, &[]);
        assert!(code.contains("#[repr(i32)]"));
        assert!(code.contains("#[derive(Debug, Clone, Copy, PartialEq, Eq, DdsEnumDerive)]"));
        assert!(code.contains("Red = 0"));
        assert!(code.contains("Blue = 2"));
    }

    #[test]
    fn test_type_ref_mapping() {
        let symbols = empty_symbols();
        assert_eq!(
            type_ref_to_rust(
                &IdlTypeRef::Primitive(PrimitiveType::Boolean),
                &[],
                &symbols
            ),
            "bool"
        );
        assert_eq!(
            type_ref_to_rust(&IdlTypeRef::Primitive(PrimitiveType::Long), &[], &symbols),
            "i32"
        );
        assert_eq!(
            type_ref_to_rust(
                &IdlTypeRef::Primitive(PrimitiveType::UnsignedLongLong),
                &[],
                &symbols
            ),
            "u64"
        );
        assert_eq!(
            type_ref_to_rust(&IdlTypeRef::String { bound: None }, &[], &symbols),
            "String"
        );
        assert_eq!(
            type_ref_to_rust(&IdlTypeRef::Named("MyType".into()), &[], &symbols),
            "MyType"
        );
    }

    #[test]
    fn test_generate_bitmask() {
        let b = IdlBitmask {
            name: "Flags".into(),
            bit_bound: 16,
            flags: vec!["Ready".into(), "Active".into()],
        };
        let code = generate_bitmask(&b, &[]);
        assert!(code.contains("#[derive(Debug, Clone, DdsBitmaskDerive)]"));
        assert!(code.contains("#[bit_bound(16)]"));
        assert!(code.contains("pub Ready: bool"));
    }

    #[test]
    fn test_generate_nested_modules() {
        let file = IdlFile {
            definitions: vec![Definition::Module(IdlModule {
                name: "dds".into(),
                definitions: vec![Definition::Module(IdlModule {
                    name: "hello_world".into(),
                    definitions: vec![Definition::Type(IdlType::Struct(IdlStruct {
                        name: "HelloWorldModel".into(),
                        fields: vec![IdlField {
                            name: "id".into(),
                            ty: IdlTypeRef::Primitive(PrimitiveType::Long),
                            annotations: vec![],
                        }],
                        annotations: vec![],
                    }))],
                })],
            })],
        };
        let code = generate_rust(&file, "nested_modules");
        assert!(code.contains("pub mod dds {"));
        assert!(code.contains("pub mod hello_world {"));
        assert!(code.contains("pub struct HelloWorldModel"));
        assert!(code.contains("#[dds_typename(\"dds::hello_world::HelloWorldModel\")]"));
    }

    #[test]
    fn test_generate_dds_typename_disabled() {
        let s = IdlStruct {
            name: "Point".into(),
            fields: vec![],
            annotations: vec![],
        };
        let options = CompileOptions {
            emit_dds_typename: false,
            ..Default::default()
        };
        let code = generate_struct(&s, &["dds", "hello_world"], &empty_symbols(), &options);
        assert!(!code.contains("dds_typename"));
    }

    #[test]
    fn test_generate_dds_typename_top_level() {
        let s = IdlStruct {
            name: "Point".into(),
            fields: vec![],
            annotations: vec![],
        };
        let code = generate_struct(&s, &[], &empty_symbols(), &CompileOptions::default());
        assert!(code.contains("#[dds_typename(\"Point\")]"));
    }

    #[test]
    fn test_generate_optional_field() {
        let code = gen(r#"
            struct Optional {
                @key long id;
                @optional long count;
                @optional string note;
            };
        "#);
        assert!(code.contains("pub count: Option<i32>"), "{code}");
        assert!(code.contains("pub note: Option<String>"), "{code}");
    }

    #[test]
    fn test_generate_same_module_reference() {
        let code = gen(r#"
            module m {
                struct A { long x; };
                struct B { A a; };
            };
        "#);
        assert!(code.contains("pub a: A"), "{code}");
    }

    #[test]
    fn test_generate_cross_module_reference() {
        let top = gen(r#"
            module geometry { struct Point { double x; }; };
            struct Line { geometry::Point start; };
        "#);
        assert!(top.contains("pub start: geometry::Point"), "{top}");

        let nested = gen(r#"
            module geometry { struct Point { double x; }; };
            module shapes { struct Line { geometry::Point start; }; };
        "#);
        assert!(
            nested.contains("pub start: super::geometry::Point"),
            "{nested}"
        );
    }

    #[test]
    fn test_generate_field_metadata_struct() {
        let code = gen(r#"
            module m {
                struct Inner { double x; };
                struct Topic {
                    @key long id;
                    @optional string note;
                    Inner inner;
                    sequence<long> values;
                    long fixed[3];
                };
            };
        "#);
        assert!(
            code.contains("impl cyclonedds::DdsFieldMetadata for Topic"),
            "{code}"
        );
        assert!(
            code.contains("<Inner as cyclonedds::DdsFieldMetadata>::field_schema().clone()"),
            "{code}"
        );
        assert!(code.contains("optional: true"), "{code}");
        assert!(code.contains("key: true"), "{code}");
        assert!(
            code.contains("cyclonedds::DynamicTypeSchema::Sequence"),
            "{code}"
        );
        assert!(
            code.contains("cyclonedds::DynamicTypeSchema::Array"),
            "{code}"
        );
        assert!(code.contains("DynamicValue::Null"), "{code}");
    }

    #[test]
    fn test_generate_field_metadata_inlines_typedef() {
        let code = gen(r#"
            module m {
                typedef long Count;
                struct Topic { Count id; };
            };
        "#);
        assert!(
            code.contains("cyclonedds::DynamicPrimitiveKind::Int32"),
            "{code}"
        );
        assert!(
            !code.contains("<Count as cyclonedds::DdsFieldMetadata>"),
            "{code}"
        );
    }

    #[test]
    fn test_generate_field_metadata_enum_bitmask_union() {
        let code = gen(r#"
            enum Color { Red, Green };
            @bit_bound(8) bitmask Flags { A, B };
            union Choice switch (long) {
                case 0: long number;
                default: double other;
            };
        "#);
        assert!(
            code.contains("impl cyclonedds::DdsFieldMetadata for Color"),
            "{code}"
        );
        assert!(
            code.contains("impl cyclonedds::DdsFieldMetadata for Flags"),
            "{code}"
        );
        assert!(
            code.contains("impl cyclonedds::DdsFieldMetadata for Choice"),
            "{code}"
        );
        assert!(code.contains("DynamicEnumLiteralSchema"), "{code}");
        assert!(code.contains("DynamicBitmaskFieldSchema"), "{code}");
        assert!(code.contains("DynamicUnionCaseSchema"), "{code}");
    }
}
