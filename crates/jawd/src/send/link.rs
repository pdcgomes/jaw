//! Link previews for what you send. The first URL in the text has its page fetched in the
//! interactive lane, and the page's title, description and image, made a small thumbnail, go with
//! the text as WhatsApp's extended text, as the phones send a preview; the text goes without one if
//! the page doesn't answer within WITHIN, isn't HTML, or can't be reached. Only the small thumbnail
//! the message carries goes: a high-quality one needs a media upload, which comes with sending
//! media. jawd never fetches a page for a message it receives, whose preview comes in it.
//!
//! The fetch is bounded: at most REDIRECTS_MOST redirects, PAGE_MOST of a page and IMAGE_MOST of an
//! image, HTML only for the page and an image for the thumbnail, within WITHIN for both, no cookies
//! sent or kept, and an honest user agent. Every address it's about to connect to, the first and
//! each redirect's, is refused on loopback, link-local or a private range, and so is the address
//! the connection reached once it's made; it connects straight to the page, never through a proxy,
//! which would reach addresses jawd can't see.

use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Empty};
use hyper::body::{Bytes, Incoming};
use hyper::header::{ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST, LOCATION, USER_AGENT};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::Instant;
use url::{Host, Position, Url};
use wa_proto::wa_web_protobufs_e2e as e2e;

use crate::convert::spans;
use crate::store::thumbnails;

/// How long a preview has, its page and its image together.
pub const WITHIN: Duration = Duration::from_secs(3);
/// The most redirects a fetch follows.
const REDIRECTS_MOST: usize = 5;
/// The most of a page read: what a page says about itself comes at its top.
const PAGE_MOST: usize = 512 * 1024;
/// The largest image made a thumbnail.
const IMAGE_MOST: usize = 2 * 1024 * 1024;
/// The largest image decoded, on either side, and the most it may take decoded.
const IMAGE_SIDE_MOST: u32 = 8192;
const IMAGE_DECODED_MOST: u64 = 128 * 1024 * 1024;
/// The thumbnail's longest side and its JPEG quality: Baileys, a WhatsApp Web library, sends 32
/// pixels wide; a link preview is drawn larger than a photo's, so this is an estimate.
const THUMBNAIL_SIDE: u32 = 140;
const THUMBNAIL_QUALITY: u8 = 70;
/// The longest title and description kept, in characters.
const TITLE_MOST: usize = 300;
const DESCRIPTION_MOST: usize = 600;
/// What jawd says it is when it fetches a page.
const AGENT: &str = concat!("jaw/", env!("CARGO_PKG_VERSION"), " (link preview; https://github.com/pdcgomes/jaw)");

/// What makes the previews of what's sent: a fetcher, or none, which sends text as it is.
#[derive(Clone)]
pub struct Previews {
    fetcher: Option<Arc<Fetcher>>,
    within: Duration,
}

/// A page's preview, as it goes with the text.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    /// The URL as the text has it, which WhatsApp's `matchedText` names.
    pub matched: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<Thumbnail>,
}

/// A small JPEG of the page's image, and its size in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Thumbnail {
    pub jpeg: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

impl Default for Previews {
    /// Previews fetched from the pages themselves, at public addresses only.
    fn default() -> Previews {
        Previews { fetcher: Some(Arc::new(Fetcher::new(Policy { loopback: false }))), within: WITHIN }
    }
}

impl Previews {
    /// No previews: text goes as it is.
    pub fn off() -> Previews {
        Previews { fetcher: None, within: WITHIN }
    }

    /// Previews from servers on this machine's loopback as well, within `within`, for tests' own
    /// servers; every other address is refused as it always is.
    #[cfg(test)]
    pub fn local(within: Duration) -> Previews {
        Previews { fetcher: Some(Arc::new(Fetcher::new(Policy { loopback: true }))), within }
    }

