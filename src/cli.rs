use clap::{Arg, ArgMatches, Command};

use crate::tmux::QueryScope;

#[derive(Debug)]
pub enum Subcommand<'a> {
    Create(CreateOpts<'a>),
    Export(ExportOpts<'a>),
    DumpCommand(DumpCommandOps<'a>),
    DumpConfig(DumpConfigOps<'a>),
}

impl Subcommand<'_> {
    pub fn from_matches(matches: &ArgMatches) -> Option<Subcommand<'_>> {
        match matches.subcommand() {
            None => None,
            Some(("create", sub_matches)) => {
                Some(Subcommand::Create(CreateOpts::from_matches(sub_matches)))
            }
            Some(("dump-command", sub_matches)) => Some(Subcommand::DumpCommand(
                DumpCommandOps::from_matches(sub_matches),
            )),
            Some(("dump-config", sub_matches)) => Some(Subcommand::DumpConfig(
                DumpConfigOps::from_matches(sub_matches),
            )),
            Some(("export", sub_matches)) => {
                Some(Subcommand::Export(ExportOpts::from_matches(sub_matches)))
            }
            _ => unreachable!("undefined subcommand"),
        }
    }
}

#[derive(Debug)]
pub struct CreateOpts<'a> {
    pub config_path: Option<&'a str>,
    pub session_select_mode: SessionSelectModeOption,
    pub ignore_existing_sessions: bool,
    pub tmux_args: Vec<&'a str>,
}

impl CreateOpts<'_> {
    fn from_matches(matches: &ArgMatches) -> CreateOpts<'_> {
        CreateOpts {
            config_path: matches.get_one::<String>("config").map(|s| s.as_str()),
            session_select_mode: SessionSelectModeOption::from_arg(
                matches
                    .get_one::<String>("session-select-mode")
                    .map(|s| s.as_str()),
            ),
            ignore_existing_sessions: matches.contains_id("ignore-existing-sessions"),
            tmux_args: matches
                .get_many::<String>("tmux args")
                .into_iter()
                .flatten()
                .map(|s| s.as_str())
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct ExportOpts<'a> {
    pub scope: QueryScope,
    pub format: ConfigFormat,
    pub tmux_args: Vec<&'a str>,
}

impl ExportOpts<'_> {
    fn from_matches(matches: &ArgMatches) -> ExportOpts<'_> {
        ExportOpts {
            scope: QueryScope::from_arg(matches.get_one::<String>("scope").map(|s| s.as_str())),
            format: ConfigFormat::from_arg(matches.get_one::<String>("format").map(|s| s.as_str())),
            tmux_args: matches
                .get_many::<String>("tmux args")
                .into_iter()
                .flatten()
                .map(|s| s.as_str())
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct DumpCommandOps<'a> {
    pub config_path: Option<&'a str>,
    pub session_select_mode: SessionSelectModeOption,
    pub ignore_existing_sessions: bool,
    pub tmux_args: Vec<&'a str>,
}

impl DumpCommandOps<'_> {
    fn from_matches(matches: &ArgMatches) -> DumpCommandOps<'_> {
        DumpCommandOps {
            config_path: matches.get_one::<String>("config").map(|s| s.as_str()),
            session_select_mode: SessionSelectModeOption::from_arg(
                matches
                    .get_one::<String>("session-select-mode")
                    .map(|s| s.as_str()),
            ),
            ignore_existing_sessions: matches.contains_id("ignore-existing-sessions"),
            tmux_args: matches
                .get_many::<String>("tmux args")
                .into_iter()
                .flatten()
                .map(|s| s.as_str())
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct DumpConfigOps<'a> {
    pub config_path: Option<&'a str>,
    pub format: ConfigFormat,
}

