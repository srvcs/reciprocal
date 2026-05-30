# srvcs-reciprocal

Arithmetic microservice for srvcs.cloud computing `1 / value`.

## Concern

`arithmetic: 1 / value`

This is an **orchestrator**: it owns the control flow but delegates the
arithmetic to [`srvcs-floatdivide`](https://github.com/srvcs/floatdivide). The
result is a floating-point number (an f64 that may be fractional).

It does **not** call `srvcs-isnumber` directly — input validation propagates
from its dependency: when `value` is `0`, `srvcs-floatdivide` rejects the zero
divisor with `422` and this service forwards it verbatim.

## API

### `GET /`

Service identity.

```json
{
  "service": "srvcs-reciprocal",
  "concern": "arithmetic: 1 / value",
  "depends_on": ["srvcs-floatdivide"]
}
```

### `POST /`

Request:

```json
{ "value": 4 }
```

Response `200`:

```json
{ "value": 4, "result": 0.25 }
```

`result` is `(call srvcs-floatdivide {"a": 1, "b": value}).result`, an f64.

Status codes:

- `200` — the reciprocal `1 / value`.
- `422` — the dependency rejected the input (forwarded), e.g. `value` is `0`.
- `500` — a dependency returned a malformed result.
- `503` — a dependency is unavailable.

## Configuration

| Variable                | Default                 | Purpose                          |
| ----------------------- | ----------------------- | -------------------------------- |
| `SRVCS_BIND_ADDR`       | `0.0.0.0:8080`          | Listen address.                  |
| `SRVCS_FLOATDIVIDE_URL` | `http://127.0.0.1:8090` | Base URL of `srvcs-floatdivide`. |

## Local checks

```sh
nix flake check -L
nix develop -c sh -euc 'cargo fmt --check; cargo clippy --all-targets -- -D warnings; cargo test'
nix build .#default -L
```

The Linux container is exposed as `.#container`. On Apple Silicon, use
`linux/arm64` for the practical local check; CI builds the release image on
native `x86_64-linux`.

See [`srvcs/platform`](https://github.com/srvcs/platform) for the shared service
standard and CI workflow.
