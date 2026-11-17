//! The pictures, films and recordings a session sent you (Claude Code's `SendUserFile`), served to
//! the windows at `codealong-file://localhost/<the full path, percent-encoded>` so a conversation
//! shows them in place (src/ui/SentFiles.tsx). Only a file whose name says it's one of those
//! (`mime`, which src/sentFiles.ts `mediaKind` mirrors) is read, so nothing else on the Mac can be
//! read through it; every other file a session sends stays a link that opens it in its app.
//!
//! WebKit asks for a film a range of bytes at a time: two bytes first, then a few here and there
//! as it reads the film's index, a dozen asks before it can show a frame (macOS 26.6, an offscreen
//! WKWebView, 2026-11-16). Each answered with the whole file it still plays, but reads all of a
//! long film for every ask. So a request with a range is answered with that part, at most `PART`
//! of it (WebKit asks on for the rest), and a picture, asked for without one, whole.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use tauri::http::{header, Response, StatusCode};

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
    let range = request.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).map(String::from);
    std::thread::spawn(move || {
        let response = respond(&path, range.as_deref()).unwrap_or_else(|status| status_only(status));
        responder.respond(response);
    });
}

fn respond(path: &str, range: Option<&str>) -> Result<Response<Vec<u8>>, StatusCode> {
    let path = requested(path).ok_or(StatusCode::BAD_REQUEST)?;
    let mime = mime(&path).ok_or(StatusCode::FORBIDDEN)?;
    let mut file = File::open(&path).map_err(|_| StatusCode::NOT_FOUND)?;
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
    fn a_file_is_answered_in_part_or_whole() {
        let dir = std::env::temp_dir().join(format!("codealong-sentfiles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let film = dir.join("teaser.mp4");
        std::fs::write(&film, b"0123456789").unwrap();
        let url = format!("/{}", urlencoding::encode(film.to_str().unwrap()));

        let part = respond(&url, Some("bytes=2-5")).unwrap();
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(part.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(part.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(part.body(), b"2345");

        let whole = respond(&url, None).unwrap();
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.body(), b"0123456789");

        assert_eq!(respond(&url, Some("bytes=10-")).unwrap().status(), StatusCode::RANGE_NOT_SATISFIABLE);

        let notes = dir.join("notes.txt");
        std::fs::write(&notes, b"secret").unwrap();
        assert_eq!(respond(&format!("/{}", urlencoding::encode(notes.to_str().unwrap())), None).unwrap_err(), StatusCode::FORBIDDEN);
        let gone = format!("/{}", urlencoding::encode(dir.join("gone.png").to_str().unwrap()));
        assert_eq!(respond(&gone, None).unwrap_err(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
