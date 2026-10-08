use serde::{Deserialize, Serialize};

/// 策展难度。与 memshare `PodcastLevel` 一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Beginner,
    Advanced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub sort_order: i32,
}

/// 归一后的单集。各平台的 guid、时长、封面、简介都落到这些字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: String,
    pub show_id: String,
    pub number: i32,
    pub guid: String,
    pub title: String,
    pub description: String,
    pub audio_url: String,
    pub image_url: Option<String>,
    pub duration_sec: i32,
    /// RFC3339 UTC。没有可解析的 pubDate 时为 null。
    pub published_at: Option<String>,
}

/// 归一后的节目：策展字段 + RSS 刷新出的封面、简介和单集。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Show {
    pub id: String,
    pub category_id: String,
    pub rss_url: String,
    /// 来源平台，仅作标记。解析不按平台分叉。
    pub platform: String,
    pub title: String,
    pub author: String,
    pub cover_color: String,
    pub cover_label: String,
    pub cover_image_url: Option<String>,
    pub level: Level,
    pub tagline: String,
    pub description: String,
    pub episode_count: i32,
    pub sort_order: i32,
    pub last_synced_at: Option<String>,
    pub episodes: Vec<Episode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub language: String,
    pub synced_at: String,
    pub categories: Vec<Category>,
    pub shows: Vec<Show>,
}
