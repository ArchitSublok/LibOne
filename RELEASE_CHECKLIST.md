# LibOne Windows Release Checklist

This document defines the strict pre-release verification, build execution, checksum generation, and safety checklist for publishing Windows releases of **LibOne**.

---

## 1. Release Principles & Guarantees

1. **Non-Destructive User Document Guarantee**:
   - LibOne operates strictly as a read-only metadata layer over user documents.
   - Installing, upgrading, running, or uninstalling LibOne **never** alters, moves, deletes, or uploads user documents on disk.

2. **Zero Hardcoded Secrets & No Fake Signing Claims**:
   - The repository contains no private keys, `.pfx`/`.p12` certificates, or credentials.
   - Code signing is not claimed in releases unless a valid EV/OV code-signing certificate and timestamp authority are explicitly configured in the build environment. Unsigned releases are clearly declared so users understand Windows SmartScreen prompts.

3. **Self-Contained Artifact Storage on Drive D:**:
   - All Cargo target builds, frontend dists, intermediate compiler objects, and final installer bundles are generated strictly under `D:\LibOne\.cargo-target` and `D:\LibOne\dist`.
   - Never write release artifacts or temporary toolchains to the system C: drive.

4. **Zero Bloat & Clean Bundling**:
   - Neither `node_modules/`, compiler caches, local test databases (`*.db`, `*.sqlite`), test fixtures, nor downloaded SDK caches (`.xwin-cache/`) are packaged into the release binary or installer.
   - Only the minified frontend distribution in `dist/` is compiled into the Tauri executable via `tauri-build`.

5. **Clean Uninstallation & Optional Metadata Preservation**:
   - The NSIS uninstaller (`src-tauri/nsis/hooks.nsh`) removes all application binaries, start menu shortcuts, and registry keys.
   - The uninstaller explicitly prompts the user whether to preserve or remove application metadata in `%APPDATA%\com.libone.app`:
     - **Keep Metadata**: Preserves tags, collections, notes, and index history for future installs or upgrades.
     - **Remove Metadata**: Cleans up all cached metadata and settings.
     - In **both** cases, original user documents remain 100% untouched.

6. **Automatic Upgrade Migrations**:
   - Upgrading from earlier versions (schema v1, v2, v3) to the current version automatically applies transactional migrations sequentially via `db.migrate()` on first application launch.

7. **WebView2 Runtime Assumptions**:
   - LibOne targets Windows 10 and Windows 11 (64-bit).
   - WebView2 is pre-installed on Windows 11 and modern Windows 10 builds.
   - For legacy or stripped environments, Tauri's installer is configured with the Evergreen `downloadBootstrapper` fallback (`silent: true`) to ensure seamless execution without user friction.

---

## 2. Release Artifact Locations

All release artifacts are generated on the `D:` drive:

