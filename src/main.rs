#![allow(unused_imports)]
#![allow(dead_code)]

mod error;

use crate::{
  error::{CallZomeError, CallZomeResult}
};
use std::{
  convert::TryFrom,
  error::Error,
  path::PathBuf,
  string::String,
};
use hdk3::prelude::{
  AgentPubKey,
  Deserialize,
  Serialize,
  SerializedBytes,
  SerializedBytesError,
  holochain_serial,
};
use holochain::{
  conductor::{
    Conductor,
    ConductorHandle,
    api::ZomeCall,
    api::error::{ConductorApiError, ConductorApiResult},
    config::ConductorConfig,
    error::{ConductorError, ConductorResult, CreateAppError},
    paths::{ConfigFilePath, EnvironmentRootPath},
  },
  core::{
    DnaHash,
    ribosome::ZomeCallInvocation,
    workflow::ZomeCallResult,
  },
};
use holochain_keystore::KeystoreSenderExt;
use holochain_state::{
  test_utils::TestEnvironments
};
use holochain_types::{
  app::{CellNick, InstalledCell, InstalledAppId, InstalledApp},
  cell::CellId,
  dna::{DnaFile, zome::Zome},
};
use holochain_zome_types::{
  capability::CapSecret,
  zome::{FunctionName, ZomeName},
  zome_io::{ExternInput, ExternOutput},
  ZomeCallResponse,
};
use structopt::StructOpt;
use uuid::Uuid;
use warp::Filter;

const RSS_APP_ID: &'static str = "holochain_rss-0.0.1";
const RSS_DNA_BYTES: &'static [u8] = include_bytes!("../app/rss.dna.gz");
const RSS_ZOME_NAME: &'static str = "rss";

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct RssPublisher {
  agent_key: AgentPubKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct RssChannel {
  pub uuid: String,
  pub title: String,
  pub link: String,
  pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct RssItem {
  pub uuid: String,
  pub title: Option<String>,
  pub link: Option<String>,
  pub description: Option<String>,
  pub author: Option<String>,
}

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

  #[structopt(subcommand)]
  cmd: Command,
}
#[derive(Debug, StructOpt)]
enum Command {
  Serve {
    #[structopt(
      help = "Port to listen on for HTTP server."
    )]
    port: i32
  }
}

fn main() {
  tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .init();

  holochain::conductor::tokio_runtime()
    .block_on(async_main());
}

