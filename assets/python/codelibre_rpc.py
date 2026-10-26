"""Transport between the plugin host and CodeLibre Text: one JSON object per line.

The editor writes requests (``load``, ``run``, ``result``) to our stdin; we write ``call``,
``log``, ``loaded`` and ``done`` messages to our stdout. Plugins must not write to stdout, so
``setup()`` keeps a private handle on it and sends whatever plugins print as ``log`` messages.
"""

import io
import json
import os
import sys
import threading

_input = None
_output = None
_main_thread = threading.main_thread()

# Handles a request that arrives while we wait for the result of a call (a nested ``run``).
handler = None


class _Console(io.TextIOBase):
    """``sys.stdout`` / ``sys.stderr`` for plugins: the text goes to the editor's console."""

    def writable(self):
        return True

    def write(self, text):
        if text:
            send({"op": "log", "text": text})
        return len(text)


def setup():
    global _input, _output
    _input = sys.stdin.buffer
    _output = os.fdopen(os.dup(sys.stdout.fileno()), "wb", buffering=0)
    # Anything written to file descriptor 1 from now on (e.g. by a C extension) goes to stderr.
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    sys.stdin = io.StringIO()
    sys.stdout = _Console()
    sys.stderr = _Console()


def send(message):
    _output.write((json.dumps(message) + "\n").encode("utf-8"))


def receive():
    line = _input.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)


def call(function, args):
    """Runs a ``sublime_api`` function in the editor and returns its result."""
    if threading.current_thread() is not _main_thread:
        raise RuntimeError("the sublime API cannot be used from other threads yet")
    send({"op": "call", "fn": function, "args": args})
    while True:
        message = receive()
        if message.get("op") == "result":
            if "error" in message:
                raise RuntimeError(message["error"])
            return message.get("value")
        handler(message)
