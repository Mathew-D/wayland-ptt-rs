use std::io::{Read, Write};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

fn socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir).join("vesktop.sock");
    }
    #[cfg(unix)]
    {
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/run/user/{uid}/vesktop.sock"))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from("vesktop.sock")
    }
}

pub fn run_shortcut(action: &str, pressed: bool, verbose: bool) {
    #[cfg(not(unix))]
    {
        let _ = (action, pressed, verbose);
        return;
    }

    #[cfg(unix)]
    {
        let edge = if pressed { "down" } else { "up" };
        let line = format!("run-shortcut {action}:{edge}\n");
        match UnixStream::connect(socket_path()) {
            Ok(mut stream) => {
                let result = stream.write_all(line.as_bytes()).and_then(|()| {
                    // Vesktop replies "ok"/"error" per line; read it so closing
                    // the socket doesn't cause an EPIPE in its main process.
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
                    let mut buf = [0u8; 64];
                    stream.read(&mut buf).map(|_| ())
                });
                if let Err(err) = result {
                    if verbose {
                        eprintln!("Vesktop shortcut failed: {err}");
                    }
                } else if verbose {
                    eprintln!("Vesktop {action} {edge}");
                }
            }
            Err(err) => {
                if verbose {
                    eprintln!("Vesktop socket unavailable: {err}");
                }
            }
        }
    }
}
