use hdk3::prelude::*;

pub const RSS_CHANNELS_PATH: &str = "rss_channels";

#[hdk_extern]
fn init(_: ()) -> ExternResult<InitCallbackResult> {
  let channels_path = Path::from(RSS_CHANNELS_PATH);
  channels_path.ensure()?;

  debug!("rss_pub init");

  Ok(InitCallbackResult::Pass)
}

entry_defs![
  Path::entry_def(),
  RssPublisher::entry_def(),
  RssChannel::entry_def(),
  RssItem::entry_def()
];

#[hdk_entry(id = "rss_publisher")]
#[derive(Debug, Clone, PartialEq)]
pub struct RssPublisher {
  agent_key: AgentPubKey,
}

#[hdk_entry(id = "rss_channel")]
#[derive(Debug, Clone, PartialEq)]
pub struct RssChannel {
  pub title: String,
  pub link: String,
  pub description: String,
}

#[hdk_entry(id = "rss_item")]
#[derive(Debug, Clone)]
pub struct RssItem {
  pub title: Option<String>,
  pub link: Option<String>,
  pub description: Option<String>,
  pub author: Option<String>,
}

#[hdk_extern]
pub fn create_rss_channel(channel: RssChannel) -> ExternResult<()> {
  create_entry(&channel)?;
  let entry_hash = hash_entry(&channel)?;
  let path_hash = Path::from(RSS_CHANNELS_PATH).hash()?;
  create_link(path_hash, entry_hash.clone(), ())?;
  Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct FetchRssChannelsResponse(Vec<RssChannel>);

#[hdk_extern]
pub fn fetch_rss_channels(_: ()) -> ExternResult<FetchRssChannelsResponse> {
  let path_hash = Path::from(RSS_CHANNELS_PATH).hash()?;

  let links = get_links(path_hash, None)?;

  let channels: Vec<RssChannel> = get_app_entries(links);

  Ok(FetchRssChannelsResponse(channels))
}

pub fn get_app_entries<A: TryFrom<SerializedBytes, Error = SerializedBytesError>>(
  links: Links
) -> Vec<A> {
  links
    .into_inner()
    .into_iter()
    .map(|link: link::Link| get(link.target, GetOptions::default()))
    .filter_map(HdkResult::ok)
    .filter_map(|element| element)
    .map(|element| element.entry().to_app_option::<A>())
    .filter_map(Result::ok)
    .filter_map(|channel| channel)
    .collect::<Vec<A>>()
}
