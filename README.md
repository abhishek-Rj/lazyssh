# russhx

`russhx` is a terminal user interface for managing SSH servers locally.

The goal is to make SSH access feel like using tools such as `lazygit`, `lazydocker`, or `k9s`: fast, keyboard-first, and built for people who live in the terminal.

Instead of remembering IP addresses, usernames, ports, SSH key paths, notes, and tags, you keep them in a local vault and launch SSH sessions from a TUI.

## Project Goals

- Store SSH server entries locally.
- Keep server metadata easy to browse and edit.
- Launch SSH using the same terminal session that opened `russhx`.
- Avoid cloud sync or remote dependencies.
- Keep the UI stable, dark, keyboard-driven, and DevOps-focused.
- Use SQLite as the persistence layer.
- Keep application state in memory while the UI is running.

## Current Features

- Main dashboard with:
    - server list
    - group filter list
    - server details panel
    - overview panel
    - footer shortcut bar
- Add, edit, and delete server entries.
- Manage groups:
    - create groups
    - edit group names
    - delete groups
- Assign existing groups when adding or editing a server.
- Search servers by name, host, username, group, and tags.
- Edit notes and tags.
- Render tags as colorful terminal chips.
- Copy common server values.
- Store all data in SQLite.
- Launch SSH key based connections through the normal shell.

## Data Storage

`russhx` stores data in a local SQLite database.

The database path is chosen according to the operating system:

- Linux/Unix: `$XDG_DATA_HOME/russhx/russhx.db` or `~/.local/share/russhx/russhx.db`
- macOS: `~/Library/Application Support/russhx/russhx.db`
- Windows: `%APPDATA%\russhx\russhx.db` or `%LOCALAPPDATA%\russhx\russhx.db`

No data is synced to the cloud.

## Keyboard Shortcuts

### Main Dashboard

| Shortcut       | Action                         |
| -------------- | ------------------------------ |
| `↑` / `↓`      | Move through the server list   |
| `←` / `→`      | Switch group filter            |
| `Enter`        | Connect to the selected server |
| `a`            | Add a new server               |
| `e`            | Edit selected server           |
| `d` / `Delete` | Delete selected server         |
| `g`            | Open group manager             |
| `/`            | Search                         |
| `c`            | Copy server value              |
| `n`            | Edit notes                     |
| `t`            | Edit tags                      |
| `?`            | Open help                      |
| `q`            | Quit                           |
| `Ctrl+C`       | Quit                           |

### Add/Edit Server Dialog

| Shortcut            | Action                |
| ------------------- | --------------------- |
| `Tab`               | Next field            |
| `Shift+Tab`         | Previous field        |
| `←` / `→` on Group  | Cycle existing groups |
| `←` / `→` on Auth   | Cycle auth type       |
| `Space` on Favorite | Toggle favorite       |
| `Enter`             | Save                  |
| `Esc`               | Cancel                |

### Group Manager

| Shortcut       | Action                   |
| -------------- | ------------------------ |
| `↑` / `↓`      | Select group             |
| `n`            | New group                |
| `e`            | Edit selected group      |
| `d` / `Delete` | Delete selected group    |
| `Enter`        | Save new or edited group |
| `Esc`          | Close                    |

When a group is renamed, servers assigned to that group are updated automatically.

When a group is deleted, servers assigned to it are unassigned.

### Search Dialog

| Shortcut    | Action                 |
| ----------- | ---------------------- |
| Type text   | Update search query    |
| `Backspace` | Remove character       |
| `Enter`     | Apply search           |
| `Esc`       | Clear search and close |

### Notes and Tags Editors

| Shortcut      | Action           |
| ------------- | ---------------- |
| Type text     | Edit content     |
| `Backspace`   | Remove character |
| `Shift+Enter` | Insert newline   |
| `Enter`       | Save             |
| `Esc`         | Cancel           |

### Copy Dialog

