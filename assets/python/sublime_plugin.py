"""CodeLibre Text's implementation of Sublime Text's ``sublime_plugin`` module.

Application, window and text commands, their input handlers, and the view events of
``EventListener`` and ``ViewEventListener``, and ``TextChangeListener``.
"""

import traceback

import sublime

# ----- commands ----------------------------------------------------------------------------------


def command_name(class_name):
    """``FooBarCommand`` -> ``foo_bar``, the way Sublime names commands. Like Sublime, an
    upper-case letter right after another one is kept as it is (``HTMLCommand`` -> ``h_tML``)."""
    name = class_name[0].lower()
    previous_upper = False
    for c in class_name[1:]:
        if c.isupper() and not previous_upper:
            name += "_" + c.lower()
        else:
            name += c
        previous_upper = c.isupper()
    if name.endswith("_command"):
        name = name[:-len("_command")]
    return name


def call_with_args(function, args):
    """Calls ``function(**args)``; like Sublime, falls back to no arguments if it takes none."""
    try:
        return function(**args)
    except TypeError:
        if not args:
            raise
        return function()


class Command:
    def name(self):
        return command_name(type(self).__name__)

    def is_enabled(self):
        return True

    def is_visible(self):
        return True

    def is_checked(self):
        return False

    def description(self):
        return None

    def want_event(self):
        return False

    def input(self, args):
        return None

    def input_description(self):
        return None


class ApplicationCommand(Command):
    pass


class WindowCommand(Command):
    def __init__(self, window):
        self.window = window


class TextCommand(Command):
    def __init__(self, view):
        self.view = view


# ----- event listeners ---------------------------------------------------------------------------

class EventListener:
    pass


class ViewEventListener:
    @classmethod
    def is_applicable(cls, settings):
        return True

    @classmethod
    def applies_to_primary_view_only(cls):
        return True

    def __init__(self, view):
        self.view = view


class TextChangeListener:
    @classmethod
    def is_applicable(cls, buffer):
        return True

    def __init__(self):
        self.buffer = None

    def attach(self, buffer):
        self.buffer = buffer
        _text_listeners.setdefault(buffer.buffer_id(), []).append(self)

    def detach(self):
        _text_listeners.get(self.buffer.buffer_id(), []).remove(self)
        self.buffer = None

    def is_attached(self):
        return self.buffer is not None


# ----- input handlers ----------------------------------------------------------------------------

class CommandInputHandler:
    def name(self):
        return command_name(type(self).__name__).replace("_input_handler", "")

    def placeholder(self):
        return ""

    def initial_text(self):
        return ""

    def preview(self, arg):
        return ""

    def validate(self, arg):
        return True

    def cancel(self):
        pass

    def confirm(self, arg):
        pass

    def next_input(self, args):
        return None


class TextInputHandler(CommandInputHandler):
    pass


class ListInputHandler(CommandInputHandler):
    def list_items(self):
        return []


class BackInputHandler(CommandInputHandler):
    pass


class _CommandInput:
    """Asks for a command's arguments, one handler after the other, in the window's quick and input
    panels, then runs the command with them. Runs only from panel callbacks: the editor is not
    waiting for it."""

    def __init__(self, name, args, kind, view_id, window_id, description):
        self.name = name
        self.args = args
        self.kind = kind
        self.view_id = view_id
        self.window = sublime.Window(window_id)
        self.description = description
        # The handlers already answered, to go back to.
        self.answered = []

    def ask(self, handler):
        if isinstance(handler, ListInputHandler):
            self._ask_list(handler)
        else:
            self._ask_text(handler, handler.initial_text())

    def _ask_text(self, handler, text):
        caption = self.description or handler.placeholder() or handler.name()
        on_change = None
        if type(handler).preview is not CommandInputHandler.preview:
            on_change = lambda value: self._preview(handler, value)
        self.window.show_input_panel(
            caption, text, lambda value: self._answer(handler, value), on_change, handler.cancel,
        )

    def _ask_list(self, handler):
        items = handler.list_items()
        selected = -1
        if isinstance(items, tuple) and len(items) == 2:
            items, selected = items
        values = [_item_value(item) for item in items]

        def select(index):
            if index < 0:
                handler.cancel()
            else:
                self._answer(handler, values[index])

        on_highlight = None
        if type(handler).preview is not CommandInputHandler.preview:
            on_highlight = lambda index: self._preview(handler, values[index])
        placeholder = handler.placeholder() or self.description
        rows = [item[0] if isinstance(item, tuple) else item for item in items]
        self.window.show_quick_panel(rows, select, 0, selected, on_highlight, placeholder)

    def _preview(self, handler, value):
        preview = handler.preview(value)
        if preview:
            sublime.status_message(str(getattr(preview, "contents", preview)))

    def _answer(self, handler, value):
        if not handler.validate(value):
            if isinstance(handler, ListInputHandler):
                self.ask(handler)
            else:
                self._ask_text(handler, value)
            return
        handler.confirm(value)
        if handler.name():
            self.args[handler.name()] = value
        following = handler.next_input(self.args)
        if isinstance(following, BackInputHandler):
            self._back(handler)
        elif following is not None:
            self.answered.append(handler)
            self.ask(following)
        else:
            self._run()

    def _back(self, handler):
        self.args.pop(handler.name(), None)
        if self.answered:
            previous = self.answered.pop()
            self.args.pop(previous.name(), None)
            self.ask(previous)
        else:
            self.ask(handler)

    def _run(self):
        if self.kind == "text":
            sublime.View(self.view_id).run_command(self.name, self.args)
        elif self.kind == "window":
            self.window.run_command(self.name, self.args)
        else:
            sublime.run_command(self.name, self.args)


