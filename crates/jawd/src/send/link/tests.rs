use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;

/// What the test server answers a path with: a status, headers, a body, and how long it waits first.
struct Answer {
    status: &'static str,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    wait: Duration,
}

fn page(content_type: &str, body: impl Into<Vec<u8>>) -> Answer {
    Answer {
        status: "200 OK",
        headers: vec![("Content-Type".into(), content_type.into())],
        body: body.into(),
        wait: Duration::ZERO,
    }
}

fn redirect(to: &str) -> Answer {
    Answer {
        status: "302 Found",
        headers: vec![("Location".into(), to.into()), ("Set-Cookie".into(), "session=secret; Path=/".into())],
        body: Vec::new(),
        wait: Duration::ZERO,
    }
}

/// A web server on this machine's loopback, answering each path as `answer` says, which keeps the
/// head of every request it's asked and counts its connections.
struct Server {
    base: String,
    port: u16,
    heads: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicUsize>,
}

impl Server {
    async fn start(answer: impl Fn(&str, &str) -> Answer + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let (heads, connections) = (Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicUsize::new(0)));
        let answer = Arc::new(answer);
        let (kept, counted, at) = (heads.clone(), connections.clone(), base.clone());
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                counted.fetch_add(1, Ordering::SeqCst);
                let (answer, kept, at) = (answer.clone(), kept.clone(), at.clone());
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).await.unwrap_or(0) == 1 {
                        head.push(byte[0]);
                    }
                    let head = String::from_utf8_lossy(&head).into_owned();
                    let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                    kept.lock().unwrap().push(head);
                    let reply = answer(&at, &path);
                    tokio::time::sleep(reply.wait).await;
                    let mut out = format!(
                        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                        reply.status,
                        reply.body.len()
                    );
                    for (name, value) in reply.headers {
                        out.push_str(&format!("{name}: {value}\r\n"));
                    }
                    out.push_str("\r\n");
                    let _ = stream.write_all(out.as_bytes()).await;
                    let _ = stream.write_all(&reply.body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Server { base, port, heads, connections }
    }

    fn heads(&self) -> Vec<String> {
        self.heads.lock().unwrap().clone()
    }
}

/// A PNG of `w` by `h` pixels, all one colour.
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbImage::from_pixel(w, h, image::Rgb([40, 120, 200]))
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

const ARTICLE: &str = r#"<!doctype html><html><head>
    <title>A title the page gives itself</title>
    <meta property="og:title" content="Escalada em Sintra &amp; Cascais">
    <meta content='Os melhores blocos, &#233; claro &#x2014; e mais' property=og:description>
    <meta property="og:image" content="/topo.png">
    </head><body><meta property="og:title" content="not in the head"></body></html>"#;

#[test]
fn a_page_says_what_it_is_with_open_graph_first_then_twitter_then_its_own_title() {
    let found = meta(ARTICLE);
    assert_eq!(found.title.as_deref(), Some("Escalada em Sintra & Cascais"));
    assert_eq!(found.description.as_deref(), Some("Os melhores blocos, é claro — e mais"));
    assert_eq!(found.image.as_deref(), Some("/topo.png"));
    let plain = meta(
        "<HTML><HEAD><TITLE>\n  Só o   título \n</TITLE><META NAME=\"description\" CONTENT=\"d\"><meta name=twitter:image content=x.jpg /></HEAD>",
    );
    assert_eq!(
        plain,
        Meta { title: Some("Só o título".into()), description: Some("d".into()), image: Some("x.jpg".into()) }
    );
    let long = meta(&format!("<title>{}</title>", "palavra ".repeat(100)));
    assert!(long.title.unwrap().chars().count() <= TITLE_MOST + 1);
    assert_eq!(meta("<p>nothing here</p>"), Meta::default());
}

