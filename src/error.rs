use hdk3::prelude::{
  holochain_serial,
  AgentPubKey
};
use holochain_types::{
  cell::CellId,
};
use holochain_zome_types::{
  zome::{FunctionName, ZomeName},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CallZomeError {
  #[error(transparent)]
  ConductorApiError(#[from] holochain::conductor::api::error::ConductorApiError),

  #[error(transparent)]
  RibosomeError(#[from] holochain::core::ribosome::error::RibosomeError),

  #[error("Failed to serialize zome call response")]
  SerializedBytes,

  #[error("Zome call was made which the caller was unauthorized to make")]
  UnauthorizedZomeCall(CellId, ZomeName, FunctionName, AgentPubKey),

  #[error("A remote zome call was made but there was a network error: {0}")]
  ZomeCallNetworkError(String),
}

pub type CallZomeResult<T> = Result<T, CallZomeError>;
