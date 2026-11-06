"""``distutils.spawn``: ``find_executable`` and ``spawn``."""

import os
import shutil
import subprocess

from distutils.errors import DistutilsExecError


def find_executable(executable, path=None):
    """The path of ``executable`` on ``path`` (``PATH`` when None), or None."""
    _, ext = os.path.splitext(executable)
    if os.name == "nt" and ext != ".exe":
        found = shutil.which(executable + ".exe", path=path)
        if found:
            return found
    if os.path.isfile(executable):
        return executable
    return shutil.which(executable, path=path)


def spawn(cmd, search_path=1, verbose=0, dry_run=0, env=None):
    cmd = list(cmd)
    if dry_run:
        return
    if search_path:
        found = find_executable(cmd[0])
        if found is not None:
            cmd[0] = found
    try:
        code = subprocess.call(cmd, env=env)
    except OSError as exc:
        raise DistutilsExecError("command %r failed: %s" % (cmd, exc.args[-1])) from exc
    if code:
        raise DistutilsExecError("command %r failed with exit code %s" % (cmd, code))
