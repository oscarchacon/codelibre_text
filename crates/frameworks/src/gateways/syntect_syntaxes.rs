use std::path::Path;

use entities::document::DocumentId;
use entities::symbol::Symbol;
use use_cases::gateways::{ContextFrame, ScopeSpan, SyntaxGateway, SyntaxInfo};

use crate::syntax_engine::SharedSyntaxEngine;

/// [`SyntaxGateway`] backed by the shared syntect engine.
pub struct SyntectSyntaxes(pub SharedSyntaxEngine);

impl SyntaxGateway for SyntectSyntaxes {
    fn reload(&mut self, include_sublime_packages: bool, errors: &mut Vec<String>) {
        self.0.borrow_mut().reload(include_sublime_packages, errors);
    }

    fn detect(&self, path: &Path, first_line: &str) -> String {
        self.0.borrow().detect(path, first_line)
    }

    fn resolve(&self, name_or_path: &str) -> String {
        self.0.borrow().resolve(name_or_path)
    }

    fn syntax_path(&self, syntax: &str) -> String {
        self.0.borrow().syntax_path(syntax)
    }

    fn base_scope(&self, syntax: &str) -> String {
        self.0.borrow().base_scope(syntax)
    }

    fn syntax_names(&self) -> Vec<String> {
        self.0.borrow().syntax_names()
    }

    fn color_scheme_names(&self) -> Vec<String> {
        self.0.borrow().scheme_names()
    }

    fn symbols(&self, document: DocumentId, text: &str, syntax: &str) -> Vec<Symbol> {
        self.0.borrow_mut().symbols(document, text, syntax)
    }

    fn line_scopes(&self, document: DocumentId, text: &str, syntax: &str, line: usize) -> Vec<ScopeSpan> {
        self.0.borrow_mut().line_scopes(document, text, syntax, line)
    }

    fn document_scopes(&self, document: DocumentId, text: &str, syntax: &str) -> Vec<Vec<ScopeSpan>> {
        self.0.borrow_mut().document_scopes(document, text, syntax)
    }

    fn context_backtrace(&self, document: DocumentId, text: &str, syntax: &str, line: usize, column: usize) -> Vec<ContextFrame> {
        self.0.borrow_mut().context_backtrace(document, text, syntax, line, column)
    }

    fn syntax_infos(&self) -> Vec<SyntaxInfo> {
        self.0.borrow().syntax_infos()
    }

    fn forget(&self, document: DocumentId) {
        self.0.borrow_mut().forget(document);
    }

    fn take_failures(&self) -> Vec<String> {
        self.0.borrow_mut().take_failures()
    }

    fn scheme_style(&self, scheme: &str) -> serde_json::Map<String, serde_json::Value> {
        self.0.borrow().scheme_style(scheme)
    }

    fn scope_style(&self, scheme: &str, scope: &str) -> serde_json::Map<String, serde_json::Value> {
        self.0.borrow().scope_style(scheme, scope)
    }

    fn scheme_palette(&self, scheme: &str) -> serde_json::Map<String, serde_json::Value> {
        self.0.borrow().scheme_palette(scheme)
    }
}
