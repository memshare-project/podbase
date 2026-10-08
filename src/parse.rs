//! 把各平台 RSS 收成同一套节目 / 单集。
//!
//! Anchor 用 CDATA 和 HTML；Megaphone 时长是秒数，封面在 `<image><url>`；
//! WordPress/PowerPress 常给 http 音频、空简介；Libsyn 时长是 `HH:MM:SS`；
//! NHK 用 `HH:MM:SS`；NHK Easy 没有 guid，简介只写在 `itunes:summary`。
//! 这些差异都在这里抹平，调用方只看到 [`ParsedPodcastFeed`]。

use chrono::{DateTime, SecondsFormat, Utc};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::reader::Reader;

pub const PODCAST_RSS_MAX_ITEMS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPodcastItem {
    pub guid: String,
    pub title: String,
    pub description: String,
    pub published_at: Option<String>,
    pub duration_sec: i32,
    pub audio_url: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPodcastFeed {
    pub title: String,
    pub author: String,
    pub description: String,
    pub artwork_url: Option<String>,
    pub items: Vec<ParsedPodcastItem>,
}

pub fn prefer_https(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        url.to_string()
    }
}

/// 与 memshare `episodeIdFor` 相同：djb2 变体，按 UTF-16 码元，结果取 uint32 十六进制。
pub fn episode_id_for(show_id: &str, guid: &str, audio_url: &str) -> String {
    let key = if guid.is_empty() { audio_url } else { guid };
    let mut hash: i32 = 5381;
    for unit in key.encode_utf16() {
        hash = hash.wrapping_mul(33) ^ i32::from(unit);
    }
    format!("{show_id}_{:x}", hash as u32)
}

pub fn parse_itunes_duration(raw: &str) -> i32 {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 0;
    }
    if is_plain_number(trimmed) {
        return trimmed.parse::<f64>().unwrap_or(0.0).round().max(0.0) as i32;
    }
    let parts: Vec<&str> = trimmed.split(':').collect();
    let nums: Result<Vec<f64>, _> = parts.iter().map(|part| part.parse::<f64>()).collect();
    let Ok(nums) = nums else {
        return 0;
    };
    let seconds = match nums.as_slice() {
        [hours, minutes, secs] => hours * 3600.0 + minutes * 60.0 + secs,
        [minutes, secs] => minutes * 60.0 + secs,
        _ => return 0,
    };
    seconds.round().max(0.0) as i32
}

pub fn parse_podcast_rss(xml: &str, max_items: usize) -> Result<ParsedPodcastFeed, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().allow_dangling_amp = true;

    let mut state = State::default();
    loop {
        let action = match reader.read_event() {
            Ok(Event::Start(start)) => Action::from_start(&start, false),
            Ok(Event::Empty(start)) => Action::from_start(&start, true),
            Ok(Event::End(end)) => Action::End(end.name().as_ref().to_string()),
            Ok(Event::Eof) => break,
            Ok(_) => Action::Ignore,
            Err(err) => return Err(format!("xml: {err}")),
        };

        match action {
            Action::Open(name)
                if name.eq_ignore_ascii_case("item") && state.items.len() >= max_items =>
            {
                reader
                    .read_text(QName(&name))
                    .map_err(|err| format!("xml: {err}"))?;
            }
            Action::Open(name) => {
                if name.eq_ignore_ascii_case("item") {
                    state.current = Some(ItemDraft::default());
                }
                state.stack.push(name);
            }
            Action::End(name) => {
                if name.eq_ignore_ascii_case("item") {
                    state.finish_item(max_items);
                }
                pop_name(&mut state.stack, &name);
            }
            Action::Leaf {
                name,
                attrs,
                end_name,
                is_empty,
            } => {
                if state.items.len() >= max_items && name.eq_ignore_ascii_case("item") {
                    continue;
                }
                let inner = if is_empty {
                    String::new()
                } else {
                    reader
                        .read_text(QName(end_name.as_str()))
                        .map_err(|err| format!("xml: {err}"))?
                        .into_inner()
                        .into_owned()
                };
                state.on_leaf(&name, &attrs, &inner, max_items);
            }
            Action::Ignore => {}
        }
    }

    if state
        .stack
        .iter()
        .any(|name| name.eq_ignore_ascii_case("item"))
    {
        state.finish_item(max_items);
    }

    let author = if !state.itunes_author.is_empty() {
        state.itunes_author
    } else {
        state.author
    };
    let description = if !state.itunes_summary.is_empty() {
        state.itunes_summary
    } else {
        state.description
    };

    Ok(ParsedPodcastFeed {
        title: state.title,
        author,
        description,
        artwork_url: state
            .itunes_image
            .or(state.image_url)
            .map(|url| prefer_https(&url)),
        items: state.items,
    })
}

