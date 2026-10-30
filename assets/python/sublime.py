"""CodeLibre Text's implementation of Sublime Text's ``sublime`` module.

Written from the public API reference (https://www.sublimetext.com/docs/api_reference.html).
Regions, selections, views, the window, settings, commands and timers.
What is not implemented yet is listed in ``docs/plugins.md``; calling it raises AttributeError.
"""

import collections
import heapq
import itertools
import json
import os
import re
import sys
import threading
import time
import traceback

import codelibre_rpc
import sublime_api

# ----- constants -------------------------------------------------------------------------------

LITERAL = 1
IGNORECASE = 2

CLASS_WORD_START = 1
CLASS_WORD_END = 2
CLASS_PUNCTUATION_START = 4
CLASS_PUNCTUATION_END = 8
CLASS_SUB_WORD_START = 16
CLASS_SUB_WORD_END = 32
CLASS_LINE_START = 64
CLASS_LINE_END = 128
CLASS_EMPTY_LINE = 256

DRAW_EMPTY = 1
HIDE_ON_MINIMAP = 2
DRAW_EMPTY_AS_OVERWRITE = 4
PERSISTENT = 16
DRAW_OUTLINED = 32
DRAW_NO_FILL = 32
HIDDEN = 128
DRAW_NO_OUTLINE = 256
DRAW_SOLID_UNDERLINE = 512
DRAW_STIPPLED_UNDERLINE = 1024
DRAW_SQUIGGLY_UNDERLINE = 2048
NO_UNDO = 8192

ENCODED_POSITION = 1
TRANSIENT = 4
FORCE_GROUP = 8
SEMI_TRANSIENT = 16
ADD_TO_SELECTION = 32
REPLACE_MRU = 64
CLEAR_TO_RIGHT = 128

MONOSPACE_FONT = 1
KEEP_OPEN_ON_FOCUS_LOST = 2
WANT_EVENT = 4

KIND_ID_AMBIGUOUS = 0
KIND_ID_KEYWORD = 1
KIND_ID_TYPE = 2
KIND_ID_FUNCTION = 3
KIND_ID_NAMESPACE = 4
KIND_ID_NAVIGATION = 5
KIND_ID_MARKUP = 6
KIND_ID_VARIABLE = 7
KIND_ID_SNIPPET = 8
KIND_ID_COLOR_REDISH = 9
KIND_AMBIGUOUS = (KIND_ID_AMBIGUOUS, "", "")
KIND_KEYWORD = (KIND_ID_KEYWORD, "k", "Keyword")
KIND_TYPE = (KIND_ID_TYPE, "t", "Type")
KIND_FUNCTION = (KIND_ID_FUNCTION, "f", "Function")
KIND_NAMESPACE = (KIND_ID_NAMESPACE, "a", "Namespace")
KIND_NAVIGATION = (KIND_ID_NAVIGATION, "n", "Navigation")
KIND_MARKUP = (KIND_ID_MARKUP, "m", "Markup")
KIND_VARIABLE = (KIND_ID_VARIABLE, "v", "Variable")
KIND_SNIPPET = (KIND_ID_SNIPPET, "s", "Snippet")

DIALOG_CANCEL = 0
DIALOG_YES = 1
DIALOG_NO = 2

HOVER_TEXT = 1
HOVER_GUTTER = 2
HOVER_MARGIN = 3

COOPERATE_WITH_AUTO_COMPLETE = 2
HIDE_ON_MOUSE_MOVE = 4
HIDE_ON_MOUSE_MOVE_AWAY = 8

INHIBIT_WORD_COMPLETIONS = 8
INHIBIT_EXPLICIT_COMPLETIONS = 16

OP_EQUAL = 0
OP_NOT_EQUAL = 1
OP_REGEX_MATCH = 2
OP_NOT_REGEX_MATCH = 3
OP_REGEX_CONTAINS = 4
OP_NOT_REGEX_CONTAINS = 5

# ----- application -----------------------------------------------------------------------------

