"""``distutils``, removed in Python 3.12: the modules plugins use (``version``, ``util``, ``spawn``,
``errors``, ``dir_util``, ``file_util``, ``sysconfig``), not the package builders (``core``,
``command``…). Its folder is at the end of ``sys.path``, so a Python that has ``distutils`` uses its
own."""

__version__ = "3.8"
