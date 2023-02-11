use std::thread;

use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

use crate::Error;

pub struct ClientStream {
    pub addr: String,
    pub stream: TcpStream,
}

pub struct SocketReaderRuntime {
    addr: String,
    sockets_db_workers_tx: async_channel::Sender<ClientStream>,
}

impl SocketReaderRuntime {
    /// Create a new socket runtime reader
    ///
    /// # Arguments
    ///
    /// * `addr` - the listen socket
    ///  * `workers_channel` - list of workers channel
    ///
    /// # Return
    ///
    /// * Result<SocketRuntimeReader, Error>
    ///
    pub fn new(addr: String, sockets_db_workers_tx: async_channel::Sender<ClientStream>) -> Self {
        SocketReaderRuntime {
            addr,
            sockets_db_workers_tx,
        }
    }

    /// Start the socket reader runtime
    /// It is in charge opening the DB socket and create a dedicated task (SocketProcessor)
    /// for any connection accepted
    ///
    /// # Return
    ///
    /// * Result<(), Error>
    ///
    pub fn start(&mut self) -> Result<(), Error> {
        let addr = self.addr.clone();

        let sockets_db_workers_tx = self.sockets_db_workers_tx.clone();

        let _ = thread::Builder::new()
            .name("socket-reader".to_string())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .build()
                    .unwrap();

                rt.block_on(async move {
                    info!(socket = addr, "Start listening");
                    let listener = TcpListener::bind(addr.clone()).await.unwrap();
                    loop {
                        let (stream, client_addr) = listener.accept().await.unwrap();

                        debug!(
                            client_addr = client_addr.to_string(),
                            "Accept connection from client"
                        );

                        let client_stream = ClientStream {
                            addr: client_addr.to_string(),
                            stream,
                        };
                        sockets_db_workers_tx.send(client_stream).await.unwrap();
                    }
                });
            });
        Ok(())
    }
}
