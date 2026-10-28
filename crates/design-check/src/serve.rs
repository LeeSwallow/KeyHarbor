//! A tiny static file server over std, for looking at design-system/ in a browser and for the render
//! smoke test. GET only, one thread per connection, bound to loopback. Not for anything else.
//! It serves only the folders in `SERVED` (not the rest of the checkout, not .git) and never
//! follows a symbolic link.

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

/// The folders under the root that are served: the design system, and the app icon the
/// showcase header shows (`../docs/design/assets/app-icon.svg`).
const SERVED: &[&str] = &["design-system", "docs/design/assets"];

/// Serves the `SERVED` folders of `root` on `addr` (use port 0 for any free port) from a background thread.
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

/// Maps a request path to a file under `root`, refusing anything that climbs out of it, lies
/// outside the `SERVED` folders, or passes through a symbolic link.
fn file_for(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    if !SERVED.iter().any(|dir| rel.starts_with(dir)) {
        return None;
    }
    let mut full = root.to_path_buf();
    for part in rel.components() {
        full.push(part);
        if is_symlink(&full) {
            return None;
        }
    }
    if full.is_dir() {
        full.push("index.html");
        if is_symlink(&full) {
            return None;
        }
    }
    Some(full)
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_paths_that_leave_the_root() {
        let root = Path::new("/srv");
        assert_eq!(
            file_for(root, "/design-system/index.html?theme=dark"),
            Some(PathBuf::from("/srv/design-system/index.html"))
        );
        assert_eq!(file_for(root, "/../etc/passwd"), None);
        assert_eq!(file_for(root, "/design-system/../../x"), None);
    }

    #[test]
    fn serves_only_the_design_system() {
        let root = Path::new("/srv");
        assert_eq!(
            file_for(root, "/docs/design/assets/app-icon.svg"),
            Some(PathBuf::from("/srv/docs/design/assets/app-icon.svg"))
        );
        for outside in [
            "/",
            "/.git/config",
            "/Cargo.toml",
            "/crates/cli/src/main.rs",
            "/design-system-x/index.html",
            "/docs/design/README.md",
        ] {
            assert_eq!(file_for(root, outside), None, "{outside}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symbolic_links() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let secret = root.join("secret.txt");
        fs::write(&secret, "not for the browser").unwrap();
        fs::create_dir_all(root.join("design-system/icons")).unwrap();
        fs::write(root.join("design-system/index.html"), "<p>ok</p>").unwrap();
        symlink(&secret, root.join("design-system/link.txt")).unwrap();
        symlink(root, root.join("design-system/up")).unwrap();
        symlink(&secret, root.join("design-system/icons/index.html")).unwrap();
        assert_eq!(
            file_for(root, "/design-system/index.html"),
            Some(root.join("design-system/index.html"))
        );
        assert_eq!(file_for(root, "/design-system/link.txt"), None);
        assert_eq!(file_for(root, "/design-system/up/secret.txt"), None);
        assert_eq!(file_for(root, "/design-system/icons/"), None);
    }
}
