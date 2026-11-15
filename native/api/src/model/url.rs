//! What a player may type as an address, and what a screen is allowed to open.
//!
//! Addresses are parsed by the `url` crate, which reads them as Chromium does. The
//! blacklist depends on it: a home-made parser that disagrees with the browser about where
//! the host is (`http://good.example@bad.example/`, backslashes, upper case) is a way round.

/// The longest address a screen keeps. Longer ones are refused, not cut: half an address
/// is another address.
pub const MAX_LEN: usize = 2048;

/// Why an address was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    Empty,
    TooLong,
    /// Not something a browser can open.
    Malformed,
}

/// An address as typed, made ready to open: trimmed, with `http://` in front when it
/// names no scheme (as the Java mod does; sites that want https redirect by themselves,
/// and a device on the local network often has nothing else).
pub fn clean(typed: &str) -> Result<String, Refused> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err(Refused::Empty);
    }
    let address = if has_scheme(typed) { typed.to_owned() } else { format!("http://{typed}") };
    if address.len() > MAX_LEN {
        return Err(Refused::TooLong);
    }
    url::Url::parse(&address).map(|_| address).map_err(|_| Refused::Malformed)
}

/// Whether `typed` starts with a scheme: `https://…`, or one that takes no slashes.
fn has_scheme(typed: &str) -> bool {
    typed.contains("://") || ["about:", "data:"].iter().any(|scheme| typed.starts_with(scheme))
}

/// The host an address leads to, lower case, as the browser will see it.
pub fn host(address: &str) -> Option<String> {
    let address = address.trim();
    let parsed = if has_scheme(address) { url::Url::parse(address) } else { url::Url::parse(&format!("http://{address}")) };
    parsed.ok()?.host_str().map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

/// The sites a server does not want on its screens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Blacklist {
    hosts: Vec<String>,
}

impl Blacklist {
    /// From the configured entries: hosts, one each. Blank entries are dropped (the Java
    /// mod's empty default became one entry, the empty host, and blocked every address
    /// without one); a leading `*.` or dot is the same as none.
    pub fn new<S: AsRef<str>>(entries: impl IntoIterator<Item = S>) -> Self {
        let hosts = entries
            .into_iter()
            .map(|entry| entry.as_ref().trim().trim_start_matches("*.").trim_matches('.').to_ascii_lowercase())
            .filter(|entry| !entry.is_empty())
            .collect();
        Self { hosts }
    }

    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    /// Whether `address` leads to a listed site or to one under it (`www.` and any other
    /// subdomain; the Java mod matched the exact host only, which `www.` got round).
    pub fn blocks(&self, address: &str) -> bool {
        let Some(host) = host(address) else {
            return false;
        };
        self.hosts.iter().any(|listed| {
            host == *listed || host.strip_suffix(listed.as_str()).is_some_and(|rest| rest.ends_with('.'))
        })
    }
}

/// A YouTube video address in one of its forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YouTube {
    pub id: String,
    /// Already the full-frame player (`/embed/`), not the watch page.
    pub embed: bool,
}

impl YouTube {
    /// Reads `youtu.be/<id>`, `youtube.com/watch?v=<id>` and `youtube.com/embed/<id>`.
    pub fn parse(address: &str) -> Option<Self> {
        let parsed = url::Url::parse(address.trim()).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        let path = parsed.path();
        let video = |id: &str, embed| (is_video_id(id)).then(|| YouTube { id: id.to_owned(), embed });
        if host == "youtu.be" {
            return video(path.strip_prefix('/')?, false);
        }
        if !matches!(host.as_str(), "youtube.com" | "www.youtube.com" | "m.youtube.com") {
            return None;
        }
        if path.eq_ignore_ascii_case("/watch") {
            let id = parsed.query_pairs().find(|(key, _)| key == "v")?.1;
            return video(&id, false);
        }
        video(path.strip_prefix("/embed/")?, true)
    }

    /// The full-frame player for this video, which is what a screen wants: the watch page
    /// puts the video in a corner of it.
    ///
    /// YouTube answers "Error 153" when this address is opened with no referrer, so the
    /// browser must open it inside a page of ours or send one (see `host/src/scheme.rs`).
    pub fn embed_address(&self, autoplay: bool) -> String {
        let autoplay = if autoplay { "?autoplay=1" } else { "" };
        format!("https://www.youtube.com/embed/{}{autoplay}", self.id)
    }
}

