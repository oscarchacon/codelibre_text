"""``imghdr``, removed in Python 3.13, for plugins that tell an image's type from its first
bytes. Its folder is at the end of ``sys.path``, so a Python that still has ``imghdr`` uses its
own.

The formats, the tests and the names (``'jpeg'``, ``'png'``…) are the standard library's.
"""

import os

__all__ = ["what", "tests"]


def what(file, h=None):
    """The type of the image in ``h`` (or at the start of ``file``, a path or a binary file),
    ``'jpeg'``, ``'png'``… or ``None``."""
    opened = None
    try:
        if h is None:
            if isinstance(file, (str, bytes, os.PathLike)):
                opened = open(file, "rb")
                h = opened.read(32)
            else:
                location = file.tell()
                h = file.read(32)
                file.seek(location)
        for test in tests:
            found = test(h, file)
            if found:
                return found
    finally:
        if opened is not None:
            opened.close()
    return None


def test_jpeg(h, f):
    """JPEG data with a JFIF or Exif marker, or raw JPEG."""
    if h[6:10] in (b"JFIF", b"Exif"):
        return "jpeg"
    elif h[:4] == b"\xff\xd8\xff\xdb":
        return "jpeg"


def test_png(h, f):
    if h.startswith(b"\x89PNG\r\n\x1a\n"):
        return "png"


def test_gif(h, f):
    if h[:6] in (b"GIF87a", b"GIF89a"):
        return "gif"


def test_tiff(h, f):
    if h[:2] in (b"MM", b"II"):
        return "tiff"


def test_rgb(h, f):
    """The SGI image library."""
    if h.startswith(b"\x01\xda"):
        return "rgb"


def test_pbm(h, f):
    if len(h) >= 3 and h[0] == ord(b"P") and h[1] in b"14" and h[2] in b" \t\n\r":
        return "pbm"


def test_pgm(h, f):
    if len(h) >= 3 and h[0] == ord(b"P") and h[1] in b"25" and h[2] in b" \t\n\r":
        return "pgm"


def test_ppm(h, f):
    if len(h) >= 3 and h[0] == ord(b"P") and h[1] in b"36" and h[2] in b" \t\n\r":
        return "ppm"


def test_rast(h, f):
    """A Sun raster file."""
    if h.startswith(b"\x59\xa6\x6a\x95"):
        return "rast"


def test_xbm(h, f):
    if h.startswith(b"#define "):
        return "xbm"


def test_bmp(h, f):
    if h.startswith(b"BM"):
        return "bmp"


def test_webp(h, f):
    if h.startswith(b"RIFF") and h[8:12] == b"WEBP":
        return "webp"


def test_exr(h, f):
    if h.startswith(b"\x76\x2f\x31\x01"):
        return "exr"


tests = [
    test_jpeg, test_png, test_gif, test_tiff, test_rgb, test_pbm, test_pgm, test_ppm, test_rast,
    test_xbm, test_bmp, test_webp, test_exr,
]
