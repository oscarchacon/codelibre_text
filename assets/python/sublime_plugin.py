"""CodeLibre Text's implementation of Sublime Text's ``sublime_plugin`` module.

Phase A: application, window and text commands. The other base classes exist so plugins that
define them still import, but they are not called yet (see ``docs/plugins.md``).
"""

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


# ----- not called yet (phase B) ------------------------------------------------------------------

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

# name -> (kind, class)
commands = {}
# (name, view or window id) -> command object; Sublime keeps one object per view / window.
_instances = {}


def reset():
    commands.clear()
    _instances.clear()


def register_module(module):
    """Registers the command classes defined in (or imported into) a plugin module."""
    for value in list(vars(module).values()):
        if not isinstance(value, type) or value in _BASES or not issubclass(value, Command):
            continue
        for base, kind in KINDS:
            if issubclass(value, base):
                commands[command_name(value.__name__)] = (kind, value)
                break


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
