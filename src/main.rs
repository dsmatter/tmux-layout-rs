use colored::*;
use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::io::{IsTerminal, Read};
use std::path::Path;
use std::process::Command;
use tmux_layout::cli::{
    self, ConfigFormat, CreateOpts, DumpCommandOps, DumpConfigOps, ExportOpts,
    SessionSelectModeOption,
};
use tmux_layout::config::loader::find_default_config_file;
use tmux_layout::config::{self, Config, PartialConfig, Session};
use tmux_layout::cwd::Cwd;
use tmux_layout::tmux::import::TmuxState;
use tmux_layout::tmux::{import, QueryScope};
use tmux_layout::tmux::{SessionSelectMode, TmuxCommandBuilder};
use tmux_layout::{exit_with_error, show_info, show_warning};

fn main() {
    let matches = cli::app().get_matches();
    let Some(command) = cli::Command::from_matches(&matches) else {
        eprintln!("{}\n", matches.usage());
        exit_with_error("no subcommand given");
    };
    match command {
        cli::Command::Create(opts) => run_create(opts),
        cli::Command::Export(opts) => run_export(opts),
        cli::Command::DumpCommand(opts) => run_dump_command(opts),
        cli::Command::DumpConfig(opts) => run_dump_config(opts),
    }
}

fn run_create(opts: CreateOpts) {
    let env = EnvOpts::from_env();

    let session_select_mode = get_session_select_mode(opts.session_select_mode, &env, true);
    let mut config = load_config(opts.config_path);

    if opts.ignore_existing_sessions {
        remove_existing_sessions(&mut config.sessions, &env.tmux_path);
    }

    if config.sessions.is_empty() && config.windows.is_empty() {
        show_warning("no sessions or windows to create");
        std::process::exit(0)
    }

    let command = TmuxCommandBuilder::new(&env.tmux_path, opts.tmux_args)
        .new_windows(&config.windows, &Cwd::default())
        .new_sessions(&config.sessions)
        .select_session(config.selected_session.as_deref(), session_select_mode)
        .into_command();

    execute_command(command, &env.tmux_path);
}

fn run_export(opts: ExportOpts) {
    let EnvOpts { tmux_path, .. } = EnvOpts::from_env();
    let command_builder = TmuxCommandBuilder::new(tmux_path, opts.tmux_args);
    let tmux_state = import::query_tmux_state(command_builder, opts.scope)
        .unwrap_or_else(|err| exit_with_error(&format!("failed to query tmux state: {}", err)));

    let config = match opts.scope {
        QueryScope::CurrentWindow => {
            let window = extract_active_window(tmux_state)
                .unwrap_or_else(|| exit_with_error("failed to extract active window"));

            Config {
                windows: vec![window.into()],
                ..Default::default()
            }
        }
        _ => Config {
            sessions: tmux_state.into(),
            ..Default::default()
        },
    };

    dump_config(&config, opts.format);
}

fn run_dump_command(opts: DumpCommandOps) {
    let env = EnvOpts::from_env();
    let session_select_mode = get_session_select_mode(opts.session_select_mode, &env, false);
    let mut config = load_config(opts.config_path);

    if opts.ignore_existing_sessions {
        remove_existing_sessions(&mut config.sessions, &env.tmux_path);
    }

    if config.sessions.is_empty() && config.windows.is_empty() {
        show_warning("no sessions or windows to create");
    }

    let command = TmuxCommandBuilder::new(&env.tmux_path, opts.tmux_args)
        .new_windows(&config.windows, &Cwd::default())
        .new_sessions(&config.sessions)
        .select_session(config.selected_session.as_deref(), session_select_mode)
        .into_command();

    dump_command(command)
}

fn run_dump_config(opts: DumpConfigOps) {
    let config = load_config(opts.config_path);
    dump_config(&config, opts.format)
}

fn execute_command(mut command: Command, tmux_path: &str) -> ! {
    let exit_status = command
        .spawn()
        .unwrap_or_else(|err| {
            exit_with_error(&format!(
                "failed to start tmux (at '{}'): {}",
                tmux_path.yellow(),
                err
            ))
        })
        .wait()
        .unwrap_or_else(|err| {
            exit_with_error(&format!("failed to wait for tmux process: {}", err))
        });

    std::process::exit(exit_status.code().unwrap_or(1))
}

fn load_config(config_path: Option<&str>) -> Config {
    match config_path {
        Some("-") => load_stdin_config(),
        Some(path) => load_file_config(Path::new(path)),
        None => {
            let Some(default_path) = find_default_config_file() else {
                exit_with_error("no config file found")
            };
            show_info(&format!(
                "using config file at '{}'",
                default_path.display()
            ));
            load_file_config(&default_path)
        }
    }
}

