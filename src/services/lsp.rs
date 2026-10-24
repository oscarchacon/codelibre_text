//! LSP (Language Server Protocol) integration
//! Provides auto-complete/completion popup based on the LSP protocol

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionTextEdit,
    Documentation, MarkupContent, Position, Range, TextEdit,
};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Represents a single completion entry shown in the autocomplete popup
#[derive(Debug, Clone)]
pub struct CompletionEntry {
    pub label: String,
    pub detail: Option<String>,
    pub description: Option<String>,
    pub kind: CompletionKind,
    pub insert_text: String,
    pub range: Option<(usize, usize)>,
    pub sort_text: Option<String>,
}

impl CompletionEntry {
    fn from_lsp(item: &CompletionItem) -> Self {
        Self {
            label: item.label.clone(),
            detail: item.detail.clone(),
            description: item.documentation.as_ref().map(|doc| match doc {
                Documentation::MarkupContent(mc) => mc.value.clone(),
                Documentation::String(s) => s.clone(),
            }),
            kind: CompletionKind::from_lsp_kind(item.kind),
            insert_text: item
                .text_edit
                .as_ref()
                .map(|te| match te {
                    lsp_types::CompletionTextEdit::Edit(et) => et.new_text.clone(),
                    lsp_types::CompletionTextEdit::InsertAndReplace(et) => et.new_text.clone(),
                })
                .unwrap_or_else(|| item.label.clone()),
            range: None,
            sort_text: item.sort_text.clone(),
        }
    }
}

impl CompletionKind {
    fn from_lsp_kind(kind: Option<CompletionItemKind>) -> Self {
        match kind {
            Some(CompletionItemKind::TEXT) => Self::Text,
            Some(CompletionItemKind::METHOD) => Self::Method,
            Some(CompletionItemKind::FUNCTION) => Self::Function,
            Some(CompletionItemKind::CONSTRUCTOR) => Self::Constructor,
            Some(CompletionItemKind::FIELD) => Self::Field,
            Some(CompletionItemKind::VARIABLE) => Self::Variable,
            Some(CompletionItemKind::CLASS) => Self::Class,
            Some(CompletionItemKind::INTERFACE) => Self::Interface,
            Some(CompletionItemKind::STRUCT) => Self::Struct,
            Some(CompletionItemKind::MODULE) => Self::Module,
            Some(CompletionItemKind::PROPERTY) => Self::Property,
            Some(CompletionItemKind::KEYWORD) => Self::Keyword,
            Some(CompletionItemKind::UNIT) => Self::Unit,
            Some(CompletionItemKind::VALUE) => Self::Value,
            Some(CompletionItemKind::ENUM) => Self::Enum,
            Some(CompletionItemKind::CONSTANT) => Self::Constant,
            Some(CompletionItemKind::ENUM_MEMBER) => Self::EnumMember,
            Some(CompletionItemKind::COLOR) => Self::Color,
            Some(CompletionItemKind::FILE) => Self::File,
            Some(CompletionItemKind::REFERENCE) => Self::Reference,
            Some(CompletionItemKind::SNIPPET) => Self::Snippet,
            _ => Self::Text,
        }
    }
}

impl CompletionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Method => "method",
            Self::Function => "function",
            Self::Constructor => "constructor",
            Self::Field => "field",
            Self::Variable => "variable",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Struct => "struct",
            Self::Module => "module",
            Self::Property => "property",
            Self::Keyword => "keyword",
            Self::Unit => "unit",
            Self::Value => "value",
            Self::Enum => "enum",
            Self::Constant => "constant",
            Self::EnumMember => "enum member",
            Self::Color => "color",
            Self::File => "file",
            Self::Reference => "reference",
            Self::Snippet => "snippet",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Struct,
    Module,
    Property,
    Keyword,
    Unit,
    Value,
    Enum,
    Constant,
    EnumMember,
    Color,
    File,
    Reference,
    Snippet,
}

