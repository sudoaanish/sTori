# Changelog

All notable changes to sTori will be documented here.

## 0.1.4 - Library intelligence

- Read embedded EPUB metadata and cover images when sidecar OPF or cover files are absent.
- Cache extracted covers in sTori application data without modifying source libraries.
- Added safe library rename and removal controls; source files are never deleted and the managed download library is protected.
- Added a reviewable scan-warning panel and removed the remaining background library watcher.

## 0.1.3 - Release hardening

- Added the **Updates** panel to the visible Windows **Server & Libraries** page, with manual check and verified install controls.
- Removed short-lived Windows console flashes from startup, firewall, and port-conflict checks.
- Kept Settings and download administration desktop-only at both the frontend route and server API layers.
- Added Windows and iPhone product screenshots to the README.

## 0.1.2 - PWA serving hotfix

- Fixed the packaged Windows server locating its bundled web assets after NSIS installation. iPhone and browser readers now receive the app shell instead of a blank page.

## 0.1.1 - Desktop continuity and signed updates

- Added signed in-app update checks and installation from GitHub Releases.
- Added an optional **Start sTori with Windows** setting; it launches minimized to the system tray.
- Added a system-tray menu. Closing the desktop window now keeps the local server running until **Quit sTori** is chosen.
- Added a schema migration that safely removes previously indexed MOBI entries; sTori now clearly supports EPUB and PDF only.
- Improved dark-theme accessibility with brighter gradient-accent text and a dedicated dark-mode logo.
- Removed the inactive **Watch for changes** library option so the UI accurately reflects current behavior.
- Retained automatic pre-migration database backups and integrity verification.

## 0.1.0 - Initial Windows preview

- Added a Tauri desktop shell and local Rust/Axum ebook server.
- Added read-only library scanning and SQLite-backed metadata.
- Added EPUB and PDF reading with persisted progress.
- Added search, genres, series, and manual collections.
- Added Project Gutenberg discovery and managed EPUB downloads.
- Added an automatic two-book starter shelf on first launch.
- Added QR pairing and a responsive iPhone reading interface.
- Added database migrations, backups, diagnostics, and download integrity checks.
- Added light and dark themes plus selectable serif app and reader fonts.
- Added private Windows NSIS packaging and release smoke-test tooling.

The initial Windows installer is unsigned and may trigger a SmartScreen warning.
