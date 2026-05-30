use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-reciprocal";
pub const CONCERN: &str = "arithmetic: 1 / value";
pub const DEPENDS_ON: &[&str] = &["srvcs-floatdivide"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub floatdivide_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    pub value: f64,
}

#[derive(Serialize, ToSchema)]
pub struct ReciprocalResponse {
    pub value: f64,
    /// The reciprocal `1 / value` as a floating-point number.
    pub result: f64,
}

fn ok(value: f64, result: f64) -> Response {
    (
        StatusCode::OK,
        Json(json!({ "value": value, "result": result })),
    )
        .into_response()
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// A reachable dependency answered `200` but its body lacked a numeric
/// `result`. That is a contract violation we cannot recover from, so surface a
/// `500` rather than guessing.
fn malformed(dependency: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            json!({ "error": "dependency returned a malformed result", "dependency": dependency }),
        ),
    )
        .into_response()
}

/// Call one dependency at `url` with `body`, mapping its outcome to either the
/// parsed response body (on `200`) or an early-return `Response` the caller
/// should surface verbatim:
///
/// - unreachable / non-`200`/`422` -> `503` degraded
/// - `422` -> forwarded `422` (the dependency rejected the input)
async fn ask(url: &str, body: &Value, dependency: &str) -> Result<Value, Response> {
    match client::call(url, body).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => Ok(body),
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute `1 / value` by delegating to `srvcs-floatdivide`.
///
/// This service owns the *control flow* but delegates the arithmetic to its
/// dependency, exactly as specified:
///
/// 1. ask `srvcs-floatdivide` for `1 / value`;
/// 2. surface its `result` as the reciprocal.
///
/// If `value` is `0`, floatdivide rejects the divisor with `422`, which is
/// forwarded verbatim. If the dependency is unreachable it reports itself
/// degraded (`503`).
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = ReciprocalResponse),
        (status = 422, description = "the dependency rejected the input (forwarded)"),
        (status = 500, description = "a dependency returned a malformed result"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    let value = req.value;

    // result = 1 / value, delegated to srvcs-floatdivide.
    let body = match ask(
        &deps.floatdivide_url,
        &json!({ "a": 1, "b": value }),
        "srvcs-floatdivide",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let result = match body.get("result").and_then(Value::as_f64) {
        Some(r) => r,
        None => return malformed("srvcs-floatdivide"),
    };

    ok(value, result)
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, ReciprocalResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_all_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-reciprocal");
        assert_eq!(info.concern, "arithmetic: 1 / value");
        assert_eq!(info.depends_on, vec!["srvcs-floatdivide"]);
    }
}
