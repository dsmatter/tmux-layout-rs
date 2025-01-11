use std::ops::{Deref, DerefMut};

use super::includes::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub type Config = ConfigL<NoIncludes>;
pub type PartialConfig = ConfigL<FilePathIncludes>;

type Cwd = crate::cwd::Cwd<'static>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Session {
    pub name: String,
    #[serde(skip_serializing_if = "Cwd::is_empty")]
    pub cwd: Cwd,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Window {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Cwd::is_empty")]
    pub cwd: Cwd,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub active: bool,
    #[serde(flatten)]
    pub root_split: RootSplit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "serialization::SplitMap", into = "serialization::SplitMap")]
pub enum Split {
    Pane(Pane),
    H { left: HSplitPart, right: HSplitPart },
    V { top: VSplitPart, bottom: VSplitPart },
}

impl Split {
    pub fn into_root(self) -> RootSplit {
        RootSplit(self)
    }

    pub fn single_pane(&self) -> Option<&Pane> {
        match self {
            Split::Pane(pane) => Some(pane),
            _ => None,
        }
    }

    pub fn single_pane_mut(&mut self) -> Option<&mut Pane> {
        match self {
            Split::Pane(pane) => Some(pane),
            _ => None,
        }
    }

    pub fn pane_iter(&self) -> Panes {
        Panes::new(self)
    }

    pub fn pane_iter_mut(&mut self) -> PanesMut {
        PanesMut::new(self)
    }
}

impl Default for Split {
    fn default() -> Self {
        Split::Pane(Pane::default())
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "serialization::SplitMap", into = "serialization::SplitMap")]
#[repr(transparent)]
pub struct RootSplit(Split);

impl Deref for RootSplit {
    type Target = Split;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RootSplit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HSplitPart {
    #[serde(skip_serializing_if = "serialization::is_default_size")]
    pub width: Option<String>,
    #[serde(flatten)]
    pub split: Box<Split>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VSplitPart {
    #[serde(skip_serializing_if = "serialization::is_default_size")]
    pub height: Option<String>,
    #[serde(flatten)]
    pub split: Box<Split>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pane {
    #[serde(skip_serializing_if = "Cwd::is_empty")]
    pub cwd: Cwd,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_keys: Option<Vec<String>>,
}

/// Iterates panes in tmux index order.
pub struct Panes<'a> {
    stack: Vec<&'a Split>,
}

impl<'a> Panes<'a> {
    pub fn new(root: &'a Split) -> Self {
        Self { stack: vec![root] }
    }
}

impl<'a> Iterator for Panes<'a> {
    type Item = &'a Pane;

    fn next(&mut self) -> Option<Self::Item> {
        let split = self.stack.pop()?;
        match split {
            Split::Pane(pane) => Some(pane),
            Split::H { left, right } => {
                self.stack.push(&right.split);
                self.stack.push(&left.split);
                self.next()
            }
            Split::V { top, bottom } => {
                self.stack.push(&bottom.split);
                self.stack.push(&top.split);
                self.next()
            }
        }
    }
}

/// Iterates panes in tmux index order (mutable).
pub struct PanesMut<'a> {
    stack: Vec<&'a mut Split>,
}

impl<'a> PanesMut<'a> {
    pub fn new(root: &'a mut Split) -> Self {
        Self { stack: vec![root] }
    }
}

impl<'a> Iterator for PanesMut<'a> {
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
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pub active: bool,
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
                active: map.active,
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
                    active: pane.active,
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

    impl From<RootSplit> for SplitMap {
        fn from(mut root: RootSplit) -> Self {
            // Avoid rendering the `active` property for single root panes.
            // While unneccessary, it also leads to ambiguity in the config file.
            // The `active` property of a single root pane would be interpreted
            // as the containing window's active state.
            if let Some(single_pane) = root.single_pane_mut() {
                single_pane.active = false;
            }
            root.0.into()
        }
    }

    impl From<SplitMap> for RootSplit {
        fn from(map: SplitMap) -> Self {
            Split::from(map).into_root()
        }
    }

    pub(super) fn is_default_size(size: &Option<String>) -> bool {
        match size {
            None => true,
            Some(size) => size == "50%",
        }
    }
}

#[cfg(test)]
mod test {
    use crate::config::{model::Cwd, HSplitPart, Pane, Session, Split, VSplitPart, Window};

    use super::PartialConfig;