/// The address that shows only the video, filling the picture, for an address of a video
/// site this mod knows: a YouTube video or live, a Twitch channel, video or clip. None for
/// any other address, which is opened as it is.
///
/// The sites' own pages put the video in a corner, among comments and a chat; a screen in
/// the world wants the player alone. What the player typed stays the screen's address:
/// this is only what the browser is sent to.
pub fn player(address: &str) -> Option<String> {
    let parsed = url::Url::parse(address.trim()).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let segments: Vec<&str> = parsed.path().split('/').filter(|part| !part.is_empty()).collect();
    let named = |text: &str| !text.is_empty() && text.len() <= 64 && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    match host.as_str() {
        "youtu.be" | "youtube.com" | "www.youtube.com" | "m.youtube.com" => {
            // A live or a short is a video under another path.
            if let ["live" | "shorts", id] = segments[..]
                && is_video_id(id)
            {
                return Some(format!("https://www.youtube.com/embed/{id}?autoplay=1"));
            }
            // Whatever a channel is broadcasting now.
            if let ["channel", channel, "live"] = segments[..]
                && named(channel)
            {
                return Some(format!("https://www.youtube.com/embed/live_stream?channel={channel}&autoplay=1"));
            }
            let video = YouTube::parse(address)?;
            // An address that is the player already is left as the player typed it.
            (!video.embed).then(|| video.embed_address(true))
        }
        "twitch.tv" | "www.twitch.tv" | "m.twitch.tv" => {
            // Twitch's player asks who shows it; opened on its own, it is Twitch's.
            const OWN: &str = "parent=twitch.tv&autoplay=true&muted=false";
            match segments[..] {
                ["videos", id] if named(id) => Some(format!("https://player.twitch.tv/?video={id}&{OWN}")),
                [_, "clip", clip] if named(clip) => Some(format!("https://clips.twitch.tv/embed?clip={clip}&{OWN}")),
                [channel] if named(channel) && !TWITCH_PAGES.contains(&channel.to_ascii_lowercase().as_str()) => {
                    Some(format!("https://player.twitch.tv/?channel={}&{OWN}", channel.to_ascii_lowercase()))
                }
                _ => None,
            }
        }
        "clips.twitch.tv" => match segments[..] {
            [clip] if named(clip) && clip != "embed" => Some(format!("https://clips.twitch.tv/embed?clip={clip}&parent=twitch.tv&autoplay=true&muted=false")),
            _ => None,
        },
        _ => None,
    }
}

/// Pages of Twitch's own whose address looks like a channel's.
const TWITCH_PAGES: [&str; 14] =
    ["directory", "videos", "settings", "subscriptions", "inventory", "drops", "wallet", "search", "login", "signup", "friends", "turbo", "downloads", "jobs"];