# Filled by the plugin host from the editor's ``load`` request.
_paths = {"packages": "", "installed_packages": "", "cache": "", "executable": ""}
# Views whose selection a plugin cleared and has not refilled yet (the editor always keeps one).
_cleared_selections = set()


def version():
    return "4200"


def channel():
    return "stable"


def platform():
    if sys.platform.startswith("win"):
        return "windows"
    if sys.platform == "darwin":
        return "osx"
    return "linux"


def arch():
    return "x64"


def executable_path():
    return _paths["executable"]


def packages_path():
    return _paths["packages"]


def installed_packages_path():
    return _paths["installed_packages"]


def cache_path():
    return _paths["cache"]


def status_message(msg):
    sublime_api.status_message(str(msg))


def error_message(msg):
    sublime_api.error_message(str(msg))


def message_dialog(msg):
    sublime_api.message_dialog(str(msg))


def ok_cancel_dialog(msg, ok_title="", title=""):
    return sublime_api.ok_cancel_dialog(str(msg), str(ok_title or ""), str(title or ""))


def yes_no_cancel_dialog(msg, yes_title="", no_title="", title=""):
    return sublime_api.yes_no_cancel_dialog(str(msg), str(yes_title or ""), str(no_title or ""), str(title or ""))


def _answer_later(callback, value):
    # Sublime answers file dialogs after the call returns.
    set_timeout(lambda: callback(value))


def open_dialog(callback, file_types=[], directory=None, multi_select=False, allow_folders=False):
    # ``allow_folders`` is not supported: the native dialog picks either files or folders.
    files = sublime_api.open_dialog(directory, list(file_types))
    if files is not None and not multi_select:
        files = files[0]
    _answer_later(callback, files)


def save_dialog(callback, file_types=[], directory=None, name=None, extension=None):
    _answer_later(callback, sublime_api.save_dialog(directory, name or "", extension or "", list(file_types)))


def select_folder_dialog(callback, directory=None, multi_select=False):
    folders = sublime_api.select_folder_dialog(directory, multi_select)
    if folders is not None and not multi_select:
        folders = folders[0]
    _answer_later(callback, folders)


def find_resources(pattern):
    return sublime_api.find_resources(pattern)


def load_resource(name):
    try:
        return sublime_api.load_resource(name)
    except RuntimeError as error:
        raise FileNotFoundError(str(error)) from None


def load_binary_resource(name):
    try:
        return bytes.fromhex(sublime_api.load_binary_resource(name))
    except RuntimeError as error:
        raise FileNotFoundError(str(error)) from None


def _strip_json_comments(text):
    """Sublime's JSON without its ``//`` and ``/* */`` comments and trailing commas."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            end = i + 1
            while end < n and text[end] != '"':
                end += 2 if text[end] == "\\" else 1
            out.append(text[i:end + 1])
            i = end + 1
        elif text.startswith("//", i):
            end = text.find("\n", i)
            i = n if end < 0 else end
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = n if end < 0 else end + 2
        else:
            if c in "]}":
                # Drop a trailing comma (strings are whole items of ``out``, so never touched).
                j = len(out) - 1
                while j >= 0 and out[j].isspace():
                    j -= 1
                if j >= 0 and out[j] == ",":
                    del out[j]
            out.append(c)
            i += 1
    return "".join(out)


def expand_variables(value, variables):
    """``$name``, ``${name}``, ``${name:default}`` and ``${name/regex/replacement/}`` in a string,
    or in every string of a list or dict."""
    if isinstance(value, str):
        return sublime_api.expand_variables(value, variables)
    if isinstance(value, list):
        return [expand_variables(item, variables) for item in value]
    if isinstance(value, dict):
        return {key: expand_variables(item, variables) for key, item in value.items()}
    return value


def decode_value(data):
    """Parses Sublime's JSON (comments and trailing commas allowed), as in resource files."""
    return json.loads(_strip_json_comments(data))


def encode_value(value, pretty=False):
    return json.dumps(value, indent=4 if pretty else None)


