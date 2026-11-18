# Launch-Posts für gpui-markdown-editor 0.1.0

Entwürfe, nichts davon ist gepostet. Gepostet wird vom persönlichen Reddit-Account des
Entwicklers, nicht von einem taulit-Account.

## Voraussetzungen vor dem Posten

Stand 2026-11-18:

1. **Die Repos sind privat.** `taulit/gpui-markdown-editor` und `taulit/sheaf` sind
   `PRIVATE`. Links aus dem Post führen sonst ins Leere, und GitHub Pages braucht für
   private Repos einen bezahlten Plan. Erst öffentlich schalten.
2. **0.1.0 ist nicht auf crates.io.** Der Release-Commit ist in `main` gemergt und
   `v0.1.0` getaggt, aber `Cargo.toml` hat `publish = false` (siehe
   `docs/release-checklist.md` §6). Ohne Veröffentlichung gibt es auch keine docs.rs-Seite.
   Der Post ist so geschrieben, dass er erst **nach** `cargo publish` Sinn ergibt
   (`cargo add`, docs.rs-Link). Wer vorher postet, ersetzt die Install-Zeile durch den
   Git-Tag.
3. **Docs-Seite live:** Pages aktivieren (Settings → Pages → Source: GitHub Actions), dann
   den Workflow `Pages` laufen lassen. Danach https://taulit.github.io/gpui-markdown-editor/
   prüfen, auch die OG-Vorschau (z. B. mit https://www.opengraph.xyz).
4. **Sheaf-Link:** Der Post verweist auf Sheaf. Ist Sheaf zu dem Zeitpunkt noch privat oder
   unveröffentlicht, den Satz zu Sheaf ohne Link stehen lassen.
5. **Medien:** `docs/screenshots/showcase.gif` (720×754, 180 KB, beginnt mit dem fertigen
   Ergebnis als Vorschaubild, dann wird getippt: Überschrift, Checkliste mit abgehaktem
   Punkt, Fett, Highlight, Tabelle über das Slash-Menü). Neu erzeugen mit
   `scripts/showcase-gif.sh`. Das ältere, schlichtere `typing.gif` bleibt für die README.

---

## 1. r/rust (Hauptpost)

**Format:** Bild-/GIF-Post mit Text oder Textpost mit dem GIF eingebettet (Reddit erlaubt
Medien im Textpost). Flair: das Projekt-Flair von r/rust (zuletzt „🛠️ project“, vor dem
Posten in der Flair-Auswahl prüfen).

**Titel (Vorschlag, Alternativen unten):**

> gpui-markdown-editor 0.1: a WYSIWYG Markdown editor component for GPUI with a lossless round-trip

Alternativen:

- I built a Notion-style Markdown editor for GPUI (Zed's UI framework) that saves your files byte for byte
- gpui-markdown-editor: WYSIWYG Markdown editing for GPUI apps – input rules, tables, checklists, 1.5 ms per keystroke at 10k blocks

**Text:**

```markdown
Hi r/rust! I just released 0.1 of **gpui-markdown-editor**, a WYSIWYG Markdown editor
component for [GPUI](https://www.gpui.rs), the UI framework behind Zed. People edit
formatted text like in Apple Notes or Notion, and your app reads and writes plain
Markdown files.

I built it for Sheaf, a notes app for GNOME that stores every note as a `.md` file. I
needed an editor that doesn't mangle files written by other tools, and nothing like that
existed for GPUI: gpui-component's code editor edits the Markdown *source*, and its
`TextView` only *displays* Markdown. So I split the editor out into its own crate.

**What it does**

- **Lossless round-trip.** Open a file and save it without edits, and you get the same
  bytes back, including line endings, trailing whitespace, a BOM and front matter. Only
  blocks you edit are rewritten, in one consistent style, and their neighbours keep their
  original bytes. Syntax the editor doesn't model (HTML blocks, footnotes, math) is kept
  as raw text. Undoing every edit restores the original file byte for byte. This is tested
  on all 1354 CommonMark/GFM spec examples plus a corpus of real notes.
- **Input rules.** `# ` becomes a heading, `[] ` a checkbox, `**bold**` turns bold as you
  type. Backspace right after a rule undoes it, and `input_rules: false` turns them off.
- **Tables** (GFM) with cell editing, rectangle selection, row/column menus and keys, and
  resizable columns. Column widths and folds are stored outside the Markdown, so the file
  stays plain GFM.
- **Checklists**, images (decoded off the UI thread, resizable), attachment rows, a format
  bar, slash menu, link popover, find bar and `[[wiki link]]`/`#tag` completions. Each UI
  part can be switched off and replaced with your own UI driven by commands.
- **Extensions** for custom inline syntax (parse, serialize, style, input rule, click).
  Wiki links, tags and `==highlights==` ship as extensions.
- **A headless `Document` model** (outline, tasks, tags, rewriting) that needs no window.

**Performance** (criterion, release build, i5-11400F): a keystroke plus its frame takes
about 1.5 ms in a 10,000-block document, and opening a 1 MB file takes about 31 ms. The
budgets were 4 ms and 50 ms. Caveats: the benchmarks use GPUI's headless window, so GPU
rasterisation isn't included, and under heavy machine load the numbers went up to 4.4 ms
and 66 ms. The method and full table are on the benchmarks page.

**Known limits:** GPUI has no HTML clipboard format, so rich-text paste isn't possible
yet. Screen readers get roles but not the caret or selection. IME is tested in simulation,
but not by hand on every desktop. Extensions can add marks, but not their own block types.
It pins `gpui-pre =0.3.12` and `gpui-component =0.10.0`, and your app has to use the same
versions.

**How it was built:** I wrote it with heavy help from an AI coding assistant (Claude).
I set the design, the round-trip rules and the performance budgets, and I review the
code. The test suite (spec corpus, property tests, interaction and screenshot tests)
is how I hold it to them. I'm saying this upfront because I know people care.

    cargo add gpui-markdown-editor

- Repo: https://github.com/taulit/gpui-markdown-editor
- Docs & quickstart: https://taulit.github.io/gpui-markdown-editor/
- API docs: https://docs.rs/gpui-markdown-editor

Apache-2.0. I'd love feedback on the API, especially from anyone building GPUI apps. Bug
reports with Markdown files that don't round-trip are very welcome.
```

**Hinweise zum Text**

- Der Absatz „How it was built“ ist meine Empfehlung, keine Pflicht. Die Commits tragen
  sichtbar `Co-Authored-By: Claude`, und Leser auf r/rust und im TWiR-Umfeld fragen in
  letzter Zeit gezielt nach KI-Anteilen. Offenlegen ist ehrlicher und nimmt der
  Diskussion den Wind aus den Segeln. Formulierung bitte an deine eigene Sicht anpassen
  oder streichen.
- Alle Zahlen stammen aus `docs/performance.md` und `README.md`. Wenn neu gemessen wird,
  hier nachziehen.
- Ersten Kommentar selbst schreiben ist auf r/rust üblich, aber nicht nötig. Auf Fragen in
  den ersten Stunden antworten.

### Regeln r/rust

Nicht automatisch prüfbar: Reddit blockt Abrufe aus dieser Umgebung („blocked by network
security“), und der Mirror war nicht erreichbar. **Vor dem Posten bitte die Sidebar/Rules
im Browser lesen.** Bekannt bzw. indirekt belegt:

- Projekt-Posts zu eigenen Open-Source-Crates sind auf r/rust üblich und erlaubt.
  Kommerzielle Eigenwerbung wird strenger geprüft (laut Self-Promotion-Wiki über einen
  Drittanbieter-Mirror, möglicherweise veraltet).
- Es gibt einen Mod-Post „No more code dumps“ (reddit.com/r/rust/comments/1wkmzun). Der
  Inhalt war nicht abrufbar, vermutlich sind reine Repo-Links ohne Erklärung gemeint. Der
  Entwurf oben erklärt Motivation, Design und Grenzen und sollte das erfüllen. Den Post
  trotzdem vorher lesen.
- Eine eigene r/rust-Regel zu KI-generiertem Code war nicht auffindbar. Das Rust-Projekt
  selbst hat im August 2026 eine LLM-Policy für Beiträge zu rust-lang/rust eingeführt
  (gilt nicht für r/rust, zeigt aber die Stimmung).
- Projekt-Flair setzen.

---

## 2. This Week in Rust

- **Keine PR mehr für „Project/Tooling Updates“.** Seit Ausgabe 664 nimmt TWiR diese Links
  nur noch von r/rust (Issue rust-lang/this-week-in-rust#8575, August 2026). Der r/rust-Post
  ist also gleichzeitig die TWiR-Einreichung, und die Aufnahme liegt bei den Redakteuren.
  Bedingung: Die r/rust-Community-Regeln müssen eingehalten sein.
- **Crate of the Week:** Nominierungen laufen im Thread auf users.rust-lang.org
  („Crate of the Week“). Gerade wird dort heftig über Selbstnominierungen von
  LLM-geschriebenem Code diskutiert (Thread „Crate of the week quality rules“, September
  2026). **Empfehlung: nicht selbst nominieren.** Wenn jemand anderes es tut, gut.

---

## 3. r/ZedEditor (optional, kurz)

Ob GPUI-Bibliotheken dort on-topic sind, ließ sich nicht prüfen (Reddit blockt). Der Sub
dreht sich um den Editor selbst. **Vor dem Posten Sidebar lesen.** Wenn dort keine
GPUI-Posts zu finden sind, lieber weglassen.

**Titel:** Built with GPUI: a WYSIWYG Markdown editor component for your own GPUI apps

```markdown
For anyone building apps on GPUI (Zed's UI framework): I released
**gpui-markdown-editor**, a Notion-style Markdown editor component. It shows formatted
text while you type and saves Markdown files losslessly. It has input rules, tables,
checklists and images, and a keystroke takes about 1.5 ms in a 10k-block document. It
works with gpui-component/GPUI Kit themes.

To be clear, it's not a Zed extension (those can't add views). It's a crate for your
own GPUI application.

Repo: https://github.com/taulit/gpui-markdown-editor
Docs: https://taulit.github.io/gpui-markdown-editor/
```

---

## 4. Weitere Orte (kein Reddit, aber passende Zielgruppe)

- **awesome-gpui** (github.com/zed-industries/awesome-gpui): Eintrag per PR in
  `projects.json` (nicht in die README), validieren mit
  `uv run check-jsonschema --schemafile projects.schema.json projects.json` und
  `uv run scripts/sort_projects.py --check`. Vorschlag:

  ```json
  {
    "name": "gpui-markdown-editor",
    "category": "Libraries",
    "url": "https://github.com/taulit/gpui-markdown-editor",
    "image": "https://raw.githubusercontent.com/taulit/gpui-markdown-editor/main/docs/screenshots/showcase.gif",
    "description": "WYSIWYG Markdown editor component with a lossless round-trip, input rules, tables and checklists."
  }
  ```

- **GPUI Kit (ehemals gpui-component) Discussions:** Der angepinnte Thread „Showcase for
  used GPUI Kit“ (github.com/longbridge/gpui-kit/discussions/989) passt für einen kurzen
  Kommentar mit GIF und Link. Separat steht in der Release-Checkliste der Vorschlag, das
  Crate in GPUI Kit aufzunehmen (eigenes Issue/Discussion).
- **Zed-Discord:** Es soll dort einen GPUI-Kanal geben. Nicht geprüft.

### Nicht empfohlen

- **r/opensource:** Laut Berichten von Oktober 2026 (Tom's Hardware, runtimewire) sind
  KI-erstellte Projekte dort verboten. Für Eigenwerbung werden außerdem ein Jahr Pflege,
  mehrere Mitwirkende und ein ein Jahr alter Account verlangt. Passt alles nicht.
- **r/coolgithubprojects:** Regeln nicht prüfbar. Wenig Rust-Publikum, geringer Nutzen.
- Kein Instagram, kein Video (wie vereinbart).

## Reihenfolge

1. Repo öffentlich, `cargo publish`, Pages aktiv, Seite und OG-Vorschau geprüft.
2. r/rust posten (Werktag, vormittags US-Ostküste ≈ 15–17 Uhr MEZ), die ersten Stunden
   Fragen beantworten.
3. Am selben oder nächsten Tag: Kommentar im GPUI-Kit-Showcase und PR an awesome-gpui.
4. Optional r/ZedEditor, falls die Regeln es zulassen.
