# tmux-layout

A tool for managing tmux sessions with predefined layouts. Define your tmux workspace layouts in YAML/TOML configuration files and easily recreate them.

## Features

- Create complex tmux layouts using a simple YAML/TOML configuration format
- Export existing tmux sessions to configuration files
- Support for nested window splits (horizontal and vertical)
- Configurable working directories per session/window/pane
- Execute shell commands and send keys to panes
- Include other configuration files to build modular layouts

## Installation

```bash
cargo install tmux-layout
```

## Usage

### Create a Layout

1. Create a configuration file named `.tmux-layout.yaml` in your home directory or current directory:

```yaml
sessions:
  - name: dev
    windows:
      - name: editor
        cwd: ~/projects/myapp
        left:
          width: 60%
          shell_command: nvim
          active: true
        right:
          top:
            height: 70%
            shell_command: cargo watch -x test
          bottom:
            send_keys:
              - git status
              - Enter

  - name: server
    cwd: ~/projects/server
    windows:
      - name: logs
        cwd: logs
        shell_command: tail -f app.log
```

2. Create the tmux layout:

```bash
tmux-layout create
```

### Export Existing Sessions

Export your current tmux layout to a configuration file:

```bash
# Export all sessions
tmux-layout export > my-layout.yaml

# Export current session only
tmux-layout export --scope session > session.yaml

# Export current window only
tmux-layout export --scope window > window.yaml
```

### Command Line Options

```
SUBCOMMANDS:
    create         Create tmux layout from config file
    dump-command   Dump tmux command to stdout
    dump-config    Dump config to stdout
    export         Exports running tmux sessions into config file format
    help           Print this message or the help of the given subcommand(s)

COMMON OPTIONS (run subcommand with --help to see all options):
    -c, --config <FILE>                Config file path
    -f, --format <FORMAT>              Export config format [yaml, toml]
    -m, --session-select-mode <MODE>   Session select mode [auto, attach, switch, detached]
    -i, --ignore-existing-sessions     Don't create already existing tmux sessions
```

### Configuration Format

The configuration file can be in YAML or TOML format. The basic structure is:

```yaml
# Optional list of other config files to include
includes:
  - ~/other-layout.yaml

# Optional session to select after creation
selected_session: dev

# List of sessions to create
sessions:
  - name: session-name
    cwd: ~/base/path # Base working directory for all windows
    windows:
      - name: window-name
        cwd: sub/path # Relative to session cwd
        active: true # Make this the active window

        # Window layout splits
        left:
          width: 30% # Width of left pane
          cwd: ~/path # Working directory for this pane
          shell_command: nvim # Command to run in pane
          send_keys: # Keys to send to pane
            - ":Ex"
            - Enter

        right:
          top:
            height: 70% # Height of top pane
            actve: true # Make this the active pane
          bottom:
            shell_command: git status

# List of standalone windows (created in current session)
windows:
  - name: standalone
    cwd: ~/somewhere
```

## License

MIT