def get_clipboard(size_limit=16777216):
    return sublime_api.get_clipboard()[:size_limit]


def set_clipboard(text):
    sublime_api.set_clipboard(str(text))


class _Timers:
    """Callbacks waiting for their time. The main thread's ones (``set_timeout``) are run by the
    host's main loop (``run_due``); the async ones run on their own thread, like Sublime's."""

    def __init__(self, own_thread):
        self._condition = threading.Condition()
        self._heap = []
        self._count = itertools.count()
        self._own_thread = own_thread
        self._thread = None

    def add(self, callback, delay):
        with self._condition:
            due = time.monotonic() + max(delay or 0, 0) / 1000.0
            heapq.heappush(self._heap, (due, next(self._count), callback))
            if self._own_thread and self._thread is None:
                self._thread = threading.Thread(target=self._run_forever, name="sublime async", daemon=True)
                self._thread.start()
            self._condition.notify_all()

    def clear(self):
        with self._condition:
            del self._heap[:]

    def next_delay(self):
        """Seconds until the next callback is due, or None when there is none."""
        with self._condition:
            return max(0.0, self._heap[0][0] - time.monotonic()) if self._heap else None

    def run_due(self):
        """Runs the callbacks that are due (not the ones they add, which wait for the next turn)."""
        with self._condition:
            now, limit = time.monotonic(), next(self._count)
        while True:
            with self._condition:
                if not self._heap or self._heap[0][0] > now or self._heap[0][1] > limit:
                    return
                callback = heapq.heappop(self._heap)[2]
            _run_callback(callback)

    def _run_forever(self):
        while True:
            with self._condition:
                while True:
                    wait = self._heap[0][0] - time.monotonic() if self._heap else None
                    if wait is not None and wait <= 0:
                        callback = heapq.heappop(self._heap)[2]
                        break
                    self._condition.wait(wait)
            _run_callback(callback)


def _run_callback(callback):
    try:
        callback()
    except Exception:
        traceback.print_exc()


_main_timers = _Timers(own_thread=False)
_async_timers = _Timers(own_thread=True)


def set_timeout(callback, delay=0):
    _main_timers.add(callback, delay)
    if threading.current_thread() is not threading.main_thread():
        # The main thread may be waiting for a request: wake it so it sees the new timer.
        codelibre_rpc.post({"op": "wake"})


def set_timeout_async(callback, delay=0):
    _async_timers.add(callback, delay)


# Callbacks of open panels, by id: {"select": f, "highlight": f, ..., "final": names that close it}.
_callbacks = {}
_callback_ids = itertools.count(1)


def _register_callbacks(final, **functions):
    callback_id = next(_callback_ids)
    _callbacks[callback_id] = dict(functions, final=final)
    return callback_id


def run_callback(callback_id, name, args):
    """The editor answers a panel: ``select`` / ``highlight`` (quick panel), ``done`` / ``change``
    / ``cancel`` (input panel). A final answer forgets the panel."""
    entry = _callbacks.get(callback_id)
    if entry is None:
        return
    if name in entry["final"]:
        del _callbacks[callback_id]
    function = entry.get(name)
    if function is not None:
        function(*args)


class QuickPanelItem:
    def __init__(self, trigger, details="", annotation="", kind=KIND_AMBIGUOUS):
        self.trigger = trigger
        self.details = details
        self.annotation = annotation
        self.kind = kind

    def __repr__(self):
        return "QuickPanelItem(%r, %r, %r, %r)" % (self.trigger, self.details, self.annotation, self.kind)


HistoricPosition = collections.namedtuple("HistoricPosition", ["pt", "row", "col", "col_utf16", "col_utf8"])
TextChange = collections.namedtuple("TextChange", ["a", "b", "len_utf16", "len_utf8", "str"])


class ListInputItem:
    """A row of a ``ListInputHandler``: what it shows and the value the command gets."""

    def __init__(self, text, value, details="", annotation="", kind=KIND_AMBIGUOUS):
        self.text = text
        self.value = value
        self.details = details
        self.annotation = annotation
        self.kind = kind

    def __repr__(self):
        return "ListInputItem(%r, %r, %r, %r, %r)" % (self.text, self.value, self.details, self.annotation, self.kind)