async fn async_main() {
  human_panic::setup_panic!();

  let opt = Opt::from_args();

  // Create conductor

  let conductor = conductor_handle_from_config_path(opt.config_path)
    .await;

  tracing::info!("Holochain conductor is running.");

  let agent_key = generate_agent_key(&conductor)
    .await;

  let installed_app = install_and_activate_rss_app(&conductor, agent_key.clone())
    .await
    .expect("Failed to install app.")
    .clone();

  tracing::info!("Installed app: {:#?}", installed_app.clone());

  let installed_cell = &installed_app.cell_data[0];
  let installed_cell_id = installed_cell.clone().into_id();
  let FetchRssChannelsResponse(rss_channels) = fetch_rss_channels(&conductor, installed_cell_id.clone(), agent_key.clone())
    .await
    .expect("Failed to fetch RSS channels.");

  tracing::info!("RSS channels: {:?}", rss_channels);

  match opt.cmd {
    Command::Serve { port } => {
      serve(&conductor, port).await
    }
  };
  
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

async fn serve(conductor: &ConductorHandle, port: u16) {
  let hello = warp::path!("hello" / String)
      .map(|name| format!("Hello, {}!", name));

  warp::serve(hello)
      .run(([127, 0, 0, 1], port))
      .await;
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

async fn install_app(
  conductor: &ConductorHandle,
  agent_key: AgentPubKey,
  installed_app_id: InstalledAppId,
  dna: DnaFile,
  cell_nick: CellNick,
) -> ConductorResult<InstalledApp> {
  let dna_hash = dna.dna_hash();
  let cell_id = CellId::from((dna_hash.clone(), agent_key.clone()));
  conductor.clone().install_dna(dna.clone())
    .await?;

  let installed_cell = InstalledCell::new(cell_id.clone(), cell_nick.clone());
  let membrane_proofs = vec![(installed_cell.clone(), None)];
  conductor.clone().install_app(installed_app_id.clone(), membrane_proofs)
    .await?;

  let installed_app = InstalledApp {
    installed_app_id: installed_app_id,
    cell_data: vec![installed_cell],
  };

  Ok(installed_app)
}

async fn activate_app(
  conductor: &ConductorHandle,
  installed_app_id: InstalledAppId
) -> ConductorResult<()> {
  conductor.clone().activate_app(installed_app_id.clone())
    .await?;
  
  let errors = conductor
    .clone()
    .setup_cells()
    .await?;
  
  errors
    .into_iter()
    .find(|error| match error {
      CreateAppError::Failed {
        installed_app_id: error_app_id,
        ..
      } => error_app_id == &installed_app_id,
    })
    .map(|error| Err(ConductorError::CreateAppFailed(error)))
    .unwrap_or(Ok(()))
}

async fn install_and_activate_rss_app(
  conductor: &ConductorHandle,
  agent_key: AgentPubKey
) -> ConductorResult<InstalledApp> {
  let installed_app_id = InstalledAppId::from(RSS_APP_ID);
  let cell_nick = CellNick::from("holochain_rss");
  let dna_bytes = RSS_DNA_BYTES.into();
  let dna = DnaFile::from_file_content(dna_bytes).await?;
  let installed_app = install_app(&conductor, agent_key, installed_app_id.clone(), dna, cell_nick)
    .await?;

  activate_app(&conductor, installed_app_id)
    .await?;

  Ok(installed_app)
}

async fn find_app(
  conductor: &ConductorHandle,
  installed_app_id: InstalledAppId,
) -> ConductorResult<Option<InstalledApp>> {
  conductor.clone().get_app_info(&installed_app_id)
    .await
}

async fn call_zome<'a, I: 'a, O>(
  conductor: &ConductorHandle,
  cell_id: CellId,
  agent_key: AgentPubKey,
  zome_name: &str,
  fn_name: &str,
  cap: Option<CapSecret>,
  payload: &'a I,
) -> CallZomeResult<O>
where
  SerializedBytes: TryFrom<&'a I, Error = SerializedBytesError>,
  O: TryFrom<SerializedBytes, Error = SerializedBytesError>,
{
  let data = match SerializedBytes::try_from(payload) {
    Ok(data) => Ok(data),
    Err(_) => Err(CallZomeError::SerializedBytes)
  };

  let zome_call = ZomeCall {
    cell_id: cell_id,
    zome_name: String::from(zome_name).into(),
    fn_name: FunctionName::from(fn_name),
    payload: ExternInput::new(data?),
    cap: cap,
    provenance: agent_key
  };

  let result = conductor
    .clone()
    .call_zome(zome_call)
    .await?;
  
  match result? {
    ZomeCallResponse::Ok(output) => {
      let serialized_bytes = output.into_inner();
      match O::try_from(serialized_bytes) {
        Ok(response) => Ok(response),
        Err(_) => Err(CallZomeError::SerializedBytes)
      }
    },
    ZomeCallResponse::Unauthorized(c, z, f, a) => {
      Err(CallZomeError::UnauthorizedZomeCall(c, z, f, a))
    },
    ZomeCallResponse::NetworkError(s) => {
      Err(CallZomeError::ZomeCallNetworkError(s))
    }
  }
}

// Create RSS Channel

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct CreateRssChannelRequest(RssChannel);

async fn create_rss_channel(
  conductor: &ConductorHandle,
  cell_id: CellId,
  agent_key: AgentPubKey,
  request: CreateRssChannelRequest,
) -> CallZomeResult<()> {
  call_zome(
    conductor,
    cell_id,
    agent_key,
    RSS_ZOME_NAME,
    "create_rss_channel",
    None,
    &request,
  ).await
}

// Create RSS Item

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct CreateRssItemRequest(RssItem, String);

async fn create_rss_item(
  conductor: &ConductorHandle,
  cell_id: CellId,
  agent_key: AgentPubKey,
  request: CreateRssItemRequest,
) -> CallZomeResult<()> {
  call_zome(
    conductor,
    cell_id,
    agent_key,
    RSS_ZOME_NAME,
    "create_rss_item",
    None,
    &request,
  ).await
}

// Fetch RSS Channels

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct FetchRssChannelsResponse(Vec<RssChannel>);

async fn fetch_rss_channels(
  conductor: &ConductorHandle,
  cell_id: CellId,
  agent_key: AgentPubKey,
) -> CallZomeResult<FetchRssChannelsResponse> {
  call_zome(
    conductor,
    cell_id,
    agent_key,
    RSS_ZOME_NAME,
    "fetch_rss_channels",
    None,
    &(),
  ).await
}

// Fetch RSS Items

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct FetchRssItemsRequest(String);

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct FetchRssItemsResponse(Vec<RssItem>);

async fn fetch_rss_items(
  conductor: &ConductorHandle,
  cell_id: CellId,
  agent_key: AgentPubKey,
  request: FetchRssItemsRequest,
) -> CallZomeResult<FetchRssItemsResponse> {
  call_zome(
    conductor,
    cell_id,
    agent_key,
    RSS_ZOME_NAME,
    "fetch_rss_items",
    None,
    &request,
  ).await
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test(threaded_scheduler)]
  async fn can_install_app_and_fetch_channels() {
    tracing_subscriber::fmt()
      .with_max_level(tracing::Level::INFO)
      .init();

    let conductor = conductor_handle_from_config_path("./config.yml".into())
      .await;

    let agent_key = generate_agent_key(&conductor)
      .await;
  
    let installed_app = install_and_activate_rss_app(&conductor, agent_key.clone())
      .await
      .unwrap();

    tracing::info!("installed_app: {:?}", installed_app.clone());
  
    let cell = &installed_app.cell_data[0];
    let cell_id = cell.clone().into_id();

    let channel = RssChannel {
      uuid: Uuid::new_v4().to_string(),
      title: "My RSS Channel".to_string(),
      link: "https://holopod.host/my-rss-channel.xml".to_string(),
      description: "Welcome to the Holochain distributed RSS channel!".to_string(),
    };

    let _ = create_rss_channel(&conductor, cell_id.clone(), agent_key.clone(), CreateRssChannelRequest(channel.clone()))
      .await
      .unwrap();

    let FetchRssChannelsResponse(channels) = fetch_rss_channels(&conductor, cell_id.clone(), agent_key.clone())
      .await
      .unwrap();

    tracing::info!("channels: {:?}", channels.clone());

    assert!(channels.len() == 1);

    let item1 = RssItem {
      uuid: Uuid::new_v4().to_string(),
      title: Some("Item 1".to_string()),
      link: None,
      description: None,
      author: None,
    };

    let _ = create_rss_item(&conductor, cell_id.clone(), agent_key.clone(), CreateRssItemRequest(item1, channel.uuid.clone()))
      .await
      .unwrap();

    let item2 = RssItem {
      uuid: Uuid::new_v4().to_string(),
      title: Some("Item 2".to_string()),
      link: None,
      description: None,
      author: None,
    };

    let _ = create_rss_item(&conductor, cell_id.clone(), agent_key.clone(), CreateRssItemRequest(item2, channel.uuid.clone()))
      .await
      .unwrap();

    let FetchRssItemsResponse(items) = fetch_rss_items(&conductor, cell_id.clone(), agent_key.clone(), FetchRssItemsRequest(channel.uuid.clone()))
      .await
      .unwrap();

    tracing::info!("items: {:?}", items.clone());

    assert!(items.len() == 2);

    let shutdown = conductor.take_shutdown_handle().await.unwrap();
    conductor.shutdown().await;
    shutdown.await.unwrap();
  }
}
