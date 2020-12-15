#![allow(unused_imports)]
#![allow(dead_code)]

use std::{
  error::Error,
  path::PathBuf,
  string::String,
};
use hdk3::prelude::{AgentPubKey, Serialize, Deserialize};
use holochain::{
  conductor::{
    Conductor,
    ConductorHandle,
    api::error::ConductorApiResult,
    config::ConductorConfig,
    error::ConductorResult,
    paths::{ConfigFilePath, EnvironmentRootPath},
  },
  core::DnaHash
};
use holochain_keystore::KeystoreSenderExt;
use holochain_state::{
  test_utils::{TestEnvironments}
};
use holochain_types::{
  app::{CellNick, InstalledCell, InstalledAppId, InstalledApp},
  cell::CellId,
  dna::DnaFile,
};
use structopt::StructOpt;

const RSS_PUB_DNA_BYTES: &'static [u8] = include_bytes!("../dna/rss_pub.dna.gz");
const RSS_PUB_CELL_HANDLE: &'static str = "rss_pub";

#[derive(Debug, StructOpt)]
#[structopt(name = "holochain_rss", about = "A Holochain RSS conductor.")]
struct Opt {
  #[structopt(
    help = "Path to a YAML file containing conductor configuration",
    short = "c",
    long = "config",
    default_value = "./config.yml",
  )]
  config_path: PathBuf,
}

fn main() {
  tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();

  holochain::conductor::tokio_runtime()
    .block_on(async_main())
}

async fn async_main() {
  human_panic::setup_panic!();

  let opt = Opt::from_args();

  let conductor = conductor_handle_from_config_path(opt.config_path)
    .await;

  tracing::info!("Holochain conductor is running.");

  let agent_key = generate_agent_key(&conductor)
    .await;

  let dna_bytes = RSS_PUB_DNA_BYTES.into();
  let cell_handle = RSS_PUB_CELL_HANDLE.into();
  let dna = DnaFile::from_file_content(dna_bytes)
    .await
    .expect("Failed to load DNA from file.");
  let installed_app = install_app(&conductor, agent_key, dna, cell_handle)
    .await
    .expect("Failed to install app.");

  tracing::debug!("Installed app: {:#?}", installed_app);

  conductor
    .take_shutdown_handle()
    .await
    .expect("The holochain conductor shutdown handle has already been taken.")
    .await
    .map_err(|err| {
      tracing::error!(error = &err as &dyn Error);
      err
    })
    .expect("Failed to shut down holochain conductor.");
}

async fn conductor_handle_from_config_path(
  config_path: PathBuf
) -> ConductorHandle {
  let config_path: ConfigFilePath = config_path.into();
  let config: ConductorConfig = load_config(&config_path);

  let environment_path = config.environment_path.clone();
  create_environment(&environment_path);

  Conductor::builder()
    .config(config)
    .build()
    .await
    .expect("Failed to build holochain conductor.")
}

fn load_config(config_path: &ConfigFilePath) -> ConductorConfig {
  ConductorConfig::load_yaml(config_path.as_ref())
    .expect("Failed to load holochain conductor config.")
}

fn create_environment(environment_path: &EnvironmentRootPath) {
  let environment_path = environment_path.as_ref();
  if !environment_path.is_dir() {
    std::fs::create_dir_all(&environment_path)
      .expect("Failed to create holochain conductor environment.");
  }
}

async fn generate_agent_key(
  conductor: &ConductorHandle
) -> AgentPubKey {
  conductor
    .keystore()
    .clone()
    .generate_sign_keypair_from_pure_entropy()
    .await
    .expect("Failed to generate agent key.")
}

async fn rss_pub_app(
  conductor: &ConductorHandle,
  agent_key: AgentPubKey
) -> ConductorResult<InstalledApp> {
  let dna_bytes = RSS_PUB_DNA_BYTES.into();
  let cell_handle = RSS_PUB_CELL_HANDLE.into();
  let dna = DnaFile::from_file_content(dna_bytes).await?;
  find_or_install_app(&conductor, agent_key, dna, cell_handle)
    .await
}

fn get_installed_app_id(cell_handle: &CellNick, dna_hash: &DnaHash) -> InstalledAppId {
  format!("{}-{}", String::from(cell_handle), dna_hash)
}

async fn install_app(
  conductor: &ConductorHandle,
  agent_key: AgentPubKey,
  dna: DnaFile,
  cell_handle: CellNick
) -> ConductorResult<InstalledApp> {
  let dna_hash = dna.dna_hash();
  let cell_id = CellId::from((dna_hash.clone(), agent_key.clone()));
  conductor.clone().install_dna(dna.clone())
    .await?;

  let installed_app_id = get_installed_app_id(&cell_handle, &dna_hash);
  let installed_cell = InstalledCell::new(cell_id.clone(), cell_handle.clone());
  let membrane_proofs = vec![(installed_cell.clone(), None)];
  conductor.clone().install_app(installed_app_id.clone(), membrane_proofs)
    .await?;

  let installed_app = InstalledApp {
    installed_app_id: installed_app_id,
    cell_data: vec![installed_cell],
  };

  Ok(installed_app)
}

async fn find_app(
  conductor: &ConductorHandle,
  dna: DnaFile,
  cell_handle: CellNick
) -> ConductorResult<Option<InstalledApp>> {
  let dna_hash = dna.dna_hash();
  let installed_app_id = get_installed_app_id(&cell_handle, &dna_hash);

  conductor.clone().get_app_info(&installed_app_id)
    .await
}

async fn find_or_install_app(
  conductor: &ConductorHandle,
  agent_key: AgentPubKey,
  dna: DnaFile,
  cell_handle: CellNick
) -> ConductorResult<InstalledApp> {
  match find_app(&conductor, dna.clone(), cell_handle.clone()).await? {
    Some(installed_app) => Ok(installed_app),
    None => install_app(&conductor, agent_key, dna, cell_handle).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test(threaded_scheduler)]
  async fn can_install_app() {
    tracing_subscriber::fmt()
      .with_max_level(tracing::Level::INFO)
      .init();

    let conductor = conductor_handle_from_config_path("./config.yml".into())
      .await;

    let agent_key = generate_agent_key(&conductor)
      .await;
  
    let installed_app = rss_pub_app(&conductor, agent_key)
      .await
      .expect("can install app");

    assert_eq!(String::from(installed_app.cell_data[0].as_nick()), String::from(RSS_PUB_CELL_HANDLE));

    let shutdown = conductor.take_shutdown_handle().await.unwrap();
    conductor.shutdown().await;
    shutdown.await.unwrap();
  }
}
