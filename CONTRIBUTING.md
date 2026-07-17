# Contributing

sTori is an early Windows-first project. Bug reports and focused pull requests are welcome.

Before opening a pull request:

1. Keep configured ebook libraries read-only.
2. Do not commit books, databases, build output, local paths, access tokens, or pairing data.
3. Run `npm test`.
4. Run `cargo test --manifest-path src-tauri/Cargo.toml`.
5. Describe any Windows desktop, LAN, iPhone, or reader behavior that needs manual testing.

For security-sensitive reports, do not publish tokens, private library paths, or personal book metadata in a public issue.
