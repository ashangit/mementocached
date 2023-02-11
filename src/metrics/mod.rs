use std::net::TcpListener;

use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use lazy_static::lazy_static;
use prometheus::{register_int_counter_vec, IntCounterVec, Opts};
use tracing::log::debug;
use tracing::{error, info};

lazy_static! {
    pub static ref NUMBER_OF_REQUESTS: IntCounterVec = register_int_counter_vec!(
        Opts::new("number_of_requests", "Number of total requests"),
        &["type"]
    )
    .expect("metric can be created");
}

/// Handler of healthz endpoint
///
/// # Return
///
/// * Return ok string
///
async fn healthz_handler() -> Result<&'static str, StatusCode> {
    debug!("Handle health endpoint call");
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
async fn metrics_handler() -> Result<String, StatusCode> {
    debug!("Handle metrics endpoint call");
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();
    if let Err(_e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        //error!("could not encode prometheus metrics: {}", e.into());
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

#[derive(Debug)]
pub struct HttpEndpoint {
    pub listener: TcpListener,
    app: Router,
}

impl HttpEndpoint {
    /// Http endpoint
    ///
    /// expose metrics and healthz endpoint
    ///
    /// # Arguments
    ///
    /// * `port` - the http port to expose
    ///
    /// # Return
    ///
    /// * Return HttpEndpoint
    ///
    pub fn new(port: u16) -> Self {
        let socket = format!("0.0.0.0:{port}");
        let listener =
            TcpListener::bind(&socket).unwrap_or_else(|_| panic!("Failed to bind socket {socket}"));

        let app = Router::new()
            .route("/healthz", get(healthz_handler))
            .route("/metrics", get(metrics_handler));

        HttpEndpoint { listener, app }
    }

    /// Start the webserver for healthz and metrics endpoint
    /// Used to expose prometheus metrics
    ///
    /// # Arguments
    ///
    ///
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let local_addr = self
            .listener
            .local_addr()
            .expect("Failed to get socket for metrics endpoint")
            .to_string();
        let listener = self
            .listener
            .try_clone()
            .expect("Failed to clone socket handler for metrics endpoint");
        let app = self.app.clone();
        match axum::Server::from_tcp(listener) {
            Ok(server) => {
                info!(
                    "Http server for metrics endpoint listening on {}",
                    local_addr
                );
                server.serve(app.into_make_service()).await?;
            }
            Err(error) => error!(
                "Failed to start http endpoint on port {} due to {}",
                local_addr, error
            ),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_healthz_handler() {
        assert_eq!("ok", healthz_handler().await.unwrap());
    }

    #[tokio::test]
    async fn test_metrics_handler() {
        NUMBER_OF_REQUESTS.with_label_values(&["get"]).inc();
        NUMBER_OF_REQUESTS.with_label_values(&["set"]).inc();
        let metrics = metrics_handler().await.unwrap();
        assert!(metrics.contains("process_cpu_seconds_total"));
        assert!(metrics.contains("number_of_requests{type=\"get\"}"));
        assert!(metrics.contains("number_of_requests{type=\"set\"}"));
    }
}
