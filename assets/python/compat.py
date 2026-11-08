"""Puts back what plugins written for Sublime's Python 3.8 hosts use, on the Python we run.

Sublime runs plugins on Python 3.8 (or 3.3): plugins, and the libraries Package Control installs
for them, import modules and call functions that later Pythons removed. Every patch here only
fills a gap (``if not hasattr(...)``), so a Python that still has them is left alone. Modules that
are gone entirely live in ``shims/``, which is last on ``sys.path`` for the same reason.

``plugin_host.py`` calls :func:`apply` before it loads a plugin.
"""

import collections
import collections.abc
import sys


def allow_post_handshake_auth():
    """Apple's ``/usr/bin/python3`` is built with LibreSSL: its ``SSLContext`` has
    ``post_handshake_auth`` but refuses to set it, and Package Control sets it before every
    download. Contexts made from then on ignore it (it only matters with client certificates)."""
    import ssl

    try:
        ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT).post_handshake_auth = True
    except AttributeError:
        ssl.SSLContext.post_handshake_auth = property(lambda self: False, lambda self, value: None)
    except Exception:
        pass


def restore_entry_points_by_group():
    """``importlib.metadata.entry_points()`` as Python 3.8 plugins use it, on any Python.

    Python 3.8 gave a dict of group -> entry points (Markdown, a library Package Control
    installs, asks it ``.get("markdown.extensions", ())``); 3.12 dropped that dict, and 3.9 does
    not take the ``group=`` filter 3.10 added. Both shapes are served here."""
    import importlib.metadata as metadata
    import inspect

    original = metadata.entry_points
    try:
        takes_filters = "group" in inspect.signature(original).parameters
    except (TypeError, ValueError):
        takes_filters = False

    def select(entries, params):
        chosen = list(entries)
        for key in ("name", "value", "group"):
            wanted = params.get(key)
            if wanted is None:
                continue
            if isinstance(wanted, str):
                wanted = (wanted,)
            chosen = [entry for entry in chosen if getattr(entry, key) in wanted]
        return chosen

    def groups_of(found):
        if hasattr(found, "items"):
            return {group: list(entries) for group, entries in found.items()}
        groups = {}
        for entry in found:
            groups.setdefault(entry.group, []).append(entry)
        return groups

    class EntryPointsByGroup:
        """The dict 3.8 answered, with the ``select`` later versions added."""

        def __init__(self, groups):
            self._groups = groups

        @property
        def groups(self):
            return tuple(self._groups)

        def select(self, **params):
            return select([entry for entries in self._groups.values() for entry in entries], params)

        def get(self, group, default=None):
            if group in self._groups:
                return list(self._groups[group])
            return default

        def keys(self):
            return self._groups.keys()

        def values(self):
            return [list(entries) for entries in self._groups.values()]

        def items(self):
            return [(group, list(entries)) for group, entries in self._groups.items()]

        def __getitem__(self, group):
            return list(self._groups[group])

        def __iter__(self):
            return iter(self._groups)

        def __len__(self):
            return len(self._groups)

        def __contains__(self, group):
            return group in self._groups

    def entry_points_by_group(**params):
        if params:
            if takes_filters:
                return list(original(**params))
            found = [entry for entries in groups_of(original()).values() for entry in entries]
            return select(found, params)
        return EntryPointsByGroup(groups_of(original()))

    metadata.entry_points = entry_points_by_group


def _collection_aliases():
    """``collections.Mapping`` and the other ABCs, removed in Python 3.10: plugins written
    before ``collections.abc`` use them."""
    for name in (
        "Awaitable", "Coroutine", "AsyncIterable", "AsyncIterator", "AsyncGenerator",
        "Hashable", "Iterable", "Iterator", "Generator", "Reversible", "Sized", "Container",
        "Collection", "Callable", "Set", "MutableSet", "Mapping", "MutableMapping",
        "MappingView", "KeysView", "ItemsView", "ValuesView", "Sequence", "MutableSequence",
        "ByteString",
    ):
        if not hasattr(collections, name):
            setattr(collections, name, getattr(collections.abc, name))


