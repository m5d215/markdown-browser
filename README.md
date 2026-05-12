# markdown-browser

A terminal markdown browser with first-class GFM table rendering.

Status: early development.

## Features

- **TUI-first navigation** — a full-screen interactive browser, not just a one-shot pretty printer
- **GFM table rendering** — proper column widths with East Asian Width and ANSI-aware sizing
- **Table of contents overlay** — heading outline accessible from any view
- **Incremental search** — `/` to type, `n` / `N` to step through hits
- **Auto-reload on file change** — edit a file in another window, the buffer refreshes
- **Syntax-highlighted code blocks** — via `syntect`
- **Styled headings, lists, blockquotes, task lists** — color and structure
- **Line cursor with line numbers** — `j`/`k` move a logical-line cursor; the gutter shows line numbers (toggle with `#`)
- **Link following** — local `.md` files (relative and absolute), heading anchors (`#slug`), and external URLs handed off to the OS
- **History navigation** — back / forward through visited locations
- **In-app help** — `?` shows the full keybinding list
- **Plain-text render subcommand** — `markdown-browser render <file>` writes ANSI-styled output to stdout, suitable for piping or snapshot testing

### Out of scope (kept extensible)

- Image rendering (Sixel / Kitty / iTerm2 inline)
- Mermaid diagram rendering

The renderer architecture exposes a pluggable trait so an alternative implementation can be slotted in later without restructuring callers.

## Install

```bash
cargo install --git https://github.com/m5d215/markdown-browser
```

## Usage

```bash
markdown-browser <file>           # open the TUI browser
markdown-browser render <file>    # write ANSI-styled output to stdout
cat foo.md | markdown-browser render
```

`examples/showcase.md` exercises every supported feature; it doubles as a smoke
test and a manual reference for what's covered.

## Keybindings

Press `?` inside the TUI for the same list shown here.

| Key                            | Action                                          |
|--------------------------------|-------------------------------------------------|
| `q` / `Esc` / `Ctrl-C`         | Quit                                            |
| `?`                            | Toggle help overlay                             |
| `o`                            | Toggle table-of-contents overlay                |
| `#`                            | Toggle line numbers                             |
| `/`                            | Start search (`Enter` to commit, `Esc` to cancel) |
| `n` / `N`                      | Next / previous search hit                      |
| `Tab` / `Shift-Tab`            | Focus next / previous link                      |
| `Enter`                        | Open focused link                               |
| `[` / `]`                      | History back / forward                          |
| `Backspace`                    | History back (alias)                            |
| `j` / `k` (or `↓`/`↑`)         | Move cursor one line                            |
| `Ctrl-d` / `Ctrl-u`            | Half-page cursor move                           |
| `Ctrl-f` / `Ctrl-b` (or `PgDn`/`PgUp`) | Full-page cursor move                   |
| `g` / `G` (or `Home`/`End`)    | Jump cursor to top / bottom                     |

## License

[MIT](LICENSE-MIT).
