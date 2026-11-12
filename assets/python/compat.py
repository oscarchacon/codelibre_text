"""Puts back what plugins written for Sublime's Python 3.3 and 3.8 hosts use, on the Python we run.

We run every package on one Python (3.13, the one the installers bring, native on x64 and arm64),
where Sublime has a 3.3 host and a newer one (3.13 from build 4201 on): plugins, and the libraries
Package Control installs for them, import modules and call functions that later Pythons removed. Every patch here only
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
    (``assertRaisesRegexp`` and friends) still use them. ``assertDictContainsSubset`` (3.12),
    ``assertItemsEqual`` (3.9), ``getTestCaseNames`` / ``makeSuite`` / ``findTestCases`` (3.13)
    and the loader functions went at different times, so each is checked on its own."""
    import unittest

    for old, new in {
        "assertEquals": "assertEqual",
        "assertNotEquals": "assertNotEqual",
        "assertAlmostEquals": "assertAlmostEqual",
        "assertNotAlmostEquals": "assertNotAlmostEqual",
        "assertRegexpMatches": "assertRegex",
        "assertNotRegexpMatches": "assertNotRegex",
        "assertRaisesRegexp": "assertRaisesRegex",
        "assertItemsEqual": "assertCountEqual",
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

    if not hasattr(unittest.TestCase, "assertDictContainsSubset"):
        def assertDictContainsSubset(self, subset, dictionary, msg=None):
            """Whether ``dictionary`` contains every key and value of ``subset``."""
            missing = []
            mismatched = []
            for key, value in subset.items():
                if key not in dictionary:
                    missing.append(key)
                elif value != dictionary[key]:
                    mismatched.append("%s, expected: %s, actual: %s"
                                      % (repr(key), repr(value), repr(dictionary[key])))
            if not (missing or mismatched):
                return
            standard = ""
            if missing:
                standard = "Missing: %s" % ",".join(repr(key) for key in missing)
            if mismatched:
                if standard:
                    standard += "; "
                standard += "Mismatched values: %s" % ",".join(mismatched)
            self.fail(self._formatMessage(msg, standard))

        unittest.TestCase.assertDictContainsSubset = assertDictContainsSubset

    if not hasattr(unittest, "makeSuite"):
        def makeSuite(testCaseClass, prefix="test", sortUsing=None, suiteClass=None):
            loader = unittest.TestLoader()
            loader.testMethodPrefix = prefix
            if sortUsing is not None:
                loader.sortTestMethodsUsing = sortUsing
            if suiteClass is not None:
                loader.suiteClass = suiteClass
            return loader.loadTestsFromTestCase(testCaseClass)

        unittest.makeSuite = makeSuite

    if not hasattr(unittest, "findTestCases"):
        def findTestCases(module, prefix="test", sortUsing=None, suiteClass=None):
            loader = unittest.TestLoader()
            loader.testMethodPrefix = prefix
            if sortUsing is not None:
                loader.sortTestMethodsUsing = sortUsing
            if suiteClass is not None:
                loader.suiteClass = suiteClass
            return loader.loadTestsFromModule(module)

        unittest.findTestCases = findTestCases

    if not hasattr(unittest, "getTestCaseNames"):
        def getTestCaseNames(testCaseClass, prefix, sortUsing=None, testNamePatterns=None):
            loader = unittest.TestLoader()
            loader.testMethodPrefix = prefix
            if sortUsing is not None:
                loader.sortTestMethodsUsing = sortUsing
            loader.testNamePatterns = testNamePatterns
            return loader.getTestCaseNames(testCaseClass)

        unittest.getTestCaseNames = getTestCaseNames


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


def _asyncio_aliases():
    """The ``@coroutine`` decorator, removed in Python 3.11: old libraries Package Control
    installs still spell their generator-based coroutines this way. (``Task.all_tasks`` and
    ``Task.current_task`` are gone from the same versions, but ``Task`` is the immutable C type,
    so they cannot be put back; use ``asyncio.all_tasks`` / ``asyncio.current_task``.)"""
    import asyncio
    import asyncio.coroutines
    import types

    if not hasattr(asyncio, "coroutine"):
        marker = getattr(asyncio.coroutines, "_is_coroutine", object())

        def coroutine(func):
            wrapped = types.coroutine(func)
            # What ``asyncio.iscoroutinefunction`` looks for on a 3.8 generator coroutine.
            wrapped._is_coroutine = marker
            return wrapped

        asyncio.coroutine = coroutine


def _fractions_gcd():
    """``fractions.gcd``, removed in Python 3.9 for ``math.gcd``: old plugins compute
    reduced fractions or aspect ratios with it."""
    import fractions
    import math

    if not hasattr(fractions, "gcd"):
        fractions.gcd = math.gcd


def _base64_aliases():
    """``base64.encodestring`` / ``decodestring``, removed in Python 3.9 (``encodebytes`` /
    ``decodebytes`` since 3.1): plugins that send credentials or embed images use them."""
    import base64

    if not hasattr(base64, "encodestring"):
        base64.encodestring = base64.encodebytes
    if not hasattr(base64, "decodestring"):
        base64.decodestring = base64.decodebytes


def _plistlib_aliases():
    """``plistlib``'s old API, removed in Python 3.9: ``readPlist`` / ``writePlist`` and their
    ``Bytes`` forms, with ``Data`` for binary values. Plugins read ``.tmTheme``,
    ``.tmPreferences`` and ``.tmLanguage`` files with them."""
    import binascii
    import plistlib

    if not hasattr(plistlib, "Data"):
        class Data:
            """Binary data in a property list, as the old API gave it (``.data`` is the bytes)."""

            def __init__(self, data):
                if not isinstance(data, bytes):
                    raise TypeError("data must be as bytes")
                self.data = data

            @classmethod
            def fromBase64(cls, data):
                return cls(binascii.a2b_base64(data))

            def asBase64(self, maxlinelength=76):
                return binascii.b2a_base64(self.data)

            def __eq__(self, other):
                if isinstance(other, self.__class__):
                    return self.data == other.data
                if isinstance(other, bytes):
                    return self.data == other
                return NotImplemented

            def __hash__(self):
                return hash(self.data)

            def __repr__(self):
                return "%s(%s)" % (self.__class__.__name__, repr(self.data))

        plistlib.Data = Data

    def old_values(value):
        """What the old reader gave: ``Data`` for bytes, the rest as it is."""
        if isinstance(value, bytes):
            return plistlib.Data(value)
        if isinstance(value, dict):
            return {key: old_values(item) for key, item in value.items()}
        if isinstance(value, list):
            return [old_values(item) for item in value]
        return value

    def new_values(value):
        """What the new writer takes: bytes for ``Data``."""
        if isinstance(value, plistlib.Data):
            return value.data
        if isinstance(value, dict):
            return {key: new_values(item) for key, item in value.items()}
        if isinstance(value, (list, tuple)):
            return [new_values(item) for item in value]
        return value

    def opened(path_or_file, mode):
        if isinstance(path_or_file, (str, bytes)) or hasattr(path_or_file, "__fspath__"):
            return open(path_or_file, mode)
        return None

    if not hasattr(plistlib, "readPlist"):
        def readPlist(pathOrFile):
            file = opened(pathOrFile, "rb")
            if file is None:
                return old_values(plistlib.load(pathOrFile))
            with file:
                return old_values(plistlib.load(file))

        plistlib.readPlist = readPlist

    if not hasattr(plistlib, "writePlist"):
        def writePlist(value, pathOrFile):
            file = opened(pathOrFile, "wb")
            if file is None:
                plistlib.dump(new_values(value), pathOrFile, fmt=plistlib.FMT_XML)
                return
            with file:
                plistlib.dump(new_values(value), file, fmt=plistlib.FMT_XML)

        plistlib.writePlist = writePlist

    if not hasattr(plistlib, "readPlistFromBytes"):
        plistlib.readPlistFromBytes = lambda data: old_values(plistlib.loads(data))
    if not hasattr(plistlib, "writePlistToBytes"):
        plistlib.writePlistToBytes = lambda value: plistlib.dumps(new_values(value), fmt=plistlib.FMT_XML)


def _html_parser_aliases():
    """``HTMLParser.unescape`` (removed in Python 3.9) and ``HTMLParseError`` (3.5): plugins that
    read web pages or HTML help."""
    import html
    import html.parser

    if not hasattr(html.parser.HTMLParser, "unescape"):
        html.parser.HTMLParser.unescape = lambda self, s: html.unescape(s)
    if not hasattr(html.parser, "HTMLParseError"):
        class HTMLParseError(Exception):
            """Raised by the parser of Python 3.3 on broken HTML (newer ones never raise it)."""

            def __init__(self, msg, position=(None, None)):
                super().__init__(msg)
                self.msg = msg
                self.lineno, self.offset = position

            def __str__(self):
                result = self.msg
                if self.lineno is not None:
                    result += ", at line %d" % self.lineno
                if self.offset is not None:
                    result += ", column %d" % (self.offset + 1)
                return result

        html.parser.HTMLParseError = HTMLParseError


def _os_and_system_aliases():
    """Names Python 3.7–3.12 dropped from small modules, which old plugins still reach for:
    ``os.errno`` (3.7), ``tarfile.filemode`` (3.8), ``locale.format`` (3.12),
    ``importlib.find_loader`` (3.12), ``gettext.lgettext`` and friends (3.11),
    ``sys.getcheckinterval`` / ``setcheckinterval`` (3.9) and
    ``platform.linux_distribution`` / ``platform.dist`` (3.8)."""
    import errno
    import gettext
    import importlib
    import importlib.util
    import locale
    import os
    import platform
    import stat
    import tarfile

    if not hasattr(os, "errno"):
        os.errno = errno
    if not hasattr(tarfile, "filemode"):
        tarfile.filemode = stat.filemode
    if not hasattr(locale, "format"):
        def format(percent, value, grouping=False, monetary=False, *additional):
            return locale.format_string(percent, value, grouping, monetary, *additional)

        locale.format = format

    if not hasattr(importlib, "find_loader"):
        def find_loader(name, path=None):
            spec = importlib.util.find_spec(name, path)
            return None if spec is None else spec.loader

        importlib.find_loader = find_loader

    for old, new in {"lgettext": "gettext", "ldgettext": "dgettext",
                     "lngettext": "ngettext", "ldngettext": "dngettext"}.items():
        if not hasattr(gettext, old):
            setattr(gettext, old, getattr(gettext, new))
    for old, new in {"lgettext": "gettext", "lngettext": "ngettext"}.items():
        if not hasattr(gettext.NullTranslations, old):
            setattr(gettext.NullTranslations, old, getattr(gettext.NullTranslations, new))

    if not hasattr(sys, "getcheckinterval"):
        # Python 3.2 replaced the check interval by ``setswitchinterval``; both did nothing since.
        interval = [100]
        sys.getcheckinterval = lambda: interval[0]
        sys.setcheckinterval = lambda value: interval.__setitem__(0, int(value))

    if not hasattr(platform, "linux_distribution"):
        def os_release():
            try:
                return platform.freedesktop_os_release()
            except (OSError, AttributeError):
                return {}

        def linux_distribution(distname="", version="", id="", *args, **kwargs):
            release = os_release()
            return (release.get("NAME", distname), release.get("VERSION_ID", version),
                    release.get("VERSION_CODENAME", id))

        def dist(distname="", version="", id="", *args, **kwargs):
            release = os_release()
            return (release.get("ID", distname), release.get("VERSION_ID", version),
                    release.get("VERSION_CODENAME", id))

        platform.linux_distribution = linux_distribution
        platform.dist = dist


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
    _asyncio_aliases()
    _fractions_gcd()
    _asyncio_get_event_loop()
    _base64_aliases()
    _plistlib_aliases()
    _html_parser_aliases()
    _os_and_system_aliases()