    /// The preview of the first URL in `text`, if it has one and its page answers in time.
    pub async fn of(&self, text: &str) -> Option<Preview> {
        let fetcher = self.fetcher.as_ref()?;
        let link = spans::parse(text, &[]).into_iter().find(|span| span.t == "link")?;
        let url = Url::parse(link.url.as_deref()?).ok()?;
        let deadline = Instant::now() + self.within;
        let page = match tokio::time::timeout_at(deadline, fetcher.get(url, Wanted::Page)).await {
            Ok(Ok(page)) => page,
            Ok(Err(why)) => {
                eprintln!("jawd: no link preview for a message sent: {why}");
                return None;
            }
            Err(_) => {
                eprintln!("jawd: no link preview for a message sent: the page didn't answer within {:?}", self.within);
                return None;
            }
        };
        let meta = meta(&page.text());
        if meta.title.is_none() && meta.description.is_none() {
            return None;
        }
        // The image is decoded within the same deadline: one as large as the caps let through takes
        // a few hundred milliseconds, which the text would otherwise wait for past it.
        let thumbnail = match meta.image.as_deref().and_then(|image| page.url.join(image).ok()) {
            Some(image) => tokio::time::timeout_at(deadline, async {
                let image = fetcher.get(image, Wanted::Image).await.ok()?;
                tokio::task::spawn_blocking(move || thumbnail(&image.body)).await.ok().flatten()
            })
            .await
            .ok()
            .flatten(),
            None => None,
        };
        Some(Preview { matched: link.s, title: meta.title, description: meta.description, thumbnail })
    }
}

/// `message`, a text message as it's sent, with `preview`, as WhatsApp's phones send a link preview:
/// extended text with the URL it's for, the title and description, and the small thumbnail.
pub fn with_preview(mut message: e2e::Message, preview: &Preview) -> e2e::Message {
    let mut extended = message.extended_text_message.take().map(|x| *x).unwrap_or_default();
    if let Some(text) = message.conversation.take() {
        extended.text = Some(text);
    }
    extended.matched_text = Some(preview.matched.clone());
    extended.title = preview.title.clone();
    extended.description = preview.description.clone();
    extended.preview_type = Some(e2e::extended_text_message::PreviewType::None as i32);
    if let Some(thumbnail) = &preview.thumbnail {
        extended.jpeg_thumbnail = Some(thumbnail.jpeg.clone());
        extended.thumbnail_width = Some(thumbnail.w);
        extended.thumbnail_height = Some(thumbnail.h);
    }
    message.extended_text_message = Some(Box::new(extended));
    message
}

/// Which addresses a fetch may connect to: public ones, and loopback only for tests' servers.
#[derive(Clone, Copy, Debug)]
pub(super) struct Policy {
    loopback: bool,
}

impl Policy {
    /// Whether `ip` is an address a page may be fetched from: not on loopback (unless tests allow
    /// it), link-local or a private range, nor unspecified, multicast, broadcast or reserved, nor
    /// an IPv6 address carrying an IPv4 one that isn't allowed itself.
    pub(super) fn allows(&self, ip: IpAddr) -> bool {
        match ip.to_canonical() {
            IpAddr::V4(ip) => self.allows_v4(ip),
            IpAddr::V6(ip) => {
                let s = ip.segments();
                let carried = |high: u16, low: u16| Ipv4Addr::from((u32::from(high) << 16) | u32::from(low));
                if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
                    return self.allows_v4(carried(s[6], s[7]));
                }
                if s[0] == 0x2002 {
                    return self.allows_v4(carried(s[1], s[2]));
                }
                if ip.is_loopback() {
                    return self.loopback;
                }
                let unique_local = s[0] & 0xfe00 == 0xfc00;
                let link_local = s[0] & 0xffc0 == 0xfe80;
                let site_local = s[0] & 0xffc0 == 0xfec0;
                !(ip.is_unspecified() || ip.is_multicast() || unique_local || link_local || site_local)
            }
        }
    }

    fn allows_v4(&self, ip: Ipv4Addr) -> bool {
        if ip.is_loopback() {
            return self.loopback;
        }
        let o = ip.octets();
        let shared = o[0] == 100 && o[1] & 0xc0 == 64;
        let assignments = o[0] == 192 && o[1] == 0 && o[2] == 0;
        !(ip.is_private()
            || ip.is_link_local()
            || ip.is_unspecified()
            || ip.is_broadcast()
            || ip.is_multicast()
            || o[0] == 0
            || o[0] >= 240
            || shared
            || assignments)
    }
}

