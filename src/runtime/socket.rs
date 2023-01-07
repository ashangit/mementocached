use std::hash::Hash;
use std::hash::Hasher;
use std::sync::atomic::{AtomicUsize, Ordering};

use ahash::AHasher;
use protobuf::Message;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tracing::{debug, error, info};

use crate::command::{CommandProcess, DBAction};
use crate::connection::Connection;
use crate::metrics::NUMBER_OF_REQUESTS;
use crate::protos::kv;
use crate::protos::kv::Request;
use crate::Error;

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
    pub fn new(addr: String, workers_channel: Vec<Sender<CommandProcess>>) -> Result<Self, Error> {
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

    async fn accept_connection(
        listener: TcpListener,
        workers_channel: Vec<Sender<CommandProcess>>,
    ) {
        loop {
            let (stream, client_addr) = listener.accept().await.unwrap();

            let workers_channel = workers_channel.clone();
            tokio::spawn(async move {
                SocketProcessor::new(stream, workers_channel, client_addr.to_string())
                    .read()
                    .await;
            });
        }
    }

    pub fn start(&mut self) -> Result<(), Error> {
        let addr = self.addr.clone();
        let workers_channel = self.workers_channel.clone();
        self.rt.spawn(async move {
            info!(socket = addr, "Start listening");
            let listener = TcpListener::bind(addr.clone()).await.unwrap();
            SocketReaderRuntime::accept_connection(listener, workers_channel).await;
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
    ) -> Self {
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
            kv::request::Command::Get(get) => {
                NUMBER_OF_REQUESTS.with_label_values(&["get"]).inc();

                // Send the GET request
                let modulo: usize = self.calculate_modulo(&get.key);
                debug!(modulo = modulo, "get");

                let cmd = DBAction::Get(get);
                match self.workers_channel[modulo].send((cmd, resp_tx)).await {
                    Ok(_x) => debug!("ok"),
                    Err(issue) => error!("issue {}", issue),
                };
            }
            kv::request::Command::Set(set) => {
                NUMBER_OF_REQUESTS.with_label_values(&["set"]).inc();

                // Send the SET request
                let modulo: usize = self.calculate_modulo(&set.key);
                debug!(modulo = modulo, "set");

                let cmd = DBAction::Set(set);
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
            kv::request::Command::Delete(delete) => {
                NUMBER_OF_REQUESTS.with_label_values(&["delete"]).inc();

                // Send the DELETE request
                let modulo: usize = self.calculate_modulo(&delete.key);
                debug!(modulo = modulo, "delete");

                let cmd = DBAction::Delete(delete);
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
        }

        // Await the response
        if let Ok(result) = resp_rx.await {
            let reply = result.unwrap();

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

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn socket_reader_accept_connection() {}
}
