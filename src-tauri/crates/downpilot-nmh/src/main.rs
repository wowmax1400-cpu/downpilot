//! DownPilot native messaging host (NMH) relay.
//!
//! Chrome spawns this binary and speaks the native messaging protocol on
//! stdin/stdout: every message is a 4-byte little-endian length prefix
//! followed by a UTF-8 JSON payload. This relay forwards each frame verbatim
//! over a loopback TCP connection to the DownPilot app (127.0.0.1:19851),
//! which uses the exact same framing, so no parsing happens here.
//!
//! If the app is not running, the relay tries to launch it (downpilot on
//! PATH) and retries briefly before answering with an error frame.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const HOST: &str = "127.0.0.1:19851";

fn read_frame(r: &mut dyn Read) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

fn write_frame(w: &mut dyn Write, data: &[u8]) -> io::Result<()> {
    w.write_all(&(data.len() as u32).to_le_bytes())?;
    w.write_all(data)?;
    w.flush()
}

fn connect_with_retry() -> io::Result<TcpStream> {
    for i in 0..6 {
        match TcpStream::connect(HOST) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Err(_e) if i == 0 => {
                // App not running: try to launch it once.
                for bin in ["downpilot", "downpilot.AppImage"] {
                    if std::process::Command::new(bin).spawn().is_ok() {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(700));
            }
            Err(e) if i == 5 => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(400)),
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "downpilot app not reachable"))
}

fn main() {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    let mut stream = match connect_with_retry() {
        Ok(s) => s,
        Err(e) => {
            let err = format!(r#"{{"ok":false,"error":"DownPilot is not running: {e}"}}"#);
            let _ = write_frame(&mut stdout, err.as_bytes());
            return;
        }
    };

    while let Ok(Some(frame)) = read_frame(&mut stdin) {
        if stream.write_all(&(frame.len() as u32).to_le_bytes()).and_then(|_| stream.write_all(&frame)).is_err() {
            let _ = write_frame(&mut stdout, br#"{"ok":false,"error":"connection to DownPilot lost"}"#);
            return;
        }
        match read_frame(&mut stream) {
            Ok(Some(resp)) => {
                if write_frame(&mut stdout, &resp).is_err() {
                    return;
                }
            }
            _ => {
                let _ = write_frame(&mut stdout, br#"{"ok":false,"error":"no response from DownPilot"}"#);
                return;
            }
        }
    }
}
