# lazyssh

`lazyssh` is a terminal user interface for managing SSH servers locally.

The goal is to make SSH access feel like using tools such as `lazygit`, `lazydocker`, or `k9s`: fast, keyboard-first, and built for people who live in the terminal.

Instead of remembering IP addresses, usernames, ports, SSH key paths, notes, and tags, you keep them in a local vault and launch SSH sessions from a TUI.

## Project Goals

- Store SSH server entries locally.
- Keep server metadata easy to browse and edit.
- Launch SSH using the same terminal session that opened `lazyssh`.
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

`lazyssh` stores data in a local SQLite database.

The database path is chosen according to the operating system:

- Linux/Unix: `$XDG_DATA_HOME/lazyssh/lazyssh.db` or `~/.local/share/lazyssh/lazyssh.db`
- macOS: `~/Library/Application Support/lazyssh/lazyssh.db`
- Windows: `%APPDATA%\lazyssh\lazyssh.db` or `%LOCALAPPDATA%\lazyssh\lazyssh.db`

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

For SSH key based entries, `lazyssh` constructs a command like:

```bash
ssh -i ~/.ssh/prod.pem ubuntu@103.21.244.10 -p 22
```

Before launching SSH, `lazyssh` restores the terminal, leaves the alternate screen, disables raw mode, and then starts `ssh` in the same terminal session.

When the SSH session ends, `lazyssh` does not reopen automatically.

Password authentication is not implemented yet. For production use, password support should avoid plain SQLite password storage and use either the normal OpenSSH password prompt or encrypted local secret storage.

## Installation

Installation instructions will be added later.

## License

This project is licensed under the MIT license. See [LICENSE](./LICENSE).