#[test]
fn addresses_on_loopback_link_local_or_private_ranges_are_refused() {
    let public = Policy { loopback: false };
    for refused in [
        "127.0.0.1",
        "127.9.9.9",
        "::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "0.1.2.3",
        "224.0.0.1",
        "255.255.255.255",
        "240.0.0.1",
        "192.0.0.8",
        "::",
        "fe80::1",
        "fc00::1",
        "fd12:3456::1",
        "fec0::1",
        "ff02::1",
        "64:ff9b::a00:1",
        "2002:c0a8:101::1",
    ] {
        assert!(!public.allows(refused.parse().unwrap()), "{refused} was allowed");
    }
    for allowed in ["8.8.8.8", "93.184.216.34", "172.32.0.1", "2606:4700::1111", "64:ff9b::808:808", "2002:808:808::1"]
    {
        assert!(public.allows(allowed.parse().unwrap()), "{allowed} was refused");
    }
    let tests = Policy { loopback: true };
    assert!(tests.allows("127.0.0.1".parse().unwrap()) && tests.allows("::1".parse().unwrap()));
    assert!(!tests.allows("10.0.0.1".parse().unwrap()) && !tests.allows("169.254.169.254".parse().unwrap()));
}

#[tokio::test]
async fn a_pages_preview_has_its_title_description_and_a_small_thumbnail_of_its_image() {
    let server = Server::start(|_, path| match path {
        "/a" => redirect("/article"),
        "/article" => page("text/html; charset=utf-8", ARTICLE),
        "/topo.png" => page("image/png", png(640, 320)),
        _ => page("text/plain", "?"),
    })
    .await;
    let text = format!("olha isto {}/a, é bom", server.base);
    let preview = Previews::local(WITHIN).of(&text).await.expect("a preview");
    assert_eq!(preview.matched, format!("{}/a", server.base));
    assert_eq!(preview.title.as_deref(), Some("Escalada em Sintra & Cascais"));
    assert_eq!(preview.description.as_deref(), Some("Os melhores blocos, é claro — e mais"));
    let thumbnail = preview.thumbnail.expect("a thumbnail");
    assert_eq!((thumbnail.w, thumbnail.h), (THUMBNAIL_SIDE, THUMBNAIL_SIDE / 2));
    let decoded = image::load_from_memory_with_format(&thumbnail.jpeg, image::ImageFormat::Jpeg).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (THUMBNAIL_SIDE, THUMBNAIL_SIDE / 2));
    let heads = server.heads();
    assert_eq!(heads.len(), 3, "the redirect, the page and its image: {heads:?}");
    for head in &heads {
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains(&format!("user-agent: {}", AGENT.to_ascii_lowercase())), "{head}");
        assert!(!lower.contains("cookie"), "a cookie went back: {head}");
    }

    let sent = with_preview(
        e2e::Message { conversation: Some(text.clone()), ..Default::default() },
        &Preview { thumbnail: Some(thumbnail.clone()), ..Previews::local(WITHIN).of(&text).await.unwrap() },
    );
    let extended = sent.extended_text_message.expect("extended text");
    assert!(sent.conversation.is_none());
    assert_eq!(extended.text.as_deref(), Some(text.as_str()));
    assert_eq!(extended.matched_text, Some(preview.matched));
    assert_eq!(extended.title.as_deref(), Some("Escalada em Sintra & Cascais"));
    assert_eq!(extended.jpeg_thumbnail, Some(thumbnail.jpeg));
    assert_eq!((extended.thumbnail_width, extended.thumbnail_height), (Some(THUMBNAIL_SIDE), Some(THUMBNAIL_SIDE / 2)));
}

