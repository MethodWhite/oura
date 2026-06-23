use anyhow::{Context, Result};
use quinn::{ClientConfig, Endpoint, EndpointConfig, TransportConfig};
use std::sync::Arc;
use std::time::Duration;

pub struct QuicConnector {
    endpoint: Endpoint,
}

impl QuicConnector {
    pub fn new() -> Result<Self> {
        let mut transport = TransportConfig::default();
        transport.max_idle_timeout(Some(Duration::from_secs(30).try_into().unwrap()));
        transport.keep_alive_interval(Some(Duration::from_secs(5)));

        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerification))
            .with_no_client_auth();

        let quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?;
        let mut client_config = ClientConfig::new(Arc::new(quic_config));
        client_config.transport_config(Arc::new(transport));

        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        let mut endpoint = Endpoint::new(
            EndpointConfig::default(),
            None::<quinn::ServerConfig>,
            socket,
            Arc::new(quinn::TokioRuntime),
        )?;
        endpoint.set_default_client_config(client_config);

        Ok(Self { endpoint })
    }

    pub async fn connect(&self, host: &str, port: u16) -> Result<quinn::Connection> {
        let addr: std::net::SocketAddr = format!("{}:{}", host, port)
            .parse()
            .context("Invalid host:port")?;
        Ok(self.endpoint.connect(addr, host)?.await?)
    }

    pub async fn call_tool(
        &self, host: &str, port: u16,
        tool_name: &str, arguments: &serde_json::Value,
    ) -> Result<String> {
        let conn = self.connect(host, port).await
            .context("QUIC connection failed")?;
        let (mut send, mut recv) = conn.open_bi().await
            .context("Failed to open QUIC stream")?;

        let request = serde_json::json!({
            "jsonrpc": "2.0", "id": "1", "method": "tools/call",
            "params": { "name": tool_name, "arguments": arguments }
        });
        let body = serde_json::to_vec(&request)?;

        let len = body.len() as u32;
        send.write_all(&len.to_be_bytes()).await?;
        send.write_all(&body).await?;
        let _ = send.finish();

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        let mut resp_buf = vec![0u8; resp_len];
        recv.read_exact(&mut resp_buf).await?;

        Ok(String::from_utf8(resp_buf)?)
    }
}

#[derive(Debug)]
struct SkipVerification;

impl rustls::client::danger::ServerCertVerifier for SkipVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
