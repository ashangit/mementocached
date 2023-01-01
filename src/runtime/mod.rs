use crate::command::connection::Connection;
use crate::command::{Command, CommandProcess, Delete, Get, Set};
use crate::metrics::init_prometheus_http_endpoint;
use crate::protos::kv;
use crate::protos::kv::Request;
use crate::Error;
use ahash::AHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

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
                let (stream, client_addr) = listener.accept().await.unwrap();

                let workers_channel = workers_channel.clone();
                tokio::spawn(async move {
                    SocketProcessor::new(stream, workers_channel, client_addr.to_string())
                        .read()
                        .await;
                });
            }
        });
        Ok(())
    }
}

pub struct SocketProcessor {
    connection: Connection,
    workers_channel: Vec<Sender<CommandProcess>>,
    client_addr: String,
}

impl SocketProcessor {
    /// Create a new socket processor
    ///
    /// # Arguments
    ///
    /// * `stream` - tcp stream to client
    /// * `workers_channel` - list of workers channel
    /// * `client_addr` - the addr of the client
    ///
    /// # Return
    ///
    /// * SocketProcessor
    ///
    pub fn new(
        stream: TcpStream,
        workers_channel: Vec<Sender<CommandProcess>>,
        client_addr: String,
    ) -> SocketProcessor {
        info!(client_addr = client_addr, "Accept connection from client");
        let connection = Connection::new(stream);

        SocketProcessor {
            connection,
            workers_channel,
            client_addr,
        }
    }

    pub async fn read(&mut self) {
        loop {
            match self.connection.read_request().await {
                Ok(Some(request)) => {
                    info!("processing request");
                    self.process(request);
                }
                Ok(None) => {
                    info!(client_addr = self.client_addr, "Disconnect from client");
                    return;
                }
                Err(issue) => {
                    error!(
                        client_addr = self.client_addr,
                        issue = issue,
                        "Failure processing event from client"
                    );
                    return;
                }
            }
        }
    }

    async fn process(&mut self, request: Request) {
        let (resp_tx, resp_rx) = oneshot::channel();

        match request.command.unwrap() {
            kv::request::Command::Get(x) => {
                // Send the SET request
                let modulo: usize = self.calculate_modulo(&x.key);

                let cmd = Command::Get(Get { request: x });
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
            kv::request::Command::Set(x) => {
                // Send the GET request
                let modulo: usize = self.calculate_modulo(&x.key);

                let cmd = Command::Set(Set { request: x });
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
            kv::request::Command::Delete(x) => {
                // Send the DELETE request
                let modulo: usize = self.calculate_modulo(&x.key);

                let cmd = Command::Delete(Delete { request: x });
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
        }

        // Await the response
        match resp_rx.await {
            Ok(x) => {
                let reply = x.unwrap();

                // Write the response to the client
                //
                self.connection.write_reply(reply).await.unwrap();
            }
            Err(_) => (),
        }
    }

    fn calculate_modulo<T: Hash>(&mut self, t: T) -> usize {
        Self::calculate_hash(t) % self.workers_channel.len()
    }

    fn calculate_hash<T: Hash>(t: T) -> usize {
        let mut hasher = AHasher::default();
        t.hash(&mut hasher);
        hasher.finish() as usize
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
