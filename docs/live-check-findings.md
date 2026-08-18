# Pocket ID API live-check: verifying the code review against a real server

A code review of this repo's HTTP calls flagged ten mismatches between our requests and
the vendored OpenAPI spec ([`spec/swagger.yaml`](../spec/swagger.yaml)). Since the spec
itself turned out to be unreliable in places, every disputed finding was verified on
**2026-08-18** against a live throwaway instance of
[`ghcr.io/pocket-id/pocket-id:latest`](https://github.com/pocket-id/pocket-id), and the
trickiest one (audit-log filters) was additionally traced through the upstream Go source.

All upstream links below are pinned to commit
[`2235458`](https://github.com/pocket-id/pocket-id/tree/22354581df545effa981918dd4dbd9162f72859e).

## How the live check was run

No browser or passkey is needed to bootstrap a fresh instance with an API key:

```bash
docker run -d -p 127.0.0.1:1411:1411 \
  -e APP_URL=http://localhost:1411 \
  -e ENCRYPTION_KEY=some-16+-byte-key \
  -e ANALYTICS_DISABLED=true \
  ghcr.io/pocket-id/pocket-id:latest

# Works exactly once on a fresh instance; creates the first admin
# and returns a Set-Cookie: access_token=... session JWT.
curl -s -c cookies.txt -X POST http://localhost:1411/api/signup/setup \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","firstName":"Admin","lastName":"User","email":"admin@example.com"}'

# Mint an API key with the session cookie; the response `token` is the X-API-KEY value.
curl -s -b cookies.txt -X POST http://localhost:1411/api/api-keys \
  -H 'Content-Type: application/json' \
  -d '{"name":"livecheck","expiresAt":"2027-01-01T00:00:00Z"}'
```

## Verdict summary

| # | Finding | Verdict |
|---|---------|---------|
| 1 | SCIM update serializes omitted `token` as `null`, wiping the stored token | **Confirmed live** — token cleared to `""` |
| 2 | `update_application_configuration` doc points at a body shape the server rejects | **Confirmed live** — partial and array shapes both 400 |
| 3 | Audit-log `filters[...]` params don't exist in the spec | **Revised** — they exist, but our `userId` casing is wrong and `location` takes enum values |
| 4 | `update_oidc_client` accepts an `id` the server silently drops | **Confirmed live** — silent no-op |
| 5 | Chosen client secret unsettable through the tool | **Confirmed live** — server accepts `{"secret": ...}` |
| 6 | Preview `scopes` doc (space-separated) contradicts spec (comma-separated) | **Refuted — code is right, spec is wrong** |
| 7 | `POCKET_ID_URL` with a `/api` path produces `/api/api/...` | Confirmed from code (not live-testable meaningfully) |
| 8 | `ttl` duration strings may be rejected | **Half-confirmed** — `"1h"` works, `"7d"` 400s |
| 9 | CIMD `~<base64url>` rewrite could hide non-URL client ids | **Refuted** — such ids can't exist server-side |
| 10 | Preview endpoint declares `BearerAuth`, we send `X-API-KEY` | **Refuted** — API key works; swaggo artifact |

## Deep dive: audit-log filters (finding 3)

The spec declares only pagination and sort params on `GET /api/audit-logs/all` — yet the
server does support filtering. The params are invisible to the spec because they are
parsed straight out of the raw query string rather than a swaggo-annotated DTO.

### The generic filter mechanism

[`backend/internal/utils/list_request_util.go`](https://github.com/pocket-id/pocket-id/blob/22354581df545effa981918dd4dbd9162f72859e/backend/internal/utils/list_request_util.go)
collects every `filters[<key>]` query param, then matches each key — after capitalizing
only its first letter — **exactly** against the Go model's field names, keeping only
fields tagged `filterable:"true"`:

```go
// applyFilters applies filtering to the GORM query based on the provided filters
func applyFilters(filters map[string][]any, query *gorm.DB, meta map[string]FieldMeta) *gorm.DB {
	for key, values := range filters {
		if key == "" || len(values) == 0 {
			continue
		}

		fieldName := CapitalizeFirstLetter(key)
		fieldMeta, ok := meta[fieldName]
		if !ok || !fieldMeta.IsFilterable {
			continue // unknown or non-filterable keys are silently dropped
		}

		query = query.Where(fieldMeta.ColumnName+" IN ?", values)
	}
	return query
}
```

The audit-log model
([`backend/internal/model/audit_log.go`](https://github.com/pocket-id/pocket-id/blob/22354581df545effa981918dd4dbd9162f72859e/backend/internal/model/audit_log.go))
tags exactly two fields as filterable:

```go
type AuditLog struct {
	Base

	Event     AuditLogEvent `sortable:"true" filterable:"true"`
	IpAddress *string       `sortable:"true"`
	Country   string        `sortable:"true"`
	City      string        `sortable:"true"`
	UserAgent string        `sortable:"true"`
	Username  string        `gorm:"-"`
	Data      AuditLogData

	UserID string `filterable:"true"`
	User   User
}
```

So the casing is load-bearing:

- `filters[userID]` → capitalize → `UserID` → **matches** the field name → filters.
- `filters[userId]` → capitalize → `UserId` → no such field → **silently ignored**, and
  the full unfiltered log is returned as if it were the filtered result.

Both behaviors were reproduced live: a bogus `filters[userID]` value returned zero rows,
while a bogus `filters[userId]` value still returned everything.

### The special-cased filters: `clientName` and `location`

Two more filters are hand-implemented in
[`backend/internal/service/audit_log_service.go` (`ListAllAuditLogs`)](https://github.com/pocket-id/pocket-id/blob/22354581df545effa981918dd4dbd9162f72859e/backend/internal/service/audit_log_service.go)
before the generic pass runs:

```go
if clientName, ok := listRequestOptions.Filters["clientName"]; ok {
	dialect := s.db.Name()
	switch dialect {
	case "sqlite":
		query = query.Where("json_extract(data, '$.clientName') IN ?", clientName)
	case "postgres":
		query = query.Where("data->>'clientName' IN ?", clientName)
	...
}

if locations, ok := listRequestOptions.Filters["location"]; ok {
	mapped := make([]string, 0, len(locations))
	for _, v := range locations {
		if s, ok := v.(string); ok {
			switch s {
			case "internal":
				mapped = append(mapped, "Internal Network")
			case "external":
				mapped = append(mapped, "External Network")
			}
		}
	}
	if len(mapped) > 0 {
		query = query.Where("country IN ?", mapped)
	}
}
```

Notable: `filters[location]` accepts **only** the literal values `internal` or
`external`; any other value maps to nothing and the filter silently no-ops. (An earlier
live probe sent `LAN` and concluded the filter didn't exist — the source shows it does,
with an enum.) The frontend confirms the intended parameter set in
[`frontend/src/lib/types/audit-log.type.ts`](https://github.com/pocket-id/pocket-id/blob/22354581df545effa981918dd4dbd9162f72859e/frontend/src/lib/types/audit-log.type.ts):

```ts
export type AuditLogFilter = {
	userID: string;
	event: string;
	location: string;
	client: string;
};
```

**Fix for this repo:** in `AuditLogFilterParams` (`src/tools/admin.rs`), rename the
query key `filters[userId]` → `filters[userID]`, keep `filters[event]` and
`filters[clientName]` as-is, and constrain/document `filters[location]` to
`internal` | `external`.

## The other findings, with live evidence

### 1. SCIM update wipes the token (`src/tools/admin.rs`)

`ScimServiceProviderInput` has no `skip_serializing_if`, so an omitted `token`
serializes as JSON `null` — and the server treats that as "set it to empty":

```bash
# provider created with "token":"original-scim-token-abc", then:
curl -X PUT $BASE/api/scim/service-provider/$SID -H "X-API-KEY: $KEY" \
  -d '{"endpoint":"http://localhost:9999/scim2","oidcClientId":"...","token":null}'
# → 200 OK, response now shows "token":""  — the stored token is gone
```

Fix: add `#[serde(skip_serializing_if = "Option::is_none")]` to `token`.

### 2. Application configuration requires the full flat object (`src/tools/admin.rs`)

`PUT /api/application-configuration` accepts neither a partial object nor the
array shape that `GET /api/application-configuration/all` returns:

```bash
curl -X PUT $BASE/api/application-configuration -d '{"appName":"X"}'
# → 400: "SessionDuration is required, homePageUrl is required, ... (18 keys)"

curl -X PUT $BASE/api/application-configuration \
  -d '[{"key":"appName","type":"","value":"X","isPublic":true}]'
# → 400: "Request body is invalid"
```

Fix: the tool doc must tell callers to build the **flat camelCase object with every
required key** (see `dto.AppConfigUpdateDto` in the spec), not to merge the `/all`
array output.

### 4. Update-client silently ignores a changed `id` (`src/tools/oidc.rs`)

```bash
curl -X PUT $BASE/api/oidc/clients/$CID -d '{"id":"renamed-client","name":"...","callbackURLs":[...]}'
# → 200 OK, response still carries the OLD id
curl $BASE/api/oidc/clients/renamed-client   # → 404
```

Fix: give the update tool its own input type without `id` (the field exists only in
`dto.OidcClientCreateDto`, not `dto.OidcClientUpdateDto`).

### 5. The server accepts a chosen client secret (`src/tools/oidc.rs`)

```bash
curl -X POST $BASE/api/oidc/clients/$CID/secret -d '{"secret":"chosen-secret-0123456789abcdef"}'
# → 200 {"secret":"chosen-secret-0123456789abcdef"}   (stored verbatim)
curl -X POST $BASE/api/oidc/clients/$CID/secret
# → 200 {"secret":"dW6XJyCs0J2rpuIYVV2za5VO3bym7pKj"} (random)
```

Fix: add an optional `secret` parameter (min 16 chars per `dto.OidcClientSecretDto`)
to `create_oidc_client_secret`.

### 6. Preview scopes are space-separated — the spec is wrong

The spec describes the `scopes` query param of
`GET /api/oidc/clients/{id}/preview/{userId}` as comma-separated. Live:

```bash
# space-separated → full claims
...?scopes=openid%20profile%20email
# → accessToken.scope = "openid profile email", userInfo has email/name/etc.

# comma-separated → nothing
...?scopes=openid,profile,email
# → accessToken.scope = "", scp = [], userInfo reduced to bare sub
```

No code change needed; our doc ("space-separated") is correct. Worth an upstream
spec issue.

### 8. `ttl` is a Go duration string — so `"7d"` is a trap

The spec types `ttl` as a bare `object` (`utils.JSONDuration`, a swaggo artifact).
Live, it unmarshals via Go's `time.ParseDuration`, which has no `d` unit:

```bash
curl -X POST $BASE/api/signup-tokens -d '{"ttl":"1h","usageLimit":1}'   # → 201
curl -X POST $BASE/api/signup-tokens -d '{"ttl":"7d","usageLimit":1}'   # → 400 invalid_request_body
curl -X POST $BASE/api/signup-tokens -d '{"ttl":3600000000000,"usageLimit":1}'  # → 400 validation_failed
```

Fix: change the tool docs' example from `"7d"` to `"168h"`-style values and note the
`ns/us/ms/s/m/h` unit set.

### 9. The CIMD `~<base64url>` rewrite is safe

`client_seg` (`src/tools/mod.rs`) rewrites any client id containing a character outside
`[A-Za-z0-9._-]` into the CIMD path form `~<base64url>`. The concern was that a stored
custom id like `team+web` would be silently rewritten and miss. Live, such ids cannot
exist — creation is rejected up front:

```bash
curl -X POST $BASE/api/oidc/clients -d '{"id":"team+web", ...}'
# → 400 {"details":{"fields":[{"field":"id","code":"client_id","message":"is invalid"}]}}
# same for 'team~web', 'team!web', 'team:web', 'team web', 'https://example.com/client'
curl -X POST $BASE/api/oidc/clients -d '{"id":"team.web_ok-1", ...}'   # → 201
```

Every id that triggers the rewrite is therefore necessarily a CIMD URL client id, which
is exactly what the `~<base64url>` form is for.

### 10. The preview endpoint accepts `X-API-KEY`

The spec's lone `security: BearerAuth` annotation on the preview endpoint is a swaggo
artifact; the standard API-key middleware applies:

```bash
curl $BASE/api/oidc/clients/$CID/preview/$USERID?scopes=... -H "X-API-KEY: $KEY"
# → 200 with full idToken/accessToken/userInfo preview
```

No code change needed.

## Resulting work list for this repo

1. `src/tools/dto.rs` — `skip_serializing_if` on `ScimServiceProviderInput.token`.
2. `src/tools/admin.rs` — rewrite `update_application_configuration` docs (full flat
   object, not the `/all` array), optionally validate required keys client-side.
3. `src/tools/admin.rs` — audit filters: `userId` → `userID`, document
   `location` as `internal`/`external`.
4. `src/tools/oidc.rs` — dedicated update input type without `id`.
5. `src/tools/oidc.rs` — optional `secret` param on `create_oidc_client_secret`.
6. `src/tools/identity.rs` — ttl docs: Go duration units, drop the `"7d"` example.

Findings 6, 9, and 10 close with no change; 6 and the missing filter params are
candidates for upstream issues against the spec generation.
