# Telemetry Pipeline Guide for Monorepo-Distributed CLI Tools

A playbook for wiring DAU / WAU / MAU and feature metrics into a CLI tool
that is distributed through an internal tool-runner (single-binary fetch
of a platform-specific artifact). All names are generic — substitute the
ones your org uses.

> **Important:** this guide assumes your public source repository must stay
> clean of internal hostnames, tokens and pipeline details. The telemetry
> module itself is **not** checked into the public repo — it is injected
> via a patch at build time inside the internal monorepo.

## 1. High-level architecture

```
┌────────────────┐     HTTPS POST           ┌────────────────────┐
│ CLI binary     │ ──────────────────────▶  │ Telemetry ingest   │
│ (local buffer, │   batched JSON array     │ proxy (HTTP)       │
│  once per day) │                          └─────────┬──────────┘
└────────────────┘                                    │ forwards
                                                      ▼
                                          ┌──────────────────────┐
                                          │ Message bus (topic)  │
                                          │  — 18 h retention    │
                                          └─────────┬────────────┘
                                                    │ consumed by
                                                    ▼
                                          ┌──────────────────────┐
                                          │ Shared log-parser    │
                                          │ (org-wide service)   │
                                          │ routes topic → table │
                                          └─────────┬────────────┘
                                                    ▼
                                          ┌──────────────────────┐
                                          │ Analytical storage   │
                                          │  (distributed FS /   │
                                          │   columnar tables)   │
                                          └─────────┬────────────┘
                                                    ▼
                                          ┌──────────────────────┐
                                          │ BI dashboard         │
                                          └──────────────────────┘
```

The critical design choice: **do not stand up your own Logbroker→storage
delivery pipeline.** Reuse the shared log-parser infrastructure that the
platform team already runs for many services. You register a topic and a
parser config with them, and your events land in the shared analytical
storage automatically — no per-tool data-transfer cluster, no per-tool
credentials, no operational burden on you.

## 2. Prerequisites

Before starting, you should already have:

- The tool registered in your org's tool-runner (users invoke it as
  `<runner> <tool> <command>`).
- A service record in your org's service catalog — this is what grants
  access to shared infra (message bus, log-parser, analytical tables).
- A ticketing queue for the telemetry-ingest-proxy team.
- Access to the BI dashboard product used at your org.

## 3. Register a topic with the ingest proxy

Open a ticket in the ingest-proxy team's queue. Template:

```
Summary: Register topic <tool>/production/report

Body:
Tool: <tool>
Distribution: via internal tool-runner
Volume: ~N unique users/day, 1 ping per user per day
Payload: JSON ~200 bytes, array of flat objects

Please:
  1. Create topic <tool>/production/report on the message bus
  2. Add a shard in the ingest-proxy config so
     POST /write/<tool>/production/report routes to that topic
  3. Confirm the endpoint URL and any auth requirements
     (our clients run on user workstations, anonymous write is fine)

Proposed payload schema:
{
  "timestamp":       "uint64",
  "user":            "string",
  "version":         "string",
  "platform":        "string",
  "command_counts":  "map<string, uint64>",
  "results_total":   "uint64",
  "latency_avg_ms":  "uint64",
  "latency_max_ms":  "uint64"
}

Owner: <login>
```

The team will reply with your write endpoint — something like:

```
POST https://<ingest-proxy-host>/write/<tool>/production/report
```

**Payload format — THE footgun.** The ingest proxy iterates the request
body as a list and writes each element as one record. If you send a bare
object (`{…}`) it iterates its *keys* and you end up with one record per
key and no values. **Always wrap your events in a JSON array:**

```json
[{"user":"alice","version":"1.2.3","command_counts":{"search":7}}]
```

Even for a single event, keep the brackets.

## 4. Register a parser with the shared log-parser

This is the step that replaces a per-tool data-transfer pipeline.

Open a ticket in the shared log-parser team's queue (different from the
ingest-proxy queue). Ask them to:

1. Subscribe one of their standard consumers to your topic
   (`<tool>/production/report`).
2. Register a JSON parser for that topic with your schema.
3. Route the parsed records to a table under your service's path in the
   analytical storage, e.g. `//home/<service>/events`.

The schema you submit looks like this (adapt to your payload):