#[tokio::test]
async fn only_html_answering_in_time_and_within_its_redirects_makes_a_preview() {
    let server = Server::start(|base, path| match path {
        "/json" => page("application/json", r#"{"title": "not a page"}"#),
        "/untyped" => Answer { headers: Vec::new(), ..page("", ARTICLE) },
        "/slow" => Answer { wait: Duration::from_secs(5), ..page("text/html", ARTICLE) },
        "/bare" => page("text/html", "<p>no title, no description</p>"),
        "/missing" => Answer { status: "404 Not Found", ..page("text/html", ARTICLE) },
        "/huge-image" => page("text/html", ARTICLE.replace("/topo.png", "/huge.png")),
        "/huge.png" => page("image/png", vec![0u8; IMAGE_MOST + 1]),
        "/wide-image" => page("text/html", ARTICLE.replace("/topo.png", "/wide.png")),
        "/wide.png" => page("image/png", png(IMAGE_SIDE_MOST + 1, 1)),
        "/long" => page("text/html", format!("{ARTICLE}{}", " ".repeat(PAGE_MOST * 2))),
        hops if hops.starts_with("/hops/") => {
            let left: usize = hops["/hops/".len()..].parse().unwrap();
            if left == 0 { page("text/html", ARTICLE) } else { redirect(&format!("{base}/hops/{}", left - 1)) }
        }
        _ => page("image/png", png(10, 10)),
    })
    .await;
    let previews = Previews::local(Duration::from_millis(600));
    let of = |path: &str| {
        let previews = previews.clone();
        let text = format!("{}{path}", server.base);
        async move { previews.of(&text).await }
    };
    assert_eq!(of("/json").await, None, "not HTML");
    assert_eq!(of("/untyped").await, None, "not said to be HTML");
    assert_eq!(of("/bare").await, None, "nothing to show");
    assert_eq!(of("/missing").await, None);
    let began = Instant::now();
    assert_eq!(of("/slow").await, None, "too slow");
    assert!(began.elapsed() < Duration::from_millis(1_500), "waited {:?}", began.elapsed());
    assert!(of(&format!("/hops/{REDIRECTS_MOST}")).await.is_some(), "as many redirects as it follows");
    assert_eq!(of(&format!("/hops/{}", REDIRECTS_MOST + 1)).await, None, "one redirect too many");
    let long = of("/long").await.expect("the top of a long page");
    assert_eq!(long.title.as_deref(), Some("Escalada em Sintra & Cascais"));
    for path in ["/huge-image", "/wide-image"] {
        let preview = of(path).await.expect("a preview");
        assert!(preview.title.is_some() && preview.thumbnail.is_none(), "{path}: no thumbnail from that image");
    }
    assert_eq!(of("no link at all").await, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_preview_whose_image_is_the_largest_the_caps_decode_still_comes_within_its_deadline() {
    let big = png(5_600, 5_600);
    assert!(big.len() <= IMAGE_MOST);
    let server = Server::start(move |_, path| match path {
        "/article" => page("text/html", ARTICLE),
        _ => page("image/png", big.clone()),
    })
    .await;
    let url = format!("{}/article", server.base);
    let began = Instant::now();
    let roomy = Previews::local(WITHIN).of(&url).await.expect("a preview");
    assert!(roomy.thumbnail.is_some(), "the image decodes, in {:?}", began.elapsed());
    let within = Duration::from_millis(100);
    let began = Instant::now();
    Previews::local(within).of(&url).await;
    let took = began.elapsed();
    assert!(took < within + Duration::from_millis(150), "the preview took {took:?}, its deadline {within:?}");
}

#[tokio::test]
async fn a_page_on_loopback_a_private_range_or_reached_through_one_is_never_fetched() {
    let server = Server::start(|_, path| match path {
        "/private" => redirect("http://10.0.0.1/article"),
        "/metadata" => redirect("http://169.254.169.254/latest/meta-data/"),
        "/mapped" => redirect("http://[::ffff:192.168.1.1]/"),
        _ => page("text/html", ARTICLE),
    })
    .await;
    assert_eq!(Previews::default().of(&format!("{}/article", server.base)).await, None);
    let public = Fetcher::new(Policy { loopback: false });
    for url in [
        format!("http://127.0.0.1:{}/article", server.port),
        format!("http://localhost:{}/article", server.port),
        format!("http://[::1]:{}/article", server.port),
    ] {
        let refused = public.get(Url::parse(&url).unwrap(), Wanted::Page).await.err().expect("refused");
        assert!(refused.0.contains("loopback, link-local or a private range"), "{url}: {refused}");
    }
    assert_eq!(server.connections.load(Ordering::SeqCst), 0, "nothing on loopback was connected to");

    // Allowed to reach the test server, a redirect from it to a private address is refused all the
    // same, by the policy, before anything connects there: a machine whose network refuses such a
    // connection at once would refuse it too, so only the reason tells the two apart.
    let local = Previews::local(WITHIN);
    let fetcher = Fetcher::new(Policy { loopback: true });
    for (path, to) in [
        ("/private", "http://10.0.0.1/"),
        ("/metadata", "http://169.254.169.254/"),
        ("/mapped", "http://[::ffff:192.168.1.1]/"),
    ] {
        let began = Instant::now();
        assert_eq!(local.of(&format!("{}{path}", server.base)).await, None, "{path}");
        assert!(began.elapsed() < Duration::from_millis(500), "{path} waited on a connection: {:?}", began.elapsed());
        let url = Url::parse(&format!("{}{path}", server.base)).unwrap();
        let refused = fetcher.get(url, Wanted::Page).await.err().expect("refused");
        let host = Url::parse(to).unwrap().host().unwrap().to_string();
        assert_eq!(refused.0, format!("{host} is on loopback, link-local or a private range"), "{path}");
    }
}

mod sending {
    use std::path::Path;

    use prost::Message as _;
    use serde_json::{Value, json};
    use tokio::sync::{broadcast, mpsc};

    use super::*;
    use crate::engine::demo::{self, DemoEngine, Replies, World};
    use crate::engine::pb;
    use crate::engine::supervisor::{Backoff, Supervisor};
    use crate::send::{Sending, Timing};
    use crate::store::{Change, Chat, Lane, Store};

    const ANA: &str = "351912345678@s.whatsapp.net";

    /// jawd's sending on a jaw.db in `home`, through the demo engine, connected, with Ana's chat, and
    /// link previews made by `previews`.
    async fn sending(home: &Path, previews: Previews) -> (Store, Sending, Arc<World>) {
        let store = Store::open(home).unwrap();
        let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let (pushes, _) = broadcast::channel(256);
        let (events, incoming) = mpsc::channel(1024);
        let world: Arc<World> = Arc::default();
        world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
        let engine = DemoEngine::sharing(vec![demo::connected()], world.clone());
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let mut connected = pushes.subscribe();
        let sync = crate::sync::run(
            incoming,
            store.clone(),
            live.clone(),
            pushes.clone(),
            Some(supervisor.clone()),
            home.into(),
            Default::default(),
            crate::feed::Typing::default(),
        );
        tokio::spawn(sync);
        let attention = Arc::new(crate::attention::Attention::default());
        let timing = Timing {
            retry_first: Duration::from_millis(5),
            retry_most: Duration::from_millis(20),
            ..Timing::default()
        };
        let sending =
            Sending::start(store.clone(), Some(supervisor), live, &pushes, attention.focus(), timing, previews)
                .await
                .unwrap();
        let online = async {
            while let Ok(push) = connected.recv().await {
                if push.method == "connection" && push.params["state"] == "online" {
                    return;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), online).await.expect("the demo engine never connected");
        store.apply(Lane::Live, vec![Change::Chat(Chat { id: ANA.into(), ..Chat::default() })]).await.unwrap();
        (store, sending, world)
    }

    /// The messages the demo engine was asked to send, as what each says.
    fn sent(world: &World) -> Vec<(String, e2e::Message)> {
        world.sends().into_iter().map(|send| (send.id, e2e::Message::decode(send.raw.as_slice()).unwrap())).collect()
    }

    async fn article() -> Server {
        Server::start(|_, path| match path {
            "/article" => page("text/html", ARTICLE),
            "/slow" => Answer { wait: Duration::from_secs(3), ..page("text/html", ARTICLE) },
            "/topo.png" => page("image/png", png(300, 300)),
            _ => page("text/plain", "?"),
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn text_with_a_link_goes_with_its_pages_preview_and_shows_it_at_once() {
        let home = tempfile::tempdir().unwrap();
        let server = article().await;
        let (store, sending, world) = sending(home.path(), Previews::local(WITHIN)).await;
        let text = format!("olha {}/article", server.base);
        let answer = sending.call("send.text", json!({ "chat": ANA, "text": text })).await.unwrap();
        let link = &answer["row"]["link"];
        assert_eq!(link["url"], format!("{}/article", server.base));
        assert_eq!(link["title"], "Escalada em Sintra & Cascais");
        assert_eq!(link["description"], "Os melhores blocos, é claro — e mais");
        let preview = link["preview"].as_str().expect("the thumbnail, at once");
        let file = std::fs::read(home.path().join(preview)).unwrap();
        world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::Send(_)))).await;
        let (id, message) = sent(&world).remove(0);
        assert_eq!(Value::String(id), answer["message"]);
        let extended = message.extended_text_message.expect("WhatsApp's extended text");
        assert_eq!(extended.text.as_deref(), Some(text.as_str()));
        assert_eq!(extended.matched_text, Some(format!("{}/article", server.base)));
        assert_eq!(extended.title.as_deref(), Some("Escalada em Sintra & Cascais"));
        assert_eq!(extended.jpeg_thumbnail, Some(file), "the thumbnail that went is the one kept");
        assert_eq!(store.count("links").unwrap(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn text_asked_to_go_without_a_preview_goes_as_it_is_and_nothing_is_fetched() {
        let home = tempfile::tempdir().unwrap();
        let server = article().await;
        let (_store, sending, world) = sending(home.path(), Previews::local(WITHIN)).await;
        let text = format!("olha {}/article", server.base);
        let answer = sending.call("send.text", json!({ "chat": ANA, "text": text, "preview": false })).await.unwrap();
        assert!(answer["row"].get("link").is_none());
        world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::Send(_)))).await;
        assert_eq!(sent(&world)[0].1.conversation.as_deref(), Some(text.as_str()));
        assert!(server.heads().is_empty(), "nothing was fetched");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_page_too_slow_leaves_its_text_to_go_without_a_preview_and_the_next_message_after_it() {
        let home = tempfile::tempdir().unwrap();
        let server = article().await;
        let within = Duration::from_millis(400);
        let (_store, sending, world) = sending(home.path(), Previews::local(within)).await;
        let slow = format!("olha {}/slow", server.base);
        let first = {
            let sending = sending.clone();
            let slow = slow.clone();
            tokio::spawn(async move {
                let began = Instant::now();
                let answer = sending.call("send.text", json!({ "chat": ANA, "text": slow })).await.unwrap();
                (answer, began.elapsed())
            })
        };
        // The next message to the chat, asked while the first waits for its page.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let second = sending.call("send.text", json!({ "chat": ANA, "text": "e mais isto" })).await.unwrap();
        let (first, took) = first.await.unwrap();
        assert!(took >= within && took < within + Duration::from_millis(1_500), "answered in {took:?}");
        assert!(first["row"].get("link").is_none(), "no preview: {first}");
        world.until(|asked| asked.iter().filter(|kind| matches!(kind, pb::command::Kind::Send(_))).count() == 2).await;
        let order: Vec<Value> = sent(&world).into_iter().map(|(id, _)| Value::String(id)).collect();
        assert_eq!(order, [first["message"].clone(), second["message"].clone()], "the next one waited for it");
        assert_eq!(sent(&world)[0].1.conversation.as_deref(), Some(slow.as_str()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn asked_twice_with_one_key_while_its_page_is_fetched_one_message_goes() {
        let home = tempfile::tempdir().unwrap();
        let server =
            Server::start(|_, _| Answer { wait: Duration::from_millis(200), ..page("text/html", ARTICLE) }).await;
        let (store, sending, world) = sending(home.path(), Previews::local(WITHIN)).await;
        let params = json!({ "chat": ANA, "text": format!("olha {}/article", server.base), "key": "jaw-1" });
        let (a, b) = tokio::join!(sending.call("send.text", params.clone()), sending.call("send.text", params));
        assert_eq!(a.unwrap()["message"], b.unwrap()["message"]);
        world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::Send(_)))).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(sent(&world).len(), 1);
        assert_eq!(store.count("messages").unwrap(), 1);
        assert_eq!(server.heads().iter().filter(|head| head.starts_with("GET /article")).count(), 1, "fetched once");
    }
}
