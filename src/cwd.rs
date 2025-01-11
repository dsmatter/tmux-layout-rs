use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cwd<'a> {
    path: Option<Cow<'a, Path>>,
}

impl Cwd<'_> {
    pub fn new(path: Option<Cow<Path>>) -> Cwd {
        Cwd { path }
    }

    pub fn joined<'a>(&'a self, other: &'a Cwd<'a>) -> Cwd<'a> {
        match &other.path {
            None => self.shallow_clone(),
            Some(path) => {
                if path.is_absolute() {
                    other.shallow_clone()
                } else {
                    match &self.path {
                        None => other.shallow_clone(),
                        Some(prefix) => prefix.join(path).into(),
                    }
                }
            }
        }
    }

    pub fn to_path(&self) -> Option<&Path> {
        self.path.as_ref().map(AsRef::as_ref)
    }

    pub fn shallow_clone(&self) -> Cwd {
        Cwd {
            path: self.path.as_ref().map(|path| Cow::Borrowed(path.as_ref())),
        }
    }

    pub fn is_empty(&self) -> bool {
        match &self.path {
            None => true,
            Some(path) => path.as_ref().components().next().is_none(),
        }
    }
}

impl From<String> for Cwd<'static> {
    fn from(s: String) -> Self {
        if s.is_empty() {
            Self::default()
        } else {
            Self::new(Some(Cow::Owned(s.into())))
        }
    }
}

impl From<PathBuf> for Cwd<'static> {
    fn from(s: PathBuf) -> Self {
        if s.components().next().is_none() {
            Self::default()
        } else {
            Self::new(Some(Cow::Owned(s)))
        }
    }
}

impl From<Option<String>> for Cwd<'static> {
    fn from(os: Option<String>) -> Self {
        match os {
            None => Self::default(),
            Some(s) => s.into(),
        }
    }
}

impl<'a> From<&'a str> for Cwd<'a> {
    fn from(s: &'a str) -> Self {
        Self::new(Some(Cow::Borrowed(Path::new(s))))
    }
}

impl<'a> PartialEq<&'a str> for Cwd<'_> {
    fn eq(&self, other: &&'a str) -> bool {
        self.path.as_deref() == Some(Path::new(other))
    }
}

impl Serialize for Cwd<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.path.as_ref().map(AsRef::as_ref).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Cwd<'static> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let path: Option<String> = Deserialize::deserialize(deserializer)?;
        let expanded_path = match path {
            None => None,
            Some(path) => Some(
                shellexpand::full(&path)
                    .map_err(|err| serde::de::Error::custom(format!("{}", err)))?
                    .into_owned(),
            ),
        };
        Ok(expanded_path.into())
    }
}
