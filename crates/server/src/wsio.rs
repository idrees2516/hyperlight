//! Shared TLS/WebSocket connection helper (native-tls / OpenSSL backend).
//!
//! Two of the nine venues (Bitstamp, Gate) sit behind AWS NLBs that reset
//! rustls ClientHellos, while OpenSSL handshakes are accepted everywhere —
//! so every venue connector goes through this one native-tls backed helper.

use std::sync::OnceLock;
use tokio_tungstenite::Connector;

static CONNECTOR: OnceLock<Connector> = OnceLock::new();

/// Process-wide native-tls connector (cheap to clone; shares SSL context).
pub fn connector() -> Connector {
    CONNECTOR
        .get_or_init(|| {
            Connector::NativeTls(native_tls::TlsConnector::new().expect("native-tls init"))
        })
        .clone()
}

/// Connect to a `wss://` venue endpoint with the shared connector.
pub async fn connect(url: &str) -> Result<WebSocket, String> {
    tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector()))
        .await
        .map_err(|e| format!("connect failed: {e}"))
}

/// Alias matching the concrete stream type the connectors use.
pub type WebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
