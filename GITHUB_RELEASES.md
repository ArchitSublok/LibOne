# GitHub Releases Guide for LibOne

This guide explains how to prepare, draft, verify, and publish official GitHub Releases for **LibOne**.

---

## 1. Release Architecture & Naming Conventions

All official releases follow [Semantic Versioning (SemVer)](https://semver.org/):
- **Tag Format**: `vMAJOR.MINOR.PATCH` (e.g., `v0.1.0`)
- **Release Title**: `LibOne v0.1.0 - Windows Release`

---

## 2. Release Assets to Attach

Every GitHub Release must include the following assets attached to the release:

| File Name | Description |
| :--- | :--- |
| `LibOne_0.1.0_x64-setup.exe` | Standard Windows NSIS user installer (recommended for most users) |
| `LibOne_0.1.0_x64_portable.zip` | Standalone portable executable (contains `libone.exe`) requiring no installation |
| `LibOne_0.1.0_x64_en-US.msi` | Windows Installer package (for enterprise or silent deployments, if built) |
| `SHA256SUMS.txt` | Cryptographic SHA-256 checksum manifest for all attached binaries |

> **IMPORTANT**:
> - Never upload development databases (`*.db`, `*.sqlite`).
> - Never upload signing keys, certificates, or environment configs.
> - Always verify that all release assets were generated strictly on `D:\LibOne\.cargo-target\`.

---

## 3. GitHub Releases Template

When creating a new release on GitHub, use the following standardized Markdown template:

```markdown
# LibOne v0.1.0 - Local Document Library for Windows

LibOne helps people make sense of documents already stored on their Windows PC. It brings files scattered across Downloads, Desktop, Documents, and other folders into one searchable, organized library—without uploading them, moving them, or requiring an account.

### 🛡️ Core Privacy & Non-Destructive Guarantees
- **In-Place Indexing**: LibOne operates strictly as a read-only metadata layer over your documents where they already live.
- **Zero Document Alteration**: Original files are **never** modified, moved, renamed, uploaded, or deleted.
- **100% Local & Offline**: Zero network calls, telemetry, tracking, cloud APIs, accounts, or background uploads.
- **Parametric SQL**: All library metadata, collections, tags, and notes are managed in a local SQLite database (`%APPDATA%\com.libone.app\libone.db`).

---

### ✨ What's New in v0.1.0
- **Recursive Scan Engine**: Fast directory traversal with non-destructive cancel and error resilience.
- **Stable File Identity**: Win32 NTFS File ID tracking preserves tag and collection links across renames and moves.
- **Real-Time Watcher**: Background change detection with debouncing and safe coalescing.
- **Collections & Tags**: Tagging, color-coded collections, and markdown file notes.
- **Diagnostics View**: Transparent local system diagnostics with redacted text copy and local export.
- **Backup & Restore**: Atomic local database backup with pre-restore integrity validation.
- **Clean Uninstallation**: Uninstaller cleanly removes app binaries while giving you the option to preserve your metadata for future installs.

---

### ⚠️ Windows SmartScreen & Code Signing Notice
> [!NOTE]
> This release of LibOne is distributed as an independent open-source build without a commercial EV code signing certificate.
> When launching the installer for the first time, Windows SmartScreen may display:
> *"Windows protected your PC - Microsoft Defender SmartScreen prevented an unrecognized app from starting."*
>
> **To proceed safely:**
> 1. Click **More info**.
> 2. Verify that the SHA-256 hash matches the published checksum below.
> 3. Click **Run anyway**.

---

### 🖥️ System Requirements
- **Operating System**: Windows 10 (64-bit, build 19041+) or Windows 11 (64-bit)
- **Web Runtime**: Microsoft Edge WebView2 Runtime (pre-installed on Windows 11 and modern Windows 10; downloaded automatically if missing)
- **Disk Space**: ~35 MB for installation; minimal storage for metadata SQLite database.

---

### 📦 Verification & SHA-256 Checksums

Verify the authenticity of your download by running the following command in PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 <DownloadedFileName>
```

| File Name | SHA-256 Checksum |
| :--- | :--- |
| `LibOne_0.1.0_x64-setup.exe` | `<insert_sha256_here>` |
| `LibOne_0.1.0_x64_portable.zip` | `<insert_sha256_here>` |
```

---

## 4. Step-by-Step Release Workflow

### Step 1: Complete the Pre-Release Verification
Follow [`RELEASE_CHECKLIST.md`](RELEASE_CHECKLIST.md) to ensure all type-checking, Rust checks, and builds pass cleanly.

### Step 2: Build Release Bundles
```powershell
# 1. Build frontend
Set-Location D:\LibOne
npm run build

# 2. Build Tauri bundles
npm run tauri build
```

### Step 3: Stage Release Assets and Generate Hashes
```powershell
# Create release artifacts staging folder
New-Item -ItemType Directory -Force -Path "D:\LibOne\release_artifacts"

# Copy installer
Copy-Item "D:\LibOne\.cargo-target\release\bundle\nsis\LibOne_0.1.0_x64-setup.exe" "D:\LibOne\release_artifacts\"

# Package portable zip
Compress-Archive -Path "D:\LibOne\.cargo-target\release\libone.exe" -DestinationPath "D:\LibOne\release_artifacts\LibOne_0.1.0_x64_portable.zip" -Force

# Generate SHA-256 checksums file
Get-ChildItem -Path "D:\LibOne\release_artifacts\*" -Exclude "SHA256SUMS.txt" | ForEach-Object {
    $hash = (Get-FileHash -Path $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$hash  $($_.Name)"
} | Out-File -Encoding ascii -FilePath "D:\LibOne\release_artifacts\SHA256SUMS.txt"

# Verify hashes
Get-Content "D:\LibOne\release_artifacts\SHA256SUMS.txt"
```

### Step 4: Publish via GitHub CLI (Optional)
If the GitHub CLI (`gh`) is authenticated:
```powershell
gh release create v0.1.0 `
  --title "LibOne v0.1.0 - Windows Release" `
  --notes-file "D:\LibOne\release_notes.md" `
  "D:\LibOne\release_artifacts\LibOne_0.1.0_x64-setup.exe" `
  "D:\LibOne\release_artifacts\LibOne_0.1.0_x64_portable.zip" `
  "D:\LibOne\release_artifacts\SHA256SUMS.txt"
```
Or upload via the GitHub web interface at `https://github.com/ArchitSublok/LibOne/releases/new`.
