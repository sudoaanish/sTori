<div align="center">

<img src="public/stori-logo.png" alt="sTori logo" width="240" />

# sTori

### A local-first ebook server and personal reading room for Windows and iPhone.

sTori turns a Windows PC into a private ebook library server. Read on the desktop, pair an iPhone over the local network, and keep books, metadata, collections, and reading progress under your control.

<p>
  <a href="https://github.com/sudoaanish/sTori/releases">
    <img src="https://img.shields.io/badge/Version-v0.1.2%20Preview-34206B?style=for-the-badge&logo=github" alt="sTori v0.1.2 preview" />
  </a>
  <a href="https://github.com/sudoaanish/sTori/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/sudoaanish/sTori?style=for-the-badge" alt="MIT license" />
  </a>
</p>

<p>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Windows 10 and 11" />
  <img src="https://img.shields.io/badge/iPhone-PWA-000000?style=for-the-badge&logo=apple&logoColor=white" alt="iPhone PWA" />
  <img src="https://img.shields.io/badge/Tauri-2-24C8D8?style=for-the-badge&logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Rust-Axum-CE412B?style=for-the-badge&logo=rust&logoColor=white" alt="Rust and Axum" />
  <img src="https://img.shields.io/badge/Network-Local%20LAN%20%7C%20Hotspot-174A7E?style=for-the-badge" alt="Local LAN or hotspot" />
</p>

<p>
  <a href="#features">Features</a> |
  <a href="#installation">Installation</a> |
  <a href="#first-launch">First launch</a> |
  <a href="#using-stori">Usage</a> |
  <a href="#connect-an-iphone">Connect an iPhone</a> |
  <a href="#development">Development</a>
</p>

</div>

---

> **Windows preview:** sTori is early software. Installers are currently unsigned, so Windows SmartScreen may display a warning.

## Features

- Tauri 2 desktop application with a Rust/Axum server on port `1822`
- SQLite catalog, reading progress, annotations, collections, and series
- Read-only scanning of existing ebook libraries
- EPUB and PDF reading (MOBI is not currently supported)
- Project Gutenberg discovery and managed EPUB downloads
- Automatic two-book starter shelf on first launch
- Search, genres, inferred series, and manual collections
- Short-lived QR pairing for reader-only iPhone sessions
- Responsive installable web-app shell
- Database backups, integrity diagnostics, and download verification
- Light and dark themes with selectable serif app and reader fonts
- Signed in-app updates, system-tray operation, and optional Windows startup

Configured source libraries are treated as read-only. Downloads managed by sTori are stored under the current Windows user's Downloads folder in `sTori Books`; no username is hardcoded.

## Installation

### Windows installer

1. Open the [sTori Releases page](https://github.com/sudoaanish/sTori/releases).
2. Download the Windows installer named similar to:

   ```text
   sTori_0.1.2_x64-setup.exe
   ```

3. Run the installer and follow the Windows prompts.
4. Open **sTori** from the Start menu.

Node.js, Rust, Python, and a separate web server are not required when using the packaged installer.

### SmartScreen warning

The preview installer is not yet code-signed. Windows may show **Windows protected your PC**. If the installer came from the official `sudoaanish/sTori` Releases page, select **More info**, verify that the displayed application is sTori, and choose **Run anyway**.

Published releases include a SHA-256 checksum so the downloaded installer can be verified before it is run.

## First launch

On the first launch, sTori:

1. Starts its private local server on port `1822`.
2. Creates its application database and managed library.
3. Adds the current user's `Downloads\sTori Books` folder as a managed library.
4. Downloads and indexes *The Great Gatsby* and *Frankenstein* as a starter shelf when internet access is available.

The starter downloads run in the background. Their state is visible under **Download EPUBs**.

## Using sTori

### Add an existing ebook library

1. Open **Server** in the desktop sidebar.
2. Find **Libraries** and select **Add library**.
3. Enter a display name.
4. Select **Browse…** and choose the folder that directly contains the books or author folders.
5. Select **Add & scan**.

sTori reads supported book metadata and cover files without reorganizing or modifying the source library. Use **Scan** beside a library—or **Refresh / rescan library** in Settings—after adding or changing books.

### Read and resume a book

1. Open **Home**, **Library**, **Search**, or a collection.
2. Select a book cover to open its detail page.
3. Choose **Start reading** or **Continue reading**.
4. Tap or click the center of the reader to reveal reading controls.

Reading progress is saved by the PC server and can be resumed from either the desktop or a paired iPhone.

### Download public-domain EPUBs

1. Open **Download EPUBs** in the desktop sidebar.
2. Search the Project Gutenberg catalog by title or author.
3. Select **Add to library**.
4. Follow progress in the download queue.

Completed books are verified, imported into `Downloads\sTori Books`, and indexed automatically.

### Collections, series, and search

- Search by title, author, series, or genre.
- Use **Collections** to browse inferred series and manually curated shelves.
- Genre rows on Home provide quick access to broader categories.

### Settings, diagnostics, and backups

- Use the gear button for light/dark themes and app/reader font choices.
- Open **Server** to manage libraries, pairing, devices, diagnostics, and backups.
- The diagnostics cards report database health, available storage, and Windows Firewall status.

## Connect an iPhone

1. Put the PC and iPhone on the same Wi-Fi network, or connect the iPhone to the PC's Windows Mobile Hotspot.
2. Keep sTori running on the PC.
3. Open **Server** in the sTori desktop app.
4. Select **Create pairing code**.
5. Scan the displayed QR code with the iPhone Camera app.
6. Open the link in Safari and complete pairing.
7. In Safari, select **Share → Add to Home Screen** to install the sTori web app.

The exact LAN address depends on the computer and network. The Server page detects and displays the appropriate address. Paired iPhones receive reader access only; library administration remains restricted to the desktop.

## Troubleshooting

- **The iPhone cannot connect:** confirm both devices are on the same network, keep sTori open, and check the Windows Firewall diagnostic under **Server**.
- **Port 1822 is already in use:** close another running sTori development or desktop process and reopen the app.
- **A library is empty:** verify that the selected folder directly contains the intended book structure, then run **Scan**.
- **Starter books do not appear:** open **Download EPUBs** to inspect the queue and confirm the PC has internet access.
- **The phone asks to pair again:** create a fresh one-use pairing code from the desktop Server page.

Do not expose port `1822` directly to the public internet. sTori is designed for a trusted home LAN or personal hotspot.

## Development

Requirements:

- Node.js and npm
- Rust stable
- The [Tauri 2 Windows prerequisites](https://v2.tauri.app/start/prerequisites/)

```powershell
npm install
npm run build
npm run tauri dev
```

The desktop API is available at `http://127.0.0.1:1822`.

For browser-only development using a workspace-local database:

```powershell
npm run build
cargo run --manifest-path src-tauri/Cargo.toml --features dev-server --bin stori-server
```

### Tests

```powershell
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

The Windows release helper creates an NSIS build under `.release` by default:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/release_windows.ps1
```

## Privacy and security

sTori is designed for a trusted home network. Folder selection, rescanning, downloads, backups, and library administration are desktop-only operations. Paired reader clients can access indexed books and reading state but cannot browse arbitrary PC folders or manage libraries.

## License

sTori is licensed under the [MIT License](LICENSE).

Created by Aanish Farrukh / [sudoaanish](https://github.com/sudoaanish).

Copyright © 2026 Aanish Farrukh.
