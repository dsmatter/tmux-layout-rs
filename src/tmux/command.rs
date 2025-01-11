use crate::config::{Pane, RootSplit, Session, Split, Window};
use crate::cwd::Cwd;
use crate::show_warning;
use std::fmt;
use std::marker::PhantomData;
use std::{ffi::OsStr, process::Command};

#[derive(Debug, Clone, Copy)]
pub enum QueryScope {
    AllSessions,
    CurrentSession,
    CurrentWindow,
}

#[derive(Debug, Clone, Copy)]
pub enum SessionSelectMode {
    Attach,
    Switch,
    Detached,
}

#[derive(Debug)]
pub struct TmuxCommandBuilder {
    command: Command,
    first_command: bool,
    current_session_name: Option<String>,
    window_count: u32,
    active_window_index: Option<u32>,
}

impl TmuxCommandBuilder {
    pub fn new(
        tmux_path: impl AsRef<OsStr>,
        tmux_args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Self {
        let mut command = Command::new(tmux_path);
        command.args(tmux_args);

        Self {
            command,
            first_command: true,
            current_session_name: None,
            window_count: 0,
            active_window_index: None,
        }
    }

    pub fn into_command(self) -> Command {
        self.command
    }

    pub fn query_panes(mut self, format: impl AsRef<OsStr>, scope: QueryScope) -> Self {
        self.push_new_command("list-panes").push("-F").push(format);
        self.push_query_scope_arg(scope);
        self
    }

    pub fn query_clients(mut self) -> Self {
        self.push_new_command("list-clients");
        self
    }

    pub fn select_session(mut self, name: Option<&str>, mode: SessionSelectMode) -> Self {
        let select = match mode {
            SessionSelectMode::Detached => return self,
            SessionSelectMode::Switch => Self::switch_client,
            SessionSelectMode::Attach => Self::attach_session,
        };
        let target = match name {
            None => Target::default(),
            Some(name) => Target::session(name),
        };
        select(&mut self, target);
        self
    }

    pub fn new_sessions<'a>(self, sessions: impl IntoIterator<Item = &'a Session>) -> Self {
        sessions
            .into_iter()
            .fold(self, |b, session| b.new_session(session))
    }

    pub fn new_session(mut self, session: &Session) -> Self {
        if session.windows.is_empty() {
            return self;
        }

        self.current_session_name = Some(session.name.clone());

        self.push_new_command("new-session")
            .push_flag_arg("-s", Some(&session.name))
            .push_cwd_arg(&session.cwd)
            .push("-d");

        self.create_initial_window(&session.windows[0], &session.cwd)
            .new_windows(&session.windows[1..], &session.cwd)
    }

