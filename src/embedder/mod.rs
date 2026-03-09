use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

pub const MODEL_ID: &str = "all-MiniLM-L6-v2";
pub const DIMS: usize = 384;

pub struct Embedder {
    model: TextEmbedding,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(true),
        )?;
        Ok(Self { model })
    }

    /// Build text representation for a symbol.
    pub fn make_text(
        name: &str,
        kind: &str,
        file_path: &str,
        parent_name: Option<&str>,
    ) -> String {
        match parent_name {
            Some(p) => format!("{kind} {p}.{name} in {file_path}"),
            None => format!("{kind} {name} in {file_path}"),
        }
    }

    /// Embed a batch of texts.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = self.model.embed(text_refs, None)?;
        Ok(embeddings)
    }

    /// Embed a single query string.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let mut results = self.model.embed(vec![query], None)?;
        results
            .pop()
            .ok_or_else(|| anyhow::anyhow!("embedding returned empty result"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_text_with_parent() {
        let text = Embedder::make_text("parse", "Method", "src/parser.rs", Some("Parser"));
        assert_eq!(text, "Method Parser.parse in src/parser.rs");
    }

    #[test]
    fn make_text_without_parent() {
        let text = Embedder::make_text("main", "Function", "src/main.rs", None);
        assert_eq!(text, "Function main in src/main.rs");
    }
}
