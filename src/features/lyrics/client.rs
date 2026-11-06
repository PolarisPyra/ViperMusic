use crate::features::library::Track;
use serde::Deserialize;
use std::{sync::OnceLock, time::Duration};

static HTTP_CLIENT: OnceLock<ureq::Agent> = OnceLock::new();

#[derive(Clone, Debug)]
/// One displayable lyric line, optionally tied to a playback timestamp.
pub(crate) struct LyricLine {
    /// Timestamp from the LRC source, or `None` for plain lyrics.
    pub(crate) timestamp: Option<Duration>,
    /// Text shown in the lyrics panel.
    pub(crate) text: String,
}

#[derive(Clone, Debug)]
/// Lyrics fetched for a track from the configured source.
pub(crate) struct Lyrics {
    /// Lyrics lines in playback order.
    pub(crate) lines: Vec<LyricLine>,
    /// Whether the source provided synchronized lyrics.
    pub(crate) synced: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibResponse {
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

/// Fetch the best available synced or plain lyrics for a track from LRCLIB.
///
/// # Arguments
/// * `track` - Track metadata used for an exact title, artist, album, and duration match.
///
/// # Returns
/// The lyrics when found, `None` when the service has no match, or an error message if lookup
/// fails.
pub(super) fn fetch(track: &Track) -> Result<Option<Lyrics>, String> {
    if track.title.trim().is_empty() || track.artist.trim().is_empty() {
        return Ok(None);
    }
    let client = HTTP_CLIENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(6)))
            .user_agent(concat!("Viper/", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .build()
            .into()
    });
    let mut query = vec![
        ("track_name", track.title.as_str()),
        ("artist_name", track.artist.as_str()),
    ];
    if !track.album.trim().is_empty() {
        query.push(("album_name", track.album.as_str()));
    }
    let duration_seconds = track
        .duration_ms
        .map(|duration| (duration as f64 / 1_000.0).to_string());
    if let Some(duration) = duration_seconds.as_deref() {
        query.push(("duration", duration));
    }
    let mut response = client
        .get("https://lrclib.net/api/get")
        .query_pairs(query)
        .call()
        .map_err(|error| format!("Could not fetch lyrics: {error}"))?;
    let status = response.status();
    if status == ureq::http::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!("Lyrics service returned an error: HTTP {status}"));
    }
    let response: LrclibResponse = response
        .body_mut()
        .read_json()
        .map_err(|error| format!("Could not read lyrics response: {error}"))?;
    if response.instrumental == Some(true) {
        return Ok(None);
    }
    if let Some(text) = response
        .synced_lyrics
        .filter(|text| !text.trim().is_empty())
    {
        let lines = parse_synced(&text);
        if !lines.is_empty() {
            return Ok(Some(Lyrics {
                lines,
                synced: true,
            }));
        }
    }
    Ok(response
        .plain_lyrics
        .filter(|text| !text.trim().is_empty())
        .map(|text| Lyrics {
            lines: text
                .lines()
                .map(|line| LyricLine {
                    timestamp: None,
                    text: line.to_owned(),
                })
                .collect(),
            synced: false,
        }))
}

fn parse_synced(lyrics: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for line in lyrics.lines() {
        let mut rest = line;
        let mut timestamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some((stamp, tail)) = stripped.split_once(']') else {
                break;
            };
            let Some(timestamp) = parse_timestamp(stamp) else {
                break;
            };
            timestamps.push(timestamp);
            rest = tail;
        }
        let text = rest.trim();
        if text.is_empty() {
            continue;
        }
        lines.extend(timestamps.into_iter().map(|timestamp| LyricLine {
            timestamp: Some(timestamp),
            text: text.to_owned(),
        }));
    }
    lines.sort_unstable_by_key(|line| line.timestamp);
    lines
}

fn parse_timestamp(timestamp: &str) -> Option<Duration> {
    let (minutes, seconds) = timestamp.split_once(':')?;
    let (seconds, fraction) = seconds
        .split_once('.')
        .map_or((seconds, "0"), |(seconds, fraction)| (seconds, fraction));
    let minutes = minutes.parse::<u64>().ok()?;
    let seconds = seconds.parse::<u64>().ok()?;
    if seconds >= 60 {
        return None;
    }
    let millis = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()?.checked_mul(100)?,
        2 => fraction.parse::<u64>().ok()?.checked_mul(10)?,
        _ => fraction.get(..3)?.parse::<u64>().ok()?,
    };
    Some(Duration::from_millis(
        minutes
            .checked_mul(60_000)?
            .checked_add(seconds.checked_mul(1_000)?)?
            .checked_add(millis)?,
    ))
}