/// What a fetch is for, which says what it accepts and how much it reads.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Wanted {
    /// HTML, of which the first PAGE_MOST is read, however long it is.
    Page,
    /// An image, refused if it's larger than IMAGE_MOST.
    Image,
}

impl Wanted {
    fn accept(self) -> &'static str {
        match self {
            Wanted::Page => "text/html,application/xhtml+xml;q=0.9",
            Wanted::Image => "image/jpeg,image/png,image/webp;q=0.9",
        }
    }

    fn takes(self, content_type: &str) -> bool {
        match self {
            Wanted::Page => content_type.starts_with("text/html") || content_type.starts_with("application/xhtml+xml"),
            Wanted::Image => {
                ["image/jpeg", "image/jpg", "image/png", "image/webp"].iter().any(|t| content_type.starts_with(t))
            }
        }
    }
}

/// What a fetch got: where it ended up after redirects, its body and its content type.
struct Fetched {
    url: Url,
    body: Vec<u8>,
    content_type: String,
}

impl Fetched {
    /// The page's text, in the charset it says, UTF-8 unless it says Latin-1 or Windows-1252.
    fn text(&self) -> String {
        let head = String::from_utf8_lossy(&self.body[..self.body.len().min(2048)]).to_ascii_lowercase();
        let said = |charset: &str| {
            self.content_type.contains(charset)
                || head.contains(&format!("charset={charset}"))
                || head.contains(&format!("charset=\"{charset}"))
        };
        if ["iso-8859-1", "latin1", "windows-1252"].iter().any(|charset| said(charset)) {
            self.body.iter().map(|&b| b as char).collect()
        } else {
            String::from_utf8_lossy(&self.body).into_owned()
        }
    }
}

