use std::{future::Future, net::SocketAddr, sync::Arc};

use anyhow::Result;
use tonic::transport::Server as TonicServer;

use flash_cat_common::{
    consts::{DEFAULT_GRPC_TIMEOUT, DEFAULT_HTTP2_KEEPALIVE_INTERVAL, DEFAULT_HTTP2_KEEPALIVE_TIMEOUT, DEFAULT_TCP_KEEPALIVE},
    proto::{FILE_DESCRIPTOR_SET, relay_service_server::RelayServiceServer},
};

use crate::{grpc::GrpcServer, relay::RelayState};

pub(crate) async fn start_server(
    state: Arc<RelayState>,
    addr: SocketAddr,
    signal: impl Future<Output = ()>,
) -> Result<()> {
    TonicServer::builder()
        .http2_keepalive_interval(Some(DEFAULT_HTTP2_KEEPALIVE_INTERVAL))
        .http2_keepalive_timeout(Some(DEFAULT_HTTP2_KEEPALIVE_TIMEOUT))
        .http2_adaptive_window(Some(true)) // enable adaptive window size
        .tcp_keepalive(Some(DEFAULT_TCP_KEEPALIVE)) // set TCP keepalive
        .timeout(DEFAULT_GRPC_TIMEOUT) // set overall request timeout
        .add_service(RelayServiceServer::new(GrpcServer::new(state)))
        .add_service(tonic_reflection::server::Builder::configure().register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET).build_v1()?)
        .serve_with_shutdown(addr, signal)
        .await?;
    Ok(())
}
