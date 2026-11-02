"""``LooseVersion`` and ``StrictVersion`` as ``distutils.version`` had them."""

import re


class Version:
    def __init__(self, vstring=None):
        if vstring:
            self.parse(vstring)

    def __repr__(self):
        return "%s('%s')" % (self.__class__.__name__, str(self))

    def __eq__(self, other):
        c = self._cmp(other)
        return c if c is NotImplemented else c == 0

    def __lt__(self, other):
        c = self._cmp(other)
        return c if c is NotImplemented else c < 0

    def __le__(self, other):
        c = self._cmp(other)
        return c if c is NotImplemented else c <= 0

    def __gt__(self, other):
        c = self._cmp(other)
        return c if c is NotImplemented else c > 0

    def __ge__(self, other):
        c = self._cmp(other)
        return c if c is NotImplemented else c >= 0


class StrictVersion(Version):
    version_re = re.compile(r"^(\d+)\.(\d+)(\.(\d+))?([ab](\d+))?$", re.ASCII)

    def parse(self, vstring):
        match = self.version_re.match(vstring)
        if not match:
            raise ValueError("invalid version number '%s'" % vstring)
        major, minor, patch, prerelease, prerelease_num = match.group(1, 2, 4, 5, 6)
        self.version = tuple(map(int, [major, minor, patch or 0]))
        self.prerelease = (prerelease[0], int(prerelease_num)) if prerelease else None

    def __str__(self):
        version = ".".join(map(str, self.version[:2] if self.version[2] == 0 else self.version))
        if self.prerelease:
            version += self.prerelease[0] + str(self.prerelease[1])
        return version

    def _cmp(self, other):
        if isinstance(other, str):
            other = StrictVersion(other)
        elif not isinstance(other, StrictVersion):
            return NotImplemented
        if self.version != other.version:
            return -1 if self.version < other.version else 1
        if self.prerelease == other.prerelease:
            return 0
        if not self.prerelease:
            return 1
        if not other.prerelease:
            return -1
        return -1 if self.prerelease < other.prerelease else 1


class LooseVersion(Version):
    component_re = re.compile(r"(\d+ | [a-z]+ | \.)", re.VERBOSE)

    def parse(self, vstring):
        self.vstring = vstring
        components = [x for x in self.component_re.split(vstring) if x and x != "."]
        self.version = [int(x) if x.isdigit() else x for x in components]

    def __str__(self):
        return self.vstring

    def _cmp(self, other):
        if isinstance(other, str):
            other = LooseVersion(other)
        elif not isinstance(other, LooseVersion):
            return NotImplemented
        if self.version == other.version:
            return 0
        return -1 if self.version < other.version else 1
