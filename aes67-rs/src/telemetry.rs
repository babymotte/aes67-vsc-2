/*
 *  Copyright (C) 2025 Michael Bachmann
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use crate::{
    config::{Config, EndpointConfig},
    error::TelemetryResult,
};
use opentelemetry::{global, trace::TracerProvider};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_resource_detectors::{
    HostResourceDetector, OsResourceDetector, ProcessResourceDetector,
};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider};
use std::io;
use supports_color::Stream;
use tracing::info;
use tracing_subscriber::{
    EnvFilter, Layer, filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

pub async fn init(config: &Config) -> TelemetryResult<()> {
    let subscriber = tracing_subscriber::registry().with(
        fmt::Layer::new()
            .with_ansi(supports_color::on(Stream::Stderr).is_some())
            .with_writer(io::stderr)
            .with_filter(EnvFilter::from_default_env())
            .with_filter(filter_fn(|meta| {
                !meta.is_span() && meta.fields().iter().any(|f| f.name() == "message")
            })),
    );

    if let Some(tracing_config) = &config.telemetry {
        global::set_text_map_propagator(TraceContextPropagator::new());

        let exporter = {
            let builder = opentelemetry_otlp::SpanExporter::builder();

            match &tracing_config.endpoint {
                EndpointConfig::Grpc(endpoint) => {
                    builder.with_tonic().with_endpoint(endpoint).build()?
                }
                EndpointConfig::Http(endpoint) => {
                    builder.with_http().with_endpoint(endpoint).build()?
                }
            }
        };

        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                Resource::builder()
                    .with_service_name(config.app.name.clone())
                    .with_detectors(&[
                        Box::new(HostResourceDetector::default()),
                        Box::new(OsResourceDetector),
                        Box::new(ProcessResourceDetector),
                    ])
                    .build(),
            )
            .build();

        let tracer = tracer_provider.tracer(config.instance_name().to_owned());

        global::set_tracer_provider(tracer_provider);

        let opentelemetry = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(
                EnvFilter::from_default_env()
                    .add_directive("tower_http=debug".parse()?)
                    .add_directive("aes67_vsc_2=debug".parse()?),
            );

        subscriber.with(opentelemetry).init();
        info!("Telemetry enabled.");
    } else {
        subscriber.init();
        info!("Telemetry disabled.");
    }

    Ok(())
}
