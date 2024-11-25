use std::{future::Future, net::SocketAddr, sync::Arc};

use anyhow::Result;
use flash_cat_common::proto::{relay_service_server::RelayServiceServer, FILE_DESCRIPTOR_SET};
use tonic::transport::Server as TonicServer;

use crate::{grpc::GrpcServer, relay::RelayState};

pub(crate) async fn start_server(
    state: Arc<RelayState>,
    addr: SocketAddr,
    signal: impl Future<Output = ()>,
) -> Result<()> {
    TonicServer::builder()
        .add_service(RelayServiceServer::new(GrpcServer::new(state)))
        .add_service(
            tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
                .build_v1()?,
        )
        .serve_with_shutdown(addr, signal)
        .await?;
    Ok(())
}
