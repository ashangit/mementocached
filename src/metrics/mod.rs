use lazy_static::lazy_static;
use poem::http::StatusCode;
use poem::listener::TcpListener;
use poem::{get, handler, Route, Server};
use prometheus::{register_int_counter_vec, IntCounterVec, Opts};
use tracing::{error, info};

lazy_static! {
    pub static ref DURATION: IntCounterVec = register_int_counter_vec!(
        Opts::new("duration", "Duration converting file"),
        &[
            "source_file",
            "source_format",
            "destination_file",
            "destination_format"
        ]
    )
    .expect("metric can be created");
}

/// Handler of healthz endpoint
///
/// # Return
///
/// * Return ok string
///
#[handler]
async fn healthz_handler() -> Result<&'static str, StatusCode> {
    Ok("ok")
}

/// Handler of metrics endpoint
///
/// transform default and custom metrics to a string
///
/// # Return
///
/// * Return prometheus metrics string or https status code representing the faced issue
///
#[handler]
async fn metrics_handler() -> Result<String, StatusCode> {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();
    if let Err(_e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let res = match String::from_utf8(buffer) {
        Ok(v) => v,
        Err(e) => {
            error!("prometheus metrics could not be from_utf8'd: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    Ok(res)
}

/// Initialize the webserver for healthz and metrics endpoint
/// Used to expose prometheus metrics
///
/// # Arguments
///
/// * `http_port` - listening port of the webserver
///
#[async_backtrace::framed]
pub async fn init_prometheus_http_endpoint(
    http_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Route::new()
        .at("/healthz", get(healthz_handler))
        .at("/metrics", get(metrics_handler));

    info!(port = http_port, "Start http endpoint");
    Server::new(TcpListener::bind(format!("0.0.0.0:{http_port}")))
        .run(app)
        .await?;

    Ok(())
}
