# Orthanc

[![CI](https://github.com/marcelotrevisani/orthanc/actions/workflows/ci.yml/badge.svg)](https://github.com/marcelotrevisani/orthanc/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/py-orthanc.svg)](https://crates.io/crates/py-orthanc)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Sponsor](https://img.shields.io/badge/sponsor-%E2%9D%A4-ea4aaa?logo=githubsponsors)](https://github.com/sponsors/marcelotrevisani)

A desktop app for managing Python virtual environments with [uv](https://github.com/astral-sh/uv).
Keep every project's venv, dependencies, and git status in one window, on Linux, macOS, and Windows.

Built in Rust with [iced](https://github.com/iced-rs/iced).

## Features

- **Environments**: create, update, and delete uv virtual environments for your projects. New venvs go in `<project>/.venv` by default, or in a folder you choose.
- **Dependencies**: list installed packages, change versions, uninstall, and view the dependency tree as a graph. Install from:
  - PyPI
  - a local folder, editable (`-e`) or not
  - a GitHub repository, with an optional branch, tag, or commit
- **Git status**: each project shows its branch, uncommitted changes, and commits ahead/behind its upstream.
- **Open in your tools**: start a terminal with the venv activated, or open the project in PyCharm with the venv already set as its interpreter.
- **Environment variables**: set them globally or per environment. They're applied when you open a terminal.
- **Private package index**: use any PEP 503 index, with or without a username and password. Choose whether public PyPI is also used for packages the private index doesn't have.
- **GitHub**: see pull requests across an organisation, and view or re-run workflow runs for a project.
- **System tray**: Orthanc keeps running in the tray, with quick access to each environment's terminal.
- **Light and dark themes**.

## Install

You need [uv](https://docs.astral.sh/uv/getting-started/installation/) on your `PATH`.

### Download a binary

Each [release](https://github.com/marcelotrevisani/orthanc/releases) includes ready-to-run binaries:

| Platform              | File                          |
|-----------------------|-------------------------------|
| Linux x86_64          | `orthanc-linux-x86_64`        |
| Linux ARM64           | `orthanc-linux-aarch64`       |
| macOS (Apple Silicon) | `orthanc-darwin-aarch64`      |
| macOS (Intel)         | `orthanc-darwin-x86_64`       |
| Windows x86_64        | `orthanc-windows-x86_64.exe`  |

On Linux and macOS, make the file executable first: `chmod +x orthanc-*`.
On macOS, the binary isn't signed, so you need to clear the download quarantine once:
`xattr -d com.apple.quarantine orthanc-darwin-*`.
The Linux binaries need glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36, or later).

Orthanc can update itself from **Settings → Updates**.

### With Cargo

```sh
cargo install py-orthanc
```

The crate is called `py-orthanc` because `orthanc` is already taken on crates.io. The installed command is still `orthanc`.

### Build from source

You need Rust 1.80 or newer.

```sh
git clone https://github.com/marcelotrevisani/orthanc.git
cd orthanc
cargo run --release
```

## Configuration

Settings are saved to `orthanc.toml` in your OS config folder:

| OS      | Path                                                  |
|---------|-------------------------------------------------------|
| Linux   | `~/.config/orthanc/orthanc.toml`                      |
| macOS   | `~/Library/Application Support/orthanc/orthanc.toml`  |
| Windows | `%APPDATA%\orthanc\orthanc.toml`                      |

Everything can be changed from the in-app **Settings** page. GitHub features need a personal access token.

## Contributing

Issues and pull requests are welcome. Before opening a PR, run:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

## Support the project

If Orthanc saves you time, please consider [sponsoring its development on GitHub](https://github.com/sponsors/marcelotrevisani).
You can also use the **Sponsor** button in the app's sidebar.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