/// LSP client state for a single document
#[derive(Debug)]
pub struct LspClient {
    pub document_uri: Option<String>,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub capabilities: Option<LspCapabilities>,
    pub completions: Vec<CompletionEntry>,
    /// Currently shown completion popup state
    pub popup_visible: bool,
    pub popup_selected_index: usize,
    pub trigger_position: Option<Position>,
    pub trigger_text: String,
    /// Buffer ID this client is attached to
    pub buffer_id: Uuid,
}

impl LspClient {
    pub fn new(buffer_id: Uuid) -> Self {
        Self {
            document_uri: None,
            server_name: None,
            server_version: None,
            capabilities: None,
            completions: Vec::new(),
            popup_visible: false,
            popup_selected_index: 0,
            trigger_position: None,
            trigger_text: String::new(),
            buffer_id,
        }
    }

    pub fn set_document_uri(&mut self, uri: String) {
        self.document_uri = Some(uri);
    }

    pub fn set_server_info(&mut self, name: String, version: String) {
        self.server_name = Some(name);
        self.server_version = Some(version);
    }

    pub fn set_capabilities(&mut self, caps: LspCapabilities) {
        self.capabilities = Some(caps);
    }

    pub fn has_capability(&self, cap: &str) -> bool {
        match self.capabilities {
            Some(ref caps) => caps.supports.iter().any(|s| s == cap),
            None => false,
        }
    }

    pub fn show_completions(&mut self, completions: Vec<CompletionEntry>) {
        self.completions = completions;
        self.popup_visible = true;
        self.popup_selected_index = 0;
    }

    pub fn hide_completions(&mut self) {
        self.popup_visible = false;
        self.completions.clear();
        self.popup_selected_index = 0;
    }

    pub fn select_next(&mut self) {
        if !self.completions.is_empty() {
            self.popup_selected_index = (self.popup_selected_index + 1) % self.completions.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.completions.is_empty() {
            self.popup_selected_index = self
                .popup_selected_index
                .checked_sub(1)
                .unwrap_or(self.completions.len() - 1);
        }
    }

    pub fn selected_completion(&self) -> Option<&CompletionEntry> {
        self.completions.get(self.popup_selected_index)
    }

    pub fn trigger(&mut self, text: &str, position: Position) {
        self.trigger_text = text.to_string();
        self.trigger_position = Some(position);
    }

    /// Try to get a server diagnostic (error/warning) for a position
    pub fn get_diagnostic_at(&self, line: usize, _col: usize) -> Option<String> {
        // In a real implementation, this would query diagnostic information
        // from the language server. For now, return None.
        None
    }
}

/// LSP server capabilities (simplified)
#[derive(Debug, Clone)]
pub struct LspCapabilities {
    pub supports: Vec<String>,
    pub completion_provider: bool,
    pub hover_provider: bool,
    pub diagnostic_provider: bool,
    pub signature_help_provider: bool,
    pub rename_provider: bool,
    pub format_provider: bool,
}

impl LspCapabilities {
    pub fn new() -> Self {
        Self {
            supports: Vec::new(),
            completion_provider: false,
            hover_provider: false,
            diagnostic_provider: false,
            signature_help_provider: false,
            rename_provider: false,
            format_provider: false,
        }
    }

    pub fn with_completion(mut self) -> Self {
        self.completion_provider = true;
        self.supports.push("completion".to_string());
        self
    }

    pub fn with_hover(mut self) -> Self {
        self.hover_provider = true;
        self.supports.push("hover".to_string());
        self
    }

