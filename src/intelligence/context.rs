use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct ContextBuilder;

impl ContextBuilder {
    pub fn build_context(root: &Path, query: &str) -> Result<Vec<PathBuf>> {
        let keywords = Self::extract_keywords(query);
        let mut relevant_files = Vec::new();
        let mut scores: Vec<(PathBuf, usize)> = Vec::new();

        for entry in WalkDir::new(root)
            .max_depth(10)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();

                if Self::should_skip(path) {
                    continue;
                }

                if let Ok(content) = std::fs::read_to_string(path) {
                    let score = Self::calculate_relevance(&content, &keywords);

                    if score > 0 {
                        scores.push((path.to_path_buf(), score));
                    }
                }
            }
        }

        scores.sort_by(|a, b| b.1.cmp(&a.1));

        for (path, _) in scores.iter().take(5) {
            relevant_files.push(path.clone());
        }

        Ok(relevant_files)
    }

    fn extract_keywords(query: &str) -> HashSet<String> {
        let re = Regex::new(r"\b[a-zA-Z_][a-zA-Z0-9_]{2,}\b").unwrap();
        let mut keywords = HashSet::new();

        for cap in re.find_iter(query) {
            let word = cap.as_str().to_lowercase();
            if !Self::is_common_word(&word) {
                keywords.insert(word);
            }
        }

        keywords
    }

    fn calculate_relevance(content: &str, keywords: &HashSet<String>) -> usize {
        let content_lower = content.to_lowercase();
        let mut score = 0;

        for keyword in keywords {
            let count = content_lower.matches(keyword.as_str()).count();
            score += count * 10;
        }

        score
    }

    fn should_skip(path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        if path_str.contains("target/")
            || path_str.contains(".git/")
            || path_str.contains("node_modules/")
            || path_str.contains(".vscode/")
        {
            return true;
        }

        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy();
            if ext_str == "lock"
                || ext_str == "json"
                || ext_str == "md"
                || ext_str == "txt"
                || ext_str == "yml"
                || ext_str == "yaml"
            {
                return true;
            }
        }

        false
    }

    fn is_common_word(word: &str) -> bool {
        matches!(
            word,
            "the" | "and"
                | "for"
                | "are"
                | "but"
                | "not"
                | "you"
                | "all"
                | "can"
                | "her"
                | "was"
                | "one"
                | "our"
                | "out"
                | "day"
                | "get"
                | "has"
                | "him"
                | "his"
                | "how"
                | "let"
                | "may"
                | "new"
                | "now"
                | "old"
                | "see"
                | "try"
                | "use"
                | "way"
                | "who"
                | "boy"
                | "did"
                | "its"
                | "say"
                | "she"
                | "too"
                | "any"
                | "add"
                | "set"
        )
    }
}
