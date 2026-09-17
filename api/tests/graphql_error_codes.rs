//! Exercises the error-classification extension (`graphql::RequestMetricsExt`) end
//! to end, over a real `async_graphql::Schema` built with the crate's mocks
//! (`mockdb` + `mockmail`):
//!
//! - a guard failure must carry `extensions.code = "UNAUTHENTICATED"`
//! - a plain `db::Error` bubbling out of a resolver, with no `ApiError` wrapping,
//!   must default to `extensions.code = "INTERNAL"`
//!
//! This is a small ad hoc schema rather than the crate's real (still domain-less)
//! `QueryRoot`, so it can exercise both guard and non-guard failure paths without
//! polluting the committed `schema.graphql` with test-only fields.

use std::sync::Arc;

use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema, Value};
use microticket::app;
use microticket::db;
use microticket::graphql::RequestMetricsExt;
use microticket::graphql::auth::{AuthGuard, AuthRequirement};
use microticket::mockdb;
use microticket::mockmail;

struct TestQuery;

#[Object]
impl TestQuery {
    /// Requires authentication. No `AuthInfo` is ever injected into these tests, so
    /// this always fails its guard.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn guarded(&self) -> &'static str {
        "unreachable"
    }

    /// Returns a plain `db::Error`, with no `ApiError` wrapping, so it must
    /// classify as `INTERNAL` by default.
    async fn boom(&self) -> anyhow::Result<&'static str> {
        Err(db::Error::Infrastructure("simulated failure".into()).into())
    }
}

fn build_test_schema() -> Schema<TestQuery, EmptyMutation, EmptySubscription> {
    let my_app = Arc::new(app::new(
        mockdb::Handler::new(),
        mockmail::Handler::new(),
        0,
    ));
    Schema::build(TestQuery, EmptyMutation, EmptySubscription)
        .data(my_app)
        .extension(RequestMetricsExt)
        .finish()
}

fn error_code(response: &async_graphql::Response) -> Option<String> {
    let extensions = response.errors.first()?.extensions.as_ref()?;
    match extensions.get("code")? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

#[tokio::test]
async fn guard_failure_is_unauthenticated() {
    let schema = build_test_schema();
    let response = schema.execute("{ guarded }").await;
    assert!(!response.errors.is_empty(), "expected an error");
    assert_eq!(error_code(&response).as_deref(), Some("UNAUTHENTICATED"));
}

#[tokio::test]
async fn plain_db_error_is_internal() {
    let schema = build_test_schema();
    let response = schema.execute("{ boom }").await;
    assert!(!response.errors.is_empty(), "expected an error");
    assert_eq!(error_code(&response).as_deref(), Some("INTERNAL"));
}
