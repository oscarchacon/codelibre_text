use use_cases::gateways::ClipboardGateway;

/// [`ClipboardGateway`] on the system clipboard (`arboard`).
pub struct SystemClipboard;

impl ClipboardGateway for SystemClipboard {
    fn read_text(&self) -> Result<String, String> {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .map_err(|e| e.to_string())
    }
}
