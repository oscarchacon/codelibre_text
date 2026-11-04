"""CodeLibre Text's own stand-in for the Python modules of Sublime Text's ``Default`` package.

Sublime's ``Default`` is not loaded (it is written for Sublime's native core), but some packages
import from it: these modules give them the same names. They are only imported, not loaded as
plugins: their commands and listeners are not registered. What each module has is listed in
``docs/plugins.md``.
"""
