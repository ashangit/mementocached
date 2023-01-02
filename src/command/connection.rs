use bytes::BytesMut;
use protobuf::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tracing::debug;

use crate::protos::kv::Request;
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

    pub async fn read_request(&mut self) -> Result<Option<Request>, Error> {
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
                let request: Request = Message::parse_from_bytes(&buffer).unwrap();

                return Ok(Some(request));
            }
        }
    }

    pub async fn write_reply(&mut self, reply: Vec<u8>) -> Result<(), Error> {
        self.stream.write_all(reply.as_slice()).await?;
        self.stream.flush().await?;

        Ok(())
    }
}
