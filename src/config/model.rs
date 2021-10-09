use super::includes::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub type Config = ConfigL<NoIncludes>;
pub type PartialConfig = ConfigL<FilePathIncludes>;

type Cwd = crate::cwd::Cwd<'static>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(bound = "Includes: DeserializeOwned")]
pub struct ConfigL<Includes: ConfigIncludes> {
    #[serde(default, skip_serializing_if = "ConfigIncludes::is_empty")]
    pub includes: Includes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_session: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<Session>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<Window>,
}

impl PartialConfig {
    pub fn into_config(self) -> Result<Config, UnresolvedIncludes> {
        if self.includes.is_empty() {
            Ok(Config {
                selected_session: self.selected_session,
                sessions: self.sessions,
                windows: self.windows,
                includes: NoIncludes,
            })
        } else {
            Err(UnresolvedIncludes)
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Session {
    pub name: String,
    #[serde(skip_serializing_if = "Cwd::is_empty")]
    pub cwd: Cwd,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Window {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Cwd::is_empty")]
    pub cwd: Cwd,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub active: bool,
    #[serde(flatten)]
    pub root_split: Split,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "serialization::SplitMap", into = "serialization::SplitMap")]
pub enum Split {
    Pane(Pane),
    H { left: HSplitPart, right: HSplitPart },
    V { top: VSplitPart, bottom: VSplitPart },
}

impl Split {
    pub fn pane_iter(&mut self) -> Panes {
        Panes::new(self)
    }
}

impl Default for Split {
    fn default() -> Self {
        Split::Pane(Pane::default())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HSplitPart {
    #[serde(skip_serializing_if = "serialization::is_default_size")]
    pub width: Option<String>,
    #[serde(flatten)]
    pub split: Box<Split>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VSplitPart {
    #[serde(skip_serializing_if = "serialization::is_default_size")]
    pub height: Option<String>,
    #[serde(flatten)]
    pub split: Box<Split>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pane {
    #[serde(skip_serializing_if = "Cwd::is_empty")]
    pub cwd: Cwd,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_keys: Option<Vec<String>>,
}

/// Iterates panes in tmux index order.
pub struct Panes<'a> {
    stack: Vec<&'a mut Split>,
}

impl<'a> Panes<'a> {
    pub fn new(root: &'a mut Split) -> Self {
        Self { stack: vec![root] }
    }
}

impl<'a> Iterator for Panes<'a> {
    type Item = &'a mut Pane;

    fn next(&mut self) -> Option<Self::Item> {
        let split = self.stack.pop()?;
        match split {
            Split::Pane(pane) => Some(pane),
            Split::H { left, right } => {
                self.stack.push(&mut right.split);
                self.stack.push(&mut left.split);
                self.next()
            }
            Split::V { top, bottom } => {
                self.stack.push(&mut bottom.split);
                self.stack.push(&mut top.split);
                self.next()
            }
        }
    }
}

pub(super) mod serialization {
    use super::*;
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub(super) struct SplitMap {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(super) left: Option<HSplitPart>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(super) right: Option<HSplitPart>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(super) top: Option<VSplitPart>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(super) bottom: Option<VSplitPart>,
        #[serde(skip_serializing_if = "Cwd::is_empty")]
        pub(super) cwd: Cwd,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(super) shell_command: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub(super) send_keys: Option<Vec<String>>,
    }

    impl From<SplitMap> for Split {
        fn from(map: SplitMap) -> Self {
            if map.left.is_some() || map.right.is_some() {
                return Split::H {
                    left: map.left.unwrap_or_default(),
                    right: map.right.unwrap_or_default(),
                };
            }

            if map.top.is_some() || map.bottom.is_some() {
                return Split::V {
                    top: map.top.unwrap_or_default(),
                    bottom: map.bottom.unwrap_or_default(),
                };
            }

            Split::Pane(Pane {
                cwd: map.cwd,
                shell_command: map.shell_command,
                send_keys: map.send_keys,
            })
        }
    }

    impl From<Split> for SplitMap {
        fn from(split: Split) -> Self {
            match split {
                Split::Pane(pane) => Self {
                    cwd: pane.cwd,
                    shell_command: pane.shell_command,
                    send_keys: pane.send_keys,
                    ..Default::default()
                },
                Split::H { left, right } => Self {
                    left: Some(left),
                    right: Some(right),
                    ..Default::default()
                },
                Split::V { top, bottom } => Self {
                    top: Some(top),
                    bottom: Some(bottom),
                    ..Default::default()
                },
            }
        }
    }

    pub(super) fn is_default_size(size: &Option<String>) -> bool {
        match size {
            None => true,
            Some(size) => size == "50%",
        }
    }
}
