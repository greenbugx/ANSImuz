use std::{io, time::Duration};

use interprocess::TryClone;
use interprocess::local_socket::{GenericFilePath, Stream, prelude::*};

#[cfg(unix)]
pub fn ipc_path(generation: u64) -> String {
    std::env::temp_dir()
        .join(format!(
            "ansimuz-mpv-{}-{generation}.sock",
            std::process::id()
        ))
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
pub fn ipc_path(generation: u64) -> String {
    format!(r"\\.\pipe\ansimuz-mpv-{}-{generation}", std::process::id())
}

#[cfg(not(any(unix, windows)))]
pub fn ipc_path(generation: u64) -> String {
    std::env::temp_dir()
        .join(format!(
            "ansimuz-mpv-{}-{generation}.sock",
            std::process::id()
        ))
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcEndpoint {
    endpoint: String,
}

impl IpcEndpoint {
    pub fn new(generation: u64) -> Self {
        Self {
            endpoint: ipc_path(generation),
        }
    }

    pub fn from_string(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub fn as_str(&self) -> &str {
        &self.endpoint
    }

    pub fn connect(&self) -> io::Result<IpcStream> {
        IpcStream::connect(&self.endpoint)
    }

    pub fn cleanup(&self) {
        #[cfg(unix)]
        {
            let path = std::path::Path::new(&self.endpoint);
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

pub struct IpcStream {
    inner: Stream,
}

impl IpcStream {
    pub fn connect(endpoint: &str) -> io::Result<Self> {
        let name = endpoint.to_fs_name::<GenericFilePath>()?;
        let inner = Stream::connect(name)?;
        Ok(Self { inner })
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.set_recv_timeout(timeout)
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.set_send_timeout(timeout)
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        let cloned = self.inner.try_clone()?;
        Ok(Self { inner: cloned })
    }
}

impl io::Read for IpcStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl io::Write for IpcStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_path_uniqueness() {
        let path1 = ipc_path(1);
        let path2 = ipc_path(2);
        assert_ne!(path1, path2);

        #[cfg(unix)]
        {
            assert!(path1.contains("-1.sock"));
            assert!(path2.contains("-2.sock"));
        }

        #[cfg(windows)]
        {
            assert!(path1.contains("-1"));
            assert!(path2.contains("-2"));
            assert!(path1.starts_with(r"\\.\pipe\"));
        }
    }

    #[test]
    fn test_endpoint_to_fs_name() {
        let endpoint = IpcEndpoint::new(42);
        let name_res = endpoint.as_str().to_fs_name::<GenericFilePath>();
        assert!(name_res.is_ok());
    }

    #[test]
    fn test_ipc_stream_roundtrip() {
        use interprocess::local_socket::ListenerOptions;
        use std::io::{BufRead, BufReader, Write};
        use std::thread;

        let endpoint = IpcEndpoint::new(9999);
        endpoint.cleanup();

        let name = endpoint
            .as_str()
            .to_fs_name::<GenericFilePath>()
            .expect("valid endpoint name");
        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .expect("create listener");

        let endpoint_client = endpoint.clone();
        let client_handle = thread::spawn(move || {
            let mut stream = endpoint_client.connect().expect("client connect");
            stream.write_all(b"ping\n").expect("client write");
            let mut reader = BufReader::new(stream);
            let mut reply = String::new();
            reader.read_line(&mut reply).expect("client read");
            reply
        });

        let mut server_stream = listener.accept().expect("server accept");
        let mut server_reader = BufReader::new(server_stream.try_clone().expect("server clone"));
        let mut line = String::new();
        server_reader.read_line(&mut line).expect("server read");
        assert_eq!(line.trim(), "ping");

        server_stream.write_all(b"pong\n").expect("server write");
        let client_reply = client_handle.join().expect("join client");
        assert_eq!(client_reply.trim(), "pong");

        drop(listener);
        drop(server_stream);
        endpoint.cleanup();
    }

    #[test]
    fn test_duplex_concurrent_read_write() {
        use interprocess::local_socket::ListenerOptions;
        use std::io::{BufRead, BufReader, Write};
        use std::thread;

        let endpoint = IpcEndpoint::new(9998);
        endpoint.cleanup();

        let name = endpoint
            .as_str()
            .to_fs_name::<GenericFilePath>()
            .expect("valid endpoint name");
        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .expect("create listener");

        let endpoint_client = endpoint.clone();
        let client_handle = thread::spawn(move || {
            let stream = endpoint_client.connect().expect("client connect");
            let mut writer = stream.try_clone().expect("client clone");
            let mut reader = BufReader::new(stream);

            let read_handle = thread::spawn(move || {
                let mut received = Vec::new();
                for _ in 0..3 {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("client read line");
                    received.push(line.trim().to_string());
                }
                received
            });

            for i in 1..=3 {
                thread::sleep(Duration::from_millis(10));
                writer
                    .write_all(format!("msg {i}\n").as_bytes())
                    .expect("client write line");
            }

            read_handle.join().expect("join read handle")
        });

        let server_stream = listener.accept().expect("server accept");
        let mut server_writer = server_stream.try_clone().expect("server clone");
        let mut server_reader = BufReader::new(server_stream);

        for _ in 0..3 {
            let mut line = String::new();
            server_reader
                .read_line(&mut line)
                .expect("server read line");
            server_writer
                .write_all(format!("echo {}\n", line.trim()).as_bytes())
                .expect("server write line");
        }

        let received = client_handle.join().expect("join client");
        assert_eq!(received, vec!["echo msg 1", "echo msg 2", "echo msg 3"]);
        endpoint.cleanup();
    }
}
