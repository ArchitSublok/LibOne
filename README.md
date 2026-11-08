# LibOne

**Your private, local-first document library.**

LibOne helps people make sense of documents already stored on their Windows PC. It brings files scattered across Downloads, Desktop, Documents, and other folders into one searchable library—without uploading them, moving them, or requiring an account.

## Why LibOne

Most document tools make people build and maintain an organization system themselves. LibOne is designed to do the difficult first pass: it indexes files in place, identifies likely duplicates and related documents, suggests useful groupings, and gives people a calm, simple way to find what matters.

> Turn a messy PC into a usable personal document library in minutes—without moving or uploading a single file.

## What makes it different

- **Organizes files where they already live** — LibOne creates a library layer over existing folders. Originals remain in their current locations and open in their usual apps.
- **Assisted organization** — Rather than requiring manual folders and tags from day one, LibOne can suggest collections by topic, project, person, date, and document type for the user to approve or correct.
- **One search across the folder maze** — Search file names, document text, tags, notes, and personal context from one place.
- **Document memory** — Add a private note such as why a file matters, what it relates to, its status, or when it should be reviewed—without editing the original document.
- **Duplicates and versions made understandable** — Surface exact copies, likely renamed copies, scans of the same document, and related versions so users can keep a clear canonical file.
- **Privacy by default** — The library and its metadata stay on the device. No account, subscription, or cloud upload is required.
- **Built for everyday paperwork** — Useful for bills, IDs, tax records, warranties, receipts, medical documents, household records, and project files—not only academic research.

## Core workflow

1. Choose folders such as Downloads, Desktop, and Documents.
2. LibOne indexes those files locally, without relocating them.
3. Review suggested collections, duplicate groups, and document categories.
4. Search, filter, tag, and add notes whenever you need to find something.
5. Open the original file directly from LibOne.

## Principles

- **Local-first:** your documents and metadata belong on your computer.
- **Safe by default:** no silent file moves, renames, or deletions.
- **Portable:** keep metadata exportable so the library never becomes a lock-in.
- **Simple before powerful:** a useful library should not require learning a new filing system.

## Comparison

Tagging file managers provide flexible manual organization; research managers focus on citations and scholarly sources. LibOne focuses on the step those tools often leave to the user: making an existing, disorganized collection of personal files understandable with very little setup.

## Project structure: four parts

### 1. Local Library Foundation

The reliable local layer that discovers and tracks files without taking ownership of them.

- Folder selection and scanning
- File metadata, change detection, and original-path tracking
- Local database and portable metadata export
- Secure opening of the original file in its default application

### 2. Organization & Search Intelligence

The features that turn a file index into a useful library.

- Full-text, filename, tag, and note search
- Collections, tags, and personal notes
- Document-type detection and suggested collections
- Exact-duplicate detection, version grouping, and review queues

### 3. Windows Desktop Experience

The simple interface that makes LibOne approachable for non-technical users.

- Fast first-run setup for Desktop, Downloads, and Documents
- Library, collection, search-result, and document-detail views
- Preview, filters, bulk actions, and clear links to original locations
- Review screens where users approve or reject organization suggestions

### 4. Privacy, Reliability & Release

The work that makes the app dependable enough to trust with important records.

- Local-first privacy controls and transparent data handling
- Safe-by-default operations: no silent moves, renames, or deletions
- Backup/restore, missing-file alerts, and metadata export/import
- Automated tests, Windows packaging, GitHub Releases, and update notes

## Development

### Prerequisites

- **Node.js**: v18 or later (Node v22+ recommended)
- **Rust toolchain**: Install Rust via [rustup.rs](https://rustup.rs) (`stable-x86_64-pc-windows-msvc`)
- **Visual Studio C++ Build Tools**: Required on Windows for native linking (`link.exe` and Windows SDK `kernel32.lib`)
- **WebView2 Runtime**: Included with Windows 10/11

### Setup & Running

1. **Install dependencies**:
   ```bash
   npm install
   ```

2. **Frontend development**:
   ```bash
   npm run dev
   ```

3. **Type-check and bundle frontend**:
   ```bash
   npm run build
   ```

4. **Run the full Tauri desktop application**:
   ```bash
   npm run tauri dev
   ```

5. **Build production Windows installer**:
   ```bash
   npm run tauri build
   ```

For detailed architecture, local data storage paths, and Phase 1 data flows, see [ARCHITECTURE.md](ARCHITECTURE.md).

