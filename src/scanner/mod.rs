use anyhow::Result;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use xxhash_rust::xxh3::xxh3_64;

use crate::parser::Language;

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub relative_path: String,
    pub language: Language,
    pub content_hash: u64,
    pub size_bytes: u64,
}

#[derive(Debug)]
pub struct ScanResult {
    pub files: Vec<FileInfo>,
}

/// Scan a directory for source files, respecting .gitignore.
pub fn scan(root: &Path) -> Result<ScanResult> {
    let mut files = Vec::new();

    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            // Skip common non-source directories
            !matches!(
                name.as_ref(),
                "node_modules"
                    | "vendor"
                    | ".git"
                    | ".codeatlas"
                    | "target"
                    | "dist"
                    | "build"
                    | "__pycache__"
            )
        })
        .build();

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();
        let Some(language) = detect_language(path) else {
            continue;
        };

        let content = std::fs::read(path)?;
        let content_hash = xxh3_64(&content);
        let size_bytes = content.len() as u64;

        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        files.push(FileInfo {
            path: path.to_path_buf(),
            relative_path,
            language,
            content_hash,
            size_bytes,
        });
    }

    Ok(ScanResult { files })
}

fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;
    Language::from_extension(ext)
}
