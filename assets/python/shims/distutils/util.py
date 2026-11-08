"""``distutils.util``: ``strtobool``, ``get_platform``, ``convert_path``, ``change_root``,
``split_quoted``, ``execute``, ``rfc822_escape``."""

import os
import re
import sysconfig

from distutils.errors import DistutilsPlatformError


def strtobool(val):
    """``1`` for y, yes, t, true, on and 1; ``0`` for n, no, f, false, off and 0."""
    val = val.lower()
    if val in ("y", "yes", "t", "true", "on", "1"):
        return 1
    if val in ("n", "no", "f", "false", "off", "0"):
        return 0
    raise ValueError("invalid truth value %r" % (val,))


def get_platform():
    if os.name == "nt":
        machine = sysconfig.get_platform()
        return {"win-amd64": "win-amd64", "win-arm64": "win-arm64"}.get(machine, "win32")
    return sysconfig.get_platform()


def get_host_platform():
    return get_platform()


def convert_path(pathname):
    if os.sep == "/" or not pathname:
        return pathname
    if pathname[0] == "/":
        raise ValueError("path '%s' cannot be absolute" % pathname)
    if pathname[-1] == "/":
        raise ValueError("path '%s' cannot end with '/'" % pathname)
    paths = [p for p in pathname.split("/") if p != "."]
    return os.path.join(*paths) if paths else os.curdir


def change_root(new_root, pathname):
    if os.name == "posix":
        return os.path.join(new_root, pathname.lstrip("/")) if os.path.isabs(pathname) else \
            os.path.join(new_root, pathname)
    if os.name == "nt":
        (drive, path) = os.path.splitdrive(pathname)
        if path[0] == "\\":
            path = path[1:]
        return os.path.join(new_root, path)
    raise DistutilsPlatformError("nothing known about platform '%s'" % os.name)


def split_quoted(s):
    import shlex
    return shlex.split(s.strip())


def execute(func, args, msg=None, verbose=0, dry_run=0):
    if not dry_run:
        func(*args)


def rfc822_escape(header):
    lines = header.split("\n")
    return ("\n" + 8 * " ").join(lines)


def check_environ():
    if os.name == "posix" and "HOME" not in os.environ:
        os.environ["HOME"] = os.path.expanduser("~")
    if "PLAT" not in os.environ:
        os.environ["PLAT"] = get_platform()


def subst_vars(s, local_vars):
    check_environ()

    def _subst(match):
        name = match.group(1)
        if name in local_vars:
            return str(local_vars[name])
        return os.environ[name]

    try:
        return re.sub(r"\$([a-zA-Z_][a-zA-Z_0-9]*)", _subst, s)
    except KeyError as var:
        raise ValueError("invalid variable '$%s'" % var)


def byte_compile(py_files, optimize=0, force=0, prefix=None, base_dir=None, verbose=1, dry_run=0, direct=None):
    import py_compile
    for file in py_files:
        if file.endswith(".py") and not dry_run:
            py_compile.compile(file, doraise=False)
