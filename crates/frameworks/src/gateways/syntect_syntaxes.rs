use std::path::Path;

use entities::document::DocumentId;
use entities::symbol::Symbol;
use use_cases::gateways::SyntaxGateway;

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

    fn forget(&self, document: DocumentId) {
        self.0.borrow_mut().forget(document);
    }
}
