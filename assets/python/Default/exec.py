"""``Default.exec``: ``ExecCommand``, ``AsyncProcess`` and ``ProcessListener``, which build plugins
subclass (``from Default.exec import ExecCommand``). Written from Sublime's public API.

The editor's own ``exec`` is a built-in (``crates/use_cases/src/builds.rs``) and stays so: these
classes are not registered as commands (``sublime_plugin._is_own_default``), only their subclasses
are. They behave like the built-in: the ``exec`` output panel with the build's result settings,
``[Finished in …]``, ``[Cancelled]``, "Build finished with N errors".
"""

import codecs
import functools
import os
import signal
import subprocess
import sys
import threading
import time

import sublime
import sublime_plugin

# ``CREATE_NO_WINDOW``: console programs must not flash a window.
_CREATE_NO_WINDOW = 0x08000000


class ProcessListener:
    """What an ``AsyncProcess`` tells: ``on_data(proc, text)`` with what it printed (stdout and
    stderr), ``on_finished(proc)`` once it ended. Both are called on the process's reader thread."""

    def on_data(self, proc, data):
        pass

    def on_finished(self, proc):
        pass


class AsyncProcess:
    """Runs ``shell_cmd`` (a line for the shell) or ``cmd`` (a program and its arguments; a string,
    or a list with ``shell``, is a line for the shell) and reads its output on a thread. The text is
    decoded with ``listener.encoding`` (UTF-8 by default). ``env`` values expand ``$VARIABLES``;
    ``path`` replaces ``PATH`` (expanded too, so ``$PATH:/more`` works)."""

    def __init__(self, cmd, shell_cmd, env, listener, path="", shell=False, working_dir=None, **kwargs):
        if not shell_cmd and not cmd:
            raise ValueError("shell_cmd or cmd is required")
        if shell_cmd and not isinstance(shell_cmd, str):
            raise ValueError("shell_cmd must be a string")
        self.listener = listener
        self.killed = False
        self.start_time = time.time()

        proc_env = os.environ.copy()
        proc_env.update({str(k): os.path.expandvars(str(v)) for k, v in (env or {}).items()})
        if path:
            proc_env["PATH"] = os.path.expandvars(path)

        if shell_cmd:
            line = shell_cmd
        elif isinstance(cmd, str):
            line = cmd
        elif shell:
            line = " ".join(cmd)
        else:
            line = None

        options = {"stdin": subprocess.PIPE, "stdout": subprocess.PIPE, "stderr": subprocess.STDOUT,
                   "env": proc_env, "cwd": working_dir or None}
        if sys.platform == "win32":
            options["creationflags"] = _CREATE_NO_WINDOW
            self.proc = subprocess.Popen(line if line is not None else list(cmd), shell=line is not None, **options)
        else:
            # Its own process group, so ``kill`` stops what the shell started too.
            options["start_new_session"] = True
            self.proc = subprocess.Popen(["/bin/sh", "-c", line] if line is not None else list(cmd), **options)
        self.pid = self.proc.pid
        threading.Thread(target=self._read, name="exec output", daemon=True).start()

    def _read(self):
        encoding = getattr(self.listener, "encoding", None) or "utf-8"
        try:
            decoder = codecs.getincrementaldecoder(encoding)("replace")
        except LookupError:
            decoder = codecs.getincrementaldecoder("utf-8")("replace")
        stream = self.proc.stdout
        while True:
            try:
                data = os.read(stream.fileno(), 2 ** 15)
            except OSError:
                data = b""
            text = decoder.decode(data, final=not data)
            listener = self.listener
            if text and listener is not None:
                listener.on_data(self, text)
            if not data:
                break
        stream.close()
        self.proc.wait()
        listener = self.listener
        if listener is not None:
            listener.on_finished(self)

    def kill(self):
        """Stops the process and what it started; the listener hears nothing more."""
        if self.killed:
            return
        self.killed = True
        self.listener = None
        if self.proc.poll() is not None:
            return
        try:
            if sys.platform == "win32":
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(self.proc.pid)], creationflags=_CREATE_NO_WINDOW,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            else:
                os.killpg(self.proc.pid, signal.SIGTERM)
        except OSError:
            self.proc.kill()

    def poll(self):
        """True while the process runs."""
        return self.proc.poll() is None

    def exit_code(self):
        return self.proc.poll()


