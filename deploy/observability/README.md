# Local & K3s Observability Stack (Loki + Promtail + Grafana)

Lightweight, vendor-neutral log aggregation stack for local development and self-hosted K3s clusters. All IkedaDev Rust microservices emit flat structured JSON to `stdout` via `ikeda-core::observability` (`LOG_FORMAT=json`), which Promtail scrapes from Docker and ships to Grafana Loki.

## Quick Start

Start only the observability stack (Loki on `:3100`, Promtail, Grafana on `:3000`):

```bash
docker compose --profile observability up -d
```

Or run alongside the APIs:

```bash
docker compose --profile apis --profile observability up -d
```

- **Grafana UI:** [http://localhost:3000](http://localhost:3000) (anonymous Admin access is enabled for local dev, and the Loki datasource is auto-provisioned as default).
- **Loki Readiness:** [http://localhost:3100/ready](http://localhost:3100/ready)

## Label Cardinality Policy

`promtail-config.yaml` intentionally extracts **only low-cardinality labels** (`service` and `level`) into Loki's index. High-cardinality fields such as `request_id`, `tenant_id`, and `user_id` remain in the JSON log body and are filtered at query time using LogQL's `| json` parser.

## Useful LogQL Queries

Open **Grafana -> Explore -> Loki** and try:

- **All logs for a specific service and tenant:**
  ```logql
  {service="bookings-api"} | json | tenant_id="11111111-1111-4111-8111-111111111111"
  ```
- **End-to-end request correlation across all microservices:**
  ```logql
  {service=~".+"} | json | request_id="<x-request-id-uuid>"
  ```
- **Errors across all APIs:**
  ```logql
  {level=~"ERROR|error"} | json
  ```
- **Slow HTTP requests (> 200ms):**
  ```logql
  {service=~".*-api"} | json | latency_ms > 200
  ```

## Swapping Backends Without Touching Rust Code

Because every Rust service writes structured JSON strictly to `stdout` (12-factor logging) with zero vendor SDK dependencies:

- **Google Cloud Run / GKE (Google Cloud Logging):** Cloud Run captures container `stdout` automatically and parses JSON fields (`severity`/`level`, `service`, `request_id`, `tenant_id`, etc.) directly into Cloud Logging — no Promtail or Loki needed in GCP.
- **Vector / Fluent Bit:** Replace the `promtail` container with Vector or Fluent Bit pointing at `/var/run/docker.sock` (or `/var/log/containers/*.log` on Kubernetes) and route JSON logs to S3, OpenSearch, or ClickHouse.
- **Datadog / New Relic / Honeycomb:** Deploy the vendor's log agent as a sidecar or DaemonSet with JSON log parsing enabled; no changes to `ikeda-core` or any microservice are required.