def _quick_panel_item(item):
    """A quick panel row as ``[caption, detail, hint]``: from a string, a list of strings (the
    caption, then detail lines), a QuickPanelItem or a ListInputItem."""
    if isinstance(item, ListInputItem):
        item = QuickPanelItem(item.text, item.details, item.annotation, item.kind)
    if isinstance(item, QuickPanelItem):
        details = item.details if isinstance(item.details, (list, tuple)) else [item.details]
        letter = item.kind[1] if isinstance(item.kind, (list, tuple)) and len(item.kind) > 1 else ""
        return [str(item.trigger), " · ".join(str(d) for d in details if d), str(item.annotation or ""), letter]
    if isinstance(item, (list, tuple)):
        return [str(item[0]) if item else "", " · ".join(str(d) for d in item[1:] if d), ""]
    return [str(item), "", ""]


def active_window():
    return Window(1)


def windows():
    return [Window(1)]


def run_command(cmd, args=None):
    sublime_api.run_command("application", 0, cmd, args)


def load_settings(base_name):
    return Settings(["file", base_name])


def save_settings(base_name):
    sublime_api.save_settings(base_name)


def log_commands(flag=None):
    pass


def log_input(flag=None):
    pass


def log_result_regex(flag=None):
    pass


# ----- regions and selections --------------------------------------------------------------------

class Region:
    """A range of the buffer: ``a`` is where it starts (the anchor), ``b`` where the caret is."""

    __slots__ = ("a", "b", "xpos")

    def __init__(self, a, b=None, xpos=-1):
        if b is None:
            b = a
        self.a = a
        self.b = b
        self.xpos = xpos

    def __str__(self):
        return "(%d, %d)" % (self.a, self.b)

    def __repr__(self):
        return "Region(%d, %d)" % (self.a, self.b)

    def __len__(self):
        return self.size()

    def __eq__(self, rhs):
        return isinstance(rhs, Region) and self.a == rhs.a and self.b == rhs.b

    def __hash__(self):
        return hash((self.a, self.b))

    def __lt__(self, rhs):
        return (self.begin(), self.end()) < (rhs.begin(), rhs.end())

    def __iter__(self):
        return iter((self.a, self.b))

    def __contains__(self, value):
        return self.contains(value)

    def to_tuple(self):
        return (self.a, self.b)

    def empty(self):
        return self.a == self.b

    def begin(self):
        return min(self.a, self.b)

    def end(self):
        return max(self.a, self.b)

    def size(self):
        return abs(self.a - self.b)

    def contains(self, x):
        if isinstance(x, Region):
            return self.begin() <= x.begin() and x.end() <= self.end()
        return self.begin() <= x <= self.end()

    def cover(self, rhs):
        if self.a > self.b:
            return Region(max(self.a, rhs.end()), min(self.b, rhs.begin()))
        return Region(min(self.a, rhs.begin()), max(self.b, rhs.end()))

    def intersection(self, rhs):
        if self.end() <= rhs.begin() or rhs.end() <= self.begin():
            return Region(0)
        return Region(max(self.begin(), rhs.begin()), min(self.end(), rhs.end()))

    def intersects(self, rhs):
        if self.empty() or rhs.empty():
            return self.begin() == rhs.begin()
        return self.begin() < rhs.end() and rhs.begin() < self.end()


def _region(value):
    return Region(value) if isinstance(value, int) else value


def _merge(regions):
    """Sorted regions with overlapping ones (and repeated carets) merged, as Sublime keeps them."""
    merged = []
    for region in sorted(regions):
        last = merged[-1] if merged else None
        if last is None:
            merged.append(region)
            continue
        touches_caret = region.begin() == last.end() and (region.empty() or last.empty())
        if region.begin() < last.end() or region == last or touches_caret:
            merged[-1] = last.cover(region)
        else:
            merged.append(region)
    return merged


