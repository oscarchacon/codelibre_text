"""``cgi``, removed in Python 3.13, with what plugins use of it: escaping for HTML and parsing a
header or a query string. Its folder is at the end of ``sys.path``, so a Python that still has
``cgi`` uses its own.

Not here: ``FieldStorage`` and the rest of the CGI server, which only a web server uses.
"""

import html
import re
import urllib.parse

__all__ = ["escape", "unescape", "parse_header", "parse_qs", "parse_qsl", "valid_boundary"]

# RFC 2045 boundary: 0 to 200 printable characters, ending in a non-space one.
_boundary = re.compile(r"^[ -~]{0,200}[!-~]$")


def escape(s, quote=None):
    """``&``, ``<`` and ``>`` escaped; with ``quote`` also ``"`` and ``'``."""
    return html.escape(s, quote=bool(quote))


def unescape(s):
    return html.unescape(s)


def parse_header(line):
    """``'text/html; charset="utf-8"'`` as ``('text/html', {'charset': 'utf-8'})``."""
    parts = _parseparam(";" + line)
    key = next(parts)
    values = {}
    for part in parts:
        index = part.find("=")
        if index < 0:
            continue
        name = part[:index].strip().lower()
        value = part[index + 1:].strip()
        if len(value) >= 2 and value[0] == value[-1] == '"':
            value = value[1:-1].replace("\\\\", "\\").replace('\\"', '"')
        values[name] = value
    return key, values


def _parseparam(s):
    """Splits ``s`` at the ``;`` that are not inside quotes, as the standard library did."""
    while s[:1] == ";":
        s = s[1:]
        end = s.find(";")
        while end > 0 and s.count('"', 0, end) % 2:
            end = s.find(";", end + 1)
        if end < 0:
            end = len(s)
        part = s[:end]
        yield part.strip()
        s = s[end:]


def parse_qs(qs, keep_blank_values=False, strict_parsing=False, separator="&"):
    return urllib.parse.parse_qs(qs, keep_blank_values, strict_parsing, separator=separator)


def parse_qsl(qs, keep_blank_values=False, strict_parsing=False, separator="&"):
    return urllib.parse.parse_qsl(qs, keep_blank_values, strict_parsing, separator=separator)


def valid_boundary(s):
    """Whether a string may be a MIME multipart boundary."""
    if isinstance(s, bytes):
        s = s.decode("ascii", "replace")
    return _boundary.match(s)
