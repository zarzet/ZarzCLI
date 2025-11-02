use super::{Symbol, SymbolKind};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use syn::{visit::Visit, Item};

pub struct RustParser;

impl RustParser {
    pub fn parse_file(path: &Path) -> Result<Vec<Symbol>> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file {}", path.display()))?;

        let syntax = syn::parse_file(&content)
            .with_context(|| format!("Failed to parse Rust file {}", path.display()))?;

        let mut visitor = SymbolVisitor {
            symbols: Vec::new(),
            file: path.to_path_buf(),
        };

        visitor.visit_file(&syntax);

        Ok(visitor.symbols)
    }

    #[allow(dead_code)]
    pub fn extract_dependencies(root: &Path) -> Result<Vec<String>> {
        let cargo_path = root.join("Cargo.toml");

        if !cargo_path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&cargo_path)
            .context("Failed to read Cargo.toml")?;

        let toml: toml::Value = toml::from_str(&content)
            .context("Failed to parse Cargo.toml")?;

        let mut deps = Vec::new();

        if let Some(dependencies) = toml.get("dependencies") {
            if let Some(table) = dependencies.as_table() {
                for (name, _) in table {
                    deps.push(name.clone());
                }
            }
        }

        Ok(deps)
    }
}

struct SymbolVisitor {
    symbols: Vec<Symbol>,
    file: PathBuf,
}

impl<'ast> Visit<'ast> for SymbolVisitor {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Fn(func) => {
                let name = func.sig.ident.to_string();
                self.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    file: self.file.clone(),
                    line: 0,
                });
            }
            Item::Struct(s) => {
                let name = s.ident.to_string();
                self.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Struct,
                    file: self.file.clone(),
                    line: 0,
                });
            }
            Item::Enum(e) => {
                let name = e.ident.to_string();
                self.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Enum,
                    file: self.file.clone(),
                    line: 0,
                });
            }
            Item::Trait(t) => {
                let name = t.ident.to_string();
                self.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Trait,
                    file: self.file.clone(),
                    line: 0,
                });
            }
            Item::Impl(impl_item) => {
                if let Some((_, path, _)) = &impl_item.trait_ {
                    let name = quote::quote!(#path).to_string();
                    self.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Impl,
                        file: self.file.clone(),
                        line: 0,
                    });
                }
            }
            Item::Mod(m) => {
                let name = m.ident.to_string();
                self.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Module,
                    file: self.file.clone(),
                    line: 0,
                });
            }
            Item::Const(c) => {
                let name = c.ident.to_string();
                self.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Constant,
                    file: self.file.clone(),
                    line: 0,
                });
            }
            Item::Static(s) => {
                let name = s.ident.to_string();
                self.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Static,
                    file: self.file.clone(),
                    line: 0,
                });
            }
            _ => {}
        }

        syn::visit::visit_item(self, item);
    }
}
