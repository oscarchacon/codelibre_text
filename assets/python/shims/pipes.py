"""``pipes``, removed in Python 3.13, with what plugins use of it: ``quote``, for a command line
handed to the shell. Its folder is at the end of ``sys.path``, so a Python that still has
``pipes`` uses its own.

Not here: ``Template`` and ``makepipeline``, the old command pipeline builders.
"""

import shlex

__all__ = ["quote"]


def quote(file):
    """A string quoted for a POSIX shell, as ``shlex.quote``."""
    return shlex.quote(file)
