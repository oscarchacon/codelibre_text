"""The clipboard history of ``Default.paste_from_history`` (Terminus adds what it copies to it)."""

import re


class ClipboardHistory:
    """The texts copied, newest first, each with a one-line caption: ``(caption, text)``."""

    LIST_LIMIT = 15
    CAPTION_LENGTH = 45

    def __init__(self):
        self.storage = []

    def push_text(self, text):
        """Puts ``text`` first, once (an earlier copy of it is dropped)."""
        if not text:
            return
        caption = re.sub(r"^\s+", "", text.replace("\n", ""))
        if len(caption) > self.CAPTION_LENGTH:
            caption = caption[: self.CAPTION_LENGTH] + "..."
        self.del_duplicate(text)
        self.storage.insert(0, (caption, text))
        del self.storage[self.LIST_LIMIT :]

    def get(self):
        return self.storage

    def del_duplicate(self, text):
        self.storage = [entry for entry in self.storage if entry[1] != text]

    def empty(self):
        return not self.storage


g_clipboard_history = ClipboardHistory()