#[derive(Default)]
struct State {
    stack: Vec<String>,
    title: String,
    author: String,
    itunes_author: String,
    description: String,
    itunes_summary: String,
    itunes_image: Option<String>,
    image_url: Option<String>,
    current: Option<ItemDraft>,
    items: Vec<ParsedPodcastItem>,
}

#[derive(Default)]
struct ItemDraft {
    title: String,
    description: String,
    summary: String,
    content: String,
    guid: String,
    pub_date: String,
    duration: String,
    audio_url: Option<String>,
    image_href: Option<String>,
    image_url: Option<String>,
}

#[derive(Default)]
struct LeafAttrs {
    href: Option<String>,
    url: Option<String>,
    typ: Option<String>,
}

enum Action {
    Open(String),
    End(String),
    Leaf {
        name: String,
        attrs: LeafAttrs,
        end_name: String,
        is_empty: bool,
    },
    Ignore,
}

impl Action {
    fn from_start(start: &BytesStart<'_>, is_empty: bool) -> Self {
        let name = start.name().as_ref().to_string();
        if is_container(&name) {
            if is_empty {
                Action::Ignore
            } else {
                Action::Open(name)
            }
        } else {
            Action::Leaf {
                attrs: leaf_attrs(start),
                end_name: name.clone(),
                name,
                is_empty,
            }
        }
    }
}

impl State {
    fn in_item(&self) -> bool {
        self.stack
            .iter()
            .any(|name| name.eq_ignore_ascii_case("item"))
    }

    fn in_image(&self) -> bool {
        self.stack
            .iter()
            .any(|name| name.eq_ignore_ascii_case("image"))
    }

    fn on_leaf(&mut self, name: &str, attrs: &LeafAttrs, inner: &str, max_items: usize) {
        if self.in_item() {
            if self.in_image() && !name.eq_ignore_ascii_case("url") {
                return;
            }
            let item = self.current.get_or_insert_with(ItemDraft::default);
            fill_item(item, name, attrs, inner);
            return;
        }
        if self.items.len() >= max_items {
            return;
        }
        self.fill_channel(name, attrs, inner);
    }

    fn fill_channel(&mut self, name: &str, attrs: &LeafAttrs, inner: &str) {
        if self.in_image() {
            if name.eq_ignore_ascii_case("url") && self.image_url.is_none() {
                let url = plain_text(inner);
                if !url.is_empty() {
                    self.image_url = Some(url);
                }
            }
            return;
        }

        if name.eq_ignore_ascii_case("title") && self.title.is_empty() {
            self.title = clean_text(inner);
        } else if name.eq_ignore_ascii_case("itunes:author") && self.itunes_author.is_empty() {
            self.itunes_author = clean_text(inner);
        } else if name.eq_ignore_ascii_case("author") && self.author.is_empty() {
            self.author = clean_text(inner);
        } else if name.eq_ignore_ascii_case("itunes:summary") && self.itunes_summary.is_empty() {
            self.itunes_summary = clean_text(inner);
        } else if name.eq_ignore_ascii_case("description") && self.description.is_empty() {
            self.description = clean_text(inner);
        } else if name.eq_ignore_ascii_case("itunes:image") && self.itunes_image.is_none() {
            self.itunes_image = attrs.href.clone().or_else(|| attrs.url.clone());
        }
    }

    fn finish_item(&mut self, max_items: usize) {
        let Some(item) = self.current.take() else {
            return;
        };
        if self.items.len() >= max_items {
            return;
        }
        let Some(audio_url) = item.audio_url else {
            return;
        };
        let audio_url = prefer_https(&audio_url);
        let guid = if item.guid.is_empty() {
            audio_url.clone()
        } else {
            item.guid
        };
        let description = first_non_empty([&item.description, &item.summary, &item.content]);
        let title = if item.title.is_empty() {
            "未命名单集".to_string()
        } else {
            item.title
        };
        self.items.push(ParsedPodcastItem {
            guid,
            title,
            description,
            published_at: parse_pub_date(&item.pub_date),
            duration_sec: parse_itunes_duration(&item.duration),
            audio_url,
            image_url: item
                .image_href
                .or(item.image_url)
                .map(|url| prefer_https(&url)),
        });
    }
}

