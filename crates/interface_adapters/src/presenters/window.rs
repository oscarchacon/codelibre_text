use use_cases::Editor;

/// Window title: the active file's path, a dot when it has unsaved changes.
pub fn present_title(editor: &Editor) -> String {
    match editor.active_document() {
        Some(doc) => {
            let name = doc.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| doc.name());
            let dirty = if doc.is_dirty() { " •" } else { "" };
            format!("{name}{dirty} - CodeLibre Text")
        }
        None => "CodeLibre Text".to_string(),
    }
}
