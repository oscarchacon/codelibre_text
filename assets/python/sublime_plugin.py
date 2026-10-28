"""CodeLibre Text's implementation of Sublime Text's ``sublime_plugin`` module.

Application, window and text commands, and the view events of ``EventListener`` and
``ViewEventListener``. The other base classes exist so plugins that define them still import, but
they are not called yet (see ``docs/plugins.md``).
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


# ----- not called yet (phase B) ------------------------------------------------------------------

class TextChangeListener:
    @classmethod
    def is_applicable(cls, buffer):
        return True

    def __init__(self):
        self.buffer = None


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


# ----- registry (used by plugin_host.py) ---------------------------------------------------------

KINDS = ((TextCommand, "text"), (WindowCommand, "window"), (ApplicationCommand, "application"))
_BASES = (Command, ApplicationCommand, WindowCommand, TextCommand)

# The view events the editor sends; each also has an ``_async`` variant run on the async thread.
VIEW_EVENTS = (
    "on_new", "on_load", "on_activated", "on_deactivated", "on_pre_save", "on_post_save",
    "on_pre_close", "on_close", "on_modified", "on_selection_modified",
)

# name -> (kind, class)
commands = {}
# (name, view or window id) -> command object; Sublime keeps one object per view / window.
_instances = {}
# One object per EventListener class; ViewEventListener classes, and their objects per view id.
_event_listeners = []
_view_listener_classes = []
_view_listeners = {}


def reset():
    commands.clear()
    _instances.clear()
    del _event_listeners[:]
    del _view_listener_classes[:]
    _view_listeners.clear()


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
        if value in _BASES or not issubclass(value, Command):
            continue
        for base, kind in KINDS:
            if issubclass(value, base):
                commands[command_name(value.__name__)] = (kind, value)
                break


def events():
    """The event names some listener has a method for (the editor sends only those)."""
    listeners = _event_listeners + _view_listener_classes
    names = [n for e in VIEW_EVENTS for n in (e, e + "_async")]
    return [n for n in names if any(callable(getattr(listener, n, None)) for listener in listeners)]


def _listeners_of(view, create):
    if view.view_id not in _view_listeners and create:
        _view_listeners[view.view_id] = [c(view) for c in _view_listener_classes if c.is_applicable(view.settings())]
    return _view_listeners.get(view.view_id, [])


def _call(listener, name, args):
    method = getattr(listener, name, None)
    if callable(method):
        try:
            method(*args)
        except Exception:
            traceback.print_exc()


def dispatch(name, view_id):
    """Runs the listeners of a view event, then queues their ``_async`` variants."""
    view = sublime.View(view_id)
    view_listeners = _listeners_of(view, create=bool(_view_listener_classes) and name != "on_close")
    calls = [(listener, (view,)) for listener in _event_listeners]
    calls += [(listener, ()) for listener in view_listeners]
    for listener, args in calls:
        _call(listener, name, args)
    for listener, args in calls:
        if callable(getattr(listener, name + "_async", None)):
            sublime.set_timeout_async(lambda l=listener, a=args: _call(l, name + "_async", a))
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
