use crate::command::CommandProcess;
use crate::metrics::init_prometheus_http_endpoint;
use crate::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Sender;
use tracing::{error, info};

pub struct SocketRuntimeReader {
    addr: String,
    workers_channel: Vec<Sender<CommandProcess>>,
    rt: Runtime,
}

impl SocketRuntimeReader {
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
        workers_channel: Vec<Sender<CommandProcess>>,
    ) -> Result<SocketRuntimeReader, Error> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .thread_name_fn(|| {
                static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
                let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
                format!("socket-reader-{}", id)
            })
            .build()?;

        Ok(SocketRuntimeReader {
            addr,
            workers_channel,
            rt,
        })
    }

    pub fn start(&mut self) -> Result<(), Error> {
        let addr = self.addr.clone();
        let workers_channel = self.workers_channel.clone();
        self.rt.spawn(async move {
            info!(socket = addr, "Start listening");
            let listener = TcpListener::bind(addr.clone()).await.unwrap();

            loop {
                let (_socket, client_addr) = listener.accept().await.unwrap();

                let _workers_channel = workers_channel.clone();
                tokio::spawn(async move {
                    info!(
                        client_addr = client_addr.to_string(),
                        "Accept connection from client"
                    );
                    //process(socket, workers_channel).await;
                });
            }
        });
        Ok(())
    }
}

pub struct CoreRuntime {
    port: u16,
    rt: Runtime,
}

impl CoreRuntime {
    /// Create a new core runtime
    ///
    /// # Arguments
    ///
    ///
    /// # Return
    ///
    /// * Result<CoreRuntime, Error>
    ///
    pub fn new(port: u16) -> Result<CoreRuntime, Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("core")
            .build()?;

        Ok(CoreRuntime { port, rt })
    }

    pub fn start(&mut self) -> Result<(), Error> {
        let port = self.port;
        self.rt.block_on(async move {
            if let Err(issue) = init_prometheus_http_endpoint(port).await {
                error!("Issue to start prometheus http endpoint due to {}", issue);
                std::process::abort();
            }
        });
        Ok(())
    }
}