def _item_value(item):
    if isinstance(item, sublime.ListInputItem):
        return item.value
    if isinstance(item, tuple):
        return item[1]
    return item


# ----- registry (used by plugin_host.py) ---------------------------------------------------------

KINDS = ((TextCommand, "text"), (WindowCommand, "window"), (ApplicationCommand, "application"))
_BASES = (Command, ApplicationCommand, WindowCommand, TextCommand)

# The view events the editor sends; each also has an ``_async`` variant run on the async thread.
VIEW_EVENTS = (
    "on_new", "on_load", "on_activated", "on_deactivated", "on_pre_save", "on_post_save",
    "on_pre_close", "on_close", "on_modified", "on_selection_modified", "on_revert", "on_reload",
)
# Methods the editor asks (``query``): the first listener's answer wins; the ``on_post_*`` ones
# answer nothing, every listener hears them.
QUERY_EVENTS = (
    "on_query_context", "on_text_command", "on_window_command", "on_post_text_command", "on_post_window_command",
    "on_init", "on_exit",
)
# What the editor asks about commands, and the ``Command`` methods that make it worth asking.
COMMAND_QUERIES = {
    "command_states": ("is_visible", "is_enabled", "is_checked", "description"),
    "command_input": ("input",),
}
# Key binding context operators, as the keymap writes them -> ``sublime.OP_*``.
_OPERATORS = {
    "equal": sublime.OP_EQUAL, "not_equal": sublime.OP_NOT_EQUAL, "regex_match": sublime.OP_REGEX_MATCH,
    "not_regex_match": sublime.OP_NOT_REGEX_MATCH, "regex_contains": sublime.OP_REGEX_CONTAINS,
    "not_regex_contains": sublime.OP_NOT_REGEX_CONTAINS,
}

# name -> (kind, class)
commands = {}
# (name, view or window id) -> command object; Sublime keeps one object per view / window.
_instances = {}
# One object per EventListener class; ViewEventListener classes, and their objects per view id.
_event_listeners = []
_view_listener_classes = []
_view_listeners = {}
# TextChangeListener classes, and the attached objects per buffer id.
_text_listener_classes = []
_text_listeners = {}


def reset():
    commands.clear()
    _instances.clear()
    del _event_listeners[:]
    del _view_listener_classes[:]
    _view_listeners.clear()
    del _text_listener_classes[:]
    _text_listeners.clear()


def register_module(module):
    """Registers the command classes defined in (or imported into) a plugin module, and the event
    listeners it defines."""
    for value in list(vars(module).values()):
        if not isinstance(value, type):
            continue
        defined_here = value.__module__ == module.__name__
        if issubclass(value, EventListener) and value is not EventListener and defined_here:
            _event_listeners.append(value())
        elif issubclass(value, ViewEventListener) and value is not ViewEventListener and defined_here:
            _view_listener_classes.append(value)
        elif issubclass(value, TextChangeListener) and value is not TextChangeListener and defined_here:
            _text_listener_classes.append(value)
        if value in _BASES or not issubclass(value, Command):
            continue
        for base, kind in KINDS:
            if issubclass(value, base):
                commands[command_name(value.__name__)] = (kind, value)
                break


def events():
    """The event names some listener has a method for, and the command queries some command has
    a method for (the editor sends only those)."""
    listeners = _event_listeners + _view_listener_classes
    names = [n for e in VIEW_EVENTS for n in (e, e + "_async")] + list(QUERY_EVENTS)
    found = [n for n in names if any(callable(getattr(listener, n, None)) for listener in listeners)]
    if _text_listener_classes:
        found.append("on_text_changed")
    classes = [cls for _, cls in commands.values()]
    for query_name, methods in COMMAND_QUERIES.items():
        if any(getattr(cls, m) is not getattr(Command, m) for cls in classes for m in methods):
            found.append(query_name)
    return found


def _listeners_of(view, create):
    if view.view_id not in _view_listeners and create:
        _view_listeners[view.view_id] = [c(view) for c in _view_listener_classes if c.is_applicable(view.settings())]
    return _view_listeners.get(view.view_id, [])


def _call(listener, name, args):
    """Calls a listener's method if it has one; its result, or None (also when it failed)."""
    method = getattr(listener, name, None)
    if callable(method):
        try:
            return method(*args)
        except Exception:
            traceback.print_exc()
    return None