def _inspect_aliases():
    """``inspect.getargspec`` and ``formatargspec``, removed in Python 3.11: old plugins use them
    to pass arguments to a command (``ArgSpec`` is the tuple they unpack)."""
    import inspect
    from collections import namedtuple

    if not hasattr(inspect, "ArgSpec"):
        inspect.ArgSpec = namedtuple("ArgSpec", "args varargs keywords defaults")

    if not hasattr(inspect, "getargspec"):
        def getargspec(func):
            spec = inspect.getfullargspec(func)
            if spec.kwonlyargs:
                raise ValueError("Function has keyword-only arguments or annotations, "
                                 "use getfullargspec() API which can support them")
            return inspect.ArgSpec(spec.args, spec.varargs, spec.varkw, spec.defaults)

        inspect.getargspec = getargspec

    if not hasattr(inspect, "formatargspec"):
        def formatargspec(args, varargs=None, varkw=None, defaults=None, kwonlyargs=(),
                          kwonlydefaults=None, annotations=None, formatarg=str,
                          formatvarargs=lambda name: "*" + name,
                          formatvarkw=lambda name: "**" + name,
                          formatvalue=lambda value: "=" + repr(value)):
            defaults = defaults or ()
            first_default = len(args) - len(defaults)
            specs = []
            for index, name in enumerate(args):
                text = formatarg(name)
                if index >= first_default:
                    text += formatvalue(defaults[index - first_default])
                specs.append(text)
            if varargs:
                specs.append(formatvarargs(varargs))
            elif kwonlyargs:
                specs.append("*")
            for name in kwonlyargs:
                text = formatarg(name)
                if kwonlydefaults and name in kwonlydefaults:
                    text += formatvalue(kwonlydefaults[name])
                specs.append(text)
            if varkw:
                specs.append(formatvarkw(varkw))
            return "(" + ", ".join(specs) + ")"

        inspect.formatargspec = formatargspec


def _unittest_aliases():
    """``unittest``'s old CamelCase names, removed in Python 3.12: test suites of plugins
    (``assertRaisesRegexp`` and friends) still use them."""
    import unittest

    for old, new in {
        "assertEquals": "assertEqual",
        "assertNotEquals": "assertNotEqual",
        "assertAlmostEquals": "assertAlmostEqual",
        "assertNotAlmostEquals": "assertNotAlmostEqual",
        "assertRegexpMatches": "assertRegex",
        "assertNotRegexpMatches": "assertNotRegex",
        "assertRaisesRegexp": "assertRaisesRegex",
        "assert_": "assertTrue",
        "failUnless": "assertTrue",
        "failIf": "assertFalse",
        "failUnlessEqual": "assertEqual",
        "failIfEqual": "assertNotEqual",
        "failUnlessAlmostEqual": "assertAlmostEqual",
        "failIfAlmostEqual": "assertNotAlmostEqual",
        "failUnlessRaises": "assertRaises",
    }.items():
        if not hasattr(unittest.TestCase, old) and hasattr(unittest.TestCase, new):
            setattr(unittest.TestCase, old, getattr(unittest.TestCase, new))


def _cgi_aliases():
    """``cgi.escape`` / ``cgi.unescape``: the module itself is there until Python 3.13, but both
    functions left it in 3.8, so plugins (and old libraries) that import them fail on a newer
    Python even though ``cgi`` exists."""
    import html
    import urllib.parse

    try:
        import cgi
    except ImportError:
        return
    if not hasattr(cgi, "escape"):
        cgi.escape = lambda s, quote=None: html.escape(s, quote=bool(quote))
    if not hasattr(cgi, "unescape"):
        cgi.unescape = html.unescape
    if not hasattr(cgi, "parse_qs"):
        cgi.parse_qs = urllib.parse.parse_qs
    if not hasattr(cgi, "parse_qsl"):
        cgi.parse_qsl = urllib.parse.parse_qsl


