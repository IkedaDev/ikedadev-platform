use crate::domain::models::ArticleSeo;
use ammonia::Builder;
use ikeda_core::error::AppError;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

/// Maximum serialized byte size for `content_json` (512 KiB).
pub const MAX_CONTENT_JSON_BYTES: usize = 512 * 1024;
/// Maximum serialized byte size for `content_html` (512 KiB).
pub const MAX_CONTENT_HTML_BYTES: usize = 512 * 1024;
/// Maximum serialized byte size for `seo` JSONB (16 KiB).
pub const MAX_SEO_JSON_BYTES: usize = 16 * 1024;
/// Maximum serialized byte size for `metadata` JSONB (32 KiB).
pub const MAX_METADATA_JSON_BYTES: usize = 32 * 1024;
/// Maximum media upload size (5 MiB).
pub const MAX_MEDIA_SIZE_BYTES: i64 = 5 * 1024 * 1024;
/// Allowed image MIME types for presigned uploads.
pub const ALLOWED_MEDIA_MIME_TYPES: &[&str] =
    &["image/webp", "image/jpeg", "image/png", "image/avif"];
/// Supported Deploy Hook event strings.
pub const ALLOWED_WEBHOOK_EVENTS: &[&str] = &[
    "article.published",
    "article.updated",
    "article.unpublished",
];

/// Normalizes any string into a lowercase ASCII kebab-case slug.
#[must_use]
pub fn normalize_slug(input: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_len));
    let mut prev_dash = true;

    for ch in input.trim().chars() {
        let mapped = match ch {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => Some('a'),
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => Some('e'),
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => Some('i'),
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => Some('o'),
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => Some('u'),
            'ñ' | 'Ñ' => Some('n'),
            'ç' | 'Ç' => Some('c'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        };

        if let Some(c) = mapped {
            if out.len() >= max_len {
                break;
            }
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && out.len() < max_len {
            out.push('-');
            prev_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    out
}

/// Sanitizes editorial HTML using `ammonia` with a strict whitelist of editorial tags and attributes.
#[must_use]
pub fn sanitize_html(raw_html: &str) -> String {
    let tags: HashSet<&str> = [
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "p",
        "br",
        "strong",
        "b",
        "em",
        "i",
        "u",
        "s",
        "del",
        "ul",
        "ol",
        "li",
        "blockquote",
        "pre",
        "code",
        "a",
        "img",
        "hr",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
    ]
    .into_iter()
    .collect();

    let mut tag_attributes: HashMap<&str, HashSet<&str>> = HashMap::new();
    tag_attributes.insert("a", ["href", "title", "target"].into_iter().collect());
    tag_attributes.insert(
        "img",
        ["src", "alt", "title", "width", "height"]
            .into_iter()
            .collect(),
    );
    tag_attributes.insert("code", ["class"].into_iter().collect());
    tag_attributes.insert("th", ["colspan", "rowspan"].into_iter().collect());
    tag_attributes.insert("td", ["colspan", "rowspan"].into_iter().collect());

    let url_schemes: HashSet<&str> = ["https", "http", "mailto"].into_iter().collect();

    let mut builder = Builder::empty();
    builder
        .tags(tags)
        .tag_attributes(tag_attributes)
        .url_schemes(url_schemes)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true);

    builder.clean(raw_html).to_string()
}

/// Strips HTML tags to extract plain text for word count and reading time calculations.
#[must_use]
pub fn strip_html_tags(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut inside_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                inside_tag = true;
                text.push(' ');
            }
            '>' => {
                inside_tag = false;
                text.push(' ');
            }
            c if !inside_tag => text.push(c),
            _ => {}
        }
    }
    text
}

/// Computes `(word_count, reading_time_minutes)` at 200 words per minute (minimum 1 minute).
#[must_use]
pub fn compute_reading_metrics(sanitized_html: &str) -> (i32, i32) {
    let plain = strip_html_tags(sanitized_html);
    let words = plain.split_whitespace().count();
    let word_count = i32::try_from(words).unwrap_or(i32::MAX);
    let reading_time = ((words.saturating_add(199)) / 200).max(1);
    let reading_time_minutes = i32::try_from(reading_time).unwrap_or(i32::MAX);
    (word_count, reading_time_minutes)
}

/// Validates an external media/SEO URL. Allows `https://` always, and `http://` only when `allow_http` is true.
pub fn validate_public_url(
    raw: &str,
    field_name: &str,
    allow_http: bool,
) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!(
            "{field_name} cannot be empty"
        )));
    }
    if trimmed.len() > 2048 {
        return Err(AppError::BadRequest(format!(
            "{field_name} exceeds maximum length of 2048 characters"
        )));
    }
    let parsed = Url::parse(trimmed)
        .map_err(|_| AppError::BadRequest(format!("{field_name} is not a valid URL")))?;

    match parsed.scheme() {
        "https" => {}
        "http" if allow_http => {}
        other => {
            return Err(AppError::BadRequest(format!(
                "{field_name} scheme '{other}' is not allowed; use https://"
            )));
        }
    }

    if parsed.host_str().is_none() {
        return Err(AppError::BadRequest(format!(
            "{field_name} must include a valid host"
        )));
    }

    Ok(trimmed.to_string())
}