| Shortcut  | Action              |
| --------- | ------------------- |
| `↑` / `↓` | Select value        |
| `Enter`   | Copy selected value |
| `Esc`     | Cancel              |

## SSH Behavior

For SSH key based entries, `russhx` constructs a command like:

```bash
ssh -i ~/.ssh/prod.pem ubuntu@103.21.244.10 -p 22
```

Before launching SSH, `russhx` restores the terminal, leaves the alternate screen, disables raw mode, and then starts `ssh` in the same terminal session.

When the SSH session ends, `russhx` does not reopen automatically.

Password authentication is not implemented yet. For production use, password support should avoid plain SQLite password storage and use either the normal OpenSSH password prompt or encrypted local secret storage.

## Installation

Current release: `v0.1.0`

Prebuilt binaries are published for Linux, macOS, and Windows. Users do not need Rust, Cargo, or a system SQLite installation.

You only need OpenSSH installed and available as `ssh` in your `PATH`.

### Quick Install

Linux and macOS users can install the latest release with:

```bash
curl -fsSL https://raw.githubusercontent.com/abhishek-Rj/russhx/main/install.sh | sh
```

To install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/abhishek-Rj/russhx/main/install.sh | sh -s -- v0.1.0
```

The installer downloads the matching GitHub Release archive, extracts it, and installs `russhx` into:

```text
~/.local/bin
```

If `~/.local/bin` is not in your `PATH`, the installer prints the shell command needed to add it.

### Manual Download

Download the archive for your operating system from the [GitHub Releases](https://github.com/abhishek-Rj/russhx/releases) page.

For `v0.1.0`, the release assets are:

| Operating System | CPU Architecture | Asset |
| --- | --- | --- |
| Linux | x86_64 | `russhx-v0.1.0-linux-x86_64.tar.gz` |
| macOS | Intel x86_64 | `russhx-v0.1.0-macos-x86_64.tar.gz` |
| macOS | Apple Silicon aarch64 | `russhx-v0.1.0-macos-aarch64.tar.gz` |
| Windows | x86_64 | `russhx-v0.1.0-windows-x86_64.zip` |

Linux and macOS:

```bash
tar -xzf russhx-v0.1.0-linux-x86_64.tar.gz
cd russhx-v0.1.0-linux-x86_64
mkdir -p ~/.local/bin
cp russhx ~/.local/bin/
```

Use the matching macOS archive name on macOS.

Windows:

1. Download `russhx-v0.1.0-windows-x86_64.zip`.
2. Extract the archive.
3. Move `russhx.exe` to a directory in your `PATH`.

### PATH

If you installed to `~/.local/bin` and your shell cannot find `russhx`, add it to your `PATH`.

For `bash`:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

For `zsh`:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

Then run:

```bash
russhx
```

### Verify Downloads

Each release includes a `SHA256SUMS` file.

Download the checksum file and verify your archive:

```bash
sha256sum -c SHA256SUMS
```

On macOS, if `sha256sum` is unavailable, use:

```bash
shasum -a 256 russhx-v0.1.0-macos-aarch64.tar.gz
```

Then compare the output with the matching line in `SHA256SUMS`.

### Build From Source

Building from source requires Rust:

```bash
cargo build --release
```

The binary will be available at:

```text
target/release/russhx
```

## Releases

Maintainers publish a new release by creating and pushing a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions automatically builds release binaries, packages them, generates `SHA256SUMS`, and attaches all assets to the GitHub Release.

The release workflow currently publishes:

- `russhx-v0.1.0-linux-x86_64.tar.gz`
- `russhx-v0.1.0-macos-x86_64.tar.gz`
- `russhx-v0.1.0-macos-aarch64.tar.gz`
- `russhx-v0.1.0-windows-x86_64.zip`
- `SHA256SUMS`

## License

This project is licensed under the MIT license. See [LICENSE](./LICENSE).
