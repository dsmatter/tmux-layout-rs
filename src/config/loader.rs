use shellexpand::LookupError;
use std::env::VarError;
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;

use crate::show_warning;

use super::{Config, PartialConfig};

pub fn load_config_at(path: &Path) -> Result<Config, Error> {
    let partial_config = load_partial_config_at(path)?;
    let mut config = Config {
        selected_session: partial_config.selected_session,
        sessions: partial_config.sessions,
        windows: partial_config.windows,
        ..Default::default()
    };

    for included_path in partial_config.includes.0 {
        let included_path = shellexpand::full(&included_path)?;
        let included_path = path
            .parent()
            .unwrap()
            .join(Path::new(included_path.as_ref()));

        let mut included_config = load_config_at(&included_path)?;
        // Merge sessions and windows
        config.sessions.append(&mut included_config.sessions);
        config.windows.append(&mut included_config.windows);

        // Merge selected session
        if let Some(select_session) = included_config.selected_session {
            if config.selected_session.is_none() {
                config.selected_session = Some(select_session);
            } else {
                show_warning(&format!(
                    "ignoring selected session \"{}\" from {:?}",
                    select_session, included_path
                ))
            }
        }
    }
    Ok(config)
}

pub fn load_partial_config_at(path: &Path) -> Result<PartialConfig, Error> {
    let config_bytes = fs::read(path).map_err(|error| Error::Io {
        path: path.to_owned(),
        error,
    })?;

    match path.extension().map(|s| s.as_bytes()) {
        Some(b"toml") => {
            let config_str =
                std::str::from_utf8(&config_bytes).map_err(|err| Error::ParseError {
                    path: path.to_owned(),
                    message: format!("UTF-8 error: {}", err),
                })?;

            toml::from_str(config_str).map_err(|err| Error::ParseError {
                path: path.to_owned(),
                message: format!("{}", err),
            })
        }
        Some(b"yml") | Some(b"yaml") => {
            serde_yaml::from_slice(&config_bytes).map_err(|err| Error::ParseError {
                path: path.to_owned(),
                message: format!("{}", err),
            })
        }
        _ => Err(Error::UnsupportedFormat),
    }
}

pub fn find_default_config_file() -> Option<PathBuf> {
    const BASENAME: &str = ".tmux-layout";
    const EXTS: [&str; 3] = ["yaml", "yml", "toml"];

    let current_dir = std::env::current_dir().ok()?;
    let home_dir = dirs::home_dir()?;

    for dir in &[current_dir, home_dir] {
        for ext in &EXTS {
            let file_path = dir.join(format!("{}.{}", BASENAME, ext));
            if file_path.exists() {
                return Some(file_path);
            }
        }
    }

    None
}
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to load config file at {path:?}: {error}")]
    Io { path: PathBuf, error: io::Error },
    #[error("failed to parse config file at {path:?}: {message}")]
    ParseError { path: PathBuf, message: String },
    #[error("unsupported config format (supported: YAML, TOML)")]
    UnsupportedFormat,
    #[error("variable lookup error: {0}")]
    LookupError(#[from] LookupError<VarError>),
}
