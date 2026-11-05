"""``Default.exec``: build plugins subclass ``ExecCommand`` (a window command that runs a process
and writes its output to the ``exec`` output panel) or use ``AsyncProcess`` and ``ProcessListener``.

Our own code, written from what the public API documents and build plugins expect. The editor's
own ``exec`` command stays the built-in one: classes of ``Default`` are not registered as commands
(``sublime_plugin.set_commands``); a plugin's subclass is, under its own name. Errors inline
(annotations) are not drawn yet: ``update_annotations`` only keeps them by file.
"""

import codecs
import os
import signal
import subprocess
import sys
import threading
import time

import sublime
import sublime_plugin


class ProcessListener:
    """Told what an ``AsyncProcess`` prints (``on_data``, from its reader thread) and when it ends
    (``on_finished``, on the main thread). ``encoding`` decodes its output."""

    encoding = "utf-8"

    def on_data(self, proc, data):
        pass

    def on_finished(self, proc):
        pass


def _hidden_window():
    """No console window for the process on Windows."""
    if os.name != "nt":
        return None
    info = subprocess.STARTUPINFO()
    info.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    return info


class AsyncProcess:
    """A process whose output (stdout and stderr together) goes to ``listener`` as it comes, with
    ``\\r\\n`` and ``\\r`` made ``\\n``. ``shell_cmd`` runs in the platform's shell (``cmd`` on
    Windows, ``bash``, a login one on macOS); ``cmd`` is a list run as is, or through the shell with
    ``shell``. ``path`` replaces ``PATH`` while the process starts (``$PATH`` in it expands);
    ``env`` values expand environment variables too. ``start()`` begins reading."""

    def __init__(self, cmd, shell_cmd, env, listener, path="", shell=False):
        if not shell_cmd and not cmd:
            raise ValueError("shell_cmd or cmd is required")
        if shell_cmd and not isinstance(shell_cmd, str):
            raise ValueError("shell_cmd must be a string")
        self.listener = listener
        self.killed = False
        self.start_time = time.time()

        old_path = os.environ.get("PATH", "")
        if path:
            os.environ["PATH"] = os.path.expandvars(path)
        try:
            process_env = dict(os.environ)
            process_env.update(env or {})
            process_env = {key: os.path.expandvars(value) for key, value in process_env.items()}
            if shell_cmd:
                if sys.platform == "win32":
                    cmd, shell = shell_cmd, True
                elif sys.platform == "darwin":
                    cmd, shell = ["/usr/bin/env", "bash", "-l", "-c", shell_cmd], False
                else:
                    cmd, shell = ["/usr/bin/env", "bash", "-c", shell_cmd], False
            self.proc = subprocess.Popen(
                cmd,
                bufsize=0,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                startupinfo=_hidden_window(),
                env=process_env,
                # Its own process group, so ``kill`` reaches what the shell started.
                start_new_session=sys.platform != "win32",
                shell=shell,
            )
        finally:
            if path:
                os.environ["PATH"] = old_path
        self.stdout_thread = threading.Thread(target=self.read_fileno, args=(self.proc.stdout, True), daemon=True)

    def start(self):
        self.stdout_thread.start()

    def start_input_thread(self):
        """A queue whose texts (bytes) are written to the process's input; ``None`` ends it."""
        import queue

        texts = queue.SimpleQueue()

        def write():
            while self.poll():
                text = texts.get()
                if text is None:
                    break
                self.proc.stdin.write(text)
                self.proc.stdin.flush()

        threading.Thread(target=write, daemon=True).start()
        return texts

    def kill(self):
        """Stops the process and what it started (on Windows ``taskkill /T``, as the shell's
        children would go on otherwise)."""
        if self.killed:
            return
        self.killed = True
        if sys.platform == "win32":
            subprocess.Popen("taskkill /PID %d /T /F" % self.proc.pid, startupinfo=_hidden_window(),
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        else:
            try:
                os.killpg(self.proc.pid, signal.SIGTERM)
            except OSError:
                pass
            self.proc.terminate()

    def poll(self):
        """True while the process runs."""
        return self.proc.poll() is None

    def exit_code(self):
        return self.proc.poll()

    def read_fileno(self, file, execute_finished):
        encoding = getattr(self.listener, "encoding", None) or "utf-8"
        decoder = codecs.getincrementaldecoder(encoding)("replace")
        while True:
            chunk = file.read(2 ** 16)
            data = decoder.decode(chunk, final=not chunk).replace("\r\n", "\n").replace("\r", "\n")
            if data and not self.killed:
                self.listener.on_data(self, data)
            if not chunk:
                break
        if execute_finished:
            sublime.set_timeout(lambda: self.listener.on_finished(self))


def _elapsed(seconds):
    return "%.0fms" % (seconds * 1000) if seconds < 1 else "%.1fs" % seconds


class ExecCommand(sublime_plugin.WindowCommand, ProcessListener):
    """Runs ``cmd`` / ``shell_cmd`` and writes the output to the ``exec`` output panel, whose
    ``result_*`` settings make its errors results (F4). The arguments are those of a build system."""

    OUTPUT_LIMIT = 2 ** 27

    def __init__(self, window):
        super().__init__(window)
        self.proc = None
        self.errs_by_file = {}
        self.show_errors_inline = True
        self.output_view = None
        self.input_view = None
        self.input_queue = None
        self.output_size = 0
        self.quiet = False
        self.debug_text = ""

    def run(self, cmd=None, shell_cmd=None, file_regex="", line_regex="", working_dir="", encoding="utf-8",
            env={}, quiet=False, kill=False, kill_previous=False, update_annotations_only=False, word_wrap=True,
            interactive=False, syntax="Packages/Text/Plain text.tmLanguage", path="", **kwargs):
        if update_annotations_only:
            if self.show_errors_inline:
                self.update_annotations()
            return
        if kill:
            if self.proc:
                self.proc.kill()
            return
        if kill_previous and self.proc and self.proc.poll():
            self.proc.kill()

        active = self.window.active_view()
        if not working_dir and active and active.file_name():
            working_dir = os.path.dirname(active.file_name())
        self.output_view, self.input_view = self.window.create_io_panel("exec", self.on_input if interactive else None)
        settings = self.output_view.settings()
        settings.set("result_file_regex", file_regex)
        settings.set("result_line_regex", line_regex)
        settings.set("result_base_dir", working_dir)
        settings.set("word_wrap", word_wrap)
        settings.set("line_numbers", False)
        settings.set("gutter", False)
        settings.set("scroll_past_end", False)
        self.output_view.assign_syntax(syntax)

        self.encoding = encoding
        self.quiet = quiet
        self.proc = None
        if not quiet:
            shown = shell_cmd or (cmd if isinstance(cmd, str) else " ".join(cmd or []))
            print("Running " + shown)
            sublime.status_message("Building")
        preferences = sublime.load_settings("Preferences.sublime-settings")
        if preferences.get("show_panel_on_build", True):
            self.window.run_command("show_panel", {"panel": "output.exec"})
        self.hide_annotations()
        self.show_errors_inline = preferences.get("show_errors_inline", True)

        merged_env = {"PATH": path} if path else {}
        merged_env.update(env or {})
        if active:
            merged_env.update(active.settings().get("build_env") or {})
        # The process's relative paths are relative to it, as the results' base dir.
        if working_dir:
            os.chdir(working_dir)
        self.debug_text = ("[shell_cmd: %s]\n" % shell_cmd) if shell_cmd else ("[cmd: %s]\n" % (cmd,))
        self.debug_text += "[dir: %s]\n" % os.getcwd()
        self.debug_text += "[path: %s]" % (os.path.expandvars(merged_env["PATH"]) if "PATH" in merged_env
                                            else os.environ.get("PATH", ""))
        self.output_size = 0
        try:
            self.proc = AsyncProcess(cmd, shell_cmd, merged_env, self, path, **kwargs)
            self.proc.start()
            self.input_queue = self.proc.start_input_thread() if interactive else None
        except Exception as error:
            self.write(str(error) + "\n")
            self.write(self.debug_text + "\n")
            if not quiet:
                self.write("[Finished]")
        if interactive and self.input_view is not None:
            self.window.focus_view(self.input_view)

    def is_enabled(self, kill=False, **kwargs):
        if kill:
            return self.proc is not None and self.proc.poll()
        return True

    def write(self, characters):
        self.output_view.run_command("append", {"characters": characters, "force": True, "scroll_to_end": True})

    def on_input(self, text):
        if self.input_queue is None or not self.proc or not self.proc.poll():
            return
        text += "\n"
        self.write(text)
        self.input_queue.put(text.encode(self.encoding))

    def on_data(self, proc, data):
        if proc is not self.proc or self.output_size >= self.OUTPUT_LIMIT:
            return
        self.write(data)
        self.output_size += len(data)
        if self.output_size >= self.OUTPUT_LIMIT:
            self.write("\n[Output Truncated]\n")

    def on_finished(self, proc):
        if proc is not self.proc:
            return
        if self.input_queue is not None:
            self.input_queue.put(None)
            self.input_queue = None
        if proc.killed:
            self.write("\n[Cancelled]")
            sublime.status_message("Build cancelled")
            return
        if not self.quiet:
            elapsed = _elapsed(time.time() - proc.start_time)
            code = proc.exit_code()
            if code in (0, None):
                self.write("[Finished in %s]" % elapsed)
            else:
                self.write("[Finished in %s with exit code %d]\n" % (elapsed, code))
                self.write(self.debug_text)
        errors = self.output_view.find_all_results()
        sublime.status_message("Build finished with %d errors" % len(errors) if errors else "Build finished")
        self.update_annotations()

    def update_annotations(self):
        """Keeps the errors by file (``errs_by_file``); they are not drawn in the views yet."""
        self.errs_by_file = {}
        if self.output_view is None:
            return
        for file, line, column, text in self.output_view.find_all_results_with_text():
            self.errs_by_file.setdefault(file, []).append((line, column, text))

    def hide_annotations(self):
        self.errs_by_file = {}
