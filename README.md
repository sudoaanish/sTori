# sTori

sTori is a local-first ebook server and personal reading room. The Windows desktop app owns a Rust server and serves the same React interface to an iPhone or another browser on the local network.

> **Windows preview:** sTori is early software. The initial installer is unsigned, so Windows SmartScreen may show a warning.

## Features

- Tauri 2 desktop application with a Rust/Axum server on port `1822`
- SQLite catalog, reading progress, annotations, collections, and series
- Read-only scanning of existing ebook libraries
- EPUB and PDF reading
- Project Gutenberg discovery and managed downloads
- Automatic starter shelf on first launch
- Search, genres, inferred series, and manual collections
- Short-lived QR pairing for reader-only iPhone sessions
- Responsive installable web-app shell

Configured source libraries are treated as read-only. Downloads managed by sTori are stored under the current user's Downloads folder in `sTori Books`; no Windows username is hardcoded.

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

## Pair an iPhone

1. Put the PC and iPhone on the same local network or Windows Mobile Hotspot.
2. Open the desktop **Server** page and generate a pairing QR code.
3. Scan the code with the iPhone to open the PC-hosted reader and establish a reader-only session.
4. In Safari, use **Share → Add to Home Screen** for a standalone sTori icon.

The exact LAN address depends on the computer and network. The Server page displays the address to use.

## Tests

```powershell
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

The Windows release helper creates a private NSIS build under `.release` by default:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/release_windows.ps1
```

## Privacy and security

sTori is designed for a trusted home network. Folder selection, rescanning, downloads, backups, and library administration are desktop-only operations. Paired reader clients can access indexed books and reading state but cannot browse arbitrary PC folders or manage libraries.

Do not expose port `1822` directly to the public internet.

## License

[MIT](LICENSE) © 2026 Aanish Farrukh ([@sudoaanish](https://github.com/sudoaanish)).
