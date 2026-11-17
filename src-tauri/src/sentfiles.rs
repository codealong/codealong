//! The pictures, films and recordings a session sent you (Claude Code's `SendUserFile`), served to
//! the windows at `codealong-file://localhost/<the full path, percent-encoded>` so a conversation
//! shows them in place (src/ui/SentFiles.tsx). Only a file whose name says it's one of those
//! (`mime`, which src/sentFiles.ts `mediaKind` mirrors), and whose links lead to one too, is read,
//! so nothing else on the machine can be read through it; every other file a session sends stays a
//! link that opens it in its app.
//!
//! A file where macOS asks before Codealong reads it (`asks_first`) is served only once you've
//! asked for it (`?asked=1`), so a conversation scrolled past never brings up the prompt. A window
//! learns which files those are, and which are gone, from `states`, which only looks each up.
//!
//! WebKit asks for a film a range of bytes at a time: two bytes first, then a few here and there
//! as it reads the film's index, a dozen asks before it can show a frame (macOS 26.6, an offscreen
//! WKWebView, 2026-11-16). Each answered with the whole file it still plays, but reads all of a
//! long film for every ask. So a request with a range is answered with that part, at most `PART`
//! of it (WebKit asks on for the rest), and a picture, asked for without one, whole.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::http::{header, Response, StatusCode};

use crate::os;

/// The most of a file one ranged request is answered with.
const PART: u64 = 4 << 20;

/// What a file is served as, by its name; `None` for anything that isn't a picture, a film or a
/// recording a window can show, which isn't served at all.
pub fn mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        _ => return None,
    })
}

/// The file a request names: the full path its URL carries, percent-encoded as one piece.
fn requested(path: &str) -> Option<PathBuf> {
    let decoded = urlencoding::decode(path.strip_prefix('/')?).ok()?;
    let path = PathBuf::from(decoded.into_owned());
    path.is_absolute().then_some(path)
}

/// Whether macOS asks before Codealong reads a file there: in Desktop, Documents or Downloads, on
/// another volume, in iCloud Drive or a cloud folder, or in another app's container (held.rs,
/// .claude/rules/macos-privacy.md). Asked of where a file really is (`real`): a link into
/// Downloads, or a path through `..`, reads Downloads all the same. No other OS asks.
fn asks_first(real: &Path) -> bool {
    if !os::is_macos() {
        return false;
    }
    let mut parts = real.components().map(|c| c.as_os_str().to_string_lossy());
    let (Some(_root), Some(top)) = (parts.next(), parts.next()) else { return false };
    if top == "Volumes" {
        return true;
    }
    let (Some(_user), Some(first), second) = (parts.next(), parts.next(), parts.next()) else { return false };
    top == "Users"
        && (matches!(first.as_ref(), "Desktop" | "Documents" | "Downloads")
            || (first == "Library" && second.is_some_and(|s| matches!(s.as_ref(), "Mobile Documents" | "CloudStorage" | "Containers" | "Group Containers"))))
}

/// What a window can do with a file a session sent you, found by looking it up, never reading it,
/// which macOS doesn't hold (held.rs).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Show it.
    Here,
    /// Show it once you ask: macOS asks before it can be read (`asks_first`).
    AsksFirst,
    /// Name it alone: it isn't there, isn't a file, or isn't a picture, film or recording once its
    /// links are followed.
    Gone,
}

/// Where a file is once its links are followed, and that it's a picture, film or recording both
/// as named and there.
fn real_media(path: &Path) -> Option<PathBuf> {
    mime(path)?;
    let real = std::fs::canonicalize(path).ok()?;
    mime(&real)?;
    Some(real)
}

fn state(path: &str) -> State {
    let path = Path::new(path);
    match path.is_absolute().then(|| real_media(path)).flatten() {
        Some(real) if std::fs::metadata(&real).is_ok_and(|m| m.is_file()) => if asks_first(&real) { State::AsksFirst } else { State::Here },
        _ => State::Gone,
    }
}

/// `state` for each of `paths`, in their order.
pub fn states(paths: &[String]) -> Vec<State> {
    paths.iter().map(|p| state(p)).collect()
}

/// What a `Range` header asks of a file `len` bytes long.
#[derive(Debug, PartialEq)]
enum Asked {
    /// All of it: no range, one that can't be read, or several at once, which a server may
    /// answer with the whole file.
    Whole,
    /// From the first byte to the last, both included, already cut to `PART`.
    Part(u64, u64),
    /// A range that starts past its end.
    Past,
}

