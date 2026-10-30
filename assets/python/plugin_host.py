"""CodeLibre Text plugin host: loads Sublime Text plugins and runs their commands.

The editor starts it as ``python -I -u plugin_host.py`` and talks to it over stdin/stdout, one
JSON object per line (see ``codelibre_rpc.py`` and ``docs/plugins.md``):

    editor -> host   {"op": "load", "packages": [{"name": ..., "dirs": [...]}], "paths": {...}}
    host -> editor   {"op": "loaded", "commands": [[name, kind], ...], "events": [...], "errors": [...]}
    editor -> host   {"op": "run", "kind": ..., "name": ..., "args": {...}, "view": id, "window": id}
    editor -> host   {"op": "event", "name": "on_modified", "view": id}
    editor -> host   {"op": "callback", "id": n, "name": "select", "args": [index]}   (a panel's answer)
    editor -> host   {"op": "query", "name": "on_query_context", "view": id, "window": 1, "args": [...]}
                     (also "command_states", "command_input", "settings_changed", "on_init", "on_exit")
    host -> editor   {"op": "answer", "value": true / ["command", {args}] / null}       (ends a query)
    host -> editor   {"op": "call", "id": n, "fn": "view_insert", "args": [...]}     (any number of times)
    editor -> host   {"op": "result", "id": n, "value": ...}  or  {"op": "result", "id": n, "error": "..."}
    host -> editor   {"op": "done", "error": null or "traceback"}

Between requests the main loop runs the ``set_timeout`` callbacks; they (and other threads) may
call the editor at any time, and the editor answers those calls while it is idle too.
"""

import importlib.abc
import importlib.machinery
import importlib.util
import json
import os
import sys
import traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import codelibre_rpc  # noqa: E402

codelibre_rpc.setup()

import sublime  # noqa: E402
import sublime_plugin  # noqa: E402

# Modules of the loaded plugins, to call plugin_unloaded() and forget them on reload.
_plugin_modules = []
_package_names = []
_next_edit_token = 0


class ArchiveLoader(importlib.abc.SourceLoader):
    """Runs a module of an unpacked ``.sublime-package`` under the path it would have inside the
    archive, ``<Installed Packages>/<package>.sublime-package/<file>``, as in Sublime: plugins
    (Package Control) work out where things are from ``__file__``."""

    def __init__(self, fullname, path):
        self.fullname = fullname
        self.path = path

    def get_filename(self, fullname=None):
        return self.path

    def get_data(self, path):
        with open(ArchiveFinder.real_path(path), "rb") as file:
            return file.read()


class ArchiveFinder:
    """Finds the modules of packages by their paths inside the archive."""

    # Path inside the archive -> folder it was unpacked to.
    roots = {}

    @classmethod
    def real_path(cls, path):
        for virtual, real in cls.roots.items():
            if path == virtual or path.startswith(virtual + os.sep):
                return real + path[len(virtual):]
        return None

    @classmethod
    def find_spec(cls, fullname, path=None, target=None):
        name = fullname.rpartition(".")[2]
        for entry in path or []:
            if cls.real_path(entry) is None:
                continue
            package = os.path.join(entry, name)
            init = os.path.join(package, "__init__.py")
            if os.path.isfile(cls.real_path(init)):
                return importlib.util.spec_from_file_location(
                    fullname, init, loader=ArchiveLoader(fullname, init), submodule_search_locations=[package]
                )
            module = package + ".py"
            if os.path.isfile(cls.real_path(module)):
                return importlib.util.spec_from_file_location(fullname, module, loader=ArchiveLoader(fullname, module))
            if os.path.isdir(cls.real_path(package)):
                spec = importlib.machinery.ModuleSpec(fullname, None, is_package=True)
                spec.submodule_search_locations = [package]
                return spec
        return None


sys.meta_path.insert(0, ArchiveFinder)


def visible_path(directory, archives):
    """The path plugins see for a package's folder; registers it when it was unpacked."""
    parent, name = os.path.split(directory)
    for unpacked, archive_folder in archives:
        if os.path.normpath(parent) == os.path.normpath(unpacked):
            virtual = os.path.join(archive_folder, name + ".sublime-package")
            ArchiveFinder.roots[virtual] = directory
            return virtual
    return directory


def unload_plugins():
    for module in _plugin_modules:
        unloaded = getattr(module, "plugin_unloaded", None)
        if callable(unloaded):
            try:
                unloaded()
            except Exception:
                traceback.print_exc()
    for name in list(sys.modules):
        if any(name == p or name.startswith(p + ".") for p in _package_names):
            del sys.modules[name]
    del _plugin_modules[:]
    del _package_names[:]
    ArchiveFinder.roots.clear()
    importlib.invalidate_caches()
    sublime_plugin.reset()
    sublime._main_timers.clear()
    sublime._async_timers.clear()


