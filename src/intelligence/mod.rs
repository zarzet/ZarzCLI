mod rust_parser;
mod symbol_search;
mod context;

pub use rust_parser::RustParser;
pub use symbol_search::SymbolSearcher;
pub use context::ContextBuilder;

use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    #[allow(dead_code)]
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Constant,
    Static,
}

#[derive(Debug)]
pub struct ProjectIntelligence {
    root: PathBuf,
}

impl ProjectIntelligence {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn find_symbol(&self, name: &str) -> Result<Vec<Symbol>> {
        SymbolSearcher::search(&self.root, name)
    }

    #[allow(dead_code)]
    pub fn get_file_symbols(&self, file: &Path) -> Result<Vec<Symbol>> {
        RustParser::parse_file(file)
    }

    pub fn get_relevant_context(&self, query: &str) -> Result<Vec<PathBuf>> {
        ContextBuilder::build_context(&self.root, query)
    }

    #[allow(dead_code)]
    pub fn analyze_dependencies(&self) -> Result<Vec<String>> {
        RustParser::extract_dependencies(&self.root)
    }
}
