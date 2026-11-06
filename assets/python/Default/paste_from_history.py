"""The clipboard history of ``Default.paste_from_history``. The editor keeps it (copy and cut add
to it, Edit > Paste from History lists it); this is its face for plugins (Terminus adds what it
copies to it)."""

import re

import sublime_api


class ClipboardHistory:
    """The texts copied, newest first, each with a one-line caption: ``(caption, text)``."""

    LIST_LIMIT = 15
    CAPTION_LENGTH = 45

    def push_text(self, text):
        """Puts ``text`` first, once (an earlier copy of it is dropped)."""
        if text:
            sublime_api.clipboard_history_push(str(text))

    def get(self):
        return [(self.caption(text), text) for text in sublime_api.clipboard_history()]

    def caption(self, text):
        caption = re.sub(r"^\s+", "", text.replace("\n", "").replace("\r", ""))
        if len(caption) > self.CAPTION_LENGTH:
            caption = caption[: self.CAPTION_LENGTH] + "..."
        return caption

    def del_duplicate(self, text):
        texts = [t for t in sublime_api.clipboard_history() if t != text]
        sublime_api.clipboard_history_clear()
        for t in reversed(texts):
            sublime_api.clipboard_history_push(t)

    def empty(self):
        return not sublime_api.clipboard_history()

    @property
    def storage(self):
        return self.get()


g_clipboard_history = ClipboardHistory()