class ExecCommand(sublime_plugin.WindowCommand, ProcessListener):
    """Sublime's ``exec`` build target, for plugins that subclass it. The output goes to the
    ``exec`` output panel; ``on_data`` / ``on_finished`` hand over to the main thread
    (``append_data`` / ``finish``), which subclasses override as in Sublime."""

    proc = None
    output_view = None
    encoding = "utf-8"
    quiet = False
    errs_by_file = {}

    def run(self, cmd=None, shell_cmd=None, file_regex="", line_regex="", working_dir="", encoding="utf-8", env={},
            quiet=False, kill=False, kill_previous=False, update_annotations_only=False, word_wrap=True,
            syntax="Packages/Text/Plain text.tmLanguage", **kwargs):
        if update_annotations_only:
            return
        if kill:
            if self.proc is not None:
                self.proc.kill()
                self.proc = None
                self.write("\n%s\n" % sublime._tr("[Cancelled]"))
            return
        if self.proc is not None:
            # A new build replaces the running one, as the built-in's does.
            self.proc.kill()
            self.proc = None

        self.encoding = encoding or "utf-8"
        self.quiet = quiet
        if not working_dir:
            variables = self.window.extract_variables()
            working_dir = variables.get("file_path") or variables.get("folder") or ""
        self.working_dir = working_dir
        self.path = kwargs.get("path") or ""
        if shell_cmd:
            self.command_line = shell_cmd
        elif isinstance(cmd, str):
            self.command_line = cmd
        else:
            self.command_line = " ".join(cmd or [])

        self.output_view = self.window.create_output_panel("exec")
        settings = self.output_view.settings()
        settings.set("result_file_regex", file_regex or "")
        settings.set("result_line_regex", line_regex or "")
        settings.set("result_base_dir", working_dir)
        settings.set("word_wrap", word_wrap)
        settings.set("line_numbers", False)
        settings.set("gutter", False)
        settings.set("scroll_past_end", False)
        self.output_view.assign_syntax(syntax)
        self.output_view.set_read_only(True)
        if sublime.load_settings("Preferences.sublime-settings").get("show_panel_on_build", True):
            self.window.run_command("show_panel", {"panel": "output.exec"})

        try:
            self.proc = AsyncProcess(cmd, shell_cmd, env, self, path=self.path, shell=kwargs.get("shell", False),
                                     working_dir=working_dir or None)
        except Exception as error:
            self.proc = None
            self.write("%s\n" % sublime._tr("[Could not run: {error}]", error=error))

    def is_enabled(self, kill=False, **kwargs):
        if kill:
            return self.proc is not None and self.proc.poll()
        return True

    def write(self, characters):
        if self.output_view is not None:
            self.output_view.run_command("append", {"characters": characters, "force": True, "scroll_to_end": True})

    def append_string(self, proc, text):
        """Writes what ``proc`` printed, unless a newer build replaced it. Line endings are ``\\n``."""
        if proc is not self.proc:
            return
        self.write(text.replace("\r", ""))

    def append_data(self, proc, data):
        """Sublime 3's name for ``append_string``; ``data`` may be bytes."""
        if isinstance(data, bytes):
            data = data.decode(self.encoding, "replace")
        self.append_string(proc, data)

    def on_data(self, proc, data):
        sublime.set_timeout(functools.partial(self.append_data, proc, data), 0)

    def on_finished(self, proc):
        sublime.set_timeout(functools.partial(self.finish, proc), 0)

    def finish(self, proc):
        """``[Finished in …]`` (with the command, folder and path when it failed) and the number of
        errors in the status bar."""
        if proc is not self.proc:
            return
        seconds = "%.1f" % (time.time() - proc.start_time)
        code = proc.exit_code()
        if code == 0:
            report = sublime._tr("[Finished in {time}s]", time=seconds)
        else:
            report = sublime._tr("[Finished in {time}s with exit code {code}]", time=seconds,
                                 code="?" if code is None else code)
            report += "\n[cmd: %s]\n[dir: %s]" % (self.command_line, self.working_dir)
            if self.path:
                report += "\n[path: %s]" % self.path
        if not self.quiet or code != 0:
            self.write("\n%s\n" % report)
        errors = len(self.output_view.find_all_results()) if self.output_view is not None else 0
        if errors:
            message = sublime._tr_n("Build finished with {count} error", "Build finished with {count} errors", errors)
        else:
            message = sublime._tr("Build finished")
        sublime.status_message(message)

    # Sublime draws the errors as annotations (phase C); these let subclasses call them.
    def update_annotations(self):
        pass

    def hide_annotations(self):
        pass
