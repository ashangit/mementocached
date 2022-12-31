use argparse::{ArgumentParser, Store, StoreTrue};
use mementocached::Error;

use std::thread;

use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;

use mementocached::command::CommandProcess;

use mementocached::runtime::{CoreRuntime, SocketRuntimeReader};

fn main() -> Result<(), Error> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let mut cpus = num_cpus::get();
    let mut port = 6379;
    let mut http_port = 8080;
    let mut debug_mode = false;

    {
        // this block limits scope of borrows by ap.refer() method
        let mut argument_parser = ArgumentParser::new();
        argument_parser.refer(&mut cpus).add_option(
            &["--cores"],
            Store,
            "Number of cores to use (default: nb cores of the host",
        );
        argument_parser
            .refer(&mut port)
            .add_option(&["--port"], Store, "DB port (default: 6379)");
        argument_parser.refer(&mut http_port).add_option(
            &["--http-port"],
            Store,
            "Http port (default: 8080)",
        );
        argument_parser.refer(&mut debug_mode).add_option(
            &["--debug-mode"],
            StoreTrue,
            "Enable debug mode [enable console subscriber for the tokio console] (default: false)",
        );
        argument_parser.parse_args_or_exit();
    }

    // Init tokio console subscriber if enabled
    // Used to debug trace async task with https://github.com/tokio-rs/console
    if debug_mode {
        console_subscriber::init();
    }

    // TODO
    // each thread manage it's own hashmap

    let mut workers_channel = Vec::new();
    for worker_index in 1..=cpus {
        let (tx, rx) = mpsc::channel::<CommandProcess>(32);

        //let tx_local = tx.clone();
        workers_channel.push(tx);

        // Spawn Worker threads
        let _ = thread::Builder::new()
            .name(format!("Worker {worker_index}"))
            .spawn(|| {
                let _ = worker(rx);
            });
    }

    // Init socket reader runtime
    let mut socket_rt_reader =
        SocketRuntimeReader::new(format!("127.0.0.1:{port}"), workers_channel)?;
    socket_rt_reader.start()?;

    // Init core runtime
    let mut core_rt = CoreRuntime::new(http_port)?;
    core_rt.start()?;

    Ok(())
}

fn worker(_rx: Receiver<CommandProcess>) -> Result<(), Error> {
    let _worker_rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;
    Ok(())
}
