use std::sync::Arc;
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::catalog::{CATEGORIES, CategorySeed, SHOWS, ShowSeed, platform_of};
use crate::model::{Catalog, Category, Episode, Show};
use crate::parse::{PODCAST_RSS_MAX_ITEMS, ParsedPodcastFeed, episode_id_for, parse_podcast_rss};

const RSS_UA: &str = "PodbasePodcastSync/0.1";
const MAX_GUID: usize = 2048;
const MAX_TITLE: usize = 2000;
const MAX_ITEM_DESC: usize = 20_000;
const MAX_SHOW_DESC: usize = 20_000;
const MAX_AUTHOR: usize = 500;

pub struct SyncOptions {
    pub show_id: Option<String>,
    pub max_items: usize,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            show_id: None,
            max_items: PODCAST_RSS_MAX_ITEMS,
        }
    }
}

pub async fn sync_catalog(opts: SyncOptions) -> Result<Catalog, String> {
    let selected: Vec<ShowSeed> = SHOWS
        .iter()
        .copied()
        .filter(|show| match opts.show_id.as_deref() {
            Some(id) => show.id == id,
            None => true,
        })
        .collect();
    if let Some(id) = opts.show_id.as_deref()
        && selected.is_empty()
    {
        return Err(format!("unknown show {id}"));
    }

    let synced_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let client = reqwest::Client::builder()
        .user_agent(RSS_UA)
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|err| format!("http client: {err}"))?;

    let semaphore = Arc::new(Semaphore::new(6));
    let mut tasks = JoinSet::new();
    for (index, seed) in selected.into_iter().enumerate() {
        let client = client.clone();
        let semaphore = Arc::clone(&semaphore);
        let synced_at = synced_at.clone();
        let max_items = opts.max_items;
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.expect("semaphore open");
            let show = sync_one(&client, seed, max_items, &synced_at).await;
            (index, show)
        });
    }

    let mut shows = Vec::with_capacity(tasks.len());
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((index, show)) => {
                if let Some(err) = &show.error {
                    eprintln!("err {} {err}", show.id);
                } else {
                    eprintln!(
                        "ok  {} {} episodes={}",
                        show.id, show.platform, show.episode_count
                    );
                }
                shows.push((index, show));
            }
            Err(err) => return Err(format!("sync task: {err}")),
        }
    }
    shows.sort_by_key(|(index, _)| *index);

    Ok(Catalog {
        language: "ja".to_string(),
        synced_at,
        categories: CATEGORIES.iter().copied().map(category_from_seed).collect(),
        shows: shows.into_iter().map(|(_, show)| show).collect(),
    })
}

async fn sync_one(
    client: &reqwest::Client,
    seed: ShowSeed,
    max_items: usize,
    synced_at: &str,
) -> Show {
    match fetch_rss(client, seed.rss_url).await {
        Ok(xml) => match parse_podcast_rss(&xml, max_items) {
            Ok(feed) => assemble(seed, feed, synced_at),
            Err(err) => failed(seed, err),
        },
        Err(err) => failed(seed, err),
    }
}

async fn fetch_rss(client: &reqwest::Client, rss_url: &str) -> Result<String, String> {
    let response = client
        .get(rss_url)
        .header(
            reqwest::header::ACCEPT,
            "application/rss+xml, application/xml, text/xml, */*",
        )
        .send()
        .await
        .map_err(|err| format!("fetch: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("RSS HTTP {status}"));
    }
    let xml = response
        .text()
        .await
        .map_err(|err| format!("body: {err}"))?;
    if !xml.contains("<rss")
        && !xml.contains("<RSS")
        && !xml.contains("<item")
        && !xml.contains("<ITEM")
    {
        return Err("RSS body empty or invalid".to_string());
    }
    Ok(xml)
}

fn assemble(seed: ShowSeed, feed: ParsedPodcastFeed, synced_at: &str) -> Show {
    let total = feed.items.len() as i32;
    let episodes = feed
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let image_url = item.image_url.clone().or_else(|| feed.artwork_url.clone());
            Episode {
                id: episode_id_for(seed.id, &item.guid, &item.audio_url),
                show_id: seed.id.to_string(),
                number: total - index as i32,
                guid: limit_chars(&item.guid, MAX_GUID),
                title: limit_chars(&item.title, MAX_TITLE),
                description: limit_chars(&item.description, MAX_ITEM_DESC),
                audio_url: item.audio_url.clone(),
                image_url,
                duration_sec: item.duration_sec,
                published_at: item.published_at.clone(),
            }
        })
        .collect::<Vec<_>>();
    let episode_count = episodes.len() as i32;

    Show {
        id: seed.id.to_string(),
        category_id: seed.category_id.to_string(),
        rss_url: seed.rss_url.to_string(),
        platform: platform_of(seed.rss_url).to_string(),
        title: if seed.title.is_empty() {
            feed.title
        } else {
            seed.title.to_string()
        },
        author: if seed.author.is_empty() {
            limit_chars(&feed.author, MAX_AUTHOR)
        } else {
            seed.author.to_string()
        },
        cover_color: seed.cover_color.to_string(),
        cover_label: seed.cover_label.to_string(),
        cover_image_url: feed.artwork_url,
        level: seed.level,
        tagline: seed.tagline.to_string(),
        description: if seed.description.is_empty() {
            limit_chars(&feed.description, MAX_SHOW_DESC)
        } else {
            seed.description.to_string()
        },
        episode_count,
        sort_order: seed.sort_order,
        last_synced_at: Some(synced_at.to_string()),
        episodes,
        error: None,
    }
}

fn failed(seed: ShowSeed, err: String) -> Show {
    Show {
        id: seed.id.to_string(),
        category_id: seed.category_id.to_string(),
        rss_url: seed.rss_url.to_string(),
        platform: platform_of(seed.rss_url).to_string(),
        title: seed.title.to_string(),
        author: seed.author.to_string(),
        cover_color: seed.cover_color.to_string(),
        cover_label: seed.cover_label.to_string(),
        cover_image_url: None,
        level: seed.level,
        tagline: seed.tagline.to_string(),
        description: seed.description.to_string(),
        episode_count: 0,
        sort_order: seed.sort_order,
        last_synced_at: None,
        episodes: Vec::new(),
        error: Some(err),
    }
}

fn category_from_seed(seed: CategorySeed) -> Category {
    Category {
        id: seed.id.to_string(),
        title: seed.title.to_string(),
        subtitle: seed.subtitle.to_string(),
        sort_order: seed.sort_order,
    }
}

fn limit_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect()
    }
}
