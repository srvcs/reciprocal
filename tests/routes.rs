use axum::body::Body;
use axum::extract::Json as AxumJson;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_reciprocal::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

// --- Computing mocks for every srvcs arithmetic dependency. ---
//
// `srvcs-reciprocal` only composes `srvcs-floatdivide`, but the srvcs mock
// kit defines a *computing* mock per primitive so any orchestrator can be
// tested against true answers rather than canned values.

/// `srvcs-floatadd`: `{"a", "b"}` -> `{"result": a + b}` (f64).
#[allow(dead_code)]
async fn spawn_floatadd() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = num(&body, "a");
            let b = num(&body, "b");
            Json(json!({ "result": a + b }))
        }),
    );
    serve(app).await
}

/// `srvcs-floatmultiply`: `{"a", "b"}` -> `{"result": a * b}` (f64).
#[allow(dead_code)]
async fn spawn_floatmultiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = num(&body, "a");
            let b = num(&body, "b");
            Json(json!({ "result": a * b }))
        }),
    );
    serve(app).await
}

/// `srvcs-floatdivide`: `{"a", "b"}` -> `{"result": a / b}` (f64), or `422`
/// on a zero divisor.
async fn spawn_floatdivide() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = num(&body, "a");
            let b = num(&body, "b");
            if b == 0.0 {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": "divide by zero" })),
                );
            }
            (StatusCode::OK, Json(json!({ "result": a / b })))
        }),
    );
    serve(app).await
}

/// `srvcs-floatsubtract`: `{"a", "b"}` -> `{"result": a - b}` (f64).
#[allow(dead_code)]
async fn spawn_floatsubtract() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = num(&body, "a");
            let b = num(&body, "b");
            Json(json!({ "result": a - b }))
        }),
    );
    serve(app).await
}

/// `srvcs-floatpower`: `{"base", "exp"}` -> `{"result": base.powf(exp)}`.
#[allow(dead_code)]
async fn spawn_floatpower() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let base = num(&body, "base");
            let exp = num(&body, "exp");
            Json(json!({ "result": base.powf(exp) }))
        }),
    );
    serve(app).await
}

/// `srvcs-ln`: `{"value"}` -> `{"result": value.ln()}`.
#[allow(dead_code)]
async fn spawn_ln() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = num(&body, "value");
            Json(json!({ "result": value.ln() }))
        }),
    );
    serve(app).await
}

/// `srvcs-multiply`: `{"a", "b"}` -> `{"result": a * b}` (i64).
#[allow(dead_code)]
async fn spawn_multiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_i64).unwrap_or(0);
            let b = body.get("b").and_then(Value::as_i64).unwrap_or(0);
            Json(json!({ "result": a * b }))
        }),
    );
    serve(app).await
}

/// `srvcs-reciprocal`: `{"value"}` -> `{"result": 1 / value}` (f64).
#[allow(dead_code)]
async fn spawn_reciprocal() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = num(&body, "value");
            Json(json!({ "result": 1.0 / value }))
        }),
    );
    serve(app).await
}

/// `srvcs-root`: `{"value", "n"}` -> `{"result": value.powf(1/n)}`.
#[allow(dead_code)]
async fn spawn_root() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = num(&body, "value");
            let n = num(&body, "n");
            Json(json!({ "result": value.powf(1.0 / n) }))
        }),
    );
    serve(app).await
}

/// Spawn a mock returning a fixed status + body (used for error-path tests).
async fn spawn_fixed(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    serve(app).await
}

fn num(body: &Value, key: &str) -> f64 {
    body.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

async fn serve(app: AxumRouter) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn app(floatdivide_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            floatdivide_url: floatdivide_url.to_string(),
        },
    )
}

async fn reciprocal(floatdivide_url: &str, value: f64) -> (StatusCode, Value) {
    let res = app(floatdivide_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "value": value }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn status_of(uri: &str) -> StatusCode {
    app(DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

fn result_f64(body: &Value) -> f64 {
    body["result"].as_f64().expect("result is a number")
}

// --- Standard endpoints. ---

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_ok() {
    assert_eq!(status_of("/metrics").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

#[tokio::test]
async fn generates_request_id_when_absent() {
    let res = app(DEAD_URL)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.headers().contains_key("x-request-id"),
        "response must carry a generated x-request-id"
    );
}

#[tokio::test]
async fn index_reports_identity() {
    let res = app(DEAD_URL)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["service"], "srvcs-reciprocal");
    assert_eq!(body["concern"], "arithmetic: 1 / value");
    assert_eq!(body["depends_on"], json!(["srvcs-floatdivide"]));
}

// --- Correctness cases, against the computing floatdivide mock. ---

#[tokio::test]
async fn reciprocal_4_is_quarter() {
    let fd = spawn_floatdivide().await;
    let (status, body) = reciprocal(&fd, 4.0).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["value"], 4.0);
    assert!((result_f64(&body) - 0.25).abs() < 1e-9);
}

#[tokio::test]
async fn reciprocal_2_is_half() {
    let fd = spawn_floatdivide().await;
    let (status, body) = reciprocal(&fd, 2.0).await;
    assert_eq!(status, StatusCode::OK);
    assert!((result_f64(&body) - 0.5).abs() < 1e-9);
}

#[tokio::test]
async fn reciprocal_of_fraction_is_inverse() {
    // 1 / 0.25 = 4
    let fd = spawn_floatdivide().await;
    let (status, body) = reciprocal(&fd, 0.25).await;
    assert_eq!(status, StatusCode::OK);
    assert!((result_f64(&body) - 4.0).abs() < 1e-9);
}

#[tokio::test]
async fn reciprocal_of_negative() {
    // 1 / -5 = -0.2
    let fd = spawn_floatdivide().await;
    let (status, body) = reciprocal(&fd, -5.0).await;
    assert_eq!(status, StatusCode::OK);
    assert!((result_f64(&body) - (-0.2)).abs() < 1e-9);
}

#[tokio::test]
async fn reciprocal_of_three_is_third() {
    // 1 / 3 = 0.3333...
    let fd = spawn_floatdivide().await;
    let (status, body) = reciprocal(&fd, 3.0).await;
    assert_eq!(status, StatusCode::OK);
    assert!((result_f64(&body) - (1.0 / 3.0)).abs() < 1e-9);
}

// --- Error / degraded paths. ---

#[tokio::test]
async fn forwards_422_from_floatdivide_on_zero() {
    // value == 0 -> floatdivide rejects the zero divisor -> forward 422.
    let fd = spawn_floatdivide().await;
    let (status, _) = reciprocal(&fd, 0.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn forwards_422_from_floatdivide() {
    let fd = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "value is not a number" }),
    )
    .await;
    let (status, _) = reciprocal(&fd, 4.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn degrades_when_floatdivide_unreachable() {
    let (status, body) = reciprocal(DEAD_URL, 4.0).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatdivide");
}

#[tokio::test]
async fn malformed_floatdivide_result_is_500() {
    let fd = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-number" })).await;
    let (status, body) = reciprocal(&fd, 4.0).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-floatdivide");
}