def _configparser_aliases():
    """``SafeConfigParser`` and ``readfp``, removed in Python 3.12: plugins that read their
    settings with them (``SafeConfigParser`` is ``ConfigParser`` since 3.2)."""
    import configparser

    if not hasattr(configparser, "SafeConfigParser"):
        configparser.SafeConfigParser = configparser.ConfigParser
    for name in ("RawConfigParser", "ConfigParser"):
        parser = getattr(configparser, name)
        if not hasattr(parser, "readfp"):
            parser.readfp = parser.read_file


def _ssl_aliases():
    """``ssl.wrap_socket`` and ``ssl.match_hostname``, removed in Python 3.12: old libraries
    Package Control installs (requests before 2.16, pip's vendored code) call them."""
    import ssl

    if not hasattr(ssl, "wrap_socket"):
        def wrap_socket(sock, keyfile=None, certfile=None, server_side=False,
                        cert_reqs=ssl.CERT_NONE, ssl_version=None, ca_certs=None,
                        do_handshake_on_connect=True, suppress_ragged_eofs=True, ciphers=None):
            context = ssl.SSLContext(ssl_version or ssl.PROTOCOL_TLS)
            if certfile:
                context.load_cert_chain(certfile, keyfile)
            if ca_certs:
                context.load_verify_locations(ca_certs)
            if ciphers:
                context.set_ciphers(ciphers)
            context.verify_mode = cert_reqs
            return context.wrap_socket(sock, server_side=server_side,
                                       do_handshake_on_connect=do_handshake_on_connect,
                                       suppress_ragged_eofs=suppress_ragged_eofs)

        ssl.wrap_socket = wrap_socket

    if not hasattr(ssl, "match_hostname"):
        import ipaddress

        def dnsname_match(domain, hostname):
            if not domain or not hostname:
                return False
            domain = domain.rstrip(".").lower()
            hostname = hostname.rstrip(".").lower()
            if not domain.startswith("*."):
                return domain == hostname
            suffix = domain[1:]
            if "." not in hostname:
                return False
            if not hostname.endswith(suffix):
                return False
            return "." not in hostname[:-len(suffix)]

        def match_hostname(cert, hostname):
            if not cert:
                raise ValueError("empty or no certificate, match_hostname needs a SSL socket "
                                 "or SSL context with either CERT_OPTIONAL or CERT_REQUIRED")
            dnsnames = []
            for key, value in cert.get("subjectAltName", ()):
                if key == "DNS":
                    if dnsname_match(value, hostname):
                        return
                    dnsnames.append(value)
                elif key == "IP Address":
                    try:
                        if ipaddress.ip_address(hostname) == ipaddress.ip_address(value):
                            return
                    except ValueError:
                        pass
                    dnsnames.append(value)
            if not dnsnames:
                for subject in cert.get("subject", ()):
                    for key, value in subject:
                        if key == "commonName":
                            if dnsname_match(value, hostname):
                                return
                            dnsnames.append(value)
            if len(dnsnames) > 1:
                raise ssl.CertificateError("hostname %r doesn't match either of %s"
                                           % (hostname, ", ".join(map(repr, dnsnames))))
            elif len(dnsnames) == 1:
                raise ssl.CertificateError("hostname %r doesn't match %r" % (hostname, dnsnames[0]))
            raise ssl.CertificateError("no appropriate commonName or subjectAltName fields were found")

        ssl.match_hostname = match_hostname


def _typing_aliases():
    """``typing.ByteString`` / ``Text`` and the ``typing.io`` / ``typing.re`` namespaces, removed
    by newer Pythons (3.14 here still has ``ByteString``): plugin code imports them."""
    import typing

    if not hasattr(typing, "ByteString"):
        typing.ByteString = collections.abc.ByteString
    if not hasattr(typing, "Text"):
        typing.Text = str
    if not hasattr(typing, "io"):
        import types
        typing.io = types.SimpleNamespace(IO=typing.IO, TextIO=typing.TextIO,
                                          BinaryIO=typing.BinaryIO)
    if not hasattr(typing, "re"):
        import types
        typing.re = types.SimpleNamespace(Pattern=typing.Pattern, Match=typing.Match)