fn asked(range: Option<&str>, len: u64) -> Asked {
    let Some(spec) = range.and_then(|r| r.trim().strip_prefix("bytes=")) else { return Asked::Whole };
    if spec.contains(',') || len == 0 {
        return Asked::Whole;
    }
    let Some((from, to)) = spec.split_once('-') else { return Asked::Whole };
    let (from, to) = (from.trim(), to.trim());
    let (first, last) = if from.is_empty() {
        // The last `to` bytes.
        let Ok(n) = to.parse::<u64>() else { return Asked::Whole };
        if n == 0 {
            return Asked::Past;
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let Ok(first) = from.parse::<u64>() else { return Asked::Whole };
        let last = if to.is_empty() {
            len - 1
        } else {
            match to.parse::<u64>() {
                Ok(last) if last >= first => last.min(len - 1),
                _ => return Asked::Whole,
            }
        };
        if first >= len {
            return Asked::Past;
        }
        (first, last)
    };
    Asked::Part(first, last.min(first + PART - 1))
}

/// Serves one request, on a thread of its own: a read on a slow volume shouldn't hold the others.
pub fn serve(request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let path = request.uri().path().to_string();
    let you_asked = request.uri().query().is_some_and(|q| q.split('&').any(|p| p == "asked=1"));
    let range = request.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).map(String::from);
    std::thread::spawn(move || {
        let response = respond(&path, you_asked, range.as_deref()).unwrap_or_else(|status| status_only(status));
        responder.respond(response);
    });
}

