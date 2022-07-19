// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::{middleware_log, AccountsApi, BasicApi, IndexApi};

use crate::context::Context;
use anyhow::Context as AnyhowContext;
use aptos_config::config::NodeConfig;
use aptos_logger::info;
use poem::{
    http::{header, Method},
    listener::{Listener, RustlsCertificate, RustlsConfig, TcpListener},
    middleware::Cors,
    EndpointExt, Route, Server,
};
use poem_openapi::{ContactObject, LicenseObject, OpenApiService};
use tokio::runtime::Runtime;

pub fn attach_poem_to_runtime(
    runtime: &Runtime,
    context: Context,
    config: &NodeConfig,
) -> anyhow::Result<()> {
    let context = Arc::new(context);

    let apis = (
        AccountsApi {
            context: context.clone(),
        },
        BasicApi {
            context: context.clone(),
        },
        IndexApi { context },
    );

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string());
    let license =
        LicenseObject::new("Apache 2.0").url("https://www.apache.org/licenses/LICENSE-2.0.html");
    let contact = ContactObject::new()
        .name("Aptos Labs")
        .url("https://github.com/aptos-labs/aptos-core");

    // These APIs get merged.
    let api_service = OpenApiService::new(apis, "Aptos Node API", version)
        .description("The Aptos Node API is a RESTful API for client applications to interact with the Aptos blockchain.")
        .license(license)
        .contact(contact)
        .external_document("https://github.com/aptos-labs/aptos-core");

    let spec_json = api_service.spec_endpoint();
    let spec_yaml = api_service.spec_endpoint_yaml();

    let address = config.api.address;

    let listener = match (&config.api.tls_cert_path, &config.api.tls_key_path) {
        (Some(tls_cert_path), Some(tls_key_path)) => {
            info!("Using TLS for API");
            let cert = std::fs::read_to_string(tls_cert_path).context(format!(
                "Failed to read TLS cert from path: {}",
                tls_cert_path
            ))?;
            let key = std::fs::read_to_string(tls_key_path).context(format!(
                "Failed to read TLS key from path: {}",
                tls_key_path
            ))?;
            let rustls_certificate = RustlsCertificate::new().cert(cert).key(key);
            let rustls_config = RustlsConfig::new().fallback(rustls_certificate);
            TcpListener::bind(address).rustls(rustls_config).boxed()
        }
        _ => {
            info!("Not using TLS for API");
            TcpListener::bind(address).boxed()
        }
    };

    runtime.spawn(async move {
        let cors = Cors::new()
            .allow_methods(vec![Method::GET, Method::POST])
            .allow_headers(vec![header::CONTENT_TYPE, header::ACCEPT]);
        let route = Route::new()
            .nest("/", api_service)
            // TODO: I prefer "spec" here but it's not backwards compatible.
            // Consider doing it later if we cut over to this entirely.
            // TODO: Consider making these part of the API itself.
            .at("/openapi.json", spec_json)
            .at("/openapi.yaml", spec_yaml)
            .with(cors)
            .around(middleware_log);
        Server::new(listener)
            .run(route)
            .await
            .map_err(anyhow::Error::msg)
    });

    Ok(())
}