    pub fn new_windows<'a>(
        self,
        windows: impl IntoIterator<Item = &'a Window>,
        parent_cwd: &Cwd,
    ) -> Self {
        let mut builder = windows
            .into_iter()
            .fold(self, |b, win| b.new_window(win, parent_cwd, None));

        builder.select_active_window();
        builder
    }

    pub fn new_window(
        mut self,
        window: &Window,
        parent_cwd: &Cwd,
        before_target: Option<&str>,
    ) -> Self {
        if window.active {
            if self.active_window_index.is_none() {
                self.active_window_index = Some(self.window_count);
            } else {
                let session_name = self.current_session_name.as_deref().unwrap_or("(current)");
                show_warning(&format!(
                    "Multiple active windows in session '{}'",
                    session_name
                ));
            }
        }
        self.window_count += 1;

        let window_cwd = parent_cwd.joined(&window.cwd);
        self.push_new_command("new-window")
            .push_flag_arg("-n", window.name.as_deref())
            .push_cwd_arg(&window_cwd);

        if let Some(before_target) = before_target {
            let target = self.session_target().window(before_target);
            self.push("-b").push_target_arg(target);
        } else {
            self.push_target_arg(self.session_target());
        }

        self.apply_root_split(&window.root_split, &window_cwd);
        self.select_active_pane(window);
        self
    }

    fn create_initial_window(mut self, window: &Window, parent_cwd: &Cwd) -> Self {
        self.active_window_index = None;
        self.window_count = 0;

        // Create our first window at index 0 (pushing the intial window to index 1).
        self = self.new_window(window, parent_cwd, Some("0"));

        // Kill the initial window.
        let target = self.session_target().window("1");
        self.push_new_command("kill-window").push_target_arg(target);

        self
    }

    fn select_active_pane(&mut self, window: &Window) {
        let active_panes = window
            .root_split
            .pane_iter()
            .enumerate()
            .filter(|(_, pane)| pane.active)
            .collect::<Vec<_>>();

        if active_panes.len() > 1 {
            let session_name = self.current_session_name.as_deref().unwrap_or("(current)");
            show_warning(&format!(
                "Multiple active panes in window '{}' of session '{}'",
                window.name.as_deref().unwrap_or("(unnamed)"),
                session_name
            ));
        }

        if let Some(active_pane) = active_panes.first() {
            let pane_index = active_pane.0;
            let target = self
                .session_target()
                .current_window()
                .pane(pane_index.to_string());

            self.push_new_command("select-pane").push_target_arg(target);
        }
    }

    fn apply_root_split(&mut self, split: &RootSplit, parent_cwd: &Cwd) -> &mut Self {
        // We now have a fresh window with a single, unconfigured pane.
        // To apply our options to the pane, we created a horizontal split
        // with our designated first pane on the right. Afterwards we kill
        // the initial placeholder pane.

        let first_pane = root_pane(split);
        let first_pane_cwd = parent_cwd.joined(&first_pane.cwd);
        self.split_pane(
            Axis::Horizontal,
            SplitFlow::Regular,
            &first_pane_cwd,
            first_pane.shell_command.as_deref(),
            None,
        );

        let first_pane_target = self.session_target().current_window().pane("0");
        self.push_new_command("kill-pane")
            .push_target_arg(first_pane_target);

        self.apply_split(split, parent_cwd)
    }

    fn apply_split(&mut self, split: &Split, parent_cwd: &Cwd) -> &mut Self {
        let flow = SplitFlow::from(split);

        match split {
            Split::Pane(pane) => {
                if let Some(keys) = &pane.send_keys {
                    self.send_keys(keys);
                }
                self
            }
            Split::H { left, right } => {
                let (parent, child) = match flow {
                    SplitFlow::Regular => (left, right),
                    SplitFlow::Inverted => (right, left),
                };
                let child_pane = root_pane(&child.split);
                let child_pane_cwd = parent_cwd.joined(&child_pane.cwd);

                self.split_pane(
                    Axis::Horizontal,
                    flow,
                    &child_pane_cwd,
                    child_pane.shell_command.as_deref(),
                    child.width.as_deref(),
                )
                .apply_split(&child.split, parent_cwd)
                .select_pane_at(flow.direction(Axis::Horizontal).inverted())
                .apply_split(&parent.split, parent_cwd)
            }
            Split::V { top, bottom } => {
                let (parent, child) = match flow {
                    SplitFlow::Regular => (top, bottom),
                    SplitFlow::Inverted => (bottom, top),
                };
                let child_pane = root_pane(&child.split);
                let child_pane_cwd = parent_cwd.joined(&child_pane.cwd);

                self.split_pane(
                    Axis::Vertical,
                    flow,
                    &child_pane_cwd,
                    child_pane.shell_command.as_deref(),
                    child.height.as_deref(),
                )
                .apply_split(&child.split, parent_cwd)
                .select_pane_at(flow.direction(Axis::Vertical).inverted())
                .apply_split(&parent.split, parent_cwd)
            }
        }
    }

    fn send_keys(&mut self, keys: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        let target = self.session_target();
        self.push_new_command("send-keys").push_target_arg(target);
        keys.into_iter().fold(self, |b, key| b.push_arg(Some(key)))
    }

    fn split_pane(
        &mut self,
        axis: Axis,
        flow: SplitFlow,
        cwd: &Cwd,
        shell_command: Option<&str>,
        size: Option<&str>,
    ) -> &mut Self {
        let target = self.session_target();
        self.push_new_command("split-window")
            .push_target_arg(target)
            .push_axis_arg(axis)
            .push_flow_arg(flow)
            .push_cwd_arg(cwd)
            .push_flag_arg("-l", size)
            .push_arg(shell_command)
    }

    fn select_pane_at(&mut self, direction: Direction) -> &mut Self {
        let target = self.session_target();
        self.push_new_command("select-pane")
            .push_target_arg(target)
            .push_direction_arg(direction)
    }

    fn select_window_at(&mut self, direction: Direction) -> &mut Self {
        let target = self.session_target();
        self.push_new_command("select-window")
            .push_target_arg(target)
            .push_next_prev_arg(direction)
    }

    fn select_window(&mut self, target: Target<Window>) -> &mut Self {
        self.push_new_command("select-window")
            .push_target_arg(target)
    }

    fn switch_client(&mut self, target: Target<Session>) -> &mut Self {
        self.push_new_command("switch-client")
            .push_target_arg(target)
    }

    fn attach_session(&mut self, target: Target<Session>) -> &mut Self {
        self.push_new_command("attach-session")
            .push_target_arg(target)
    }

    fn select_active_window(&mut self) -> &mut Self {
        if let Some(index) = self.active_window_index {
            if let Some(session_name) = self.current_session_name.as_deref() {
                let target = Target::session(session_name).window(index.to_string());
                self.select_window(target);
            } else {
                let steps = self.window_count - index - 1;
                for _ in 0..steps {
                    self.select_window_at(Direction::Left);
                }
            }
        }
        self
    }

    fn session_target(&self) -> Target<Session> {
        self.current_session_name
            .as_ref()
            .map(|name| Target::session(name.clone()))
            .unwrap_or_default()
    }

    // Primitives

    fn push_cwd_arg(&mut self, cwd: &Cwd) -> &mut Self {
        self.push_flag_arg("-c", cwd.to_path())
    }

    fn push_target_arg<Scope>(&mut self, target: Target<Scope>) -> &mut Self
    where
        Target<Scope>: fmt::Display,
    {
        self.push_flag_arg("-t", Some(target.to_string()))
    }

    fn push_axis_arg(&mut self, axis: Axis) -> &mut Self {
        match axis {
            Axis::Horizontal => self.push("-h"),
            Axis::Vertical => self.push("-v"),
        }
    }

    fn push_direction_arg(&mut self, direction: Direction) -> &mut Self {
        match direction {
            Direction::Left => self.push("-L"),
            Direction::Right => self.push("-R"),
            Direction::Up => self.push("-U"),
            Direction::Down => self.push("-D"),
        }
    }

    fn push_next_prev_arg(&mut self, direction: Direction) -> &mut Self {
        match direction {
            Direction::Left => self.push("-p"),
            Direction::Right => self.push("-n"),
            Direction::Up => self.push("-p"),
            Direction::Down => self.push("-n"),
        }
    }

    fn push_flow_arg(&mut self, flow: SplitFlow) -> &mut Self {
        match flow {
            SplitFlow::Regular => self,
            SplitFlow::Inverted => self.push_arg(Some("-b")),
        }
    }

    fn push_query_scope_arg(&mut self, scope: QueryScope) -> &mut Self {
        match scope {
            QueryScope::AllSessions => self.push("-a"),
            QueryScope::CurrentSession => self.push("-s"),
            QueryScope::CurrentWindow => self,
        }
    }

    fn push_flag_arg(
        &mut self,
        flag: impl AsRef<OsStr>,
        arg: Option<impl AsRef<OsStr>>,
    ) -> &mut Self {
        if let Some(arg) = arg {
            self.push(flag).push(arg);
        }
        self
    }

    fn push_arg(&mut self, arg: Option<impl AsRef<OsStr>>) -> &mut Self {
        if let Some(arg) = arg {
            self.push(arg);
        }
        self
    }

    fn push_new_command(&mut self, command: &str) -> &mut Self {
        if self.first_command {
            self.first_command = false;
        } else {
            self.push(";");
        }
        self.push(command)
    }

    fn push(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(arg);
        self
    }
}

