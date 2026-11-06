# Config we write, and the real tool that checks it

A parser we wrote agreeing with an exporter we wrote proves nothing. Each row below is a CI job in
`.github/workflows/exporters.yml` that feeds the generated file to the software that will read it.

The table is written from those jobs by `scripts/integrations-status.mjs`; a test fails when it
is stale. A container image pinned in a job with `image:` is listed. For every other job the column says
"not pinned here": the version is whatever the CI runner provides, and this page does not guess it.

<!-- integrations:start -->

| Config we write | Checked by | Pinned image |
| --- | --- | --- |
| `ssh_config` | OpenSSH | not pinned here |
| `wireguard` | wg parser | not pinned here |
| `nginx_*` | nginx -t | not pinned here |
| `compose` | docker compose config | not pinned here |
| `apache_vhost` | httpd | not pinned here |
| `haproxy_*` | haproxy -c | not pinned here |
| `ansible_*` | syntax check | not pinned here |
| `pg_connection` | libpq | `postgres:16` |
| `traefik_*` | traefik | not pinned here |
| `k8s_*` | a real cluster | not pinned here |
| `prometheus` | promtool | not pinned here |
| `grafana` | provisioning | not pinned here |
| `homepage` | services.yaml | not pinned here |
| `grafana` | enrich --online | not pinned here |

<!-- integrations:end -->

What this does not show: the versions of the tools behind "not pinned here" can move with the
runner image. A project type is only called stable once its job here is green.
