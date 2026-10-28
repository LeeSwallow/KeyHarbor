//! A tiny static file server over std, for looking at design/ in a browser and for the render
//! smoke test. GET only, one thread per connection, bound to loopback. Not for anything else.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::{fs, io, thread};

const TYPES: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("json", "application/json"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
];

/// Serves `root` on `addr` (use port 0 for any free port) from a background thread.
/// Returns the base URL, e.g. `http://127.0.0.1:53211`.
pub fn spawn(root: PathBuf, addr: &str) -> io::Result<String> {
    let listener = TcpListener::bind(addr)?;
    let local: SocketAddr = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            thread::spawn(move || {
                let _ = respond(&root, stream);
            });
        }
    });
    Ok(format!("http://{local}"))
}

fn respond(root: &Path, mut stream: TcpStream) -> io::Result<()> {
    let mut request = String::new();
    let mut reader = BufReader::new(&stream);
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    let target = request.split_whitespace().nth(1).unwrap_or("/");
    match file_for(root, target).and_then(|p| fs::read(&p).ok().map(|body| (p, body))) {
        Some((path, body)) => {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let kind = TYPES
                .iter()
                .find(|(e, _)| *e == ext)
                .map_or("application/octet-stream", |(_, t)| t);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            stream.write_all(&body)
        }
        None => stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    }
}

/// Maps a request path to a file under `root`, refusing anything that climbs out of it.
fn file_for(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let full = root.join(rel);
    if full.is_dir() {
        Some(full.join("index.html"))
    } else {
        Some(full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_paths_that_leave_the_root() {
        let root = Path::new("/srv");
        assert_eq!(
            file_for(root, "/design/index.html?theme=dark"),
            Some(PathBuf::from("/srv/design/index.html"))
        );
        assert_eq!(file_for(root, "/../etc/passwd"), None);
        assert_eq!(file_for(root, "/design/../../x"), None);
    }
}