class Selection:
    """``view.sel()``: the view's selections, sorted by position."""

    def __init__(self, view_id):
        self.view_id = view_id

    def _get(self):
        if self.view_id in _cleared_selections:
            return []
        return [Region(a, b) for a, b in sublime_api.view_selection_get(self.view_id)]

    def _set(self, regions):
        if not regions:
            _cleared_selections.add(self.view_id)
            return
        _cleared_selections.discard(self.view_id)
        sublime_api.view_selection_set(self.view_id, [[r.a, r.b] for r in _merge(regions)])

    def __len__(self):
        return len(self._get())

    def __getitem__(self, index):
        return self._get()[index]

    def __iter__(self):
        return iter(self._get())

    def __eq__(self, rhs):
        return isinstance(rhs, Selection) and list(self) == list(rhs)

    def __str__(self):
        return "[%s]" % ", ".join(str(r) for r in self._get())

    def __repr__(self):
        return "Selection(%s)" % ", ".join(repr(r) for r in self._get())

    def is_valid(self):
        return View(self.view_id).is_valid()

    def clear(self):
        self._set([])

    def add(self, x):
        self._set(self._get() + [_region(x)])

    def add_all(self, regions):
        self._set(self._get() + [_region(x) for x in regions])

    def subtract(self, region):
        kept = []
        for r in self._get():
            if not r.intersects(region):
                kept.append(r)
                continue
            if r.begin() < region.begin():
                kept.append(Region(r.begin(), region.begin()))
            if region.end() < r.end():
                kept.append(Region(region.end(), r.end()))
        self._set(kept)

    def contains(self, region):
        return any(r.contains(region) for r in self._get())


# ----- settings ----------------------------------------------------------------------------------

# ``add_on_change`` callbacks by settings object (the same file seen through any Settings) and tag.
_on_change = {}


def _settings_changed(target):
    """Calls the ``add_on_change`` callbacks of a settings object that changed."""
    for callback in list(_on_change.get(tuple(target), {}).values()):
        try:
            callback()
        except Exception:
            traceback.print_exc()


class Settings:
    """A view's settings, a settings file (``load_settings``) or the window's settings."""

    def __init__(self, target):
        self.target = target

    def __getitem__(self, key):
        found, value = sublime_api.settings_get(self.target, key)
        if not found:
            raise KeyError(key)
        return value

    def __setitem__(self, key, value):
        self.set(key, value)

    def __delitem__(self, key):
        self.erase(key)

    def __contains__(self, key):
        return self.has(key)

    def get(self, key, default=None):
        found, value = sublime_api.settings_get(self.target, key)
        return value if found and value is not None else default

    def has(self, key):
        return sublime_api.settings_get(self.target, key)[0]

    def set(self, key, value):
        sublime_api.settings_set(self.target, key, value)
        _settings_changed(self.target)

    def erase(self, key):
        sublime_api.settings_erase(self.target, key)
        _settings_changed(self.target)

    def setdefault(self, key, value):
        if not self.has(key):
            self.set(key, value)
        return self.get(key)

    def update(self, other=(), **kwargs):
        pairs = other.items() if hasattr(other, "items") else other
        for key, value in list(pairs) + list(kwargs.items()):
            self.set(key, value)

    def to_dict(self):
        return sublime_api.settings_all(self.target)

    def add_on_change(self, tag, callback):
        # A plugin's own changes call it at once; the editor reports its own (``settings_changed``).
        _on_change.setdefault(tuple(self.target), {})[tag] = callback
        sublime_api.settings_watch(self.target)

    def clear_on_change(self, tag):
        _on_change.get(tuple(self.target), {}).pop(tag, None)


# ----- views and windows -------------------------------------------------------------------------

Syntax = collections.namedtuple("Syntax", ["path", "name", "hidden", "scope"])


