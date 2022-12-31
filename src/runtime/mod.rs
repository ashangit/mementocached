use crate::command::CommandProcess;
use crate::metrics::init_prometheus_http_endpoint;
use crate::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{error, info};

pub struct SocketReaderRuntime {
    addr: String,
    workers_channel: Vec<Sender<CommandProcess>>,
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
        workers_channel: Vec<Sender<CommandProcess>>,
    ) -> Result<SocketReaderRuntime, Error> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .thread_name_fn(|| {
                static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
                let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
                format!("socket-reader-{}", id)
            })
            .build()?;

        Ok(SocketReaderRuntime {
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
    /// * `port` - the listen port
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

pub struct DBManagerRuntime {
    nb_workers: usize,
    worker_channel_buffer_size: usize,
    pub workers_channel: Vec<Sender<CommandProcess>>,
}

impl DBManagerRuntime {
    /// Create a new db manager runtime
    ///
    /// # Arguments
    ///
    ///
    /// # Return
    ///
    /// * Result<DBManagerRuntime, Error>
    ///
    pub fn new(nb_workers: usize) -> Result<DBManagerRuntime, Error> {
        let workers_channel: Vec<Sender<CommandProcess>> = Vec::new();
        let worker_channel_buffer_size: usize = 32;

        Ok(DBManagerRuntime {
            nb_workers,
            worker_channel_buffer_size,
            workers_channel,
        })
    }

    pub fn start(&mut self) -> Result<(), Error> {
        for worker_index in 1..=self.nb_workers {
            let (tx, rx) = mpsc::channel::<CommandProcess>(self.worker_channel_buffer_size);
            self.workers_channel.push(tx);

            let _ = thread::Builder::new()
                .name(format!("worker-{worker_index}"))
                .spawn(|| {
                    let worker_rt_res = DBWorkerRuntime::new(rx);
                    match worker_rt_res {
                        Ok(mut worker_rt) => {
                            worker_rt.start();
                        }
                        Err(issue) => {
                            error!("Failed to create worker db {}", issue)
                        }
                    }
                });
        }
        Ok(())
    }
}

struct DBWorkerRuntime {
    rt: Runtime,
    rx: Receiver<CommandProcess>,
}

impl DBWorkerRuntime {
    /// Create a new db worker runtime
    ///
    /// # Arguments
    ///
    ///
    /// # Return
    ///
    /// * Result<DBWorkerRuntime, Error>
    ///
    pub fn new(rx: Receiver<CommandProcess>) -> Result<DBWorkerRuntime, Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()?;

        Ok(DBWorkerRuntime { rt, rx })
    }

    pub fn start(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
