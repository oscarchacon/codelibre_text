"""``distutils.sysconfig``: the standard library's ``sysconfig`` with distutils' names."""

import sys
import sysconfig as _sysconfig

PREFIX = sys.prefix
EXEC_PREFIX = sys.exec_prefix
BASE_PREFIX = sys.base_prefix
BASE_EXEC_PREFIX = sys.base_exec_prefix


def get_python_version():
    return "%d.%d" % sys.version_info[:2]


def get_python_lib(plat_specific=0, standard_lib=0, prefix=None):
    name = ("platstdlib" if plat_specific else "stdlib") if standard_lib else ("platlib" if plat_specific else "purelib")
    variables = {"base": prefix, "platbase": prefix} if prefix else None
    return _sysconfig.get_path(name, vars=variables)


def get_python_inc(plat_specific=0, prefix=None):
    variables = {"base": prefix, "platbase": prefix, "installed_base": prefix} if prefix else None
    return _sysconfig.get_path("platinclude" if plat_specific else "include", vars=variables)


def get_config_var(name):
    return _sysconfig.get_config_var(name)


def get_config_vars(*args):
    return _sysconfig.get_config_vars(*args)


def get_config_h_filename():
    return _sysconfig.get_config_h_filename()


def get_makefile_filename():
    return _sysconfig.get_makefile_filename()
