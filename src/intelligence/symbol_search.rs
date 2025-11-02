use super::{RustParser, Symbol};
use anyhow::Result;
use std::path::Path;
use walkdir::WalkDir;

pub struct SymbolSearcher;

impl SymbolSearcher {
    pub fn search(root: &Path, name: &str) -> Result<Vec<Symbol>> {
        let mut results = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();

                if let Some(ext) = path.extension() {
                    if ext == "rs" {
                        if let Ok(symbols) = RustParser::parse_file(path) {
                            for symbol in symbols {
                                if symbol.name.contains(name) {
                                    results.push(symbol);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    #[allow(dead_code)]
    pub fn search_exact(root: &Path, name: &str) -> Result<Vec<Symbol>> {
        let mut results = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();

                if let Some(ext) = path.extension() {
                    if ext == "rs" {
                        if let Ok(symbols) = RustParser::parse_file(path) {
                            for symbol in symbols {
                                if symbol.name == name {
                                    results.push(symbol);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    #[allow(dead_code)]
    pub fn find_references(root: &Path, symbol_name: &str) -> Result<Vec<(String, usize)>> {
        let mut references = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();

                if let Some(ext) = path.extension() {
                    if ext == "rs" {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            for (line_num, line) in content.lines().enumerate() {
                                if line.contains(symbol_name) {
                                    references.push((
                                        format!("{}:{}", path.display(), line_num + 1),
                                        line_num + 1,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(references)
    }
}
