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
    pub fn new(stream: TcpStream) -> Connection {
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