class View:
    """A tab showing a document. ``view_id`` is the editor's document id."""

    def __init__(self, view_id):
        self.view_id = view_id
        self.selection = Selection(view_id)

    def __eq__(self, rhs):
        return isinstance(rhs, View) and self.view_id == rhs.view_id

    def __hash__(self):
        return self.view_id

    def __bool__(self):
        return self.view_id != 0

    def __repr__(self):
        return "View(%d)" % self.view_id

    def __len__(self):
        return self.size()

    def id(self):
        return self.view_id

    def buffer_id(self):
        return self.view_id

    def buffer(self):
        return self

    def primary_view(self):
        return self

    def is_valid(self):
        return sublime_api.view_is_valid(self.view_id)

    def is_primary(self):
        return True

    def window(self):
        return Window(1) if self.is_valid() else None

    def clones(self):
        return []

    def file_name(self):
        return sublime_api.view_file_name(self.view_id)

    def name(self):
        return sublime_api.view_name(self.view_id)

    def set_name(self, name):
        sublime_api.view_set_name(self.view_id, name)

    def is_loading(self):
        return False

    def is_dirty(self):
        return sublime_api.view_is_dirty(self.view_id)

    def change_count(self):
        return sublime_api.view_change_count(self.view_id)

    def is_read_only(self):
        return sublime_api.view_is_read_only(self.view_id)

    def set_read_only(self, read_only):
        sublime_api.view_set_read_only(self.view_id, bool(read_only))

    def is_scratch(self):
        return sublime_api.view_is_scratch(self.view_id)

    def set_scratch(self, scratch):
        sublime_api.view_set_scratch(self.view_id, bool(scratch))

    def close(self, on_close=None):
        closed = sublime_api.view_close(self.view_id)
        if on_close is not None:
            set_timeout(lambda: on_close(closed))
        return closed

    def encoding(self):
        return sublime_api.view_encoding(self.view_id)

    def line_endings(self):
        return sublime_api.view_line_endings(self.view_id)

    def set_line_endings(self, line_endings):
        sublime_api.view_set_line_endings(self.view_id, line_endings)

    def syntax(self):
        name = sublime_api.view_syntax(self.view_id)
        return Syntax("", name, False, "")

    def assign_syntax(self, syntax):
        if isinstance(syntax, Syntax):
            syntax = syntax.path or syntax.name
        sublime_api.view_assign_syntax(self.view_id, syntax)

    def set_syntax_file(self, syntax_file):
        self.assign_syntax(syntax_file)

    def settings(self):
        return Settings(["view", self.view_id])

    def size(self):
        return sublime_api.view_size(self.view_id)

    def substr(self, x):
        if isinstance(x, Region):
            return sublime_api.view_cached_substr(self.view_id, x.begin(), x.end())
        return sublime_api.view_cached_substr(self.view_id, x, x + 1)

    def insert(self, edit, pt, text):
        return sublime_api.view_insert(self.view_id, pt, text)

    def erase(self, edit, region):
        sublime_api.view_erase(self.view_id, region.begin(), region.end())

    def replace(self, edit, region, text):
        sublime_api.view_replace(self.view_id, region.begin(), region.end(), text)

    def sel(self):
        return self.selection

    def line(self, x):
        r = _region(x)
        return Region(*sublime_api.view_line(self.view_id, r.begin(), r.end()))

    def full_line(self, x):
        r = _region(x)
        return Region(*sublime_api.view_full_line(self.view_id, r.begin(), r.end()))

    def lines(self, region):
        return [Region(a, b) for a, b in sublime_api.view_lines(self.view_id, region.begin(), region.end())]

    def split_by_newlines(self, region):
        pieces = []
        start = region.begin()
        for line in self.substr(region).split("\n"):
            pieces.append(Region(start, start + len(line)))
            start += len(line) + 1
        return pieces

    def word(self, x):
        r = _region(x)
        return Region(*sublime_api.view_word(self.view_id, r.begin(), r.end()))

    def rowcol(self, tp):
        row, col = sublime_api.view_rowcol(self.view_id, tp)
        return (row, col)

    def text_point(self, row, col):
        return sublime_api.view_text_point(self.view_id, row, col)

    def find(self, pattern, start_pt, flags=0):
        return Region(*sublime_api.view_find(self.view_id, pattern, start_pt, flags))

    def find_all(self, pattern, flags=0, fmt=None, extractions=None):
        regions = [Region(a, b) for a, b in sublime_api.view_find_all(self.view_id, pattern, flags)]
        if fmt is not None and extractions is not None:
            expression = re.compile(re.escape(pattern) if flags & LITERAL else pattern,
                                    re.IGNORECASE if flags & IGNORECASE else 0)
            for region in regions:
                match = expression.match(self.substr(region))
                extractions.append(match.expand(fmt) if match else "")
        return regions

    def run_command(self, cmd, args=None):
        sublime_api.run_command("view", self.view_id, cmd, args)

    def show(self, x, show_surrounds=True, keep_to_left=False, animate=True):
        point = _point_of(x)
        if point is not None:
            sublime_api.view_show(self.view_id, point, False)

    def show_at_center(self, x, animate=True):
        point = _point_of(x)
        if point is not None:
            sublime_api.view_show(self.view_id, point, True)

    def visible_region(self):
        return Region(*sublime_api.view_visible_region(self.view_id))

    # Scopes need the syntax engine (phase C): nothing matches yet.
    def find_all_results(self):
        """The results of the view's ``result_*`` settings: ``(file, line, column)`` tuples."""
        return [(file, row, col) for file, row, col, _ in sublime_api.view_find_all_results(self.view_id)]

    def find_all_results_with_text(self):
        return [tuple(result) for result in sublime_api.view_find_all_results(self.view_id)]

    def scope_name(self, pt):
        return ""

    def match_selector(self, pt, selector):
        return False

    def score_selector(self, pt, selector):
        return 0

    def add_regions(self, key, regions, scope="", icon="", flags=0, annotations=None, annotation_color="",
                    on_navigate=None, on_close=None, **kwargs):
        """Marks regions (drawn with the scope's color, see ``DRAW_*``); they move with the text.
        Annotations are not shown yet."""
        rows = [[r.a, r.b] for r in (_region(r) for r in regions)]
        sublime_api.view_add_regions(self.view_id, str(key), rows, scope or "", icon or "", int(flags))

    def get_regions(self, key):
        return [Region(a, b) for a, b in sublime_api.view_get_regions(self.view_id, str(key))]

    def erase_regions(self, key):
        sublime_api.view_erase_regions(self.view_id, str(key))

    def set_status(self, key, value):
        sublime_api.view_set_status(self.view_id, str(key), str(value))

    def get_status(self, key):
        return sublime_api.view_get_status(self.view_id, str(key))

    def erase_status(self, key):
        sublime_api.view_erase_status(self.view_id, str(key))


