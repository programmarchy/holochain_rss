#![allow(unused_imports)]
#![allow(dead_code)]

mod error;

use crate::{
  error::{RssError, RssResult}
};
use hdk3::prelude::*;
use link::Link;

pub const RSS_CHANNELS_PATH: &str = "rss_channels";
const RSS_HEAD_ITEM_TAG: &'static [u8; 4] = b"head";
const RSS_ITEM_TAG: &'static [u8; 4] = b"item";

#[hdk_extern]
fn init(_: ()) -> RssResult<InitCallbackResult> {
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
  pub uuid: String,
  pub title: String,
  pub link: String,
  pub description: String,
}

#[hdk_entry(id = "rss_item")]
#[derive(Debug, Clone)]
pub struct RssItem {
  pub uuid: String,
  pub title: Option<String>,
  pub link: Option<String>,
  pub description: Option<String>,
  pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct CreateRssChannelRequest(RssChannel);

#[hdk_extern]
pub fn create_rss_channel(request: CreateRssChannelRequest) -> RssResult<()> {
  let CreateRssChannelRequest(channel) = request;
  create_entry(&channel)?;
  let channel_hash = hash_entry(&channel)?;
  let path_hash = Path::from(RSS_CHANNELS_PATH).hash()?;
  create_link(path_hash, channel_hash.clone(), ())?;
  Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct CreateRssItemRequest(RssItem, EntryHash);

#[hdk_extern]
pub fn create_rss_item(request: CreateRssItemRequest) -> RssResult<()> {
  let CreateRssItemRequest(item, channel_hash) = request;
  create_entry(&item)?;
  let item_hash = hash_entry(&item)?;
  update_rss_item_links(channel_hash, item_hash)?;
  Ok(())
}

fn rss_item_tag() -> LinkTag {
  LinkTag::new(*RSS_ITEM_TAG)
}

fn rss_head_item_tag() -> LinkTag {
  LinkTag::new(*RSS_HEAD_ITEM_TAG)
}

fn update_rss_item_links(channel_hash: EntryHash, item_hash: EntryHash) -> RssResult<()> {
  // Create a link from the channel to each item
  create_link(channel_hash.clone(), item_hash.clone(), rss_item_tag())?;

  // Create a link to the channel's head item, and chain items together
  match get_rss_head_item_link(channel_hash.clone())? {
    Some(previous_link) => {
      let previous_item_hash = previous_link.target;
      create_link(item_hash.clone(), previous_item_hash.clone(), ())?;
      create_link(channel_hash.clone(), item_hash.clone(), rss_head_item_tag())?;
      delete_link(previous_link.create_link_hash)?;
      Ok(())
    },
    None => {
      create_link(channel_hash.clone(), item_hash.clone(), rss_head_item_tag())?;
      Ok(())
    }
  }
}

fn get_rss_head_item_link(channel_hash: EntryHash) -> RssResult<Option<Link>> {
  let links = get_links(
    channel_hash, 
    Some(rss_head_item_tag()),
  )?;

  let first_link = links
    .into_inner()
    .first()
    .map(|link| link.clone());
  
  Ok(first_link)
}

#[derive(Debug, Clone, Serialize, Deserialize, SerializedBytes)]
pub struct FetchRssChannelsResponse(Vec<RssChannel>);

#[hdk_extern]
pub fn fetch_rss_channels(_: ()) -> RssResult<FetchRssChannelsResponse> {
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