```json
[
  {"name": "timestamp",      "type": "UINT64"},
  {"name": "user",           "type": "STRING"},
  {"name": "version",        "type": "STRING"},
  {"name": "platform",       "type": "STRING"},
  {"name": "command_counts", "type": "ANY"},
  {"name": "results_total",  "type": "UINT64"},
  {"name": "latency_avg_ms", "type": "UINT64"},
  {"name": "latency_max_ms", "type": "UINT64"}
]
```

Use `ANY` for nested maps and arrays — the shared parser stores them as
opaque JSON nodes and you can still query with `Yson::…` functions in
BI.

Ask them to also attach any **standard envelope columns** the message
bus adds (timestamps, source host, sequence numbers) so you can debug
ingestion without re-sending events.

**Permissions.** You (and anyone who should read the events) need read
access to the destination table. Typically this is done by assigning an
org-standard role on your service in the catalog — the shared parser
team will tell you which role. Expect propagation delays measured in
hours, not minutes; plan around it.

## 5. Client-side telemetry module

Keep two hard rules — everything else is style:

1. **Fire-and-forget with a short network timeout** (≤ 2 s). A command
   must never wait for telemetry. If the ingest proxy is down, the user
   must not notice.
2. **One event per user per day is enough for DAU/WAU/MAU.** Do not ship
   per-command pings. Batch counters locally and flush once.

### 5.1 Storage layout

Put the buffer and a flush marker in the OS cache dir:

```
macOS:    ~/Library/Caches/<tool>/
Linux:    $XDG_CACHE_HOME/<tool>/      (fallback ~/.cache/<tool>/)
Windows:  %LOCALAPPDATA%\<tool>\

  telemetry.jsonl        # append-only buffer, one event per line
  last-telemetry-flush   # empty file; its mtime is "last flush time"
```

The marker is an empty file on purpose — you check `mtime`, not content.
This makes the flush idempotent across concurrent processes and makes
"force flush now" a one-liner in tests: `touch -t <old-ts> <marker>`.

### 5.2 Control flow

Every instrumented command does:

```
on command start:
  append one line to telemetry.jsonl
  if now - mtime(last-telemetry-flush) > 30 min:
      touch(last-telemetry-flush)        # BEFORE network I/O
      spawn background flush
```

Touching the marker *before* the network call is load-bearing: if two
processes race, only one wins the mtime update, the other sees a fresh
marker and skips.

The background flush does:

```
read and truncate telemetry.jsonl atomically
aggregate events into one payload per (user, version, platform, day)
POST [payload] to the ingest-proxy endpoint with timeout 2s
  on 2xx: done
  on any error: write buffer back, exit silently
```

Do not retry — the next command will try again, and daily throttling
means you never burn CPU in a tight loop.

### 5.3 Payload rules

- **Never log user content.** No file paths, no queries, no diagnostic
  messages. Aggregate them into counters and histograms at send time.
- `user` is the OS login. That's fine for internal DAU — your users
  already accept that when they run an internal tool.
- Prefer **counters and summary stats** (`count`, `avg`, `max`) over raw
  per-call records. Smaller payload, fewer privacy concerns, same
  dashboards.
- Version-stamp every event with the tool version. A regression shows up
  as a change in latency/empty-result rate *within one version*, not
  across all time.

### 5.4 Reference event shape

```json
[
  {
    "timestamp": 1712345678,
    "user": "alice",
    "version": "1.4.2",
    "platform": "darwin-arm64",
    "command_counts": {"search": 23, "symbol": 4, "outline": 1},
    "results_total": 412,
    "empty_count": 3,
    "latency_avg_ms": 17,
    "latency_max_ms": 203,
    "index_files": 112384
  }
]
```

## 6. Inject telemetry only in internal builds

Your source repo is public. The telemetry module — with hardcoded
ingest-proxy URL and internal field names — must not land there. Solve
this with **patch-based injection at build time**, not conditional
compilation:

- Telemetry code (`telemetry.{rs,go,py,…}`) lives only in the internal
  monorepo, under the tool's fork directory.
- A versioned `patches/vX.Y.Z.patch` next to it adds the call sites into
  the patched source (hooks in command entry points plus `mod telemetry;`
  in the crate root / package import).
- The build driver inside the monorepo pulls the public tarball, reads
  its version, applies the matching patch, copies the telemetry file in,
  then runs the standard build.
- The resulting binary is uploaded as a tool-runner artifact.

This keeps the public build (Homebrew, npm, GitHub Releases) completely
telemetry-free, and only the artifacts shipped through the internal
tool-runner carry telemetry. Users who install from the public channel
pay zero telemetry cost.