| Artifact Type | Format | Output Path |
| :--- | :--- | :--- |
| **NSIS Setup Installer** | Executable installer (`.exe`) | `D:\LibOne\.cargo-target\release\bundle\nsis\LibOne_0.1.0_x64-setup.exe` |
| **MSI Installer** | Windows Installer package (`.msi`) | `D:\LibOne\.cargo-target\release\bundle\msi\LibOne_0.1.0_x64_en-US.msi` |
| **Portable Executable** | Standalone binary (`.exe`) | `D:\LibOne\.cargo-target\release\libone.exe` |
| **Frontend Distribution** | Static minified HTML/CSS/JS | `D:\LibOne\dist\` |
| **Checksum Manifest** | Plain text SHA-256 hashes | `D:\LibOne\release_artifacts\SHA256SUMS.txt` |

---

## 3. Pre-Release Verification Checklist

Run these steps in order before building release binaries:

- [ ] **Working Tree Cleanliness**:
  Ensure no uncommitted secrets, debug traces, or temporary databases exist.
  ```powershell
  git status
  ```

- [ ] **Exclude Secrets & Keys in `.gitignore`**:
  Verify `.gitignore` blocks all certificates (`*.pfx`, `*.p12`, `*.key`), local databases (`*.db`, `*.sqlite`), and build caches.

- [ ] **Rust Formatting Check**:
  ```powershell
  Set-Location D:\LibOne\src-tauri
  cargo fmt --check
  ```

- [ ] **Rust Compilation Check**:
  ```powershell
  Set-Location D:\LibOne\src-tauri
  cargo check
  ```

- [ ] **TypeScript Type Check**:
  ```powershell
  Set-Location D:\LibOne
  npx tsc --noEmit
  ```

- [ ] **Frontend Build Verification**:
  ```powershell
  Set-Location D:\LibOne
  npm run build
  ```

- [ ] **App Version and Metadata Alignment**:
  - `package.json`: `"version": "0.1.0"`
  - `src-tauri/Cargo.toml`: `version = "0.1.0"`
  - `src-tauri/tauri.conf.json`: `"version": "0.1.0"`

- [ ] **App Icon Verification**:
  Verify all icon assets exist and are valid in `src-tauri/icons/`:
  - `icon.ico` (multi-resolution Windows icon)
  - `32x32.png`, `128x128.png`, `128x128@2x.png`
  - `icon.png`

---

## 4. Release Build Instructions

Follow these exact commands to build the release artifacts without running any installer:

### Step 1: Build the Optimized Frontend
```powershell
Set-Location D:\LibOne
npm run build
```
*Output: `D:\LibOne\dist\` containing index.html and bundled assets.*

### Step 2: Build the Production Release Binary (Portable)
```powershell
Set-Location D:\LibOne\src-tauri
cargo build --release
```
*Output: `D:\LibOne\.cargo-target\release\libone.exe` (stripped, LTO enabled, panic=abort).*

### Step 3: Build the Tauri Bundles (NSIS Setup & MSI)
```powershell
Set-Location D:\LibOne
npm run tauri build
```
*Outputs:*
- NSIS Setup: `D:\LibOne\.cargo-target\release\bundle\nsis\LibOne_0.1.0_x64-setup.exe`
- MSI Package: `D:\LibOne\.cargo-target\release\bundle\msi\LibOne_0.1.0_x64_en-US.msi` (if WiX toolset is present)

> **SAFETY NOTICE**: **Do NOT run or execute the generated installer automatically.** Always inspect the generated bundle size and verify checksums before distributing.

---

## 5. Checksum Generation

To allow users to verify the integrity and authenticity of release binaries, generate SHA-256 checksums:

```powershell
# Create staging folder for release assets
New-Item -ItemType Directory -Force -Path "D:\LibOne\release_artifacts"

# Copy portable binary and installer to staging
Copy-Item "D:\LibOne\.cargo-target\release\bundle\nsis\LibOne_0.1.0_x64-setup.exe" "D:\LibOne\release_artifacts\"
Copy-Item "D:\LibOne\.cargo-target\release\libone.exe" "D:\LibOne\release_artifacts\LibOne_0.1.0_x64_portable.exe"

# Generate SHA256 checksums
Get-ChildItem -Path "D:\LibOne\release_artifacts\*.exe" | ForEach-Object {
    $hash = (Get-FileHash -Path $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$hash  $($_.Name)"
} | Out-File -Encoding ascii -FilePath "D:\LibOne\release_artifacts\SHA256SUMS.txt"

# Display generated checksums
Get-Content "D:\LibOne\release_artifacts\SHA256SUMS.txt"
```

---

## 6. Post-Build Smoke Test Checklist (Manual Verification)

Perform these tests on a clean Windows machine or isolated test environment:

1. **Clean Installation**:
   - Run `LibOne_0.1.0_x64-setup.exe` in current-user mode.
   - Confirm install finishes without admin prompt or elevation requests.
   - Verify desktop and start menu shortcuts launch LibOne.

2. **Document Non-Destruction Verification**:
   - Register a folder containing test PDFs, text files, and images.
   - Confirm documents are indexed and searchable.
   - Confirm file modified timestamps and original contents on disk are identical.

3. **In-App Diagnostics Check**:
   - Open Settings -> Diagnostics.
   - Verify App Version (`0.1.0`), Database Schema Version (`4`), and database path (`%APPDATA%\com.libone.app\libone.db`).
   - Confirm Integrity Check reports `OK`.

4. **Upgrade Migration Verification**:
   - Place a simulated schema v1 or v2 database in `%APPDATA%\com.libone.app\libone.db`.
   - Launch LibOne v0.1.0.
   - Verify database migrations complete automatically without data loss.

5. **Clean Uninstallation Verification**:
   - Run the uninstaller via Windows Settings -> Apps -> Installed Apps -> LibOne -> Uninstall.
   - When prompted:
     - Test "No" (Preserve metadata): App binaries removed; `%APPDATA%\com.libone.app\libone.db` remains intact; original document files untouched.
     - Test "Yes" (Remove metadata): `%APPDATA%\com.libone.app` removed; original document files untouched.
