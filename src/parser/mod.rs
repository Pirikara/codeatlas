pub mod extract;
pub mod calls;
pub mod dataflow;
pub mod imports;

use anyhow::{Context, Result};
use tree_sitter::Parser;

pub use extract::{Symbol, SymbolKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Ruby,
    Go,
    TypeScript,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::Ruby => write!(f, "ruby"),
            Language::Go => write!(f, "go"),
            Language::TypeScript => write!(f, "typescript"),
        }
    }
}

impl Language {
    /// Detect language from a file extension. Returns `None` for unsupported extensions.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rb" => Some(Language::Ruby),
            "go" => Some(Language::Go),
            "ts" | "tsx" | "js" | "jsx" => Some(Language::TypeScript),
            _ => None,
        }
    }
}

/// Holds Tree-sitter parsers for each language.
pub struct ParserPool {
    ruby: Parser,
    go: Parser,
    typescript: Parser,
}

impl ParserPool {
    pub fn new() -> Result<Self> {
        Ok(Self {
            ruby: create_parser(tree_sitter_ruby::LANGUAGE.into())
                .context("Failed to load Ruby parser")?,
            go: create_parser(tree_sitter_go::LANGUAGE.into())
                .context("Failed to load Go parser")?,
            typescript: create_parser(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            )
            .context("Failed to load TypeScript parser")?,
        })
    }

    /// Parse source code and extract symbols only.
    #[allow(dead_code)]
    pub fn parse(&mut self, language: Language, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse_tree(language, source)?;
        let symbols = extract::extract_symbols(language, &tree, source);
        Ok(symbols)
    }

    /// Full parse: extract symbols, imports, calls, and data flows.
    pub fn parse_full(
        &mut self,
        language: Language,
        source: &[u8],
    ) -> Result<(Vec<Symbol>, Vec<imports::RawImport>, Vec<calls::RawCall>, Vec<dataflow::RawFlow>)> {
        let tree = self.parse_tree(language, source)?;
        let symbols = extract::extract_symbols(language, &tree, source);
        let raw_imports = imports::extract_imports(language, &tree, source);
        let raw_calls = calls::extract_calls(language, &tree, source);
        let raw_flows = dataflow::extract_dataflows(language, &tree, source);
        Ok((symbols, raw_imports, raw_calls, raw_flows))
    }

    fn parse_tree(&mut self, language: Language, source: &[u8]) -> Result<tree_sitter::Tree> {
        let parser = match language {
            Language::Ruby => &mut self.ruby,
            Language::Go => &mut self.go,
            Language::TypeScript => &mut self.typescript,
        };
        parser.parse(source, None).context("Tree-sitter parse failed")
    }
}

fn create_parser(language: tree_sitter::Language) -> Result<Parser> {
    let mut parser = Parser::new();
    parser.set_language(&language)?;
    Ok(parser)
}

#[cfg(test)]
mod tests {
    use super::Language;

    #[test]
    fn from_extension_known() {
        assert_eq!(Language::from_extension("rb"), Some(Language::Ruby));
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("js"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("jsx"), Some(Language::TypeScript));
    }

    #[test]
    fn from_extension_unknown() {
        assert_eq!(Language::from_extension("py"), None);
        assert_eq!(Language::from_extension("rs"), None);
        assert_eq!(Language::from_extension(""), None);
    }
}