/// Validates all URL fields inside `ArticleSeo`.
pub fn validate_seo(seo: &ArticleSeo, allow_http: bool) -> Result<(), AppError> {
    if let Some(ref url) = seo.canonical_url {
        if !url.trim().is_empty() {
            validate_public_url(url, "seo.canonical_url", allow_http)?;
        }
    }
    if let Some(ref url) = seo.og_image_url {
        if !url.trim().is_empty() {
            validate_public_url(url, "seo.og_image_url", allow_http)?;
        }
    }
    let serialized_len = serde_json::to_vec(seo)
        .map_err(|e| AppError::BadRequest(format!("Invalid SEO payload: {e}")))?
        .len();
    if serialized_len > MAX_SEO_JSON_BYTES {
        return Err(AppError::BadRequest(format!(
            "seo payload exceeds maximum size of {MAX_SEO_JSON_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Validates serialized JSONB payload size against `max_bytes`.
pub fn validate_json_size(
    value: &serde_json::Value,
    field_name: &str,
    max_bytes: usize,
) -> Result<(), AppError> {
    let len = serde_json::to_vec(value)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON in {field_name}: {e}")))?
        .len();
    if len > max_bytes {
        return Err(AppError::BadRequest(format!(
            "{field_name} exceeds maximum size of {max_bytes} bytes"
        )));
    }
    Ok(())
}

/// Checks if an IPv4 address is private, loopback, link-local, metadata, or otherwise non-public.
#[must_use]
fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback() // 127.0.0.0/8
        || ip.is_private() // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local() // 169.254.0.0/16 (includes 169.254.169.254 GCP/AWS metadata)
        || ip.is_unspecified() // 0.0.0.0
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64) // 100.64.0.0/10 CGNAT
        || octets[0] == 0 // 0.0.0.0/8
}

/// Checks if an IPv6 address is loopback, unique-local, link-local, or mapped to a blocked IPv4.
#[must_use]
fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_ipv4(v4);
    }
    let seg0 = ip.segments()[0];
    ip.is_loopback() // ::1
        || ip.is_unspecified() // ::
        || ip.is_multicast()
        || (seg0 & 0xfe00) == 0xfc00 // fc00::/7 Unique Local (ULA)
        || (seg0 & 0xffc0) == 0xfe80 // fe80::/10 Link-Local
}

/// Checks if an IP address is blocked by SSRF policy.
#[must_use]
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

/// Strict Anti-SSRF validation for SSG Deploy Hook target URLs.
/// Requires `https://`, rejects credentials in URL, blocks `localhost`, `.local`, `.internal`,
/// metadata hostnames, and private/loopback/link-local IPv4 and IPv6 addresses.
pub fn validate_webhook_target_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return Err(AppError::BadRequest(
            "target_url must be a valid HTTPS URL up to 2048 characters".to_string(),
        ));
    }

    let parsed = Url::parse(trimmed)
        .map_err(|_| AppError::BadRequest("target_url is not a valid URL".to_string()))?;

    if parsed.scheme() != "https" {
        return Err(AppError::BadRequest(
            "Deploy Hook target_url must use https://".to_string(),
        ));
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::BadRequest(
            "Deploy Hook target_url must not contain embedded credentials".to_string(),
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::BadRequest("target_url must include a host".to_string()))?;

    let host_lower = host.trim_end_matches('.').to_ascii_lowercase();
    if host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".internal")
        || host_lower == "metadata.google.internal"
        || host_lower == "metadata"
    {
        return Err(AppError::BadRequest(format!(
            "SSRF protection blocked disallowed host: '{host}'"
        )));
    }

    // Strip brackets from IPv6 literals if present
    let host_unbracketed = host_lower
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(&host_lower);

    if let Ok(ip) = host_unbracketed.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(AppError::BadRequest(format!(
                "SSRF protection blocked non-public IP address: '{ip}'"
            )));
        }
    }

    Ok(trimmed.to_string())
}

/// Validates webhook event names against `ALLOWED_WEBHOOK_EVENTS`.
pub fn validate_webhook_events(events: &[String]) -> Result<Vec<String>, AppError> {
    if events.is_empty() {
        return Err(AppError::BadRequest(
            "At least one webhook event must be specified".to_string(),
        ));
    }
    let mut normalized = Vec::with_capacity(events.len());
    for event in events {
        let e = event.trim();
        if !ALLOWED_WEBHOOK_EVENTS.contains(&e) {
            return Err(AppError::BadRequest(format!(
                "Unsupported webhook event: '{e}'"
            )));
        }
        if !normalized.iter().any(|existing: &String| existing == e) {
            normalized.push(e.to_string());
        }
    }
    Ok(normalized)
}

/// Maps an allowed image MIME type to its canonical file extension.
pub fn extension_for_mime(mime_type: &str) -> Result<&'static str, AppError> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/webp" => Ok("webp"),
        "image/jpeg" => Ok("jpg"),
        "image/png" => Ok("png"),
        "image/avif" => Ok("avif"),
        other => Err(AppError::BadRequest(format!(
            "Unsupported media MIME type '{other}'. Allowed: image/webp, image/jpeg, image/png, image/avif"
        ))),
    }
}
