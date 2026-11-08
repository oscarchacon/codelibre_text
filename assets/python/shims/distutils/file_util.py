"""``distutils.file_util``: ``copy_file``, ``move_file``, ``write_file``."""

import os
import shutil

from distutils.errors import DistutilsFileError


def copy_file(src, dst, preserve_mode=1, preserve_times=1, update=0, link=None, verbose=1, dry_run=0):
    """Copies ``src`` to ``dst`` (a file or a folder); ``(destination, copied)``."""
    if not os.path.isfile(src):
        raise DistutilsFileError("can't copy '%s': doesn't exist or not a regular file" % src)
    target = os.path.join(dst, os.path.basename(src)) if os.path.isdir(dst) else dst
    if update and os.path.exists(target) and os.path.getmtime(target) >= os.path.getmtime(src):
        return (target, 0)
    if dry_run:
        return (target, 1)
    if link == "hard":
        os.link(src, target)
    elif link == "sym":
        os.symlink(src, target)
    else:
        (shutil.copy2 if preserve_times else shutil.copyfile)(src, target)
    return (target, 1)


def move_file(src, dst, verbose=1, dry_run=0):
    if dry_run:
        return dst
    target = os.path.join(dst, os.path.basename(src)) if os.path.isdir(dst) else dst
    try:
        shutil.move(src, target)
    except OSError as exc:
        raise DistutilsFileError("couldn't move '%s' to '%s': %s" % (src, target, exc))
    return target


def write_file(filename, contents):
    with open(filename, "w") as f:
        for line in contents:
            f.write(line + "\n")
