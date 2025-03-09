use std::{collections::HashMap, path::Path, process::Stdio};
use thiserror::Error;

use crate::{
    config::{self},
    cwd::Cwd,
    tmux::{self, TmuxCommandBuilder},
};

pub use parser::Error as ParseError;

use super::command::QueryScope;

pub fn query_tmux_state(
    command_builder: TmuxCommandBuilder,
    scope: QueryScope,
) -> Result<TmuxState, Error> {
    let mut command = command_builder
        .query_panes(parser::TMUX_FORMAT, scope)
        .into_command();

    let command_out = command.stderr(Stdio::inherit()).output()?;
    if !command_out.status.success() {
        return Err(Error::CommandExitCode(
            command_out.status.code().unwrap_or(1),
        ));
    }

    let state_desc = command_out.stdout;
    let state_desc = std::str::from_utf8(&state_desc)
        .map_err(|_| Error::ParseError("command output not UTF-8".into()))?;

    Ok(parser::parse_tmux_state(state_desc)?)
}
#[derive(Debug, Clone)]
pub struct TmuxState {
    pub sessions: HashMap<SessionId, Session>,
}

impl From<TmuxState> for Vec<config::Session> {
    fn from(state: TmuxState) -> Self {
        let mut sessions = state.sessions.into_values().collect::<Vec<_>>();
        sessions.sort_by_key(|s| s.id);
        sessions.into_iter().map(Into::into).collect()
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub cwd: String,
    pub windows: HashMap<WindowId, Window>,
}

impl From<Session> for config::Session {
    fn from(session: Session) -> Self {
        let session_cwd = session.cwd.into();

        let mut windows = session.windows.into_values().collect::<Vec<_>>();
        windows.sort_by_key(|w| w.index);

        let windows = windows
            .into_iter()
            .map(|w| w.into_config_window(&session_cwd))
            .collect();

        config::Session {
            name: session.name,
            cwd: session_cwd,
            windows,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    pub index: WindowIndex,
    pub name: String,
    pub layout: tmux::Layout,
    pub active: bool,
    pub panes: HashMap<PaneId, Pane>,
}

impl Window {
    fn into_config_window(self, session_cwd: &Cwd) -> config::Window {
        let session_cwd_path = session_cwd.to_path();

        let mut panes = self.panes.into_values().collect::<Vec<_>>();
        panes.sort_by_key(|p| p.index);

        let mut root_split = config::Split::from(self.layout).into_root();
        root_split
            .pane_iter_mut()
            .zip(panes)
            .for_each(|(config_pane, pane)| {
                config_pane.active = pane.active;
                config_pane.cwd = session_cwd_path
                    .and_then(|root| Path::new(&pane.cwd).strip_prefix(root).ok())
                    .map(|p| p.to_owned().into())
                    .unwrap_or_else(|| pane.cwd.into());
            });

        config::Window {
            name: Some(self.name),
            cwd: Cwd::new(None),
            active: self.active,
            root_split,
        }
    }
}

impl From<Window> for config::Window {
    fn from(window: Window) -> Self {
        window.into_config_window(&Cwd::default())
    }
}

#[derive(Debug, Clone)]
pub struct Pane {
    pub id: PaneId,
    pub index: PaneIndex,
    pub active: bool,
    pub cwd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowIndex(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaneId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaneIndex(u32);

#[derive(Debug, Error)]
pub enum Error {
    #[error("error while invoking tmux command: {0}")]
    CommandIo(#[from] std::io::Error),
    #[error("non-successful tmux exit code: {0}")]
    CommandExitCode(i32),
    #[error("parse error: {0}")]
    ParseError(#[from] ParseError),
}

mod parser {
    use crate::tmux::layout;
    use nom::Parser;
    use shellwords::MismatchedQuotes;
    use std::borrow::Cow;
    use std::collections::{hash_map::Entry, HashMap};
    use std::fmt;
    use std::num::ParseIntError;

    use super::*;

    type Result<A> = std::result::Result<A, Error>;

    pub(super) fn parse_tmux_state(input: &str) -> Result<TmuxState> {
        let infos = parse_pane_infos(input)?;
        let mut sessions = HashMap::new();

        for info in infos {
            let session = match sessions.entry(info.session_id) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => v.insert(Session {
                    id: info.session_id,
                    name: info.session_name,
                    cwd: info.session_cwd,
                    windows: Default::default(),
                }),
            };

            let window = match session.windows.entry(info.window_id) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => v.insert(Window {
                    id: info.window_id,
                    index: info.window_index,
                    name: info.window_name,
                    layout: info.window_layout,
                    active: info.window_active,
                    panes: Default::default(),
                }),
            };

            window.panes.insert(
                info.pane_id,
                Pane {
                    id: info.pane_id,
                    index: info.pane_index,
                    active: info.pane_active,
                    cwd: info.pane_cwd,
                },
            );
        }

        Ok(TmuxState { sessions })
    }

    #[derive(Debug, Clone)]
    struct PaneInfo {
        session_id: SessionId,
        window_id: WindowId,
        pane_id: PaneId,
        session_name: String,
        session_cwd: String,
        window_index: WindowIndex,
        window_name: String,
        window_active: bool,
        window_layout: tmux::Layout,
        pane_index: PaneIndex,
        pane_active: bool,
        pane_cwd: String,
    }

    fn parse_pane_infos(input: &str) -> Result<Vec<PaneInfo>> {
        input.lines().map(parse_line).collect()
    }

    pub(super) const TMUX_FORMAT: &str = "#{q:session_id} #{q:window_id} #{q:pane_id} \
        #{q:session_name} #{q:session_path} #{q:window_index} #{q:window_name} \
        #{q:window_active} #{q:window_layout} #{q:pane_index} #{q:pane_active} \
        #{q:pane_current_path}";

    fn parse_line(line: &str) -> Result<PaneInfo> {
        let mut words = shellwords::split(line)?.into_iter();
        let mut next_word = || words.next().ok_or_else(|| Error::from("missing word"));

        let session_id_desc = next_word()?;
        let session_id = all_consuming(session_id).parse(&session_id_desc)?.1;
        let window_id_desc = next_word()?;
        let window_id = all_consuming(window_id).parse(&window_id_desc)?.1;
        let pane_id_desc = next_word()?;
        let pane_id = all_consuming(pane_id).parse(&pane_id_desc)?.1;
        let session_name = next_word()?;
        let session_cwd = next_word()?;
        let window_index = WindowIndex(next_word()?.parse()?);
        let window_name = next_word()?;
        let window_active = next_word()?.parse::<u8>()? != 0;
        let window_layout_desc = next_word()?;
        let window_layout = tmux::Layout::parse(&window_layout_desc)?;
        let pane_index = PaneIndex(next_word()?.parse()?);
        let pane_active = next_word()?.parse::<u8>()? != 0;
        let pane_cwd = next_word().unwrap_or_default();

        Ok(PaneInfo {
            session_id,
            window_id,
            pane_id,
            session_name,
            session_cwd,
            window_index,
            window_name,
            window_active,
            window_layout,
            pane_index,
            pane_active,
            pane_cwd,
        })
    }

    use nom::{
        bytes::complete::tag,
        character::complete::u32,
        combinator::{all_consuming, map},
        sequence::preceded,
        IResult,
    };

    type I<'a> = &'a str;
    type NomResult<'a, A> = IResult<I<'a>, A>;

    fn session_id(i: I) -> NomResult<SessionId> {
        map(preceded(tag("$"), u32), SessionId).parse(i)
    }

    fn window_id(i: I) -> NomResult<WindowId> {
        map(preceded(tag("@"), u32), WindowId).parse(i)
    }

    fn pane_id(i: I) -> NomResult<PaneId> {
        map(preceded(tag("%"), u32), PaneId).parse(i)
    }

    #[derive(Debug)]
    pub struct Error {
        pub message: Cow<'static, str>,
    }

    impl From<String> for Error {
        fn from(message: String) -> Self {
            Error {
                message: Cow::Owned(message),
            }
        }
    }

    impl From<&'static str> for Error {
        fn from(message: &'static str) -> Self {
            Error {
                message: Cow::Borrowed(message),
            }
        }
    }

    impl From<MismatchedQuotes> for Error {
        fn from(_: MismatchedQuotes) -> Self {
            Error::from("missing quotes")
        }
    }

    impl<E: std::error::Error> From<nom::Err<E>> for Error {
        fn from(err: nom::Err<E>) -> Self {
            Error::from(format!("{}", err))
        }
    }

    impl From<ParseIntError> for Error {
        fn from(err: ParseIntError) -> Self {
            Error::from(format!("{}", err))
        }
    }

    impl From<layout::Error> for Error {
        fn from(err: layout::Error) -> Self {
            Error::from(format!("{}", err))
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for Error {}
}