fn fill_item(item: &mut ItemDraft, name: &str, attrs: &LeafAttrs, inner: &str) {
    if name.eq_ignore_ascii_case("title") && item.title.is_empty() {
        item.title = clean_text(inner);
    } else if name.eq_ignore_ascii_case("description") && item.description.is_empty() {
        item.description = clean_text(inner);
    } else if name.eq_ignore_ascii_case("itunes:summary") && item.summary.is_empty() {
        item.summary = clean_text(inner);
    } else if name.eq_ignore_ascii_case("content:encoded")
        && item.content.is_empty()
        && item.description.is_empty()
        && item.summary.is_empty()
    {
        item.content = clean_text(inner);
    } else if name.eq_ignore_ascii_case("guid") && item.guid.is_empty() {
        item.guid = plain_text(inner);
    } else if name.eq_ignore_ascii_case("pubdate") && item.pub_date.is_empty() {
        item.pub_date = plain_text(inner);
    } else if name.eq_ignore_ascii_case("itunes:duration") && item.duration.is_empty() {
        item.duration = plain_text(inner);
    } else if name.eq_ignore_ascii_case("enclosure") && item.audio_url.is_none() {
        if let Some(url) = attrs.url.clone() {
            let typ = attrs.typ.as_deref().unwrap_or("");
            if is_audio(&url, typ) {
                item.audio_url = Some(url);
            }
        }
    } else if name.eq_ignore_ascii_case("itunes:image") && item.image_href.is_none() {
        item.image_href = attrs.href.clone().or_else(|| attrs.url.clone());
    } else if name.eq_ignore_ascii_case("url") && item.image_url.is_none() {
        let url = plain_text(inner);
        if !url.is_empty() {
            item.image_url = Some(url);
        }
    }
}

fn is_container(name: &str) -> bool {
    name.eq_ignore_ascii_case("rss")
        || name.eq_ignore_ascii_case("channel")
        || name.eq_ignore_ascii_case("item")
        || name.eq_ignore_ascii_case("image")
}

fn pop_name(stack: &mut Vec<String>, name: &str) {
    if let Some(index) = stack.iter().rposition(|got| got.eq_ignore_ascii_case(name)) {
        stack.remove(index);
    }
}

fn leaf_attrs(start: &BytesStart<'_>) -> LeafAttrs {
    let mut attrs = LeafAttrs::default();
    for attr in start.attributes().flatten() {
        let key = attr.key.as_ref().to_ascii_lowercase();
        let Ok(value) = attr.normalized_value(XmlVersion::Implicit1_0) else {
            continue;
        };
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "href" => attrs.href = Some(value),
            "url" => attrs.url = Some(value),
            "type" => attrs.typ = Some(value),
            _ => {}
        }
    }
    attrs
}

fn is_audio(url: &str, typ: &str) -> bool {
    typ.to_ascii_lowercase().starts_with("audio") || audio_extension(url)
}

fn audio_extension(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "m4a" | "aac" | "ogg" | "wav"
    )
}

fn first_non_empty(parts: [&str; 3]) -> String {
    parts
        .into_iter()
        .find(|part| !part.is_empty())
        .unwrap_or("")
        .to_string()
}

fn is_plain_number(raw: &str) -> bool {
    let mut seen_dot = false;
    let mut seen_digit = false;
    for ch in raw.chars() {
        if ch.is_ascii_digit() {
            seen_digit = true;
        } else if ch == '.' && !seen_dot {
            seen_dot = true;
        } else {
            return false;
        }
    }
    seen_digit && !raw.ends_with('.')
}

pub fn parse_pub_date(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(format_utc(dt));
    }
    let normalized = normalize_rfc2822(trimmed);
    DateTime::parse_from_rfc2822(&normalized)
        .ok()
        .map(format_utc)
}