    #[test]
    fn test_single_window_config() {
        let config_str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/config/single-window.toml"
        ));
        let config = toml::from_str::<PartialConfig>(config_str).unwrap();

        assert_eq!(
            config,
            PartialConfig {
                includes: Default::default(),
                selected_session: None,
                sessions: vec![],
                windows: vec![Window {
                    name: Some("A new window".to_string()),
                    cwd: "/tmp".into(),
                    active: false,
                    root_split: Split::H {
                        left: HSplitPart {
                            width: None,
                            split: Box::new(Split::Pane(Pane {
                                cwd: shellexpand::full("~").unwrap().into_owned().into(),
                                shell_command: Some("bash".to_string()),
                                ..Default::default()
                            })),
                        },
                        right: HSplitPart {
                            width: None,
                            split: Box::new(Split::Pane(Pane {
                                cwd: shellexpand::full("~/Downloads")
                                    .unwrap()
                                    .into_owned()
                                    .into(),
                                ..Default::default()
                            }))
                        }
                    }
                    .into_root(),
                }],
            }
        );
    }

    #[test]
    fn test_layout_config_yaml() {
        let config_str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/config/.tmux-layout.yml"
        ));
        let config = serde_yaml::from_str::<PartialConfig>(config_str).unwrap();

        assert!(config.includes.0.is_empty());
        assert_eq!(config.sessions.len(), 2);
        assert_eq!(config.selected_session.as_deref(), Some("sess1"));
        assert!(config.windows.is_empty());

        let sess1 = &config.sessions[0];
        assert_eq!(sess1.name, "sess1");
        assert_eq!(sess1.cwd, shellexpand::full("~").unwrap().as_ref());
        assert_eq!(sess1.windows.len(), 2);

        let win1 = &sess1.windows[0];
        assert_eq!(win1.name.as_deref(), Some("win1"));
        assert!(win1.active);
        assert_eq!(win1.cwd, "code");

        let split: &Split = &win1.root_split;
        let Split::H { left, right } = split else {
            panic!("expected horizontal split");
        };

        assert!(left.width.is_none());
        assert_eq!(right.width.as_deref(), Some("66%"));

        let left_split: &Split = &left.split;
        let Split::V { top, bottom } = left_split else {
            panic!("expected vertical split");
        };

        assert!(top.height.is_none());
        assert!(bottom.height.is_none());

        let top_pane = top.split.single_pane().unwrap();
        assert_eq!(
            top_pane,
            &Pane {
                cwd: "projects".into(),
                ..Default::default()
            }
        );

        let bot_pane = bottom.split.single_pane().unwrap();
        assert_eq!(
            bot_pane,
            &Pane {
                cwd: "scratch".into(),
                ..Default::default()
            }
        );

        let right_split: &Split = &right.split;
        let Split::V { top, bottom } = right_split else {
            panic!("expected vertical split");
        };

        assert!(top.height.is_none());
        assert!(bottom.height.is_none());

        assert_eq!(top.split.single_pane().unwrap(), &Pane::default());
        assert_eq!(
            bottom.split.single_pane().unwrap(),
            &Pane {
                cwd: "projects/tmux-layout".into(),
                ..Default::default()
            }
        );

        assert_eq!(
            sess1.windows[1],
            Window {
                name: Some("win2".to_string()),
                active: false,
                cwd: ".zsh".into(),
                root_split: Split::H {
                    left: HSplitPart {
                        width: None,
                        split: Box::new(Split::Pane(Pane {
                            cwd: shellexpand::full("$JAVA_HOME").unwrap().into_owned().into(),
                            ..Default::default()
                        })),
                    },
                    right: HSplitPart::default(),
                }
                .into_root(),
            }
        );

        let sess2 = &config.sessions[1];
        assert_eq!(
            sess2,
            &Session {
                name: "sess2".to_string(),
                cwd: Cwd::new(None),
                windows: vec![Window {
                    name: None,
                    active: false,
                    cwd: Cwd::new(None),
                    root_split: Split::H {
                        left: HSplitPart {
                            width: Some("20%".to_string()),
                            split: Box::new(Split::Pane(Pane {
                                send_keys: Some(vec!["ls -al".to_string(), "ENTER".to_string()]),
                                ..Default::default()
                            })),
                        },
                        right: HSplitPart {
                            width: None,
                            split: Box::new(Split::Pane(Pane {
                                shell_command: Some("bash".to_string()),
                                ..Default::default()
                            }),),
                        }
                    }
                    .into_root(),
                }],
            }
        );
    }

    #[test]
    fn test_layout_config_toml() {
        let config_str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/config/.tmux-layout.toml"
        ));
        let config = toml::from_str::<PartialConfig>(config_str).unwrap();

        assert_eq!(
            config,
            PartialConfig {
                includes: Default::default(),
                selected_session: Some("sess1".to_string()),
                windows: vec![],
                sessions: vec![
                    Session {
                        name: "sess1".to_string(),
                        cwd: shellexpand::full("~").unwrap().into_owned().into(),
                        windows: vec![
                            Window {
                                name: Some("win1".to_string()),
                                cwd: "code".into(),
                                active: true,
                                root_split: Split::H {
                                    left: HSplitPart {
                                        width: None,
                                        split: Box::new(Split::V {
                                            top: VSplitPart {
                                                height: None,
                                                split: Box::new(Split::Pane(Pane {
                                                    cwd: "projects".into(),
                                                    ..Default::default()
                                                })),
                                            },
                                            bottom: VSplitPart {
                                                height: None,
                                                split: Box::new(Split::Pane(Pane {
                                                    cwd: "scratch".into(),
                                                    ..Default::default()
                                                })),
                                            },
                                        })
                                    },
                                    right: HSplitPart {
                                        width: None,
                                        split: Box::new(Split::V {
                                            top: VSplitPart {
                                                height: None,
                                                split: Box::new(Split::Pane(Pane::default())),
                                            },
                                            bottom: VSplitPart {
                                                height: None,
                                                split: Box::new(Split::Pane(Pane {
                                                    cwd: "projects/tmux-layout".into(),
                                                    send_keys: Some(vec![
                                                        "g".to_string(),
                                                        "ENTER".to_string()
                                                    ]),
                                                    ..Default::default()
                                                })),
                                            },
                                        })
                                    }
                                }
                                .into_root(),
                            },
                            Window {
                                name: Some("win2".to_string()),
                                active: false,
                                cwd: ".zsh".into(),
                                root_split: Split::H {
                                    left: HSplitPart {
                                        width: Some("33%".to_string()),
                                        split: Box::new(Split::Pane(Pane {
                                            cwd: shellexpand::full("$JAVA_HOME")
                                                .unwrap()
                                                .into_owned()
                                                .into(),
                                            ..Default::default()
                                        })),
                                    },
                                    right: HSplitPart {
                                        width: None,
                                        split: Box::new(Split::Pane(Pane::default())),
                                    }
                                }
                                .into_root(),
                            },
                        ]
                    },
                    Session {
                        name: "sess2".to_string(),
                        cwd: Cwd::new(None),
                        windows: vec![Window {
                            name: None,
                            active: false,
                            cwd: Cwd::new(None),
                            root_split: Split::H {
                                left: HSplitPart {
                                    width: None,
                                    split: Box::new(Split::Pane(Pane {
                                        send_keys: Some(vec![
                                            "ls -al".to_string(),
                                            "ENTER".to_string()
                                        ]),
                                        ..Default::default()
                                    })),
                                },
                                right: HSplitPart {
                                    width: Some("120".to_string()),
                                    split: Box::new(Split::Pane(Pane {
                                        shell_command: Some("bash".to_string()),
                                        ..Default::default()
                                    })),
                                },
                            }
                            .into_root(),
                        }],
                    }
                ],
            }
        );
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let config_str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/config/.tmux-layout.yml"
        ));
        let config = serde_yaml::from_str::<PartialConfig>(config_str)
            .unwrap()
            .into_config()
            .unwrap();

        let serialized = serde_yaml::to_string(&config).unwrap();
        let parsed = serde_yaml::from_str::<PartialConfig>(&serialized)
            .unwrap()
            .into_config()
            .unwrap();

        assert_eq!(config, parsed);
    }

    #[test]
    fn test_config_serde_cross_format() {
        let config_str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/config/.tmux-layout.yml"
        ));
        let config = serde_yaml::from_str::<PartialConfig>(config_str)
            .unwrap()
            .into_config()
            .unwrap();

        let serialized = toml::to_string(&config).unwrap();
        let parsed = toml::from_str::<PartialConfig>(&serialized)
            .unwrap()
            .into_config()
            .unwrap();

        assert_eq!(config, parsed);
    }
}
