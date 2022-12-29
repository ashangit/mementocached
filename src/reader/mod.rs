use crate::command::CommandProcess;
use crate::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Sender;

pub struct SocketReader {
    addr: String,
    workers_channel: Vec<Sender<CommandProcess>>,
    socket_rt: Runtime,
}

impl SocketReader {
    /// Create a new Set command
    ///
    /// # Arguments
    ///
    /// * `addr` - the listen socket
    ///
    /// # Return
    ///
    /// * Result<SocketReader, Error>
    ///
    pub fn new(
        addr: String,
        workers_channel: Vec<Sender<CommandProcess>>,
    ) -> Result<SocketReader, Error> {
        let socket_rt = tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .thread_name_fn(|| {
                static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
                let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
                format!("socket-reader-{}", id)
            })
            .build()?;

        Ok(SocketReader {
            addr,
            workers_channel,
            socket_rt,
        })
    }

    pub fn start(&mut self) -> Result<(), Error> {
        let addr = self.addr.clone();
        let workers_channel = self.workers_channel.clone();
        self.socket_rt.block_on(async move {
            let listener = TcpListener::bind(addr.clone()).await.unwrap();

            println!("Listening");

            loop {
                let (_socket, _) = listener.accept().await.unwrap();

                println!("Accepted");
                let _workers_channel = workers_channel.clone();
                tokio::spawn(async move {
                    //process(socket, workers_channel).await;
                });
            }
        })
    }
}
