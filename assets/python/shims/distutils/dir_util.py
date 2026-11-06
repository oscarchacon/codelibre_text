"""``distutils.dir_util``: ``mkpath``, ``create_tree``, ``copy_tree``, ``remove_tree``."""

import os
import shutil

from distutils.errors import DistutilsFileError


def mkpath(name, mode=0o777, verbose=1, dry_run=0):
    """Makes ``name`` and its missing parents; the folders made."""
    if not isinstance(name, str):
        raise DistutilsFileError("mkpath: 'name' must be a string (got %r)" % (name,))
    name = os.path.normpath(name)
    created = []
    if os.path.isdir(name) or name == "":
        return created
    head = name
    missing = []
    while head and not os.path.isdir(head):
        missing.insert(0, head)
        head = os.path.dirname(head)
        if head == os.path.dirname(head):
            break
    for folder in missing:
        if not dry_run:
            try:
                os.mkdir(folder, mode)
            except OSError as exc:
                if not os.path.isdir(folder):
                    raise DistutilsFileError("could not create '%s': %s" % (folder, exc.args[-1]))
        created.append(folder)
    return created


def create_tree(base_dir, files, mode=0o777, verbose=1, dry_run=0):
    for folder in sorted({os.path.join(base_dir, os.path.dirname(f)) for f in files}):
        mkpath(folder, mode, verbose=verbose, dry_run=dry_run)


def copy_tree(src, dst, preserve_mode=1, preserve_times=1, preserve_symlinks=0, update=0, verbose=1, dry_run=0):
    """Copies the folder ``src`` into ``dst``; the files copied (or that would be)."""
    if not dry_run and not os.path.isdir(src):
        raise DistutilsFileError("cannot copy tree '%s': not a directory" % src)
    names = os.listdir(src) if os.path.isdir(src) else []
    if not dry_run:
        mkpath(dst)
    outputs = []
    for name in names:
        src_name = os.path.join(src, name)
        dst_name = os.path.join(dst, name)
        if preserve_symlinks and os.path.islink(src_name):
            if not dry_run:
                os.symlink(os.readlink(src_name), dst_name)
            outputs.append(dst_name)
        elif os.path.isdir(src_name):
            outputs.extend(copy_tree(src_name, dst_name, preserve_mode, preserve_times, preserve_symlinks, update,
                                     verbose, dry_run))
        else:
            if update and os.path.exists(dst_name) and os.path.getmtime(dst_name) >= os.path.getmtime(src_name):
                continue
            if not dry_run:
                (shutil.copy2 if preserve_times else shutil.copy)(src_name, dst_name)
            outputs.append(dst_name)
    return outputs


def remove_tree(directory, verbose=1, dry_run=0):
    if not dry_run:
        shutil.rmtree(directory, ignore_errors=True)


def ensure_relative(path):
    drive, path = os.path.splitdrive(path)
    if path[0:1] == os.sep:
        path = drive + path[1:]
    return path
