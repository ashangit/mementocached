use argparse::{ArgumentParser, Store, StoreTrue};

use mementocached::runtime::core::CoreRuntime;
use mementocached::runtime::db::DBManagerRuntime;
use mementocached::runtime::socket::SocketReaderRuntime;
use mementocached::Error;

// TODO each thread manage it's own hashmap
fn main() -> Result<(), Error> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let mut worker_threads = num_cpus::get();
    let mut port = 6379;
    let mut http_port = 8080;
    let mut debug_mode = false;

    {
        // this block limits scope of borrows by ap.refer() method
        let mut argument_parser = ArgumentParser::new();
        argument_parser.refer(&mut worker_threads).add_option(
            &["--worker-threads"],
            Store,
            "Number of DB worker threads to use (default: nb cores of the host)",
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

    // Init worker managing the DB
    let mut db_mgr_rt: DBManagerRuntime = DBManagerRuntime::new(worker_threads)?;
    db_mgr_rt.start()?;

    // Init socket reader runtime
    let mut socket_reader_rt =
        SocketReaderRuntime::new(format!("127.0.0.1:{port}"), db_mgr_rt.workers_channel)?;
    socket_reader_rt.start()?;

    // Init core runtime
    let mut core_rt = CoreRuntime::new(http_port)?;
    core_rt.start()?;

    // TODO se how to add those backtrace dump in case of debug mode (https://tokio.rs/blog/2022-10-announcing-async-backtrace)
    //println!("{}", async_backtrace::taskdump_tree(true));

    Ok(())
}