/// When splitting the parent pane, the split direction depens on the
/// location of size information. If we are, for instance, supposed
/// to give the right pane a width of 25%, we create a new pane on the
/// right. This way, we can always supply the size information to the
/// `-l` option of the `split-window` command.
///
/// Splitting to the left (or top for vertical splits) is considered
/// an "inverted" flow.
#[derive(Debug, Clone, Copy)]
enum SplitFlow {
    /// Flowing to the right/bottom
    Regular,
    /// Flowing to the left/top
    Inverted,
}

impl SplitFlow {
    fn direction(self, axis: Axis) -> Direction {
        match (self, axis) {
            (SplitFlow::Regular, Axis::Horizontal) => Direction::Right,
            (SplitFlow::Regular, Axis::Vertical) => Direction::Down,
            (SplitFlow::Inverted, Axis::Horizontal) => Direction::Left,
            (SplitFlow::Inverted, Axis::Vertical) => Direction::Up,
        }
    }
}

impl From<&'_ Split> for SplitFlow {
    fn from(split: &'_ Split) -> Self {
        match split {
            Split::Pane(_) => SplitFlow::Regular,
            Split::H { left, .. } => {
                if left.width.is_some() {
                    SplitFlow::Inverted
                } else {
                    SplitFlow::Regular
                }
            }
            Split::V { top, .. } => {
                if top.height.is_some() {
                    SplitFlow::Inverted
                } else {
                    SplitFlow::Regular
                }
            }
        }
    }
}

