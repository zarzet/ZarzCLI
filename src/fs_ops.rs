use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use walkdir::WalkDir;

pub struct FileSystemOps;

impl FileSystemOps {
    pub async fn create_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create parent directories for {}", path.display()))?;
        }

        fs::write(path, content)
            .await
            .with_context(|| format!("Failed to write file {}", path.display()))?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn delete_file(path: &Path) -> Result<()> {
        fs::remove_file(path)
            .await
            .with_context(|| format!("Failed to delete file {}", path.display()))?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn rename_file(from: &Path, to: &Path) -> Result<()> {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create parent directories for {}", to.display()))?;
        }

        fs::rename(from, to)
            .await
            .with_context(|| format!("Failed to rename {} to {}", from.display(), to.display()))?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn create_directory(path: &Path) -> Result<()> {
        fs::create_dir_all(path)
            .await
            .with_context(|| format!("Failed to create directory {}", path.display()))?;

        Ok(())
    }

    pub async fn read_file(path: &Path) -> Result<String> {
        fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read file {}", path.display()))
    }

    pub async fn file_exists(path: &Path) -> bool {
        fs::metadata(path).await.is_ok()
    }

    #[allow(dead_code)]
    pub fn list_files(root: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();

                if let Some(pattern) = pattern {
                    if let Some(file_name) = path.file_name() {
                        if file_name.to_string_lossy().contains(pattern) {
                            files.push(path.to_path_buf());
                        }
                    }
                } else {
                    files.push(path.to_path_buf());
                }
            }
        }

        Ok(files)
    }

    #[allow(dead_code)]
    pub fn get_directory_structure(root: &Path, max_depth: Option<usize>) -> Result<String> {
        let mut output = String::new();
        let max_depth = max_depth.unwrap_or(3);

        for entry in WalkDir::new(root)
            .max_depth(max_depth)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let depth = entry.depth();
            let indent = "  ".repeat(depth);
            let name = entry.file_name().to_string_lossy();

            if entry.file_type().is_dir() {
                output.push_str(&format!("{}{}/\n", indent, name));
            } else {
                output.push_str(&format!("{}{}\n", indent, name));
            }
        }

        Ok(output)
    }
}
