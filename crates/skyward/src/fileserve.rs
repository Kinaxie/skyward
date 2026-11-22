//! A folder served to the network over HTTP, so a computer on the same WLAN
//! can take a package or an image off the phone without a cable. Read-only:
//! a request names a file, and the file is sent.

use anyhow::{Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// How long the listener waits between looks at its stop flag.
const TICK: Duration = Duration::from_millis(100);

pub struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    port: u16,
}

impl Server {
    /// Serve `root` on `port`, on every address the phone has.
    pub fn start(root: PathBuf, port: u16) -> Result<Server> {
        let root = root.canonicalize().with_context(|| format!("open the folder served at {}", root.display()))?;
        let listener = TcpListener::bind(("0.0.0.0", port)).with_context(|| format!("listen on port {port}"))?;
        listener.set_nonblocking(true).context("make the file server non-blocking")?;
        let stop = Arc::new(AtomicBool::new(false));
        let told = stop.clone();
        let served = root.clone();
        let thread = thread::Builder::new()
            .name("fileserve".into())
            .spawn(move || {
                while !told.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let root = served.clone();
                            let _ = thread::Builder::new().name("fileserve-conn".into()).spawn(move || {
                                if let Err(e) = answer(stream, &root) {
                                    eprintln!("fileserve: {e:#}");
                                }
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(TICK),
                        Err(e) => {
                            eprintln!("fileserve: accept: {e}");
                            thread::sleep(TICK);
                        }
                    }
                }
            })
            .context("start the file server")?;
        Ok(Server { stop, thread: Some(thread), port })
    }

    /// The address a computer on the same network opens: the phone's own
    /// address on the network its route goes out by, and the port.
    pub fn url(&self) -> String {
        let ip = UdpSocket::bind("0.0.0.0:0")
            .and_then(|socket| {
                socket.connect("8.8.8.8:80")?;
                socket.local_addr()
            })
            .map(|at| at.ip().to_string())
            .unwrap_or_else(|_| "127.0.0.1".into());
        format!("http://{ip}:{}/", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One request: read its head, send the page or file it asks for, and close.
fn answer(mut stream: TcpStream, root: &Path) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10))).context("set the request's wait")?;
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < 8192 {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e).context("read the request"),
        }
    }
    let head = String::from_utf8_lossy(&head);
    let mut lines = head.lines();
    let Some(line) = lines.next() else { return reply(&mut stream, 400, "text/plain", b"no request\n") };
    let mut words = line.split_whitespace();
    let (Some(method), Some(target)) = (words.next(), words.next()) else {
        return reply(&mut stream, 400, "text/plain", b"bad request\n");
    };
    if method != "GET" && method != "HEAD" {
        return reply(&mut stream, 405, "text/plain", b"only GET and HEAD\n");
    }
    let Some(path) = requested(root, target) else {
        return reply(&mut stream, 404, "text/plain", b"not found\n");
    };
    let meta = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(_) => return reply(&mut stream, 404, "text/plain", b"not found\n"),
    };
    if meta.is_dir() {
        let page = listing(root, &path)?;
        return if method == "HEAD" {
            reply(&mut stream, 200, "text/html; charset=utf-8", b"")
        } else {
            reply(&mut stream, 200, "text/html; charset=utf-8", page.as_bytes())
        };
    }
    let mut file = fs::File::open(&path).with_context(|| format!("open {}", path.display()))?;
    let len = meta.len();
    let kind = content_type(&path);
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
    stream.write_all(head.as_bytes()).context("write the response head")?;
    if method == "GET" {
        let mut buf = [0u8; 64 << 10];
        loop {
            let n = file.read(&mut buf).context("read the file served")?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).context("write the file served")?;
        }
    }
    Ok(())
}

fn reply(stream: &mut TcpStream, code: u16, kind: &str, body: &[u8]) -> Result<()> {
    let why = match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let head = format!("HTTP/1.1 {code} {why}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(head.as_bytes()).context("write the response head")?;
    stream.write_all(body).context("write the response body")
}

/// The file or folder a request's target names under `root`; none where it
/// climbs out of it or resolves to a place outside it.
fn requested(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let decoded = percent_decode(path);
    let mut out = root.to_path_buf();
    for part in decoded.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            name => out.push(name),
        }
    }
    let resolved = out.canonicalize().ok()?;
    resolved.starts_with(root).then_some(resolved)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && at + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[at + 1]), hex(bytes[at + 2]))
        {
            out.push(hi << 4 | lo);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A folder as a page: the way up, then each entry as a link.
fn listing(root: &Path, dir: &Path) -> Result<String> {
    let mut names: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    names.sort_by_key(|e| e.file_name());
    let here = dir.strip_prefix(root).unwrap_or(Path::new(""));
    let mut page = String::from(
        "<!doctype html><meta name=viewport content=\"width=device-width\"><title>Skyward</title>\
         <style>body{font:16px system-ui;margin:24px}td{padding:4px 12px 4px 0}a{color:#1a73e8}</style><h1>Skyward</h1><p>",
    );
    page.push_str(&escape(&format!("/{}", here.display())));
    page.push_str("</p><table>");
    if !here.as_os_str().is_empty() {
        let up = here.parent().unwrap_or(Path::new(""));
        page.push_str(&format!("<tr><td><a href=\"/{}/\">..</a></td><td></td></tr>", escape(&up.display().to_string())));
    }
    for entry in names {
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let shown = escape(&name);
        let mut link = format!("/{}/{}", here.display(), name);
        if meta.is_dir() {
            link.push('/');
        }
        page.push_str(&format!(
            "<tr><td><a href=\"{}\">{}{}</a></td><td>{}</td></tr>",
            escape(&link),
            shown,
            if meta.is_dir() { "/" } else { "" },
            if meta.is_dir() { String::new() } else { size(meta.len()) }
        ));
    }
    page.push_str("</table>");
    Ok(page)
}

fn size(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "txt" | "log" | "md" => "text/plain; charset=utf-8",
        "zip" | "apk" | "jar" => "application/zip",
        "img" | "bin" => "application/octet-stream",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}