/// Why there's no preview, to say in jawd's log.
#[derive(Debug)]
struct Refused(String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn refused(why: impl Into<String>) -> Refused {
    Refused(why.into())
}

/// A stream a request goes over, plain or with TLS.
trait Io: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

pub(super) struct Fetcher {
    tls: tokio_rustls::TlsConnector,
    policy: Policy,
}

impl Fetcher {
    pub(super) fn new(policy: Policy) -> Fetcher {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("ring offers TLS 1.2 and 1.3")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Fetcher { tls: tokio_rustls::TlsConnector::from(Arc::new(config)), policy }
    }

    /// Fetches `url` for `wanted`, following redirects, each checked as the first is. Why it
    /// couldn't names the page by its host alone, since a URL's path can carry what's private.
    async fn get(&self, mut url: Url, wanted: Wanted) -> Result<Fetched, Refused> {
        for _ in 0..=REDIRECTS_MOST {
            let response = self.request(&url, wanted).await?;
            let status = response.status();
            let site = url.host_str().unwrap_or_default().to_string();
            if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|location| location.to_str().ok())
                    .ok_or_else(|| refused(format!("{site} redirects nowhere")))?;
                url = url.join(location).map_err(|e| refused(format!("{site} redirects nowhere it can: {e}")))?;
                continue;
            }
            if status != StatusCode::OK {
                return Err(refused(format!("{site} answered {status}")));
            }
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !wanted.takes(&content_type) {
                return Err(refused(format!("{site} gave {content_type:?}, not what a preview takes")));
            }
            let length = response.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
            if wanted == Wanted::Image && length.is_some_and(|length| length > IMAGE_MOST as u64) {
                return Err(refused(format!("{site}'s image is too large for a thumbnail")));
            }
            let body = read(response.into_body(), wanted).await.map_err(|why| refused(format!("{site}: {why}")))?;
            return Ok(Fetched { url, body, content_type });
        }
        Err(refused(format!("{} redirects more than {REDIRECTS_MOST} times", url.host_str().unwrap_or_default())))
    }

    /// Asks `url` for itself, over a connection to an address the policy allows.
    async fn request(&self, url: &Url, wanted: Wanted) -> Result<Response<Incoming>, Refused> {
        let https = match url.scheme() {
            "https" => true,
            "http" => false,
            other => return Err(refused(format!("a page on {other} isn't on the web"))),
        };
        let host = url.host().ok_or_else(|| refused("a page with no host"))?;
        let port = url.port_or_known_default().ok_or_else(|| refused(format!("{host} with no port")))?;
        let addresses: Vec<SocketAddr> = match &host {
            Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(*ip), port)],
            Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(*ip), port)],
            Host::Domain(domain) => tokio::net::lookup_host((*domain, port))
                .await
                .map_err(|e| refused(format!("{domain} can't be found: {e}")))?
                .collect(),
        };
        let allowed: Vec<SocketAddr> =
            addresses.into_iter().filter(|address| self.policy.allows(address.ip())).collect();
        if allowed.is_empty() {
            return Err(refused(format!("{host} is on loopback, link-local or a private range")));
        }
        let mut stream = None;
        let mut failed = None;
        for address in allowed {
            match TcpStream::connect(address).await {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(e) => failed = Some(e),
            }
        }
        let stream = stream.ok_or_else(|| {
            refused(format!("{host} can't be reached: {}", failed.map(|e| e.to_string()).unwrap_or_default()))
        })?;
        // The address the connection reached, as the system says, whatever the name resolved to.
        let reached = stream.peer_addr().map_err(|e| refused(format!("{host}: {e}")))?;
        if !self.policy.allows(reached.ip()) {
            return Err(refused(format!("{host} reached {} on loopback, link-local or a private range", reached.ip())));
        }
        let io: Box<dyn Io> = if https {
            let name = match &host {
                Host::Domain(domain) => rustls::pki_types::ServerName::try_from(domain.to_string())
                    .map_err(|e| refused(format!("{domain}: {e}")))?,
                Host::Ipv4(ip) => rustls::pki_types::ServerName::IpAddress(IpAddr::V4(*ip).into()),
                Host::Ipv6(ip) => rustls::pki_types::ServerName::IpAddress(IpAddr::V6(*ip).into()),
            };
            Box::new(self.tls.connect(name, stream).await.map_err(|e| refused(format!("{host}'s TLS: {e}")))?)
        } else {
            Box::new(stream)
        };
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(io))
            .await
            .map_err(|e| refused(format!("{host}: {e}")))?;
        tokio::spawn(connection);
        let authority = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        let request = Request::get(&url[Position::BeforePath..Position::AfterQuery])
            .header(HOST, authority)
            .header(USER_AGENT, AGENT)
            .header(ACCEPT, wanted.accept())
            .header(ACCEPT_ENCODING, "identity")
            .header(CONNECTION, "close")
            .body(Empty::<Bytes>::new())
            .map_err(|e| refused(format!("{host}: {e}")))?;
        sender.send_request(request).await.map_err(|e| refused(format!("{host}: {e}")))
    }
}

/// Reads a body: a page's first PAGE_MOST, or a whole image of IMAGE_MOST at most.
async fn read(mut body: Incoming, wanted: Wanted) -> Result<Vec<u8>, String> {
    let most = match wanted {
        Wanted::Page => PAGE_MOST,
        Wanted::Image => IMAGE_MOST,
    };
    let mut read = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| e.to_string())?;
        let Ok(data) = frame.into_data() else { continue };
        if read.len() + data.len() > most {
            if wanted == Wanted::Image {
                return Err("its image is too large for a thumbnail".into());
            }
            read.extend_from_slice(&data[..most - read.len()]);
            break;
        }
        read.extend_from_slice(&data);
    }
    Ok(read)
}