fn format_utc(dt: DateTime<chrono::FixedOffset>) -> String {
    dt.with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn normalize_rfc2822(raw: &str) -> String {
    let mut text = raw.replace("-0000", "+0000");
    if let Some(comma) = text.find(',') {
        let after = &text[comma + 1..];
        let trimmed = after.trim_start();
        let lead = after.len() - trimmed.len();
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_digit() && bytes[1] == b' ' {
            text.insert(comma + 1 + lead, '0');
        }
    }
    text
}

fn plain_text(raw: &str) -> String {
    decode_entities(&strip_cdata(raw)).trim().to_string()
}

fn clean_text(raw: &str) -> String {
    let once = collapse_ws(&decode_entities(&remove_tags(&strip_cdata(raw))));
    if once.contains('<') {
        collapse_ws(&decode_entities(&remove_tags(&once)))
    } else {
        once
    }
}

fn strip_cdata(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<![CDATA[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "<![CDATA[".len()..];
        if let Some(end) = after.find("]]>") {
            out.push_str(&after[..end]);
            rest = &after[end + 3..];
        } else {
            out.push_str(after);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn remove_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            for next in chars.by_ref() {
                if next == '>' {
                    break;
                }
            }
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn decode_entities(text: &str) -> String {
    let once = decode_entities_once(text);
    if once.contains('&') {
        decode_entities_once(&once)
    } else {
        once
    }
}

fn decode_entities_once(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        if let Some(end) = after.find(';')
            && let Some(decoded) = decode_entity(&after[..end])
        {
            out.push_str(&decoded);
            rest = &after[end + 1..];
            continue;
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<String> {
    if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        let code = u32::from_str_radix(hex, 16).ok()?;
        return char::from_u32(code).map(|ch| ch.to_string());
    }
    if let Some(dec) = entity.strip_prefix('#') {
        let code = dec.parse::<u32>().ok()?;
        return char::from_u32(code).map(|ch| ch.to_string());
    }
    match entity.to_ascii_lowercase().as_str() {
        "nbsp" => Some(" ".to_string()),
        "amp" => Some("&".to_string()),
        "lt" => Some("<".to_string()),
        "gt" => Some(">".to_string()),
        "quot" => Some("\"".to_string()),
        _ => None,
    }
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = false;
    for ch in text.chars() {
        let is_ws = matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000c}');
        if is_ws {
            if !out.is_empty() {
                pending = true;
            }
        } else {
            if pending {
                out.push(' ');
                pending = false;
            }
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title><![CDATA[日本語の会話のpodcast −ことのは−]]></title>
    <itunes:author>日本語の先生：ことのは</itunes:author>
    <description><![CDATA[<p>かんたんな日本語で話します。</p>]]></description>
    <itunes:image href="http://cdn.example.com/cover.jpg"/>
    <item>
      <title><![CDATA[Ep197　大学のサークル]]></title>
      <description><![CDATA[<p>日本の大学生の生活。</p>]]></description>
      <pubDate>Tue, 06 Oct 2026 12:00:00 GMT</pubDate>
      <guid isPermaLink="false">ep-197</guid>
      <enclosure url="https://cdn.example.com/ep197.m4a" length="100" type="audio/x-m4a" />
      <itunes:duration>1234</itunes:duration>
    </item>
    <item>
      <title>ニュース 午後09時00分</title>
      <enclosure url="https://www.nhk.or.jp/audio.mp3" length="2376720" type="audio/mp3" />
      <pubDate>Tue, 06 Oct 2026 21:05:00 +0900</pubDate>
      <guid isPermaLink="false">a2f177e9</guid>
      <itunes:duration>00:04:57</itunes:duration>
    </item>
    <item>
      <title>#1595 ありがとう</title>
      <guid isPermaLink="false">http://nihongoconteppei.com/?p=5331</guid>
      <enclosure url="http://media.blubrry.com/teppei.mp3" length="5242880" type="audio/mpeg" />
    </item>
    <item>
      <title>没有音频的条目</title>
      <guid>no-audio</guid>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parses_channel_and_audio_items() {
        let feed = parse_podcast_rss(FIXTURE, PODCAST_RSS_MAX_ITEMS).unwrap();
        assert!(feed.title.contains("ことのは"));
        assert_eq!(feed.author, "日本語の先生：ことのは");
        assert!(feed.description.contains("かんたんな日本語"));
        assert_eq!(
            feed.artwork_url.as_deref(),
            Some("https://cdn.example.com/cover.jpg")
        );
        assert_eq!(feed.items.len(), 3);
        assert_eq!(feed.items[0].guid, "ep-197");
        assert_eq!(feed.items[0].title, "Ep197　大学のサークル");
        assert_eq!(feed.items[0].duration_sec, 1234);
        assert_eq!(feed.items[0].audio_url, "https://cdn.example.com/ep197.m4a");
        assert_eq!(
            feed.items[0].published_at.as_deref(),
            Some("2026-10-06T12:00:00.000Z")
        );
        assert_eq!(feed.items[1].duration_sec, 4 * 60 + 57);
        assert_eq!(
            feed.items[1].published_at.as_deref(),
            Some("2026-10-06T12:05:00.000Z")
        );
        assert_eq!(
            feed.items[2].audio_url,
            "https://media.blubrry.com/teppei.mp3"
        );
    }

    #[test]
    fn respects_max_items() {
        let feed = parse_podcast_rss(FIXTURE, 1).unwrap();
        assert_eq!(feed.items.len(), 1);
    }

    #[test]
    fn parses_duration_shapes() {
        assert_eq!(parse_itunes_duration("90"), 90);
        assert_eq!(parse_itunes_duration("12:03"), 723);
        assert_eq!(parse_itunes_duration("1:02:03"), 3723);
        assert_eq!(parse_itunes_duration(""), 0);
    }

    #[test]
    fn episode_ids_match_memshare() {
        assert_eq!(
            episode_id_for("kotonoha", "ep-197", "https://x/a.m4a"),
            "kotonoha_45ad8f62"
        );
        assert_eq!(
            episode_id_for("nhk-news", "a2f177e9", "https://www.nhk.or.jp/audio.mp3"),
            "nhk-news_5e81d2bd"
        );
        assert_eq!(
            episode_id_for("teppei", "", "https://media.blubrry.com/teppei.mp3"),
            "teppei_849b8ee3"
        );
    }

    #[test]
    fn upgrades_http() {
        assert_eq!(
            prefer_https("http://a.example/x.mp3"),
            "https://a.example/x.mp3"
        );
        assert_eq!(
            prefer_https("https://a.example/x.mp3"),
            "https://a.example/x.mp3"
        );
    }

    #[test]
    fn normalizes_platform_shapes() {
        let xml = r#"<?xml version="1.0"?>
<rss>
  <channel>
    <title>Real Title</title>
    <itunes:summary>&lt;p&gt;slow summary&lt;/p&gt;</itunes:summary>
    <itunes:image href="https://cdn.example/cover.jpg"/>
    <image>
      <url>http://img.example/small.jpg</url>
      <title>Image Title</title>
    </image>
    <item>
      <enclosure url="https://www3.nhk.or.jp/lesson.mp3" type="audio/mpeg"/>
      <itunes:summary>Tam is looking for the share house.</itunes:summary>
      <itunes:duration>10:00</itunes:duration>
      <pubDate>Tue, 1 Oct 2019 11:00:05 +0900</pubDate>
    </item>
    <item>
      <title>秒数时长</title>
      <enclosure url="https://pdst.fm/e/traffic.megaphone.fm/TBS.mp3" type="audio/mpeg"/>
      <itunes:duration>538</itunes:duration>
      <pubDate>Wed, 07 Oct 2026 06:18:00 -0000</pubDate>
    </item>
  </channel>
</rss>"#;
        let feed = parse_podcast_rss(xml, 80).unwrap();
        assert_eq!(feed.title, "Real Title");
        assert_eq!(feed.description, "slow summary");
        assert_eq!(
            feed.artwork_url.as_deref(),
            Some("https://cdn.example/cover.jpg")
        );
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].guid, "https://www3.nhk.or.jp/lesson.mp3");
        assert_eq!(
            feed.items[0].description,
            "Tam is looking for the share house."
        );
        assert_eq!(feed.items[0].duration_sec, 600);
        assert_eq!(
            feed.items[0].published_at.as_deref(),
            Some("2019-10-01T02:00:05.000Z")
        );
        assert_eq!(feed.items[1].duration_sec, 538);
        assert_eq!(
            feed.items[1].published_at.as_deref(),
            Some("2026-10-07T06:18:00.000Z")
        );
    }
}
