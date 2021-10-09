use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NoIncludes;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FilePathIncludes(pub Vec<String>);

pub trait ConfigIncludes: Serialize + DeserializeOwned + Default + sealed::Sealed {
    fn is_empty(&self) -> bool;
}

impl ConfigIncludes for NoIncludes {
    fn is_empty(&self) -> bool {
        true
    }
}

impl ConfigIncludes for FilePathIncludes {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl sealed::Sealed for NoIncludes {}
impl sealed::Sealed for FilePathIncludes {}

#[derive(Debug, Error)]
#[error("unresolved includes")]
pub struct UnresolvedIncludes;

mod sealed {
    pub trait Sealed {}
}
