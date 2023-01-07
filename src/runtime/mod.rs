use crate::command::connection::Connection;
use crate::command::{CommandProcess, DBAction, Delete, Get, Set};
use crate::metrics::{init_prometheus_http_endpoint, NUMBER_OF_REQUESTS};
use crate::protos::kv;
use crate::protos::kv::Request;
use crate::Error;
use ahash::{AHasher, HashMap, HashMapExt};
use protobuf::Message;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, error, info};

pub type Db = Arc<Mutex<HashMap<String, Vec<u8>>>>;

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
            match self.connection.read_message().await {
                Ok(Some(buffer)) => {
                    debug!("processing request");
                    let request: Request = Message::parse_from_bytes(&buffer).unwrap();
                    self.process(request).await;
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
                NUMBER_OF_REQUESTS.with_label_values(&["get"]).inc();

                // Send the GET request
                let modulo: usize = self.calculate_modulo(&x.key);
                debug!(modulo = modulo, "get");

                let cmd = DBAction::Get(Get { request: x });
                match self.workers_channel[modulo].send((cmd, resp_tx)).await {
                    Ok(_x) => debug!("ok"),
                    Err(issue) => error!("issue {}", issue),
                };
            }
            kv::request::Command::Set(x) => {
                NUMBER_OF_REQUESTS.with_label_values(&["set"]).inc();

                // Send the SET request
                let modulo: usize = self.calculate_modulo(&x.key);
                debug!(modulo = modulo, "set");

                let cmd = DBAction::Set(Set { request: x });
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
            kv::request::Command::Delete(x) => {
                NUMBER_OF_REQUESTS.with_label_values(&["delete"]).inc();

                // Send the DELETE request
                let modulo: usize = self.calculate_modulo(&x.key);
                debug!(modulo = modulo, "delete");

                let cmd = DBAction::Delete(Delete { request: x });
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
        }

        // Await the response
        if let Ok(x) = resp_rx.await {
            let reply = x.unwrap();

            // Write the response to the client
            self.connection.write_message(reply).await.unwrap();
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
        for worker_index in 0..self.nb_workers {
            let (tx, rx) = mpsc::channel::<CommandProcess>(self.worker_channel_buffer_size);
            self.workers_channel.push(tx);

            let _ = thread::Builder::new()
                .name(format!("worker-{worker_index}"))
                .spawn(|| {
                    let worker_rt_res = DBWorkerRuntime::new();
                    match worker_rt_res {
                        Ok(mut worker_rt) => {
                            worker_rt.start(rx).unwrap();
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
    pub fn new() -> Result<DBWorkerRuntime, Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()?;

        Ok(DBWorkerRuntime { rt })
    }

    pub fn start(&mut self, mut rx: Receiver<CommandProcess>) -> Result<(), Error> {
        self.rt.block_on(async move {
            let db: Db = Arc::new(Mutex::new(HashMap::new()));

            while let Some((cmd, resp_tx)) = rx.recv().await {
                let db = db.clone();
                tokio::spawn(async move {
                    let db = db.lock().await;

                    let reply: protobuf::Result<Vec<u8>> = match cmd {
                        DBAction::Get(get) => get.execute(db),
                        DBAction::Set(set) => set.execute(db),
                        DBAction::Delete(delete) => delete.execute(db),
                    };
                    resp_tx.send(Ok(reply.unwrap())).unwrap();
                });
            }
        });
        Ok(())
    }
}
