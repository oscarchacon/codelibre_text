"""``imp``, removed in Python 3.12, for plugins written for Sublime's Python 3.3 / 3.8 hosts.

Its folder is at the end of ``sys.path``: on a Python that still has ``imp``, that one wins.
Only what plugins use: reloading, loading a source file, a new module, the lock.
"""

import importlib
import importlib.machinery
import importlib.util
import os
import sys
import types

SEARCH_ERROR = 0
PY_SOURCE = 1
PY_COMPILED = 2
C_EXTENSION = 3
PKG_DIRECTORY = 5
C_BUILTIN = 6
PY_FROZEN = 7


def reload(module):
    return importlib.reload(module)


def new_module(name):
    return types.ModuleType(name)


def get_magic():
    return importlib.util.MAGIC_NUMBER


def get_tag():
    return sys.implementation.cache_tag


def get_suffixes():
    return ([(s, "rb", C_EXTENSION) for s in importlib.machinery.EXTENSION_SUFFIXES]
            + [(s, "r", PY_SOURCE) for s in importlib.machinery.SOURCE_SUFFIXES]
            + [(s, "rb", PY_COMPILED) for s in importlib.machinery.BYTECODE_SUFFIXES])


def cache_from_source(path, debug_override=None):
    return importlib.util.cache_from_source(path)


def source_from_cache(path):
    return importlib.util.source_from_cache(path)


def load_source(name, pathname, file=None):
    loader = importlib.machinery.SourceFileLoader(name, pathname)
    spec = importlib.util.spec_from_file_location(name, pathname, loader=loader)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    loader.exec_module(module)
    return module


def find_module(name, path=None):
    """``(file, pathname, (suffix, mode, type))`` of a top-level module or package."""
    for folder in path or sys.path:
        package = os.path.join(folder or ".", name)
        if os.path.isfile(os.path.join(package, "__init__.py")):
            return None, package, ("", "", PKG_DIRECTORY)
        for suffix, mode, kind in get_suffixes():
            candidate = package + suffix
            if os.path.isfile(candidate):
                return open(candidate, mode), candidate, (suffix, mode, kind)
    raise ImportError("No module named %r" % name, name=name)


def load_module(name, file, pathname, description):
    try:
        if description[2] == PKG_DIRECTORY:
            pathname = os.path.join(pathname, "__init__.py")
        spec = importlib.util.spec_from_file_location(name, pathname)
        module = importlib.util.module_from_spec(spec)
        sys.modules[name] = module
        spec.loader.exec_module(module)
        return module
    finally:
        if file is not None:
            file.close()


def is_builtin(name):
    return name in sys.builtin_module_names


def is_frozen(name):
    return False


def lock_held():
    return False


def acquire_lock():
    pass


def release_lock():
    pass