def _threading_aliases():
    """The lowercase renames of Python 3.10, removed from their old spellings in Python 3.9 and
    later: old plugins call ``isAlive()``, ``currentThread()``, ``notifyAll()``…"""
    import threading

    for old, new in {"currentThread": "current_thread", "activeCount": "active_count"}.items():
        if not hasattr(threading, old):
            setattr(threading, old, getattr(threading, new))
    if not hasattr(threading.Thread, "isAlive"):
        threading.Thread.isAlive = threading.Thread.is_alive
    if not hasattr(threading.Thread, "getName"):
        threading.Thread.getName = lambda self: self.name
    if not hasattr(threading.Thread, "setName"):
        threading.Thread.setName = lambda self, name: setattr(self, "name", name)
    if not hasattr(threading.Thread, "isDaemon"):
        threading.Thread.isDaemon = lambda self: self.daemon
    if not hasattr(threading.Thread, "setDaemon"):
        threading.Thread.setDaemon = lambda self, daemon: setattr(self, "daemon", daemon)
    if not hasattr(threading.Condition, "notifyAll"):
        threading.Condition.notifyAll = threading.Condition.notify_all
    if not hasattr(threading.Event, "isSet"):
        threading.Event.isSet = threading.Event.is_set


def _time_clock():
    """``time.clock``, removed in Python 3.8: plugins that measure a command with it."""
    import time

    if not hasattr(time, "clock"):
        time.clock = time.perf_counter


def _pkgutil_aliases():
    """``pkgutil.ImpImporter`` / ``ImpLoader`` and ``find_loader`` / ``get_loader``: the first
    pair went in Python 3.12 and the functions were deprecated then; libraries still mention
    them (``isinstance(importer, pkgutil.ImpImporter)``)."""
    import importlib.util
    import pkgutil

    if not hasattr(pkgutil, "ImpImporter"):
        class ImpImporter:
            def __init__(self, path=None):
                self.path = path

        pkgutil.ImpImporter = ImpImporter

    if not hasattr(pkgutil, "ImpLoader"):
        class ImpLoader:
            def __init__(self, fullname, file, filename, etc):
                self.fullname = fullname
                self.file = file
                self.filename = filename
                self.etc = etc

            def load_module(self, fullname):
                import importlib
                return importlib.import_module(fullname)

        pkgutil.ImpLoader = ImpLoader

    if not hasattr(pkgutil, "find_loader"):
        def find_loader(fullname):
            spec = importlib.util.find_spec(fullname)
            if spec is None:
                return None, []
            return spec.loader, list(spec.submodule_search_locations or [])

        pkgutil.find_loader = find_loader

    if not hasattr(pkgutil, "get_loader"):
        def get_loader(module_or_name):
            name = module_or_name if isinstance(module_or_name, str) else module_or_name.__name__
            spec = importlib.util.find_spec(name)
            if spec is None:
                raise AttributeError("module %r has no loader" % name)
            return spec.loader

        pkgutil.get_loader = get_loader


def _asyncio_get_event_loop():
    """Python 3.14's ``asyncio.get_event_loop()`` raises when the thread has no loop yet, where
    3.8 made one: plugins call it before ``run_until_complete``."""
    if sys.version_info < (3, 14):
        return

    import asyncio
    import threading

    original = asyncio.get_event_loop

    def get_event_loop():
        try:
            return original()
        except RuntimeError:
            # 3.8 made a loop for the main thread only: a plugin's worker thread still has none.
            if threading.current_thread() is not threading.main_thread():
                raise
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            return loop

    asyncio.get_event_loop = get_event_loop


def apply():
    """Patches the standard library once, before any plugin is imported."""
    allow_post_handshake_auth()
    restore_entry_points_by_group()
    _collection_aliases()
    _inspect_aliases()
    _unittest_aliases()
    _cgi_aliases()
    _configparser_aliases()
    _ssl_aliases()
    _typing_aliases()
    _threading_aliases()
    _time_clock()
    _pkgutil_aliases()
    _asyncio_get_event_loop()
