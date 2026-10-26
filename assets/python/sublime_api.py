"""The editor's side of the API. ``sublime_api.view_insert(view_id, pt, text)`` becomes a call to
``view_insert`` in CodeLibre Text (``crates/use_cases/src/plugin_api.rs``)."""

import codelibre_rpc


def __getattr__(name):
    if name.startswith("__"):
        raise AttributeError(name)

    def function(*args):
        return codelibre_rpc.call(name, list(args))

    function.__name__ = name
    return function