    pub fn with_diagnostics(mut self) -> Self {
        self.diagnostic_provider = true;
        self.supports.push("diagnostic".to_string());
        self
    }
}

/// LSP workspace - manages clients for multiple documents
#[derive(Debug)]
pub struct LspWorkspace {
    pub clients: HashMap<Uuid, LspClient>,
    /// Simulated completions for demo when no real LSP server is connected
    pub simulated_completions: HashMap<String, Vec<CompletionEntry>>,
}

impl LspWorkspace {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            simulated_completions: HashMap::new(),
        }
    }

    pub fn register_client(&mut self, client: LspClient) -> Uuid {
        let id = client.buffer_id;
        self.clients.insert(id, client);
        id
    }

    pub fn get_client(&self, buffer_id: Uuid) -> Option<&LspClient> {
        self.clients.get(&buffer_id)
    }

    pub fn get_client_mut(&mut self, buffer_id: Uuid) -> Option<&mut LspClient> {
        self.clients.get_mut(&buffer_id)
    }

    pub fn remove_client(&mut self, buffer_id: Uuid) {
        self.clients.remove(&buffer_id);
    }

    /// Get active client for a buffer
    pub fn active_client(&self, buffer_id: Uuid) -> Option<&LspClient> {
        self.clients.get(&buffer_id)
    }

    /// Add simulated completions for a file type (used as demo when no LSP server)
    pub fn add_simulated_completions(
        &mut self,
        file_ext: &str,
        items: Vec<CompletionEntry>,
    ) {
        self.simulated_completions.insert(file_ext.to_string(), items);
    }

    /// Get simulated completions (fallback when no real LSP)
    pub fn get_simulated_completions(&self, file_ext: &str) -> Vec<CompletionEntry> {
        self.simulated_completions
            .get(file_ext)
            .cloned()
            .unwrap_or_default()
    }
}