impl DumpConfigOps<'_> {
    fn from_matches(matches: &ArgMatches) -> DumpConfigOps<'_> {
        DumpConfigOps {
            config_path: matches.get_one::<String>("config").map(|s| s.as_str()),
            format: ConfigFormat::from_arg(matches.get_one::<String>("format").map(|s| s.as_str())),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ConfigFormat {
    Yaml,
    Toml,
}

impl ConfigFormat {
    fn from_arg(arg: Option<&str>) -> ConfigFormat {
        match arg {
            Some("yaml") | None => ConfigFormat::Yaml,
            Some("toml") => ConfigFormat::Toml,
            _ => unreachable!("undefined ConfigFormat"),
        }
    }
}

impl QueryScope {
    fn from_arg(arg: Option<&str>) -> QueryScope {
        match arg {
            Some("all") => QueryScope::AllSessions,
            Some("session") => QueryScope::CurrentSession,
            Some("window") => QueryScope::CurrentWindow,
            _ => unreachable!("undefined ExportScope"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SessionSelectModeOption {
    #[default]
    Auto,
    Attach,
    Switch,
    Detached,
}

impl SessionSelectModeOption {
    fn from_arg(arg: Option<&str>) -> SessionSelectModeOption {
        match arg {
            Some("auto") | None => SessionSelectModeOption::Auto,
            Some("attach") => SessionSelectModeOption::Attach,
            Some("switch") => SessionSelectModeOption::Switch,
            Some("detached") => SessionSelectModeOption::Detached,
            _ => unreachable!("undefined AttachOption"),
        }
    }
}

pub fn app() -> Command<'static> {
    let config_arg = Arg::new("config")
        .help(
            "Config file path. If not given the config file is searched for at:\n\
              - ./tmux-layout.{yaml,yml,toml}\n\
              - ~/tmux-layout.{yaml,yml,toml}\n",
        )
        .required(false)
        .short('c')
        .long("config")
        .takes_value(true)
        .value_name("FILE")
        .required(false);

    let format_arg = Arg::new("format")
        .help("Export config format")
        .required(false)
        .short('f')
        .long("format")
        .takes_value(true)
        .value_name("FORMAT")
        .value_parser(["yaml", "toml"])
        .default_value("yaml");

    let session_select_mode_arg = Arg::new("session-select-mode")
        .help(
            "Session select mode:\n\
                - switch: switch existing client to selected (or last created) session\n\
                - attach: attach to selected (or last created) session\n\
                - detached: don't attach/switch to any session\n\
                - auto: switch when there is a tmux client, \
                  attach when running from a TTY, \
                  detached otherwise\n",
        )
        .short('m')
        .long("session-select-mode")
        .takes_value(true)
        .value_name("MODE")
        .value_parser(["auto", "attach", "switch", "detached"])
        .default_value("auto")
        .required(false);

    let ignore_existing_sessions_arg = Arg::new("ignore-existing-sessions")
        .help("Don't create already existing tmux sessions")
        .short('i')
        .long("ignore-existing-sessions")
        .required(false);

    let tmux_args = Arg::new("tmux args")
        .required(false)
        .last(true)
        .multiple_values(true);

    Command::new("tmux-layout")
        .version("0.1.0")
        .author("Daniel Strittmatter <github@smattr.de>")
        .about("Starts tmux sessions in pre-defined layouts")
        .subcommand(
            Command::new("create")
                .about("Create tmux layout from config file")
                .arg(&config_arg)
                .arg(&session_select_mode_arg)
                .arg(&ignore_existing_sessions_arg)
                .arg(&tmux_args),
        )
        .subcommand(
            Command::new("dump-command")
                .about("Dump tmux command to stdout")
                .arg(&config_arg)
                .arg(&session_select_mode_arg)
                .arg(&ignore_existing_sessions_arg)
                .arg(&tmux_args),
        )
        .subcommand(
            Command::new("dump-config")
                .arg(&config_arg)
                .about("Dump config to stdout")
                .arg(&format_arg),
        )
        .subcommand(
            Command::new("export")
                .about("Exports running tmux sessions into tmux-layout config file format")
                .arg(
                    Arg::new("scope")
                        .help("Export scope")
                        .required(false)
                        .short('s')
                        .long("scope")
                        .takes_value(true)
                        .value_name("SCOPE")
                        .value_parser(["all", "session", "window"])
                        .default_value("all"),
                )
                .arg(&format_arg)
                .arg(&tmux_args),
        )
}

#[test]
fn verify_cli() {
    app().debug_assert();
}
