use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tracing::debug;

use crate::Error;

#[derive(Debug)]
pub struct Connection {
    stream: BufWriter<TcpStream>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            stream: BufWriter::new(stream),
        }
    }

    pub async fn read_message(&mut self) -> Result<Option<BytesMut>, Error> {
        let request_size: usize = self.stream.read_u64().await? as usize;

        debug!(size = request_size, "Request size");

        if 0 == request_size {
            return Ok(None);
        }

        let mut buffer = BytesMut::with_capacity(request_size);

        loop {
            if 0 == self.stream.read_buf(&mut buffer).await? {
                // The remote closed the connection. For this to be a clean
                // shutdown, there should be no data in the read buffer. If
                // there is, this means that the peer closed the socket while
                // sending a frame.
                if buffer.is_empty() {
                    return Ok(None);
                } else {
                    return Err("Connection reset by peer".into());
                }
            }

            if buffer.len() == buffer.capacity() {
                return Ok(Some(buffer));
            }
        }
    }

    pub async fn write_message(&mut self, reply: Vec<u8>) -> Result<(), Error> {
        let slice = [reply.len().to_be_bytes().as_slice(), reply.as_slice()].concat();
        self.stream.write_all(slice.as_slice()).await?;
        self.stream.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    async fn write_read_message() {
        let message_send = "test_write_read_message";

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let _ = thread::Builder::new().spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .unwrap();
            rt.block_on(async {
                let socket_client = TcpStream::connect(addr).await.unwrap();
                let mut connection_client = Connection::new(socket_client);

                connection_client
                    .write_message(message_send.as_bytes().to_vec())
                    .await
                    .unwrap();
            });
        });

        let (stream, _) = listener.accept().await.unwrap();
        let mut connection_server = Connection::new(stream);

        let buffer = connection_server.read_message().await.unwrap().unwrap();
        let message_rcv = std::str::from_utf8(&buffer).unwrap();

        assert_eq!(message_send, message_rcv);
    }

    #[tokio::test]
    async fn write_read_message_multiple_step() {
        let message_send = "test_write_read_message";

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let _ = thread::Builder::new().spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .unwrap();
            rt.block_on(async {
                let mut socket_client = TcpStream::connect(addr).await.unwrap();
                socket_client
                    .write_all(message_send.len().to_be_bytes().as_slice())
                    .await
                    .unwrap();
                socket_client.flush().await.unwrap();
                socket_client
                    .write_all("test_write_".as_bytes())
                    .await
                    .unwrap();
                socket_client.flush().await.unwrap();
                socket_client
                    .write_all("read_message".as_bytes())
                    .await
                    .unwrap();
                socket_client.flush().await.unwrap();
            });
        });

        let (stream, _) = listener.accept().await.unwrap();
        let mut connection_server = Connection::new(stream);

        let buffer = connection_server.read_message().await.unwrap().unwrap();
        let message_rcv = std::str::from_utf8(&buffer).unwrap();

        assert_eq!(message_send, message_rcv);
    }

    #[tokio::test]
    async fn unexpected_eof() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let _ = thread::Builder::new().spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .unwrap();
            rt.block_on(async {
                let mut socket_client = TcpStream::connect(addr).await.unwrap();
                socket_client
                    .write_all(8_i32.to_be_bytes().as_slice())
                    .await
                    .unwrap();
                socket_client.flush().await.unwrap();
            });
        });

        let (stream, _) = listener.accept().await.unwrap();
        let mut connection_server = Connection::new(stream);

        let buffer = connection_server.read_message().await;
        assert!(buffer.is_err())
    }
}