/// Initialize default simulated completions for common file types
pub fn initialize_default_simulated_completions() -> HashMap<String, Vec<CompletionEntry>> {
    let mut map = HashMap::new();

    // Rust completions
    map.insert(
        "rs".to_string(),
        vec![
            CompletionEntry {
                label: "fn".to_string(),
                detail: Some("function".to_string()),
                description: Some("Define a function".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "fn ${1:name}(${2:params}) {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("1".to_string()),
            },
            CompletionEntry {
                label: "struct".to_string(),
                detail: Some("type".to_string()),
                description: Some("Define a struct".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "struct ${1:name} {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("2".to_string()),
            },
            CompletionEntry {
                label: "impl".to_string(),
                detail: Some("block".to_string()),
                description: Some("Implement a trait or struct".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "impl ${1:Type} for ${2:name} {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("3".to_string()),
            },
            CompletionEntry {
                label: "let".to_string(),
                detail: Some("statement".to_string()),
                description: Some("Bind a variable".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "let ${1:binding} = $0;".to_string(),
                range: None,
                sort_text: Some("4".to_string()),
            },
            CompletionEntry {
                label: "match".to_string(),
                detail: Some("expression".to_string()),
                description: Some("Destructure a value".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "match ${1:value} {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("5".to_string()),
            },
            CompletionEntry {
                label: "if let".to_string(),
                detail: Some("expression".to_string()),
                description: Some("Conditional pattern matching".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "if let ${1:pattern} = ${2:expression} {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("6".to_string()),
            },
            CompletionEntry {
                label: "while let".to_string(),
                detail: Some("expression".to_string()),
                description: Some("Loop while pattern matches".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "while let ${1:pattern} = ${2:expression} {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("7".to_string()),
            },
            CompletionEntry {
                label: "pub".to_string(),
                detail: Some("modifier".to_string()),
                description: Some("Public visibility".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "pub ".to_string(),
                range: None,
                sort_text: Some("8".to_string()),
            },
            CompletionEntry {
                label: "vec![]".to_string(),
                detail: Some("macro".to_string()),
                description: Some("Create a Vec".to_string()),
                kind: CompletionKind::Snippet,
                insert_text: "vec![$0]".to_string(),
                range: None,
                sort_text: Some("9".to_string()),
            },
            CompletionEntry {
                label: "println!".to_string(),
                detail: Some("macro".to_string()),
                description: Some("Print to stdout".to_string()),
                kind: CompletionKind::Snippet,
                insert_text: "println!(\"$0\");".to_string(),
                range: None,
                sort_text: Some("10".to_string()),
            },
        ],
    );

    // Python completions
    map.insert(
        "py".to_string(),
        vec![
            CompletionEntry {
                label: "def".to_string(),
                detail: Some("function".to_string()),
                description: Some("Define a function".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "def ${1:name}(${2:params}):\n    $0".to_string(),
                range: None,
                sort_text: Some("1".to_string()),
            },
            CompletionEntry {
                label: "class".to_string(),
                detail: Some("class".to_string()),
                description: Some("Define a class".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "class ${1:name}:\n    $0".to_string(),
                range: None,
                sort_text: Some("2".to_string()),
            },
            CompletionEntry {
                label: "if __name__".to_string(),
                detail: Some("entry point".to_string()),
                description: Some("Check script entry point".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "if __name__ == \"__main__\":\n    $0".to_string(),
                range: None,
                sort_text: Some("3".to_string()),
            },
            CompletionEntry {
                label: "for".to_string(),
                detail: Some("loop".to_string()),
                description: Some("For loop".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "for ${1:item} in ${2:iterable}:\n    $0".to_string(),
                range: None,
                sort_text: Some("4".to_string()),
            },
            CompletionEntry {
                label: "while".to_string(),
                detail: Some("loop".to_string()),
                description: Some("While loop".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "while ${1:condition}:\n    $0".to_string(),
                range: None,
                sort_text: Some("5".to_string()),
            },
            CompletionEntry {
                label: "import".to_string(),
                detail: Some("statement".to_string()),
                description: Some("Import a module".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "import ${1:module}".to_string(),
                range: None,
                sort_text: Some("6".to_string()),
            },
            CompletionEntry {
                label: "from".to_string(),
                detail: Some("statement".to_string()),
                description: Some("Import from a module".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "from ${1:module} import ${2:name}".to_string(),
                range: None,
                sort_text: Some("7".to_string()),
            },
            CompletionEntry {
                label: "print".to_string(),
                detail: Some("function".to_string()),
                description: Some("Print to stdout".to_string()),
                kind: CompletionKind::Function,
                insert_text: "print($0)".to_string(),
                range: None,
                sort_text: Some("8".to_string()),
            },
        ],
    );

    // JavaScript completions
    map.insert(
        "js".to_string(),
        vec![
            CompletionEntry {
                label: "function".to_string(),
                detail: Some("function".to_string()),
                description: Some("Define a function".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "function ${1:name}(${2:params}) {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("1".to_string()),
            },
            CompletionEntry {
                label: "const".to_string(),
                detail: Some("declaration".to_string()),
                description: Some("Declare a constant".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "const ${1:name} = $0;".to_string(),
                range: None,
                sort_text: Some("2".to_string()),
            },
            CompletionEntry {
                label: "let".to_string(),
                detail: Some("declaration".to_string()),
                description: Some("Declare a variable".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "let ${1:name} = $0;".to_string(),
                range: None,
                sort_text: Some("3".to_string()),
            },
            CompletionEntry {
                label: "console.log".to_string(),
                detail: Some("method".to_string()),
                description: Some("Log to console".to_string()),
                kind: CompletionKind::Function,
                insert_text: "console.log($0);".to_string(),
                range: None,
                sort_text: Some("4".to_string()),
            },
            CompletionEntry {
                label: "for".to_string(),
                detail: Some("loop".to_string()),
                description: Some("For loop".to_string()),
                kind: CompletionKind::Keyword,
                insert_text: "for (let ${1:i} = 0; ${1:i} < ${2:limit}; ${1:i}++) {\n    $0\n}".to_string(),
                range: None,
                sort_text: Some("5".to_string()),
            },
            CompletionEntry {
                label: "forEach".to_string(),
                detail: Some("method".to_string()),
                description: Some("Iterate over array".to_string()),
                kind: CompletionKind::Method,
                insert_text: ".forEach((${1:item}) => {\n    $0\n})".to_string(),
                range: None,
                sort_text: Some("6".to_string()),
            },
        ],
    );

    map
}

/// Find completions for the current buffer state (simulated or from real LSP)
pub fn find_completions(
    workspace: &LspWorkspace,
    buffer_id: Uuid,
    syntax: &str,
    _trigger_text: &str,
) -> Vec<CompletionEntry> {
    // Determine file extension from syntax name
    let ext = syntax_to_extension(syntax);
    workspace.get_simulated_completions(&ext)
}

/// Convert syntax name to file extension for lookup
fn syntax_to_extension(syntax: &str) -> String {
    match syntax {
        "rust" => "rs".to_string(),
        "python" => "py".to_string(),
        "javascript" | "typescript" => "js".to_string(),
        "cpp" => "cpp".to_string(),
        "html" => "html".to_string(),
        "css" => "css".to_string(),
        "json" => "json".to_string(),
        _ => "txt".to_string(),
    }
}

/// Render the completion popup in egui
pub fn render_completion_popup(
    ctx: &egui::Context,
    client: &LspClient,
) {
    use fuzzy_matcher::FuzzyMatcher;
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let popup_bg = egui::Color32::from_rgb(30, 30, 46);

    egui::Area::new(egui::Id::new(format!("lsp_completion_{}", client.buffer_id)))
        .order(egui::Order::Foreground)
        .default_pos(egui::pos2(200.0, 200.0))
        .show(ctx, |ui| {
            ui.set_max_width(400.0);
            ui.set_min_width(250.0);
            ui.visuals_mut().override_text_color = None;

            for (i, completion) in client.completions.iter().enumerate() {
                let is_selected = i == client.popup_selected_index;

                ui.horizontal(|ui| {
                    let kind_label = completion.kind.as_str();
                    let kind_color = match completion.kind {
                        CompletionKind::Keyword => egui::Color32::from_rgb(255, 120, 200),
                        CompletionKind::Function | CompletionKind::Method => egui::Color32::from_rgb(130, 170, 255),
                        CompletionKind::Class | CompletionKind::Struct | CompletionKind::Interface => egui::Color32::from_rgb(190, 210, 255),
                        CompletionKind::Variable | CompletionKind::Field => egui::Color32::from_rgb(200, 200, 200),
                        CompletionKind::Snippet => egui::Color32::from_rgb(100, 180, 255),
                        CompletionKind::Constant => egui::Color32::from_rgb(100, 180, 255),
                        _ => egui::Color32::from_rgb(150, 150, 170),
                    };

                    ui.label(egui::RichText::new(kind_label)
                        .color(kind_color)
                        .monospace()
                        .size(12.0));

                    ui.label(egui::RichText::new(&completion.label)
                        .color(if is_selected {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(200, 200, 200)
                        }));

                    if let Some(detail) = &completion.detail {
                        ui.label(egui::RichText::new(format!(" - {}", detail))
                            .color(egui::Color32::from_rgb(120, 120, 140))
                            .size(11.0));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(sort) = &completion.sort_text {
                            ui.label(egui::RichText::new(sort)
                                .color(egui::Color32::from_rgb(80, 80, 100))
                                .size(10.0));
                        }
                    });
                });

                if ui.rect_contains_pointer(ui.max_rect()) {
                    if i != client.popup_selected_index {
                        // Could update selected index via callback
                    }
                }
            }
        });
}

/// Filter completions based on user's typed text
pub fn filter_completions<'a>(
    completions: &'a [CompletionEntry],
    prefix: &str,
) -> Vec<&'a CompletionEntry> {
    if prefix.is_empty() {
        return completions.iter().collect();
    }

    let prefix_lower = prefix.to_lowercase();
    completions
        .iter()
        .filter(|c| c.label.to_lowercase().contains(&prefix_lower))
        .collect()
}
