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
                format!("socket-reader-{id}")
            })
            .build()?;

        Ok(SocketReaderRuntime {
            addr,
            workers_channel,
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
        let workers_channel = self.workers_channel.clone();
        self.rt.spawn(async move {
            info!(socket = addr, "Start listening");
            let listener = TcpListener::bind(addr.clone()).await.unwrap();
            SocketReaderRuntime::accept_connection(listener, workers_channel).await;
        });
        Ok(())
    }

    async fn accept_connection(
        listener: TcpListener,
        workers_channel: Vec<Sender<CommandProcess>>,
    ) {
        loop {
            let (stream, client_addr) = listener.accept().await.unwrap();
            debug!(
                client_addr = client_addr.to_string(),
                "Accept connection from client"
            );

            let workers_channel = workers_channel.clone();
            tokio::spawn(async move {
                SocketProcessor::new(stream, workers_channel, client_addr.to_string())
                    .read()
                    .await;
            });
        }
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
        let connection = Connection::new(stream);

        SocketProcessor {
            connection,
            workers_channel,
            client_addr,
        }
    }

    /// Read the protobuf message from the client socket
    /// Then send the message to the appropriate db worker, depending to the hash(key) modulo number
    /// of db worker
    /// Finally write back the result to the client socket
    ///
    /// # Return
    ///
    /// * Result<(), Error>
    ///
    pub async fn read(&mut self) {
        loop {
            match self.connection.read_message().await {
                Ok(Some(buffer)) => {
                    debug!("Parse request");
                    let request: Request = Message::parse_from_bytes(&buffer).unwrap();
                    self.process(request).await;
                }
                Ok(None) => {
                    info!(
                        client_addr = self.client_addr,
                        "Connection closed by client"
                    );
                    return;
                }
                Err(issue) => {
                    error!(
                        client_addr = self.client_addr,
                        issue = issue.to_string(),
                        "Failure reading message from client"
                    );
                    return;
                }
            }
        }
    }

    async fn process(&mut self, request: Request) {
        let (resp_tx, resp_rx) = oneshot::channel();

        match request.command.unwrap() {
            kv::request::Command::Get(request) => {
                NUMBER_OF_REQUESTS.with_label_values(&["get"]).inc();

                // Send the GET request
                let modulo: usize = self.calculate_modulo(&request.key);
                debug!(
                    key = request.key,
                    "Send get request to DB worker {}", modulo
                );

                let cmd = DBAction::Get(request);
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
            kv::request::Command::Set(request) => {
                NUMBER_OF_REQUESTS.with_label_values(&["set"]).inc();

                // Send the SET request
                let modulo: usize = self.calculate_modulo(&request.key);
                debug!(
                    key = request.key,
                    "Send set request to DB worker {}", modulo
                );

                let cmd = DBAction::Set(request);
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
            kv::request::Command::Delete(request) => {
                NUMBER_OF_REQUESTS.with_label_values(&["delete"]).inc();

                // Send the DELETE request
                let modulo: usize = self.calculate_modulo(&request.key);
                debug!(
                    key = request.key,
                    "Send delete request to DB worker {}", modulo
                );

                let cmd = DBAction::Delete(request);
                self.workers_channel[modulo]
                    .send((cmd, resp_tx))
                    .await
                    .unwrap();
            }
        }

        // Await the response
        if let Ok(result) = resp_rx.await {
            debug!("Get response message from DB worker");
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
