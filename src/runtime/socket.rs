use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tracing::{debug, info};

use crate::Error;

pub struct ClientStream {
    pub addr: String,
    pub stream: TcpStream,
}

pub struct SocketReaderRuntime {
    addr: String,
    sockets_db_workers_tx: async_channel::Sender<ClientStream>,
    rt: Runtime,
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
    pub fn new(
        addr: String,
        sockets_db_workers_tx: async_channel::Sender<ClientStream>,
    ) -> Result<Self, Error> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .thread_name_fn(|| {
                static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
                let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
                format!("socket-reader-{id}")
            })
            .build()?;

        Ok(SocketReaderRuntime {
            addr,
            sockets_db_workers_tx,
            rt,
        })
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

        let sender_mpmc = self.sockets_db_workers_tx.clone();

        self.rt.spawn(async move {
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
                sender_mpmc.send(client_stream).await.unwrap();
            }
        });
        Ok(())
    }
}