def load_package(name, dirs, archives, errors):
    """Imports the top-level ``.py`` files of a package as ``<package>.<file>`` so that relative
    imports between them work, as in Sublime. ``dirs`` go in load order: a file of a later one
    overrides the same file of an earlier one."""
    package = importlib.util.module_from_spec(importlib.machinery.ModuleSpec(name, None, is_package=True))
    package.__path__ = [visible_path(directory, archives) for directory in reversed(dirs)]
    sys.modules[name] = package
    _package_names.append(name)
    loaded = set()
    for directory, visible in zip(reversed(dirs), package.__path__):
        for file_name in sorted(os.listdir(directory)):
            if not file_name.endswith(".py") or file_name in loaded:
                continue
            loaded.add(file_name)
            module_name = "%s.%s" % (name, file_name[:-3])
            path = os.path.join(visible, file_name)
            try:
                loader = ArchiveLoader(module_name, path) if visible != directory else None
                spec = importlib.util.spec_from_file_location(module_name, path, loader=loader)
                module = importlib.util.module_from_spec(spec)
                sys.modules[module_name] = module
                spec.loader.exec_module(module)
            except Exception:
                sys.modules.pop(module_name, None)
                errors.append("%s:\n%s" % (path, traceback.format_exc()))
                continue
            sublime_plugin.register_module(module)
            _plugin_modules.append(module)


def add_library_folders():
    """``Lib/python38``, next to this file's folder, where Package Control installs libraries.
    It also lists ``python33``, whose libraries are for a Python we do not run."""
    lib = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.makedirs(os.path.join(lib, "python33"), exist_ok=True)
    folder = os.path.join(lib, "python38")
    os.makedirs(folder, exist_ok=True)
    if folder not in sys.path:
        sys.path.append(folder)


def load(message):
    unload_plugins()
    sublime._paths.update(message.get("paths") or {})
    add_library_folders()
    errors = []
    for package in message.get("packages", []):
        load_package(package["name"], package["dirs"], message.get("archives") or [], errors)
    for module in _plugin_modules:
        loaded = getattr(module, "plugin_loaded", None)
        if callable(loaded):
            try:
                loaded()
            except Exception:
                errors.append("%s plugin_loaded():\n%s" % (module.__name__, traceback.format_exc()))
    finish_command()
    commands = [[name, kind] for name, (kind, _) in sorted(sublime_plugin.commands.items())]
    return {"op": "loaded", "commands": commands, "events": sublime_plugin.events(), "errors": errors}


def run(message):
    global _next_edit_token
    name = message["name"]
    if name not in sublime_plugin.commands:
        return {"op": "done", "error": "no plugin command named %r" % name}
    args = message.get("args") or {}
    error = None
    try:
        kind, command = sublime_plugin.command_object(name, message.get("view") or 0, message.get("window") or 1)
        if sublime_plugin.call_with_args(command.is_enabled, args):
            if kind == "text":
                _next_edit_token += 1
                command.run(sublime.Edit(_next_edit_token), **args)
            else:
                command.run(**args)
    except Exception:
        error = traceback.format_exc()
    finish_command()
    return {"op": "done", "error": error}


def event(message):
    """A view event: the listeners' tracebacks go to the console, so the event itself succeeds."""
    sublime_plugin.dispatch(message["name"], message.get("view") or 0)
    finish_command()
    return {"op": "done", "error": None}


def callback(message):
    error = None
    try:
        sublime.run_callback(message.get("id"), message.get("name"), message.get("args") or [])
    except Exception:
        error = traceback.format_exc()
    finish_command()
    return {"op": "done", "error": error}


def query(message):
    """A question for the listeners (``on_query_context``, ``on_text_command``…); their
    tracebacks go to the console and count as no answer."""
    value = sublime_plugin.query(message["name"], message.get("view") or 0, message.get("window") or 1,
                                 message.get("args") or [])
    try:
        json.dumps(value)
    except (TypeError, ValueError):
        print("%s answered %r, which is not JSON" % (message["name"], value))
        value = None
    finish_command()
    return {"op": "answer", "value": value}


def finish_command():
    sublime._cleared_selections.clear()


def handle(message):
    op = message.get("op")
    if op == "load":
        codelibre_rpc.send(load(message))
    elif op == "run":
        codelibre_rpc.send(run(message))
    elif op == "event":
        codelibre_rpc.send(event(message))
    elif op == "callback":
        codelibre_rpc.send(callback(message))
    elif op == "query":
        codelibre_rpc.send(query(message))
    elif op == "wake":
        pass
    elif op == "exit":
        sys.exit(0)
    else:
        codelibre_rpc.send({"op": "done", "error": "unknown request %r" % op})


def main():
    codelibre_rpc.handler = handle
    while True:
        sublime._main_timers.run_due()
        sublime._cleared_selections.clear()
        message = codelibre_rpc.next_request(sublime._main_timers.next_delay())
        if message is not None:
            handle(message)


if __name__ == "__main__":
    main()