/// Finds the root pane for the given split (i.e. the pane all
/// rescursive splits are created on).
///
/// The path to the root pane depends on the flows of the
/// intermediate splits, which themselves depend on the splits'
/// size information.
fn root_pane(split: &Split) -> &Pane {
    match split {
        Split::Pane(pane) => pane,
        Split::H { left, right } => match SplitFlow::from(split) {
            SplitFlow::Regular => root_pane(&left.split),
            SplitFlow::Inverted => root_pane(&right.split),
        },
        Split::V { top, bottom } => match SplitFlow::from(split) {
            SplitFlow::Regular => root_pane(&top.split),
            SplitFlow::Inverted => root_pane(&bottom.split),
        },
    }
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn inverted(self) -> Self {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

impl From<Direction> for Axis {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::Left | Direction::Right => Axis::Horizontal,
            Direction::Up | Direction::Down => Axis::Vertical,
        }
    }
}

#[derive(Debug, Clone)]
struct Target<Scope> {
    session: Option<String>,
    window: Option<String>,
    pane: Option<String>,
    _scope: PhantomData<Scope>,
}

impl Target<Session> {
    fn session(session: impl Into<String>) -> Self {
        Self {
            session: Some(session.into()),
            window: None,
            pane: None,
            _scope: PhantomData,
        }
    }

    fn window(self, window: impl Into<String>) -> Target<Window> {
        Target {
            session: self.session,
            window: Some(window.into()),
            pane: None,
            _scope: PhantomData,
        }
    }

    fn current_window(self) -> Target<Window> {
        Target {
            session: self.session,
            window: None,
            pane: None,
            _scope: PhantomData,
        }
    }
}

impl Target<Window> {
    fn pane(self, pane: impl Into<String>) -> Target<Pane> {
        Target {
            session: self.session,
            window: self.window,
            pane: Some(pane.into()),
            _scope: PhantomData,
        }
    }
}

impl fmt::Display for Target<Session> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.session.as_deref().unwrap_or(""))
    }
}

impl fmt::Display for Target<Window> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}.",
            self.session.as_deref().unwrap_or(""),
            self.window.as_deref().unwrap_or(""),
        )
    }
}

impl fmt::Display for Target<Pane> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}.{}",
            self.session.as_deref().unwrap_or(""),
            self.window.as_deref().unwrap_or(""),
            self.pane.as_deref().unwrap_or("")
        )
    }
}

impl Default for Target<Session> {
    fn default() -> Self {
        Self {
            session: None,
            window: None,
            pane: None,
            _scope: PhantomData,
        }
    }
}