/// `image` (a JPEG, PNG or WebP) as a small JPEG, or None if it isn't one jawd can read.
fn thumbnail(image: &[u8]) -> Option<Thumbnail> {
    let mut reader = image::ImageReader::new(Cursor::new(image)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(IMAGE_SIDE_MOST);
    limits.max_image_height = Some(IMAGE_SIDE_MOST);
    limits.max_alloc = Some(IMAGE_DECODED_MOST);
    reader.limits(limits);
    let small = reader.decode().ok()?.thumbnail(THUMBNAIL_SIDE, THUMBNAIL_SIDE).to_rgb8();
    let mut jpeg = Vec::new();
    small.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, THUMBNAIL_QUALITY)).ok()?;
    (jpeg.len() <= thumbnails::THUMBNAIL_MOST).then_some(Thumbnail { jpeg, w: small.width(), h: small.height() })
}

/// What a page says about itself, as link previews show it.
#[derive(Debug, Default, PartialEq)]
struct Meta {
    title: Option<String>,
    description: Option<String>,
    /// Its image's URL, as the page gives it.
    image: Option<String>,
}

/// What `html` says about itself in its head: Open Graph first, then Twitter's cards, then its own
/// title and description.
fn meta(html: &str) -> Meta {
    let lower = html.to_ascii_lowercase();
    let end = lower.find("<body").unwrap_or(lower.len());
    let (head, lower) = (&html[..end], &lower[..end]);
    let mut found: Vec<(String, String)> = Vec::new();
    let mut at = 0;
    while let Some(start) = lower[at..].find("<meta").map(|start| at + start) {
        let Some(close) = lower[start..].find('>').map(|close| start + close) else { break };
        let attributes = attributes(&head[start + "<meta".len()..close]);
        let key = ["property", "name"]
            .iter()
            .find_map(|name| attributes.iter().find(|(k, _)| k == name).map(|(_, v)| v.to_ascii_lowercase()));
        let content = attributes.iter().find(|(k, _)| k == "content").map(|(_, v)| v.clone());
        if let (Some(key), Some(content)) = (key, content) {
            found.push((key, content));
        }
        at = close;
    }
    let first = |keys: &[&str]| {
        keys.iter().find_map(|key| found.iter().find(|(k, v)| k == key && !v.trim().is_empty()).map(|(_, v)| v.clone()))
    };
    let title = lower.find("<title").and_then(|start| {
        let open = start + lower[start..].find('>')? + 1;
        let close = open + lower[open..].find("</title")?;
        Some(head[open..close].to_string())
    });
    let tidy = |text: Option<String>, most: usize| {
        text.map(|text| one_line(&entities(&text), most)).filter(|text| !text.is_empty())
    };
    Meta {
        title: tidy(first(&["og:title", "twitter:title"]).or(title), TITLE_MOST),
        description: tidy(first(&["og:description", "twitter:description", "description"]), DESCRIPTION_MOST),
        image: first(&["og:image:secure_url", "og:image", "og:image:url", "twitter:image", "twitter:image:src"])
            .map(|image| entities(image.trim())),
    }
}

/// A tag's attributes, each name in lower case and its value as written, unquoted.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = tag;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        let name_end = rest.find(|c: char| c.is_whitespace() || c == '=' || c == '/').unwrap_or(rest.len());
        if name_end == 0 {
            break;
        }
        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();
        let Some(after) = rest.strip_prefix('=') else {
            found.push((name, String::new()));
            continue;
        };
        rest = after.trim_start();
        let (value, after) = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => match rest[1..].find(quote) {
                Some(end) => (&rest[1..1 + end], &rest[2 + end..]),
                None => (&rest[1..], ""),
            },
            _ => {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (&rest[..end], &rest[end..])
            }
        };
        found.push((name, value.to_string()));
        rest = after;
    }
    found
}

/// `text` with HTML's character references read: the five XML ones, a non-breaking space, and
/// numbered ones.
fn entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let read = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &rest[1..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => name.strip_prefix('#').and_then(|number| {
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => number.parse().ok(),
                    };
                    code.and_then(char::from_u32)
                }),
            };
            c.map(|c| (c, end))
        });
        match read {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `text` on one line, its runs of whitespace as single spaces, at most `most` characters.
fn one_line(text: &str, most: usize) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match joined.char_indices().nth(most) {
        Some((cut, _)) => format!("{}…", joined[..cut].trim_end()),
        None => joined,
    }
}

#[cfg(test)]
mod tests;
