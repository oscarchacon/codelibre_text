//! Syntax highlighting using tree-sitter
//! Supports Rust, Python, JavaScript, TypeScript, C++, HTML, CSS, JSON

use egui::Color32;
use tree_sitter::{Parser, Node};
use std::ffi::CString;

/// Syntax highlight colors
#[derive(Debug, Clone, Copy)]
pub struct HighlightColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl HighlightColor {
    pub fn to_egui_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.r, self.g, self.b, self.a)
    }
}

/// Highlight type for tree-sitter nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightType {
    Function,
    Type,
    Keyword,
    String,
    Number,
    Comment,
    Variable,
    Constant,
    Parameter,
    Operator,
    Punctuation,
    Tag,
    Attribute,
    Value,
}

impl HighlightType {
    pub fn color(&self, dark_theme: bool) -> HighlightColor {
        if dark_theme {
            match self {
                Self::Function => HighlightColor { r: 130, g: 170, b: 255, a: 255 },
                Self::Type => HighlightColor { r: 190, g: 210, b: 255, a: 255 },
                Self::Keyword => HighlightColor { r: 255, g: 120, b: 200, a: 255 },
                Self::String => HighlightColor { r: 170, g: 200, b: 120, a: 255 },
                Self::Number => HighlightColor { r: 240, g: 180, b: 100, a: 255 },
                Self::Comment => HighlightColor { r: 100, g: 130, b: 100, a: 180 },
                Self::Variable => HighlightColor { r: 220, g: 220, b: 220, a: 255 },
                Self::Constant => HighlightColor { r: 100, g: 180, b: 255, a: 255 },
                Self::Parameter => HighlightColor { r: 255, g: 200, b: 130, a: 255 },
                Self::Operator => HighlightColor { r: 150, g: 180, b: 220, a: 255 },
                Self::Punctuation => HighlightColor { r: 120, g: 140, b: 180, a: 200 },
                Self::Tag => HighlightColor { r: 255, g: 120, b: 120, a: 255 },
                Self::Attribute => HighlightColor { r: 180, g: 160, b: 255, a: 255 },
                Self::Value => HighlightColor { r: 170, g: 220, b: 120, a: 255 },
            }
        } else {
            match self {
                Self::Function => HighlightColor { r: 0, g: 0, b: 200, a: 255 },
                Self::Type => HighlightColor { r: 100, g: 50, b: 200, a: 255 },
                Self::Keyword => HighlightColor { r: 180, g: 0, b: 0, a: 255 },
                Self::String => HighlightColor { r: 0, g: 130, b: 0, a: 255 },
                Self::Number => HighlightColor { r: 180, g: 100, b: 0, a: 255 },
                Self::Comment => HighlightColor { r: 0, g: 130, b: 0, a: 180 },
                Self::Variable => HighlightColor { r: 0, g: 0, b: 0, a: 255 },
                Self::Constant => HighlightColor { r: 0, g: 100, b: 200, a: 255 },
                Self::Parameter => HighlightColor { r: 200, g: 120, b: 0, a: 255 },
                Self::Operator => HighlightColor { r: 100, g: 100, b: 100, a: 200 },
                Self::Punctuation => HighlightColor { r: 100, g: 100, b: 100, a: 180 },
                Self::Tag => HighlightColor { r: 180, g: 0, b: 0, a: 255 },
                Self::Attribute => HighlightColor { r: 100, g: 0, b: 200, a: 255 },
                Self::Value => HighlightColor { r: 0, g: 130, b: 0, a: 255 },
            }
        }
    }

    pub fn language_name(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Type => "type",
            Self::Keyword => "keyword",
            Self::String => "string",
            Self::Number => "number",
            Self::Comment => "comment",
            Self::Variable => "variable",
            Self::Constant => "constant",
            Self::Parameter => "parameter",
            Self::Operator => "operator",
            Self::Punctuation => "punctuation",
            Self::Tag => "tag",
            Self::Attribute => "attribute",
            Self::Value => "value",
        }
    }
}

/// Syntax highlighter for a language
pub struct SyntaxHighlighter {
    pub language_name: String,
    parser: Parser,
}

// tree_sitter::Parser doesn't implement Debug, so we derive it manually
impl std::fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("language_name", &self.language_name)
            .finish()
    }
}

impl SyntaxHighlighter {
    pub fn new(language_name: &str) -> Result<Self, String> {
        let mut parser = Parser::new();

        let lang_fn = match language_name {
            "rust" => tree_sitter_rust::LANGUAGE,
            "python" => tree_sitter_python::LANGUAGE,
            "javascript" => tree_sitter_javascript::LANGUAGE,
            "typescript" => tree_sitter_typescript::LANGUAGE_TSX,
            "cpp" => tree_sitter_cpp::LANGUAGE,
            "html" => tree_sitter_html::LANGUAGE,
            "css" => tree_sitter_css::LANGUAGE,
            "json" => tree_sitter_json::LANGUAGE,
            "text" => tree_sitter_rust::LANGUAGE,
            _ => return Err(format!("Unsupported language: {}", language_name)),
        };

        let language = tree_sitter::Language::new(lang_fn);

        parser
            .set_language(&language)
            .map_err(|e| e.to_string())?;

        Ok(Self {
            language_name: language_name.to_string(),
            parser,
        })
    }

    pub fn highlight(&mut self, text: &str) -> Vec<HighlightChunk> {
        let mut results = Vec::new();
        if let Some(tree) = self.parser.parse(text, None) {
            let root = tree.root_node();
            self.highlight_node(root, text, &mut results);
        } else {
            results.push(HighlightChunk {
                start: 0,
                end: text.len(),
                highlight_type: HighlightType::Variable, // Default fallback
            });
        }
        results
    }

    fn highlight_node(
        &self,
        node: Node,
        text: &str,
        results: &mut Vec<HighlightChunk>,
    ) {
        let node_type = node.kind();

        // Determine highlight type from node type
        let highlight_type = self.node_type_to_highlight(node_type);

        if node.child_count() == 0 {
            // Leaf node - apply highlighting
            let start = node.start_byte();
            let end = node.end_byte();
            if start < end {
                results.push(HighlightChunk {
                    start,
                    end,
                    highlight_type,
                });
            }
        } else {
            // Non-leaf node - recurse into children
            for child in node.children(&mut node.walk()) {
                self.highlight_node(child, text, results);
            }
        }
    }

    fn node_type_to_highlight(&self, node_type: &str) -> HighlightType {
        match node_type {
            "function" | "function_item" | "function_signature" | "method" | "function_definition" => HighlightType::Function,
            "struct_item" | "enum_item" | "type_identifier" | "trait_item" | "struct_definition" | "enum_definition" => HighlightType::Type,
            "keyword" | "use" | "mod" | "fn" | "let" | "mut" | "const" | "if" | "else" | "for"
            | "while" | "loop" | "match" | "impl" | "trait" | "struct" | "enum" | "pub"
            | "self" | "super" | "crate" | "return" | "async" | "await" | "move" | "ref" => {
                HighlightType::Keyword
            }
            "string" | "string_content" | "concatenated_string" => HighlightType::String,
            "integer_literal" | "float_literal" => HighlightType::Number,
            "comment" | "line_comment" | "block_comment" => HighlightType::Comment,
            "identifier" | "parameter" => HighlightType::Variable,
            "constant_item" | "self_expression" => HighlightType::Constant,
            _ => HighlightType::Variable,
        }
    }
}

/// A chunk of highlighted text
#[derive(Debug, Clone)]
pub struct HighlightChunk {
    pub start: usize,
    pub end: usize,
    pub highlight_type: HighlightType,
}