fn load_file_config(config_path: &Path) -> Config {
    config::loader::load_config_at(Path::new(config_path))
        .unwrap_or_else(|err| exit_with_error(&format!("{}", err)))
}

fn load_stdin_config() -> Config {
    let mut config_bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut config_bytes)
        .unwrap_or_else(|err| exit_with_error(&format!("Reading from STDIN failed: {}", err)));

    // Guess format
    let partial_config: PartialConfig = if config_bytes.starts_with(b"[[") {
        let config_str = std::str::from_utf8(&config_bytes)
            .unwrap_or_else(|err| exit_with_parse_error(&err, "(STDIN)"));

        toml::from_str(config_str).unwrap_or_else(|err| exit_with_parse_error(&err, "(STDIN)"))
    } else {
        let config_str = std::str::from_utf8(&config_bytes)
            .unwrap_or_else(|err| exit_with_parse_error(&err, "(STDIN)"));

        serde_yaml::from_slice(&config_bytes)
            .or_else(|_| toml::from_str(config_str))
            .unwrap_or_else(|err| exit_with_parse_error(&err, "(STDIN)"))
    };

    partial_config
        .into_config()
        .unwrap_or_else(|_| exit_with_error("config given to STDIN can't have file includes"))
}

fn dump_command(command: Command) {
    println!("{:?}", command);
}

fn dump_config(config: &Config, format: ConfigFormat) {
    match format {
        ConfigFormat::Yaml => println!("{}", serde_yaml::to_string(config).unwrap()),
        ConfigFormat::Toml => {
            let toml_str = toml::to_string(config).unwrap_or_else(|err| {
                show_warning("emitting TOML is unstable. Try using the YAML format instead.");
                exit_with_error(&format!("failed to emit TOML: {}", err));
            });
            println!("{}", toml_str);
        }
    }
}

fn extract_active_window(tmux_state: TmuxState) -> Option<import::Window> {
    tmux_state
        .sessions
        .into_values()
        .next()?
        .windows
        .into_values()
        .find(|w| w.active)
}

fn get_session_select_mode(
    opt: SessionSelectModeOption,
    env: &EnvOpts,
    allow_overwrite: bool,
) -> SessionSelectMode {
    let is_terminal = std::io::stdin().is_terminal();

    match opt {
        SessionSelectModeOption::Switch => SessionSelectMode::Switch,
        SessionSelectModeOption::Detached => SessionSelectMode::Detached,
        SessionSelectModeOption::Attach => {
            if is_terminal || !allow_overwrite {
                SessionSelectMode::Attach
            } else {
                show_warning(
                    "Ignoring 'attach' mode because we are not running from a TTY. \
                    Note that 'attach' mode is not available if the config is provided via \
                    STDIN.",
                );
                SessionSelectMode::Detached
            }
        }
        SessionSelectModeOption::Auto => {
            if has_tmux_clients(&env.tmux_path) {
                SessionSelectMode::Switch
            } else if is_terminal {
                SessionSelectMode::Attach
            } else {
                SessionSelectMode::Detached
            }
        }
    }
}

fn has_tmux_clients(tmux_path: &str) -> bool {
    match Command::new(tmux_path).arg("list-clients").output() {
        Err(_) => {
            show_warning("Error while listing tmux clients");
            false
        }
        Ok(output) => !output.stdout.is_empty(),
    }
}

fn remove_existing_sessions(sessions: &mut Vec<Session>, tmux_path: &str) {
    let builder = TmuxCommandBuilder::new(tmux_path, std::iter::empty::<String>());
    let tmux_state =
        import::query_tmux_state(builder, QueryScope::AllSessions).unwrap_or_else(|err| {
            exit_with_error(&format!(
                "failed to query tmux state (needed for --ignore-existing-sessions): {}",
                err
            ))
        });

    let existing_sessions = tmux_state
        .sessions
        .into_values()
        .map(|s| s.name)
        .collect::<HashSet<_>>();

    sessions.retain(|s| !existing_sessions.contains(&s.name));
}

fn exit_with_parse_error(err: &dyn Error, config_path: &str) -> ! {
    exit_with_error(&format!(
        "Parsing config file '{}' failed: {}",
        config_path.yellow(),
        err
    ))
}

#[derive(Debug)]
struct EnvOpts {
    tmux_path: String,
}

impl EnvOpts {
    fn from_env() -> Self {
        // Allow overriding path of tmux executable
        let tmux_path = env::var("TMUX_PATH");
        let tmux_path = tmux_path.unwrap_or_else(|_| "tmux".to_string());

        Self { tmux_path }
    }
}
