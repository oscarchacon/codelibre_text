"""The side bar commands of ``Default.side_bar`` that packages subclass (FileManager hides
``RenamePathCommand`` behind its own). As with every class of our ``Default`` modules, they are
commands only through a package's subclass."""

import os

import sublime
import sublime_plugin


class RenamePathCommand(sublime_plugin.WindowCommand):
    """Asks for a new name in the input panel and renames the file or folder ``paths[0]``. An
    open, unmodified view of a renamed file is opened again from its new path."""

    def run(self, paths):
        path = paths[0]
        branch, leaf = os.path.split(path)
        view = self.window.show_input_panel(
            "New Name:", leaf, lambda name: self.on_done(path, branch, name), None, None
        )
        if view is not None:
            stem = os.path.splitext(leaf)[0]
            view.sel().clear()
            view.sel().add(sublime.Region(0, len(stem) if os.path.isfile(path) else len(leaf)))

    def is_case_change(self, old, new):
        return old.lower() == new.lower() and old != new

    def on_done(self, old, branch, leaf):
        new = os.path.join(branch, leaf)
        if new == old:
            return
        try:
            if os.path.exists(new) and not self.is_case_change(old, new):
                raise OSError("File already exists")
            os.rename(old, new)
        except OSError as error:
            sublime.status_message("Unable to rename: %s" % error)
            return
        if os.path.isfile(new):
            view = self.window.find_open_file(old)
            if view is not None and not view.is_dirty():
                group, index = self.window.get_view_index(view)
                view.close()
                self.window.open_file(new, group=group)

    def is_visible(self, paths=None):
        return bool(paths) and len(paths) == 1

    def is_enabled(self, paths=None):
        return bool(paths) and len(paths) == 1 and os.path.exists(paths[0])