A minimal patch adds three things: the `mod telemetry;` declaration,
one `telemetry::record(...)` call per command handler, and an
`on_exit` flush. Keep it small — patches that refactor break on every
version bump.

## 7. Testing the pipeline end-to-end

Three checkpoints, in order:

### 7.1 Ingest proxy accepts your payload

```bash
curl -sf -X POST \
  -H 'Content-Type: application/json' \
  -d '[{"user":"test","version":"dev","command_counts":{"search":1}}]' \
  'https://<ingest-proxy-host>/write/<tool>/production/report'
```

2xx means the message reached the proxy. No echo back — check the
message-bus diagnostics UI for a fresh write.

### 7.2 Messages reach the message-bus topic

Read a handful of the most recent records directly from the topic. Your
org's bus CLI usually needs a specific cluster endpoint (not the install
host):

```bash
<bus-cli> -s <cluster-endpoint> data read \
  -t <tool>/production/report -c <consumer> -m 10 -o 10
```

If the proxy step worked but this one shows nothing, the shard mapping
in the proxy config is wrong — go back to step 3.

### 7.3 Records appear in the analytical table

Wait a few minutes after a fresh event, then query your destination
table. A one-liner `SELECT * FROM <table> ORDER BY timestamp DESC LIMIT
5` is enough.

If the topic has records but the table is empty, the parser registration
(step 4) is incomplete — ticket the shared log-parser team. Include
timestamps of the missing records.

### 7.4 Forcing a flush during development

The client throttles itself to once per 30 minutes. To force a flush:

```bash
# macOS
touch -t $(date -v-31M +%Y%m%d%H%M.%S) \
  ~/Library/Caches/<tool>/last-telemetry-flush

# Linux
touch -d '31 minutes ago' \
  "${XDG_CACHE_HOME:-$HOME/.cache}/<tool>/last-telemetry-flush"

<tool> <any-instrumented-command>   # the next command triggers flush
```

## 8. Dashboards

Once records are in the table, ship three queries to your BI product:

```sql
-- DAU (last 30 days)
SELECT DATE(timestamp) AS day,
       COUNT(DISTINCT user) AS dau
FROM `//home/<service>/events`
WHERE timestamp >= CurrentUtcDate() - Interval("P30D")
GROUP BY DATE(timestamp)
ORDER BY day;

-- WAU
SELECT DATE_TRUNC('week', timestamp) AS week,
       COUNT(DISTINCT user) AS wau
FROM `//home/<service>/events`
GROUP BY DATE_TRUNC('week', timestamp)
ORDER BY week;

-- MAU + stickiness (DAU/MAU)
SELECT DATE_TRUNC('month', timestamp) AS month,
       COUNT(DISTINCT user) AS mau
FROM `//home/<service>/events`
GROUP BY DATE_TRUNC('month', timestamp)
ORDER BY month;
```

Stickiness rule of thumb: 20 % is fine for an ad-hoc dev tool, 50 % is
great for a daily driver.

Add a version-adoption chart on top of this — it answers the only two
questions you actually care about when cutting a release:

```sql
-- What % of today's users are on the latest version?
SELECT version,
       COUNT(DISTINCT user) AS users,
       100.0 * COUNT(DISTINCT user)
             / SUM(COUNT(DISTINCT user)) OVER () AS share_pct
FROM `//home/<service>/events`
WHERE timestamp >= CurrentUtcDate() - Interval("P1D")
GROUP BY version
ORDER BY users DESC;
```

## 9. Common pitfalls

- **Sending a bare object instead of an array** — see §3. Manifests as
  "records exist in the topic but all columns are empty".
- **Forgetting to wrap the flush in a timeout** — on a slow VPN the CLI
  hangs for 30 s on every instrumented command. Always set a hard
  ≤ 2 s timeout on the HTTP client.
- **Logging user content as strings** — a field you introduce "just for
  debugging" eventually ends up in the analytical table and in the
  dashboards of whoever has read access. Only ship counters.
- **Relying on per-call pings** — producer outages or network flaps
  multiply into thousands of retries. Daily throttling absorbs this for
  free.
- **Assuming the public build should "just skip" telemetry at runtime
  via env vars** — once the hostname is in the binary, someone will
  grep it out of a support log. Compile it out via the patch-injection
  flow instead.
- **Rebuilding the patch on every commit** — patches against individual
  versions are cheap and obvious; patches against `main` rot. Keep one
  patch per released version.
