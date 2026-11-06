"""``uu``, removed in Python 3.13, for plugins that read or write a uuencoded file. Its folder is
at the end of ``sys.path``, so a Python that still has ``uu`` uses its own.
"""

import binascii
import os

__all__ = ["Error", "encode", "decode"]


class Error(Exception):
    pass


def encode(in_file, out_file, name=None, mode=None):
    """Writes ``in_file`` (a path or a binary file) uuencoded to ``out_file``."""
    if isinstance(in_file, (str, bytes, os.PathLike)):
        with open(in_file, "rb") as source:
            encode(source, out_file, name, mode)
        return
    if isinstance(out_file, (str, bytes, os.PathLike)):
        with open(out_file, "wb") as target:
            encode(in_file, target, name, mode)
        return
    if name is None:
        if getattr(in_file, "name", "<stdin>") not in ("<stdin>", "-"):
            name = os.path.basename(in_file.name)
        else:
            name = "-"
    if mode is None:
        try:
            mode = os.stat(in_file.name).st_mode & 0o777
        except (AttributeError, OSError):
            mode = 0o666
    out_file.write(("begin %o %s\n" % (mode, name)).encode("ascii"))
    while True:
        data = in_file.read(45)
        if not data:
            break
        out_file.write(binascii.b2a_uu(data))
    out_file.write(b"`\nend\n")
    out_file.flush()


def decode(in_file, out_file=None, mode=None, quiet=False):
    """Decodes a uuencoded ``in_file`` to ``out_file`` (a path or a binary file; the file named
    in the ``begin`` line when it is ``None``), with the mode it carried when ``mode`` is none."""
    if isinstance(in_file, (str, bytes, os.PathLike)):
        with open(in_file, "rb") as source:
            return decode(source, out_file, mode, quiet)
    line = in_file.readline()
    if not line:
        raise Error("No data")
    if not line.startswith(b"begin "):
        raise Error("No start line")
    parts = line.split()
    if len(parts) < 3:
        raise Error("Bad begin line")
    if mode is None:
        try:
            mode = int(parts[1], 8)
        except ValueError as error:
            raise Error("Bad mode in begin line") from error
    if out_file is None:
        out_file = parts[2].decode("ascii", "replace")
    opened = None
    if isinstance(out_file, (str, bytes, os.PathLike)):
        opened = open(out_file, "wb")
        out_file = opened
    try:
        while True:
            line = in_file.readline()
            if not line:
                raise Error("No end line")
            if line.strip() == b"end":
                break
            if line.strip() in (b"", b"`"):
                continue
            try:
                out_file.write(binascii.a2b_uu(line))
            except binascii.Error as error:
                raise Error("Invalid line: %r" % line) from error
        out_file.flush()
    finally:
        if opened is not None:
            opened.close()
    if not quiet and opened is not None:
        print("Wrote %s" % os.fsdecode(opened.name))
    return mode