def query(name, view_id, window_id, args):
    """A method the editor asks (see ``QUERY_EVENTS``): ``on_query_context`` answers true or false
    for a key binding context it knows, ``on_text_command`` / ``on_window_command`` a
    ``(name, args)`` that replaces the command. None when no listener answers."""
    args = list(args)
    if name == "command_states":
        return [command_state(n, a or {}, view_id, window_id) for n, a in args[0]]
    if name == "command_input":
        return start_input(args[0], args[1] or {}, view_id, window_id)
    if name == "settings_changed":
        sublime._settings_changed(args[0])
        return None
    if name == "on_text_changed":
        _text_changed(sublime.View(view_id), args[0])
        return None
    if name == "on_exit":
        for listener in _event_listeners:
            _call(listener, name, [])
        return None
    if name == "on_init":
        views = [sublime.View(view) for view in args[0]]
        for listener in _event_listeners:
            _call(listener, name, [views])
        return None
    if name in ("on_window_command", "on_post_window_command"):
        window = sublime.Window(window_id)
        calls = [(listener, [window] + args) for listener in _event_listeners]
    else:
        view = sublime.View(view_id)
        if name == "on_query_context":
            args[1] = _OPERATORS.get(args[1], args[1])
        calls = [(listener, [view] + args) for listener in _event_listeners]
        calls += [(listener, args) for listener in _listeners_of(view, create=bool(_view_listener_classes))]
    for listener, call_args in calls:
        result = _call(listener, name, call_args)
        if result is not None and not name.startswith("on_post_"):
            return bool(result) if name == "on_query_context" else result
    if name.startswith("on_post_"):
        for listener, call_args in calls:
            _call_async(listener, name + "_async", call_args)
    return None


def _call_async(listener, name, args):
    if callable(getattr(listener, name, None)):
        sublime.set_timeout_async(lambda: _call(listener, name, args))


def _buffer_listeners(view):
    """The listeners attached to a view's buffer, attaching the applicable classes the first time."""
    buffer = view.buffer()
    if buffer.buffer_id() not in _text_listeners:
        _text_listeners[buffer.buffer_id()] = []
        for cls in _text_listener_classes:
            if cls.is_applicable(buffer):
                cls().attach(buffer)
    return list(_text_listeners[buffer.buffer_id()])


def _text_changed(view, changes):
    def position(place):
        return sublime.HistoricPosition(place[0], place[1], place[2], place[2], place[2])

    changes = [
        sublime.TextChange(
            position(c["a"]), position(c["b"]), len(c["text"].encode("utf-16-le")) // 2, len(c["text"].encode("utf-8")),
            c["text"],
        )
        for c in changes
    ]
    for listener in _buffer_listeners(view):
        _call(listener, "on_text_changed", [changes])
        _call_async(listener, "on_text_changed_async", [changes])


def command_state(name, args, view_id, window_id):
    """What a command says about itself in menus and the palette; the defaults if it fails."""
    state = {"visible": True, "enabled": True, "checked": False, "description": None}
    try:
        _, command = command_object(name, view_id, window_id)
        state["visible"] = bool(call_with_args(command.is_visible, args))
        state["enabled"] = bool(call_with_args(command.is_enabled, args))
        state["checked"] = bool(call_with_args(command.is_checked, args))
        description = call_with_args(command.description, args)
        state["description"] = None if description is None else str(description)
    except Exception:
        traceback.print_exc()
    return state


def start_input(name, args, view_id, window_id):
    """A command chosen in the palette. True when its input handler now asks for the arguments
    (the command runs afterwards); false when it can run as it is."""
    if name not in commands:
        return False
    try:
        kind, command = command_object(name, view_id, window_id)
        handler = command.input(dict(args))
        if handler is None:
            return False
        _CommandInput(name, dict(args), kind, view_id, window_id, command.input_description()).ask(handler)
    except Exception:
        traceback.print_exc()
        return False
    return True


def dispatch(name, view_id):
    """Runs the listeners of a view event, then queues their ``_async`` variants."""
    view = sublime.View(view_id)
    view_listeners = _listeners_of(view, create=bool(_view_listener_classes) and name != "on_close")
    calls = [(listener, (view,)) for listener in _event_listeners]
    calls += [(listener, ()) for listener in view_listeners]
    for listener, args in calls:
        _call(listener, name, args)
    for listener, args in calls:
        _call_async(listener, name + "_async", args)
    if name in ("on_revert", "on_reload") and _text_listener_classes:
        for listener in _buffer_listeners(view):
            _call(listener, name, ())
            _call_async(listener, name + "_async", ())
    if name == "on_close":
        _view_listeners.pop(view_id, None)


def command_object(name, view_id, window_id):
    kind, cls = commands[name]
    key = (name, view_id if kind == "text" else window_id if kind == "window" else 0)
    if key not in _instances:
        if kind == "text":
            _instances[key] = cls(sublime.View(view_id))
        elif kind == "window":
            _instances[key] = cls(sublime.Window(window_id))
        else:
            _instances[key] = cls()
    return kind, _instances[key]
