---
name: mosaic-api-mapping
description: How to author Mosaic Tessera recipes against HTTP/JSON sources — the `http_json` driver, `url:` field, `json_path:` for navigating nested response bodies (e.g., `$.data.items[*]`), bearer / API-token auth via `${env.VAR}` interpolation in `credentials:`, and the documented Phase 5A constraints (GET-only, single-page, no streaming, no retries beyond Stream D's defaults). Use whenever the source is a REST endpoint returning JSON, when a user provides an API URL or response sample, or when debugging an MC5013 / MC5014 / MC5015 fired against a `driver: http_json` recipe. Builds on `skills/import/recipe-format/SKILL.md`.
---

# Authoring Mosaic Recipes for HTTP / JSON Sources

This skill covers the Phase 5A `driver: http_json` path — authoring recipes that pull data from REST endpoints returning JSON. The recipe schema and the six semantic rules live in [`../recipe-format/SKILL.md`](../recipe-format/SKILL.md); this file goes deeper on the HTTP-shaped concerns: the `url:` and `json_path:` fields, response-shape navigation, auth patterns, and Phase 5A's documented HTTP limitations.

---

## When to use the HTTP/JSON driver

Use `driver: http_json` when:

- The source is a REST endpoint returning JSON.
- The endpoint is reachable via a single HTTP **GET** request.
- The response can be navigated with a single `json_path:` expression that picks an array of row-shaped objects.
- Authentication, if needed, fits into a single header (Bearer token, API key) supplied via `credentials:` interpolation.

**Don't** use `driver: http_json` when:

- The endpoint requires POST / PUT / DELETE (Phase 5A is GET-only).
- The data is paginated and the recipe needs to follow `next` links (Phase 5A is single-page).
- The data is a stream / SSE / chunked-transfer feed (Phase 5A is single-shot).
- Auth requires OAuth dance, JWT signing, or rotating headers per request (Phase 5A is single-headers-from-env).

For any of those cases, recommend an upstream extract step: pull to a CSV/SQLite first, then use a `driver: csv` or `driver: sqlite` recipe.

---

## Phase 5A constraints (binding — be explicit with the user)

The HTTP/JSON driver in Phase 5A is intentionally minimal. Document these limits on every API recipe:

| Constraint | What it means |
|---|---|
| **GET-only** | No POST bodies, no PUT, no DELETE. The endpoint must accept a GET. |
| **Single-page** | The driver fetches one response and stops. No pagination, no cursoring, no `next` link following. If the data is paginated, fetch upstream. |
| **Single-shot** | One HTTP call per recipe execution. No retry-on-failure beyond `ureq`'s defaults (handled by Stream D). |
| **Headers-only auth** | Authorization comes from a single header in `credentials:`. No per-request header rotation, no OAuth refresh. |
| **No streaming response** | The full JSON body is buffered before parsing. Multi-GB responses will pressure memory. |
| **No POST body / form params** | Query params go in the URL itself. The recipe has no `body:` or `params:` field. |

These are Phase 5A scope limits — most are relaxed in Phase 5C as demand-driven driver expansion. For now, work within them.

---

## The `url:` field

The `url:` field is a literal HTTP/HTTPS URL. Any query parameters the endpoint needs are part of the URL itself:

```yaml
source:
  driver: http_json
  url: "https://api.example.com/v1/marketing/monthly?quarter=Q3&plan=baseline"
  json_path: "$.data.items[*]"
```

Rules:

1. The URL is recorded verbatim — no interpolation in `url:` itself in Phase 5A. (`${env.VAR}` interpolation is `credentials:`-only.)
2. The URL must be HTTP or HTTPS. Other schemes (file://, ftp://) aren't supported.
3. URL-encoding is the author's responsibility. If a query parameter contains spaces or special characters, encode them in the recipe (`q=q3+actuals` or `q=q3%20actuals`).
4. Don't put credentials in the URL (e.g., `https://user:pass@host/...`). Use `credentials:` instead.

---

## The `json_path:` field

`json_path:` is a JSONPath expression that selects an **array of row-shaped objects** from the response body. The driver iterates this array and treats each element as a row whose keys are the source-column names.

### Common shapes

#### Top-level array

```json
[
  {"month": "Jan_2026", "channel": "Paid_Search", "market": "Tampa", "spend": 10500, "cpc": 1.5},
  {"month": "Jan_2026", "channel": "Paid_Search", "market": "Orlando", "spend": 9800, "cpc": 1.55}
]
```

```yaml
json_path: "$[*]"
```

#### Wrapped array (most common in real APIs)

```json
{
  "data": {
    "items": [
      {"month": "Jan_2026", "channel": "Paid_Search", ...},
      {"month": "Jan_2026", "channel": "Paid_Social", ...}
    ]
  },
  "meta": { "total": 24, "next": null }
}
```

```yaml
json_path: "$.data.items[*]"
```

#### Single key wrapping the array

```json
{ "results": [ {...}, {...} ] }
```

```yaml
json_path: "$.results[*]"
```

#### Nested under a category

```json
{
  "marketing": {
    "monthly": [ {...} ]
  }
}
```

```yaml
json_path: "$.marketing.monthly[*]"
```

### What `json_path:` is NOT

- **Not a transformation language.** You can't do filtering, projection, or computation in JSONPath. If the response shape doesn't match the cube columns, fix it upstream or pull to CSV first.
- **Not a multi-array selector.** The driver expects ONE array of rows. Selectors that return multiple disjoint arrays (`$..items`) are out of scope.
- **Not a recursive descent in Phase 5A.** Stick to direct paths (`$.a.b.c[*]`); avoid `$..` recursive descent — behavior is implementation-defined.

When in doubt, ask the user for a sample response, then pick the simplest path that selects the row array.

### Mapping JSON keys to recipe columns

Each row-object's keys are the recipe's `source:` names. The keys must match exactly (case-sensitive):

```json
[ {"period": "Jan_2026", "spend_usd": 10500, "cpc_usd": 1.5} ]
```

```yaml
json_path: "$[*]"
columns:
  - { source: period,    dimension: Time }
  - { source: spend_usd, measure: Spend, type: f64 }
  - { source: cpc_usd,   measure: CPC,   type: f64 }
```

The recipe doesn't rename — `source: period` matches the JSON key `period`. If the API key is `Period` (capitalized), the recipe must use `source: Period`.

---

## Auth patterns

All auth in Phase 5A flows through the `credentials:` block as `${env.VAR}` references. Stream D injects them as request headers.

### Bearer token (most common)

```yaml
credentials:
  authorization: "Bearer ${env.ACME_API_TOKEN}"
```

The full string `Bearer <token>` goes into the `Authorization` header. The `${env.ACME_API_TOKEN}` resolves at runtime; an unset variable fires **MC5013**.

### API key in a custom header

```yaml
credentials:
  x-api-key: "${env.ACME_API_KEY}"
```

The credential key (`x-api-key` here) is the literal HTTP header name. Stream D forwards it as-is.

### Basic auth (rare; encode upstream)

Phase 5A doesn't have a dedicated Basic-auth resolver. If you need Basic auth, the user must pre-compute the `Basic <base64(user:pass)>` string and put it in `credentials.authorization`:

```yaml
credentials:
  authorization: "Basic ${env.ACME_BASIC_TOKEN}"   # token = base64("user:pass")
```

Recommend computing the base64 in shell once (`echo -n 'user:pass' | base64`) and storing the result in the env var.

### No auth

For public endpoints, omit `credentials:` entirely or set it empty:

```yaml
credentials: {}
```

### What's NOT supported

- OAuth flows requiring multiple round-trips (token refresh, code exchange).
- JWT signing per request.
- Rotating headers (e.g., per-request HMAC signatures).
- Cookies / session tokens that need to be set by a prior request.

For these, pull data upstream to CSV/SQLite and use a file-based recipe.

---

## Common API-recipe pitfalls

### "Response shape doesn't match the columns"

The most common HTTP recipe failure: the row-objects don't carry the keys the recipe expects. Always ask for a sample response and confirm the JSON shape before authoring. A 200 OK with the wrong body is worse than a 4xx — silently produces zero rows.

### "Pagination isn't followed"

Phase 5A reads exactly one page. If the response says `meta.total: 24` but only 10 rows are in `data.items`, the driver still stops. The recipe will silently import 10 rows. Mention this constraint to the user; recommend an upstream pagination loop that concatenates pages into a single CSV.

### "Auth header is wrong / unset"

`MC5013` fires at runtime when `${env.VAR}` resolves to nothing. Walk through:

1. Is the env var set in the shell that ran `mc tessera apply`?
2. Does the credential key match what the API expects (`Authorization` vs `X-API-Key` vs `Api-Key`)?
3. Is the token format right (`Bearer X`, `Basic X`, raw token, etc.)?

`mc-recipe` doesn't fire MC5013 itself — Stream D does, at runtime.

### "Response is HTML / error JSON, not data"

Some APIs return 200 OK with an error body when auth fails or rate limits hit. The driver tries to parse the JSON path against that error body and gets nothing. Recommend the user verify the request manually (`curl -H "Authorization: Bearer $TOKEN" <url>`) before authoring the recipe.

### "Numbers come back as strings"

Some APIs return numeric measures as quoted strings (`"spend": "10500"` instead of `"spend": 10500`). Stream D's row transformer rejects string-where-numeric-expected. Either:
- Ask the API maintainer to fix the typing, or
- Pull through DuckDB's `read_json` first (which can coerce on the fly), then run a `driver: duckdb` recipe.

### "Nested object instead of scalar"

If the JSON response has nested objects:

```json
{ "spend": { "amount": 10500, "currency": "USD" } }
```

The recipe can't pluck `$.spend.amount` per-row — it can only address top-level keys of each row. Recommend a pre-transform (DuckDB `read_json` or a small ETL step) to flatten.

---

## Worked example — Acme Conservative scenario from REST

```yaml
version: 1
name: acme_http_conservative
description: "Conservative scenario import from a REST endpoint."
model: ../models/acme.yaml

source:
  driver: http_json
  url: "https://api.example.com/v1/acme/marketing/monthly?plan=conservative"
  json_path: "$.data.items[*]"

columns:
  - { source: period,        dimension: Time }
  - { source: channel,       dimension: Channel }
  - { source: market,        dimension: Market }
  - { source: spend_usd,     measure: Spend, type: f64 }
  - { source: cpc_usd,       measure: CPC,   type: f64 }
  # The endpoint also returns a campaign_id we don't need:
  - { source: campaign_id,   skip: true }

defaults:
  Scenario: Conservative
  Version: Working

write_disposition: replace
incremental: false
batch: { size: 5000 }
on_error: skip_row
on_missing_element: error

credentials:
  authorization: "Bearer ${env.ACME_API_TOKEN}"
```

This is `crates/mc-recipe/examples/recipes/acme-http-json-import.recipe.yaml` — the canonical HTTP/JSON reference. Treat it as the starting point for any REST recipe and adjust the URL, `json_path:`, columns, and credentials to fit the source.

---

## When to push back to the user

API sources fail validation more often than file-based ones because the response shape isn't always knowable from a description. When the user asks for an HTTP recipe, reach for these clarifying questions:

1. **"Can you share a sample response (or curl invocation)?"** — confirms the shape before authoring.
2. **"Is the data paginated? If yes, how many pages?"** — Phase 5A is single-page; if pages > 1, recommend upstream extraction.
3. **"What auth does it use?"** — Bearer / API-key fits in `credentials:`; OAuth / signed-request doesn't.
4. **"Are dimension values returned in the cube's element naming convention?"** — `Paid_Search` vs `paid_search` vs `paid search`. Mismatches fire `MC5009` at validation time and rows fail `on_missing_element` at runtime.

Don't author an API recipe blind. The HTTP driver's failure modes are quieter than file-based drivers' failure modes — a wrong `json_path:` produces zero rows, not a parse error.

---

## Cross-references

- General recipe schema + the 18 MC5xxx codes: [`../recipe-format/SKILL.md`](../recipe-format/SKILL.md).
- CSV driver: [`../csv-mapping/SKILL.md`](../csv-mapping/SKILL.md).
- SQL-family drivers (SQLite / DuckDB / Postgres): [`../sql-mapping/SKILL.md`](../sql-mapping/SKILL.md).
- Acme reference model: [`../../domain-schemas/marketing-mix/SKILL.md`](../../domain-schemas/marketing-mix/SKILL.md).
- Worked HTTP/JSON example: `crates/mc-recipe/examples/recipes/acme-http-json-import.recipe.yaml`.