fn respond(path: &str, you_asked: bool, range: Option<&str>) -> Result<Response<Vec<u8>>, StatusCode> {
    let path = requested(path).ok_or(StatusCode::BAD_REQUEST)?;
    mime(&path).ok_or(StatusCode::FORBIDDEN)?;
    let real = match std::fs::canonicalize(&path) {
        Ok(real) => real,
        Err(_) => return Err(StatusCode::NOT_FOUND),
    };
    let mime = mime(&real).ok_or(StatusCode::FORBIDDEN)?;
    if asks_first(&real) && !you_asked {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut file = File::open(&real).map_err(|_| StatusCode::NOT_FOUND)?;
    let meta = file.metadata().map_err(|_| StatusCode::NOT_FOUND)?;
    if !meta.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    let len = meta.len();
    let built = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        // An agent often sends a file again once it's changed it, under the same name.
        .header(header::CACHE_CONTROL, "no-cache");
    let built = match asked(range, len) {
        Asked::Whole => {
            let mut body = Vec::with_capacity(len as usize);
            file.read_to_end(&mut body).map_err(|_| StatusCode::NOT_FOUND)?;
            built.header(header::CONTENT_LENGTH, body.len()).body(body)
        }
        Asked::Part(first, last) => {
            let mut body = vec![0; (last - first + 1) as usize];
            file.seek(SeekFrom::Start(first)).and_then(|_| file.read_exact(&mut body)).map_err(|_| StatusCode::NOT_FOUND)?;
            built
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {first}-{last}/{len}"))
                .header(header::CONTENT_LENGTH, body.len())
                .body(body)
        }
        Asked::Past => built.status(StatusCode::RANGE_NOT_SATISFIABLE).header(header::CONTENT_RANGE, format!("bytes */{len}")).body(Vec::new()),
    };
    built.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn status_only(status: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = status;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pictures_films_and_recordings_are_served() {
        assert_eq!(mime(Path::new("/tmp/teaser.MP4")), Some("video/mp4"));
        assert_eq!(mime(Path::new("/tmp/shot.jpeg")), Some("image/jpeg"));
        assert_eq!(mime(Path::new("/tmp/voice.m4a")), Some("audio/mp4"));
        assert_eq!(mime(Path::new("/Users/me/.ssh/id_ed25519")), None);
        assert_eq!(mime(Path::new("/Users/me/notes.md")), None);
        assert_eq!(mime(Path::new("/Users/me/logo.svg")), None);
    }

    #[test]
    fn a_request_names_a_full_path() {
        assert_eq!(requested("/%2FUsers%2Fme%2Fsome%20film.mp4"), Some(PathBuf::from("/Users/me/some film.mp4")));
        assert_eq!(requested("/shots%2Fafter.png"), None);
        assert_eq!(requested(""), None);
    }

    #[test]
    fn ranges_are_read_as_webkit_asks_them() {
        assert_eq!(asked(None, 100), Asked::Whole);
        // WebKit's first ask of a film.
        assert_eq!(asked(Some("bytes=0-1"), 100), Asked::Part(0, 1));
        assert_eq!(asked(Some("bytes=10-"), 100), Asked::Part(10, 99));
        assert_eq!(asked(Some("bytes=90-500"), 100), Asked::Part(90, 99));
        assert_eq!(asked(Some("bytes=-30"), 100), Asked::Part(70, 99));
        assert_eq!(asked(Some("bytes=-300"), 100), Asked::Part(0, 99));
        assert_eq!(asked(Some("bytes=100-"), 100), Asked::Past);
        assert_eq!(asked(Some("bytes=-0"), 100), Asked::Past);
        assert_eq!(asked(Some("bytes=5-2"), 100), Asked::Whole);
        assert_eq!(asked(Some("bytes=0-1,5-6"), 100), Asked::Whole);
        assert_eq!(asked(Some("items=0-1"), 100), Asked::Whole);
        assert_eq!(asked(Some("bytes=0-1"), 0), Asked::Whole);
    }

    #[test]
    fn a_long_film_comes_in_parts() {
        let len = 100 * PART;
        assert_eq!(asked(Some("bytes=0-"), len), Asked::Part(0, PART - 1));
        assert_eq!(asked(Some(&format!("bytes={PART}-")), len), Asked::Part(PART, 2 * PART - 1));
    }

    #[test]
    fn macos_asks_first_in_its_protected_places() {
        let _mac = os::AsIf::new(os::Os::MacOs);
        let asks = |p: &str| asks_first(Path::new(p));
        assert!(asks("/Users/me/Downloads/clip.mp4"));
        assert!(asks("/Users/me/Desktop/shot.png"));
        assert!(asks("/Users/me/Documents/northwind/after.png"));
        assert!(asks("/Users/me/Library/Mobile Documents/com~apple~CloudDocs/a.png"));
        assert!(asks("/Users/me/Library/Containers/com.example.atlas/Data/a.png"));
        assert!(asks("/Volumes/Atlas/renders/b.mov"));
        assert!(!asks("/private/tmp/scratchpad/teaser.mp4"));
        assert!(!asks("/Users/me/Library/Application Support/Codealong/chats/videos/a.mp4"));
        assert!(!asks("/Users/me/projects/kestrel/Downloads/a.png"));
        assert!(!asks("/Users/me/DownloadsArchive/a.png"));
        assert!(!asks("/Users/me"));
        os::as_if(os::Os::Linux, || assert!(!asks("/Users/me/Downloads/clip.mp4")));
    }

    #[test]
    fn a_file_is_answered_in_part_or_whole() {
        let dir = std::env::temp_dir().join(format!("codealong-sentfiles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let film = dir.join("teaser.mp4");
        std::fs::write(&film, b"0123456789").unwrap();
        let url = format!("/{}", urlencoding::encode(film.to_str().unwrap()));

        let part = respond(&url, false, Some("bytes=2-5")).unwrap();
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(part.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(part.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(part.body(), b"2345");

        let whole = respond(&url, false, None).unwrap();
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.body(), b"0123456789");

        assert_eq!(respond(&url, false, Some("bytes=10-")).unwrap().status(), StatusCode::RANGE_NOT_SATISFIABLE);

        let notes = dir.join("notes.txt");
        std::fs::write(&notes, b"secret").unwrap();
        assert_eq!(respond(&format!("/{}", urlencoding::encode(notes.to_str().unwrap())), false, None).unwrap_err(), StatusCode::FORBIDDEN);
        let gone = format!("/{}", urlencoding::encode(dir.join("gone.png").to_str().unwrap()));
        assert_eq!(respond(&gone, false, None).unwrap_err(), StatusCode::NOT_FOUND);

        // A link named as a picture to a file that isn't one is neither shown nor served.
        let link = dir.join("preview.png");
        os::files::symlink(&notes, &link).unwrap();
        assert_eq!(respond(&format!("/{}", urlencoding::encode(link.to_str().unwrap())), false, None).unwrap_err(), StatusCode::FORBIDDEN);
        let states = states(&[film.to_str().unwrap().into(), link.to_str().unwrap().into(), notes.to_str().unwrap().into(), dir.join("gone.png").to_str().unwrap().into(), "shots/after.png".into()]);
        assert_eq!(states, [State::Here, State::Gone, State::Gone, State::Gone, State::Gone]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
