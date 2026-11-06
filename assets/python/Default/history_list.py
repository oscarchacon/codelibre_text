"""The jump history of ``Default.history_list``: selections a plugin pushes before it moves the
caret far away (Anaconda's Find Usages, Goto Definition), so it can go back to them. The editor
has no Jump Back / Jump Forward commands yet; plugins can call ``back`` / ``forward`` themselves."""

import sublime

LIST_LIMIT = 120


class JumpHistory:
    """The selections pushed for one view, oldest first, and where the last jump left it."""

    def __init__(self):
        self.history_list = []
        self.current_item = -1

    def push_selection(self, view):
        """Keeps the view's selection as the newest entry (forward entries are dropped)."""
        regions = [(r.a, r.b) for r in view.sel()]
        if not regions:
            return
        del self.history_list[self.current_item + 1:]
        if not self.history_list or self.history_list[-1] != regions:
            self.history_list.append(regions)
        del self.history_list[:-LIST_LIMIT]
        self.current_item = len(self.history_list) - 1

    def _restore(self, view, regions):
        view.sel().clear()
        view.sel().add_all([sublime.Region(a, b) for a, b in regions])
        view.show(view.sel()[0])

    def back(self, view):
        """Selects the entry before the current one (keeping where the caret is now, when it moved
        since the newest entry, to come forward to); False when there is none."""
        if self.history_list and self.current_item == len(self.history_list) - 1:
            here = [(r.a, r.b) for r in view.sel()]
            if here and here != self.history_list[-1]:
                self.history_list.append(here)
                self.current_item += 1
        if self.current_item <= 0:
            return False
        self.current_item -= 1
        self._restore(view, self.history_list[self.current_item])
        return True

    def forward(self, view):
        """Selects the entry after the current one; False when there is none."""
        if self.current_item + 1 >= len(self.history_list):
            return False
        self.current_item += 1
        self._restore(view, self.history_list[self.current_item])
        return True

    def len(self):
        return len(self.history_list)


_histories = {}


def get_jump_history(view_id):
    return _histories.setdefault(view_id, JumpHistory())


def get_jump_history_for_view(view):
    return get_jump_history(view.id())
