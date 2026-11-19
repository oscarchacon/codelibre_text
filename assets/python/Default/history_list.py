"""The jump history of ``Default.history_list``. The editor keeps it, one per window, as Sublime
Text 4 does (Goto > Jump Back / Jump Forward, ``jump_back``, ``jump_forward``,
``add_jump_record``); this module is what plugins import to add to it (Anaconda's Find Usages,
Goto Definition push the place they leave)."""

import sublime

LIST_LIMIT = 120


def _regions(selection):
    regions = []
    for region in selection:
        if isinstance(region, sublime.Region):
            regions.append([region.a, region.b])
        elif isinstance(region, int):
            regions.append([region, region])
        else:
            regions.append([int(region[0]), int(region[1])])
    return regions


def _place(view):
    window = view.window()
    active = window.active_view() if window else None
    if active is None:
        return None
    return active.id(), [(r.a, r.b) for r in active.sel()]


class JumpHistory:
    """The jump history of a window: the editor's, reached through its commands."""

    LIST_LIMIT = LIST_LIMIT

    def __init__(self, window=None):
        self.window = window

    def push_selection(self, view, selection=None, is_activation=False):
        """Records the view's selection (or ``selection``: regions, points or pairs) as a place
        to jump back to."""
        regions = _regions(view.sel() if selection is None else selection)
        if regions:
            view.run_command("add_jump_record", {"selection": regions})

    def _jump(self, view, command):
        window = view.window()
        if window is None:
            return False
        before = _place(view)
        window.run_command(command)
        return _place(view) != before

    def back(self, view):
        """Goes to the place before the current one, keeping where the caret is now to come
        forward to; False when there is none."""
        self.push_selection(view)
        return self._jump(view, "jump_back")

    def forward(self, view):
        """Goes to the place after the current one; False when there is none."""
        return self._jump(view, "jump_forward")


def _history_for_window(window):
    return JumpHistory(window)


def _history_for_view(view):
    return _history_for_window(view.window())


def get_jump_history(window_id):
    """Sublime 3's: the history of the window with this id."""
    return JumpHistory(sublime.Window(window_id))


# Sublime 3's name, which Anaconda and others still use.
get_jump_history_for_view = _history_for_view