/// YouTube ids are letters, digits, `-` and `_`. Anything else is not put in an address.
fn is_video_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_video_sites_address_becomes_its_player_alone() {
        let yt = |id: &str| Some(format!("https://www.youtube.com/embed/{id}?autoplay=1"));
        assert_eq!(player("https://www.youtube.com/watch?v=aqz-KE-bpKQ&t=10"), yt("aqz-KE-bpKQ"));
        assert_eq!(player("https://youtu.be/aqz-KE-bpKQ"), yt("aqz-KE-bpKQ"));
        assert_eq!(player("https://www.youtube.com/live/jfKfPfyJRdk?feature=share"), yt("jfKfPfyJRdk"), "a live");
        assert_eq!(player("https://youtube.com/shorts/abc_DEF-123"), yt("abc_DEF-123"));
        assert_eq!(
            player("https://www.youtube.com/channel/UC4R8DWoMoI7CAwX8_LjQHig/live").as_deref(),
            Some("https://www.youtube.com/embed/live_stream?channel=UC4R8DWoMoI7CAwX8_LjQHig&autoplay=1"),
            "what a channel broadcasts now"
        );
        let own = "parent=twitch.tv&autoplay=true&muted=false";
        assert_eq!(player("https://www.twitch.tv/Some_Channel"), Some(format!("https://player.twitch.tv/?channel=some_channel&{own}")));
        assert_eq!(player("https://twitch.tv/videos/123456789"), Some(format!("https://player.twitch.tv/?video=123456789&{own}")));
        assert_eq!(player("https://www.twitch.tv/somebody/clip/FunnyClip-abc123"), Some(format!("https://clips.twitch.tv/embed?clip=FunnyClip-abc123&{own}")));
        assert_eq!(player("https://clips.twitch.tv/FunnyClip-abc123"), Some(format!("https://clips.twitch.tv/embed?clip=FunnyClip-abc123&{own}")));
    }

    #[test]
    fn any_other_address_is_opened_as_it_is() {
        for other in [
            "https://www.youtube.com/",
            "https://www.youtube.com/results?search_query=cats",
            "https://www.youtube.com/@somebody/live",
            "https://www.youtube.com/embed/aqz-KE-bpKQ",
            "https://www.twitch.tv/",
            "https://www.twitch.tv/directory",
            "https://www.twitch.tv/directory/game/Minecraft",
            "https://www.twitch.tv/some channel",
            "https://player.twitch.tv/?channel=x&parent=twitch.tv",
            "https://example.org/watch?v=aqz-KE-bpKQ",
            "about:blank",
            "not an address",
        ] {
            assert_eq!(player(other), None, "{other}");
        }
    }

    #[test]
    fn a_typed_address_gets_a_scheme_and_is_checked() {
        assert_eq!(clean("  example.org/a b  ").as_deref(), Ok("http://example.org/a b"));
        assert_eq!(clean("https://example.org").as_deref(), Ok("https://example.org"));
        assert_eq!(clean("192.168.1.20:8080/ui").as_deref(), Ok("http://192.168.1.20:8080/ui"));
        assert_eq!(clean("about:blank").as_deref(), Ok("about:blank"));
        assert_eq!(clean("   "), Err(Refused::Empty));
        assert_eq!(clean("http://"), Err(Refused::Malformed));
        assert_eq!(clean(&format!("example.org/{}", "a".repeat(MAX_LEN))), Err(Refused::TooLong));
    }

    #[test]
    fn an_address_with_a_percent_sign_is_just_an_address() {
        // The Java mod threw on these: it logged the address through a format string.
        let address = "https://example.org/search?q=100%25%20sure";
        assert_eq!(clean(address).as_deref(), Ok(address));
    }

    #[test]
    fn the_host_is_the_one_the_browser_will_use() {
        assert_eq!(host("HTTPS://WWW.Example.ORG/Path").as_deref(), Some("www.example.org"));
        assert_eq!(host("example.org:8080").as_deref(), Some("example.org"));
        assert_eq!(host("http://good.example@bad.example/").as_deref(), Some("bad.example"), "user info is not the host");
        assert_eq!(host("http://bad.example\\@good.example/").as_deref(), Some("bad.example"), "a backslash ends the host");
        assert_eq!(host("http://example.org./").as_deref(), Some("example.org"), "a trailing dot is the same site");
        assert_eq!(host("about:blank"), None);
    }

    #[test]
    fn the_blacklist_stops_a_site_and_what_is_under_it() {
        let list = Blacklist::new(["Example.org", " *.bad.test ", ""]);
        for blocked in [
            "example.org",
            "http://EXAMPLE.org/page",
            "https://www.example.org",
            "https://a.b.example.org:8443/x",
            "http://user@example.org/",
            "http://example.org./",
            "bad.test",
            "cdn.bad.test",
        ] {
            assert!(list.blocks(blocked), "{blocked}");
        }
        for allowed in ["notexample.org", "example.org.evil.test", "example.com", "about:blank", "", "http://"] {
            assert!(!list.blocks(allowed), "{allowed}");
        }
    }

    #[test]
    fn an_empty_blacklist_blocks_nothing() {
        // The Java mod's default, "", split into one empty entry.
        let list = Blacklist::new("".split(','));
        assert!(list.is_empty());
        assert!(!list.blocks("file:///C:/secret.txt"));
        assert!(!list.blocks("https://example.org"));
    }

    #[test]
    fn youtube_addresses_are_read_in_their_three_forms() {
        let video = |id: &str, embed| Some(YouTube { id: id.to_owned(), embed });
        assert_eq!(YouTube::parse("https://youtu.be/aqz-KE-bpKQ"), video("aqz-KE-bpKQ", false));
        assert_eq!(YouTube::parse("https://www.youtube.com/watch?v=aqz-KE-bpKQ&t=10"), video("aqz-KE-bpKQ", false));
        assert_eq!(YouTube::parse("https://youtube.com/watch?list=x&v=aqz-KE-bpKQ"), video("aqz-KE-bpKQ", false));
        assert_eq!(YouTube::parse("https://www.youtube.com/embed/aqz-KE-bpKQ?autoplay=1"), video("aqz-KE-bpKQ", true));
        for other in [
            "https://www.youtube.com/",
            "https://www.youtube.com/watch",
            "https://www.youtube.com/watch?vv=x",
            "https://youtu.be/",
            "https://www.youtube.com/embed/",
            "https://example.org/watch?v=aqz-KE-bpKQ",
            "https://youtu.be/a\"onload=\"x",
            "not an address",
        ] {
            assert_eq!(YouTube::parse(other), None, "{other}");
        }
    }

    #[test]
    fn a_video_becomes_its_full_frame_player() {
        let video = YouTube::parse("https://www.youtube.com/watch?v=aqz-KE-bpKQ").unwrap();
        assert_eq!(video.embed_address(false), "https://www.youtube.com/embed/aqz-KE-bpKQ");
        assert_eq!(video.embed_address(true), "https://www.youtube.com/embed/aqz-KE-bpKQ?autoplay=1");
    }
}
