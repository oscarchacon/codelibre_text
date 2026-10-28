"""Transport between the plugin host and CodeLibre Text: one JSON object per line.

The editor writes requests (``load``, ``run``, ``event``, ``result``) to our stdin; we write
``call``, ``log``, ``loaded`` and ``done`` messages to our stdout. Plugins must not write to stdout,
so ``setup()`` keeps a private handle on it and sends whatever plugins print as ``log`` messages.

A reader thread takes every line from stdin: a ``result`` goes to the thread whose ``call`` has the
same ``id`` (plugins may call the API from any thread), anything else is a request for the main
thread (``next_request``). While the main thread waits for a result it also serves requests (a
nested ``run`` when a plugin runs another plugin command).
"""

import collections
import io
import json
import os
import sys
import threading

_input = None
_output = None
_main_thread = threading.main_thread()

# Handles a request that arrives while the main thread waits for the result of a call.
handler = None

_condition = threading.Condition()
_write_lock = threading.Lock()
_requests = collections.deque()
_results = {}
_closed = False
_next_id = 0


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
    threading.Thread(target=_read, name="codelibre_rpc reader", daemon=True).start()


def _read():
    global _closed
    while True:
        line = _input.readline()
        if not line:
            break
        message = json.loads(line)
        with _condition:
            if message.get("op") == "result":
                _results[message.get("id")] = message
            else:
                _requests.append(message)
            _condition.notify_all()
    with _condition:
        _closed = True
        _condition.notify_all()


def send(message):
    data = (json.dumps(message) + "\n").encode("utf-8")
    with _write_lock:
        _output.write(data)


def post(message):
    """Queues a request for the main thread as if the editor had sent it (wakes up its wait)."""
    with _condition:
        _requests.append(message)
        _condition.notify_all()


def next_request(timeout=None):
    """The next request for the main thread, or None after ``timeout`` seconds. Exits when the
    editor closes our stdin."""
    with _condition:
        _condition.wait_for(lambda: _requests or _closed, timeout)
        if _requests:
            return _requests.popleft()
        if _closed:
            sys.exit(0)
        return None


def call(function, args):
    """Runs a ``sublime_api`` function in the editor and returns its result."""
    global _next_id
    on_main_thread = threading.current_thread() is _main_thread
    with _condition:
        _next_id += 1
        call_id = _next_id
    send({"op": "call", "id": call_id, "fn": function, "args": args})
    while True:
        with _condition:
            _condition.wait_for(lambda: call_id in _results or _closed or (on_main_thread and _requests))
            if call_id in _results:
                message = _results.pop(call_id)
                break
            if _closed:
                raise RuntimeError("the editor closed the plugin host")
            request = _requests.popleft()
        handler(request)
    if "error" in message:
        raise RuntimeError(message["error"])
    return message.get("value")
