# markdown-browser

A terminal markdown browser with first-class GFM table rendering.

Status: early development.

## Features

### Core

- **TUI-first navigation** — a full-screen interactive browser, not just a one-shot pretty printer
- **GFM table rendering** — proper column widths with East Asian Width and ANSI-aware sizing
- **Table of contents overlay** — heading outline accessible from any view
- **Syntax-highlighted code blocks** — via `syntect`
- **Styled headings, lists, blockquotes** — color and structure
- **Link following** — local `.md` files (relative and absolute), heading anchors (`#slug`), and external URLs delegated to `open`
- **History navigation** — back/forward through visited locations
- **Plain-text render subcommand** — `markdown-browser render <file>` writes ANSI-styled output to stdout, suitable for piping or snapshot testing

### Planned extensions

- Incremental search (`/` + `n`/`N`)
- Auto-reload on file change
- Yank code block to clipboard
- Task list (`- [x]`) rendering
- In-app help screen
- Customizable keybindings

### Future

- Directory browser

### Out of scope (kept extensible)

- Image rendering (Sixel / Kitty / iTerm2 inline)
- Mermaid diagram rendering

These won't ship in the foreseeable future — the maintainer's terminal stack (Ghostty + tmux) can't render Sixel reliably — but the renderer architecture exposes a pluggable trait so an alternative implementation can be slotted in without restructuring.

## License

[MIT](LICENSE-MIT).
