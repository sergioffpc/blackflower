#![doc = include_str!("../README.md")]

mod config;
mod endpoint;
mod error;
mod handles;
mod streams;

pub use config::{
    ALPN_PROTOCOL, AdmissionLimits, BOOTSTRAP_DEADLINE, ClientEndpointConfig, ClientTrustRoot,
    ServerEndpointConfig, ServerTlsConfig,
};
pub use endpoint::{ClientConnection, QuicClient, QuicServer, ServerConnection, UdpByteStats};
pub use error::QuicError;
pub use handles::{ClientNetworkHandle, NetworkEvent, ServerNetworkHandle};
pub use streams::{BootstrapTransfer, SessionControlStream};

use config::{client_config, server_config, validate_alpn};
