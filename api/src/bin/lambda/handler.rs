use std::fmt::Display;
use std::sync::Arc;
use std::time::Instant;

use async_graphql::{
    Request as GraphQlRequest, Response as GraphQlResponse, ServerError as GraphQlError,
};
use http::{HeaderMap, Method, StatusCode};
use lambda_http::request::RequestContext;
use lambda_http::{Body, Error, Request, RequestExt, Response};
use sha2::{Digest, Sha256};

use microticket::app;
use microticket::auth::{self, AuthInfo};
use microticket::db;
use microticket::graphql;
use microticket::mail;
use microticket::request_metrics::{self, RequestMetrics};
use microticket::storage;
use microticket::telemetry::{self, RequestTelemetry};

use crate::errors::{ClientError, ServerError};

type GraphQlSchema<H, M, S> = graphql::MicroticketSchema<app::MyApp<H, M, S>>;

pub struct Handler<
    H: db::Handler + Send + Sync,
    M: mail::Handler + Send + Sync,
    S: storage::Handler + Send + Sync,
> {
    app: Arc<app::MyApp<H, M, S>>,
    schema: GraphQlSchema<H, M, S>,
}

impl<
    H: db::Handler + Send + Sync + 'static,
    M: mail::Handler + Send + Sync + 'static,
    S: storage::Handler + Send + Sync + 'static,
> Handler<H, M, S>
{
    pub fn new(app: Arc<app::MyApp<H, M, S>>, schema: GraphQlSchema<H, M, S>) -> Self {
        Self { app, schema }
    }

    pub async fn handle_request(&self, request: Request) -> Result<Response<Body>, Error> {
        let request_start = Instant::now();
        let headers = request.headers().clone();

        // Authoritative client IP from the Function URL's request context (not
        // spoofable), falling back to the first X-Forwarded-For hop.
        let client_ip = match request.request_context_ref() {
            Some(RequestContext::ApiGatewayV2(ctx)) => {
                graphql::ClientIp(ctx.http.source_ip.clone())
            }
            _ => graphql::ClientIp::from_forwarded_for(
                headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()),
            ),
        };

        let query = if request.method() == Method::POST {
            self.graphql_request_from_post(request)
        } else if request.method() == Method::GET {
            return graphiql_for_request();
        } else {
            Err(ClientError::MethodNotAllowed)
        };
        let mut query = match query {
            Err(e) => return error_response(StatusCode::BAD_REQUEST, graphql_error(e)),
            Ok(q) => q,
        };

        let auth_opt = match self.try_auth(&headers).await {
            Err(auth::AuthError::Permanent(ref msg)) => {
                emit_auth_failure_telemetry(401, request_start, msg);
                return error_response(
                    StatusCode::UNAUTHORIZED,
                    graphql_error(format!("Authentication error: {}", msg)),
                );
            }
            Err(auth::AuthError::Transient(ref msg)) => {
                tracing::error!("Transient auth error: {}", msg);
                emit_auth_failure_telemetry(503, request_start, msg);
                return error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    graphql_error("Service temporarily unavailable"),
                );
            }
            Ok(opt) => opt,
        };
        let (caller_type, caller_id) = auth::caller_info(auth_opt.as_ref());
        if let Some(auth) = auth_opt {
            query = query.data(auth);
        }

        query = query
            .data(self.app.clone())
            .data(client_ip)
            .data(graphql::get_dataloader(self.app.clone()));

        let operation_context = telemetry::extract_operation_context(&mut query);
        let metrics = Arc::new(RequestMetrics::default());
        let gql_response = request_metrics::METRICS
            .scope(metrics.clone(), self.schema.execute(query))
            .await;
        let gql_error_count = gql_response.errors.len();

        // Serialize first so the emitted telemetry carries the real final HTTP status.
        let result = serde_json::to_string(&gql_response);
        let status: u16 = if result.is_ok() { 200 } else { 500 };

        RequestTelemetry {
            status,
            operation_type: operation_context.operation_type,
            operation_name: operation_context.operation_name(),
            caller_type,
            caller_id: &caller_id,
            latency_ms: request_start.elapsed().as_secs_f64() * 1000.0,
            graphql_error_count: gql_error_count,
            ..Default::default()
        }
        .with_metrics(&metrics)
        .emit();

        match result {
            Ok(response_body) => Response::builder()
                .status(200)
                .body(Body::Text(response_body))
                .map_err(ServerError::from)
                .map_err(Error::from),
            Err(e) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                graphql_error(ServerError::from(e)),
            ),
        }
    }

    /// Parse a POST body into a GraphQL request. Also computes the request body's
    /// hex SHA-256 for the debug log — there is no signed-header scheme in
    /// microticket to bind it into (unlike seslogin's kiosk `SLKey` auth), but
    /// logging it costs nothing and gives an audit trail for "was this exact body
    /// received".
    fn graphql_request_from_post(&self, request: Request) -> Result<GraphQlRequest, ClientError> {
        match request.into_body() {
            Body::Text(text) => {
                tracing::debug!(body_sha256 = %body_sha256_hex(text.as_bytes()), "received request body");
                serde_json::from_str::<GraphQlRequest>(&text).map_err(ClientError::from)
            }
            Body::Binary(binary) => {
                tracing::debug!(body_sha256 = %body_sha256_hex(&binary), "received request body");
                serde_json::from_slice::<GraphQlRequest>(&binary).map_err(ClientError::from)
            }
            _ => Err(ClientError::EmptyBody),
        }
    }

    async fn try_auth(&self, headers: &HeaderMap) -> Result<Option<AuthInfo>, auth::AuthError> {
        let auth_header = headers
            .get("Authorization")
            .and_then(|value| value.to_str().ok());
        match auth::verify_authorization_header(&*self.app, auth_header).await {
            Some(res) => res.map(Some),
            None => Ok(None),
        }
    }
}

fn body_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Emits request telemetry for an auth failure that short-circuits before GraphQL
/// execution. `status` is 401 (permanent / unauthenticated) or 503 (transient
/// backend error). `auth_error` is the reason auth failed, recorded in the
/// `api_request` log.
fn emit_auth_failure_telemetry(status: u16, request_start: Instant, auth_error: &str) {
    RequestTelemetry {
        status,
        latency_ms: request_start.elapsed().as_secs_f64() * 1000.0,
        auth_error,
        ..Default::default()
    }
    .emit();
}

fn graphql_error(message: impl Display) -> String {
    let message = message.to_string();
    let response = GraphQlResponse::from_errors(vec![GraphQlError::new(message, None)]);
    serde_json::to_string(&response).expect("Valid response should never fail to serialize")
}

fn error_response(status: StatusCode, body: String) -> Result<Response<Body>, Error> {
    Ok(Response::builder().status(status).body(Body::Text(body))?)
}

fn graphiql_for_request() -> Result<Response<Body>, Error> {
    let html = async_graphql::http::GraphiQLSource::build().finish();
    Response::builder()
        .status(200)
        .header("Content-Type", "text/html")
        .body(Body::Text(html))
        .map_err(Error::from)
}
