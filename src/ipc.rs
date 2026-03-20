use std::{io, path::PathBuf};

use log::{debug, error, info};
use tokio::net::{UnixListener, UnixStream};
use tokio::runtime::Handle;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

use crate::events::{Event, EventManager};

pub struct IpcSocket {
    path: PathBuf,
}

impl Drop for IpcSocket {
    fn drop(&mut self) {
        self.try_remove_socket();
    }
}

impl IpcSocket {
    pub fn new(handle: &Handle, path: PathBuf, ev: EventManager) -> io::Result<Self> {
        let path = if path.exists() && Self::is_open_socket(&path) {
            let mut new_path = path;
            new_path.set_file_name(format!("ncytm.{}.sock", std::process::id()));
            new_path
        } else if path.exists() && !Self::is_open_socket(&path) {
            std::fs::remove_file(&path)?;
            path
        } else {
            path
        };

        info!("Creating IPC domain socket at {path:?}");

        let listener_path = path.clone();
        handle.spawn(async move {
            let listener =
                UnixListener::bind(listener_path).expect("Could not create IPC domain socket");
            Self::worker(listener, ev).await;
        });

        Ok(Self { path })
    }

    fn is_open_socket(path: &PathBuf) -> bool {
        std::os::unix::net::UnixStream::connect(path).is_ok()
    }

    async fn worker(listener: UnixListener, ev: EventManager) {
        loop {
            match listener.accept().await {
                Ok((stream, sockaddr)) => {
                    debug!("Connection from {sockaddr:?}");
                    tokio::spawn(Self::stream_handler(stream, ev.clone()));
                }
                Err(e) => error!("Error accepting connection: {e}"),
            }
        }
    }

    async fn stream_handler(stream: UnixStream, ev: EventManager) -> Result<(), String> {
        let (reader, _writer) = stream.into_split();
        let mut framed_reader = FramedRead::new(reader, LinesCodec::new());

        loop {
            match framed_reader.next().await {
                Some(Ok(line)) => {
                    debug!("Received line: \"{line}\"");
                    ev.send(Event::IpcInput(line));
                }
                Some(Err(e)) => {
                    error!("Error reading line: {e}");
                    return Err(e.to_string());
                }
                None => {
                    debug!("Closing IPC connection");
                    return Ok(());
                }
            }
        }
    }

    /// Try to remove the IPC socket if there is one for this instance of `ncytm`. Don't do
    /// anything if the socket has already been removed for some reason.
    fn try_remove_socket(&mut self) {
        if std::fs::remove_file(&self.path).is_ok() {
            info!("removed socket at {:?}", self.path);
        } else {
            info!("socket already removed");
        }
    }
}
