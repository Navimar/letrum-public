# Letrum

Letrum is a desktop manuscript editor for writers who want a continuous drafting
workspace without hiding the project from Git and AI agents.

Many writing tools are comfortable while the writer stays inside the app, but
they become awkward when Codex or another agent needs to read, edit, compare,
and explain changes in a project.

Letrum keeps the manuscript as plain `.md` or `.txt` files and gives the writer
a continuous manuscript view over those files.

## What It Does

- keeps scenes as regular Markdown or text files in the project root
- stores scene order directly in filenames with prefixes like `001_`, `002_`
- opens selected scenes as one continuous manuscript canvas
- supports notes for each scene, stored as separate files in `letrum_notes/`
- creates an `AGENTS.md` file that explains the project structure to AI agents
- reloads external file changes aggressively so AI edits show up in the editor
- keeps scene files easy to inspect with Git and Codex review tools
- supports search, scene splitting, renaming, deleting, and drag reordering
- shows word and character counts for the manuscript or current selection

## Project Structure

A typical project looks like this:

```text
001_opening.md
002_arrival.md
003_argument.md
letrum_notes/
  001_opening_note.md
  002_arrival_note.md
AGENTS.md
.letrum/
  scenes.json
```

Only root `.md` and `.txt` files are manuscript scenes. Folders are ignored by
the scene list, including `letrum_notes/`. Files inside `letrum_notes/` are
side notes linked to the matching scene.

## Run Locally

```bash
npm install
npm run tauri dev
```

Build a local desktop app:

```bash
npm run app:build
```

On macOS the built app is created at:

```text
src-tauri/target/release/bundle/macos/Letrum.app
```

## Requirements

- Node.js
- Rust
- Tauri prerequisites for your platform

For macOS builds you also need the normal Apple command line tooling installed.

## Feedback

I collect early feedback in the comments under this announcement post:

https://t.me/vaulinblog/509

## Repository Layout

```text
src/          React UI
src-tauri/    Tauri + Rust shell and filesystem commands
```

## Status

This is an early public preview. It is useful if you specifically want a
manuscript view over plain files and simple notes for each scene.
