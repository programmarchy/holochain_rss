use hdk3::prelude::*;

#[derive(thiserror::Error, Debug)]
pub enum RssError {
  #[error(transparent)]
  HdkError(#[from] HdkError),

  #[error("Failed to get latest entry.")]
  GetLatestEntry,
}

pub type RssResult<T> = Result<T, RssError>;