def _point_of(x):
    """The point ``view.show`` scrolls to: a point, a region's start or a selection's first region."""
    if isinstance(x, int):
        return x
    if isinstance(x, Region):
        return x.begin()
    if isinstance(x, Selection):
        regions = list(x)
        return regions[0].begin() if regions else None
    if isinstance(x, (list, tuple)) and x:
        return _point_of(x[0])
    return None




class Window:
    """The editor window (there is only one)."""

    def __init__(self, window_id):
        self.window_id = window_id

    def __eq__(self, rhs):
        return isinstance(rhs, Window) and self.window_id == rhs.window_id

    def __hash__(self):
        return self.window_id

    def __bool__(self):
        return self.window_id != 0

    def __repr__(self):
        return "Window(%d)" % self.window_id

    def id(self):
        return self.window_id

    def is_valid(self):
        return True

    def settings(self):
        return Settings(["window", self.window_id])

    def active_view(self):
        view_id = sublime_api.window_active_view()
        return View(view_id) if view_id else None

    def views(self):
        return [View(view_id) for view_id in sublime_api.window_views()]

    def num_groups(self):
        return 1

    def active_group(self):
        return 0

    def active_view_in_group(self, group):
        return self.active_view()

    def views_in_group(self, group):
        return self.views()

    def get_view_index(self, view):
        ids = sublime_api.window_views()
        return (0, ids.index(view.id())) if view.id() in ids else (-1, -1)

    def focus_view(self, view):
        sublime_api.window_focus_view(view.id())

    def new_file(self, flags=0, syntax=""):
        view = View(sublime_api.window_new_file())
        if syntax:
            view.assign_syntax(syntax)
        return view

    def open_file(self, fname, flags=0, group=-1):
        if flags & ENCODED_POSITION:
            match = re.match(r"^(.*?)(?::(\d+))?(?::(\d+))?$", fname)
            fname, row, col = match.group(1), match.group(2), match.group(3)
        else:
            row = col = None
        view_id = sublime_api.window_open_file(fname)
        if not view_id:
            return None
        view = View(view_id)
        if row:
            point = view.text_point(int(row) - 1, int(col or 1) - 1)
            view.sel().clear()
            view.sel().add(Region(point))
        return view

    def find_open_file(self, fname):
        view_id = sublime_api.window_find_open_file(fname)
        return View(view_id) if view_id else None

    def run_command(self, cmd, args=None):
        sublime_api.run_command("window", self.window_id, cmd, args)

    def create_output_panel(self, name, unlisted=False):
        """The view of the output panel ``name``, created (or emptied) now. Show it with
        ``run_command("show_panel", {"panel": "output." + name})``."""
        return View(sublime_api.window_create_output_panel(self.window_id, name))

    def get_output_panel(self, name):
        return self.create_output_panel(name)

    def find_output_panel(self, name):
        view_id = sublime_api.window_find_output_panel(self.window_id, name)
        return View(view_id) if view_id else None

    def destroy_output_panel(self, name):
        sublime_api.window_destroy_output_panel(self.window_id, name)

    def active_panel(self):
        return sublime_api.window_active_panel(self.window_id)

    def panels(self):
        return sublime_api.window_panels(self.window_id)

    def show_quick_panel(self, items, on_select, flags=0, selected_index=-1, on_highlight=None, placeholder=None):
        """Shows a list to pick from; ``on_select`` gets the item's index, or -1 when cancelled."""
        def select(index):
            if on_select is None:
                return
            if flags & WANT_EVENT:
                on_select(index, {"modifier_keys": {}})
            else:
                on_select(index)
        callback = _register_callbacks(("select",), select=select, highlight=on_highlight)
        rows = [_quick_panel_item(item) for item in items]
        sublime_api.window_show_quick_panel(
            self.window_id, rows, flags, selected_index, placeholder or "", callback, on_highlight is not None
        )

    def show_input_panel(self, caption, initial_text, on_done, on_change, on_cancel):
        """A one-line text field at the bottom. Returns its view."""
        callback = _register_callbacks(("done", "cancel"), done=on_done, change=on_change, cancel=on_cancel)
        return View(sublime_api.window_show_input_panel(
            self.window_id, str(caption), str(initial_text), callback, on_change is not None
        ))

    def status_message(self, msg):
        status_message(msg)

    def folders(self):
        return sublime_api.window_folders()

    def project_file_name(self):
        return None

    def project_data(self):
        return {"folders": [{"path": folder} for folder in self.folders()]}

    def extract_variables(self):
        variables = {"packages": packages_path(), "platform": platform().capitalize()
                     if platform() != "osx" else "OSX"}
        folders = self.folders()
        if folders:
            variables["folder"] = folders[0]
        view = self.active_view()
        file_name = view.file_name() if view else None
        if file_name:
            base, extension = os.path.splitext(os.path.basename(file_name))
            variables.update({
                "file": file_name,
                "file_path": os.path.dirname(file_name),
                "file_name": os.path.basename(file_name),
                "file_base_name": base,
                "file_extension": extension.lstrip("."),
            })
        return variables


class Edit:
    """The token a TextCommand receives; edits are only allowed while it runs."""

    def __init__(self, token):
        self.edit_token = token

    def __repr__(self):
        return "Edit(%d)" % self.edit_token
