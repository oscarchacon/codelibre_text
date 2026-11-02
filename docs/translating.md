# Translating CodeLibre Text

The interface (menus, Command Palette, panels, dialogs and status messages) can be shown in any
language. Sublime Text itself is English only, so this is our own addition, built so that it never
breaks Sublime compatibility:

- Translations are standard **gettext `.po` files**, editable with [Poedit](https://poedit.net/),
  Weblate, Lokalize or any text editor.
- The **English text is the key**. Anything without a translation, including the menus and
  commands of packages nobody translated, simply stays in English.
- Command names, setting names and key bindings are never translated: they are the API that
  packages and plugins use.
- The Command Palette shows translated captions but still matches what you type in English, so
  documentation and tutorials written for Sublime Text keep working.

## Choosing the language

**Preferences > Language…** (or `UI: Select Language` in the Command Palette) lists every
available language. It writes the `language` setting to `Packages/User/Preferences.sublime-settings`:

```json
{
	// "auto" (the default) follows the operating system; or "en", "es", "pt_BR"…
	"language": "es"
}
```

A region falls back to its language (`es_CL` uses `es`) and a language to its region
(`pt` uses `pt_BR`). With no match, the interface is in English.

## Where translations live

| Where | What |
|-------|------|
| `assets/Default/Localization/messages.pot` | The template: every English text of the interface. Generated, do not edit. |
| `assets/Default/Localization/<code>.po` | The languages that ship with the editor (`de`, `es`, `fr`, `pt_BR`). |
| `Packages/<Package>/Localization/<code>.po` | Languages or extra translations added by packages, loaded in package order (`User` last). |

A package can add a whole new language, improve a built-in one, or translate its own menu and
Command Palette captions: entries in later packages replace earlier ones.

## Improving or adding a language

1. Fork the repository.
2. To **start a new language**, open `assets/Default/Localization/messages.pot` in Poedit
   (*Create new translation*) and save it next to it as `<code>.po`. Use a language code such as
   `it`, `ja` or `zh_CN`. To **improve an existing language**, open its `.po` file.
3. Fill in the header (Poedit does it from *Translation > Properties*):
   - `Language: <code>`
   - `X-Language-Name: <the language's own name>`, e.g. `Italiano`. It is what the language
     picker shows.
   - `Plural-Forms`, e.g. `nplurals=2; plural=(n != 1);`. Poedit fills it in for your language.
4. Translate. Keep the `{placeholders}` exactly as they are (`{path}`, `{count}`…); you may move
   them within the sentence. Keep a trailing `…` when the English has one: it means the command
   asks for more input.
5. Register a new language in `TRANSLATIONS` in
   `crates/interface_adapters/src/gateways/builtin_resources.rs`.
6. Check it and send a pull request:

   ```bash
   cargo test -p interface_adapters --test builtin_resources
   ```

   That test fails if a built-in language misses a text, has an empty or stale entry, or changes
   a placeholder.

To try a translation without building anything, copy it to
`Packages/User/Localization/<code>.po` (Preferences > Browse Packages…). Then choose it in
Preferences > Language…, or reload with Preferences > Reload Packages.

Languages written with characters that the default font does not have (Chinese, Japanese,
Korean…) also need a font fallback that the editor does not ship yet: a translation is still
welcome, and the issue tracker is the place to discuss it.

## For developers: adding text to the interface

Every text users see goes through the translations:

- In use cases: `self.tr("Text")`, `self.tr_args("Saved {path}", &[("path", &path)])`, or
  `self.tr_n("{count} match", "{count} matches", count, &[])` for plurals.
- In controllers and presenters: the same methods on `editor`.
- In egui views: `text.tr("Label")` on the `Translations` the view receives.
- Captions in `assets/Default/*.sublime-menu` and `*.sublime-commands` are picked up automatically.

Always pass string literals, so the extractor can find them. Then regenerate the template and
translate the new texts in every built-in `.po` file:

```bash
UPDATE_TRANSLATIONS=1 cargo test -p interface_adapters --test builtin_resources
```

Messages that only developers read (logs, the console, Package Load Errors, plugin tracebacks)
stay in English.
