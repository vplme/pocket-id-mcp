# api-client

## ADDED Requirements

### Requirement: Authenticated HTTP client
The client module SHALL send the configured API key in the `X-API-KEY` header on every request to the Pocket ID instance, and SHALL construct URLs by joining the configured base URL with API paths.

#### Scenario: Header present on all requests
- **WHEN** any tool invokes any Pocket ID API operation
- **THEN** the outgoing request carries `X-API-KEY: <configured key>` and no key material appears in logs or error messages

### Requirement: Typed error mapping
The client SHALL map failure responses into structured errors that tools return as MCP tool errors: HTTP status, Pocket ID's error message body when present, and the operation attempted.

#### Scenario: API rejects a request
- **WHEN** the API returns a non-2xx response with a JSON error body
- **THEN** the tool result is an MCP error whose text includes the status code and the upstream error message, not a raw panic or opaque failure

#### Scenario: Network failure
- **WHEN** the request fails at the transport level (DNS, connect, timeout)
- **THEN** the tool returns an MCP error describing the network failure and the target host

### Requirement: Multipart upload support
The client SHALL support `multipart/form-data` uploads with a `file` field, sourcing bytes from a local file path or a fetched HTTPS URL, and setting a filename and content type inferred from the source.

#### Scenario: Upload from local path
- **WHEN** a tool is given `file_path` pointing to a readable file
- **THEN** the client streams that file as the `file` multipart field to the target endpoint

#### Scenario: Upload from URL
- **WHEN** a tool is given `url` with an HTTPS address
- **THEN** the client fetches the resource and re-uploads its bytes as the `file` multipart field, propagating the fetched content type

### Requirement: Binary download support
The client SHALL retrieve binary responses (images) preserving the response content type and bytes for tool-layer rendering.

#### Scenario: Image fetched
- **WHEN** a tool requests an image endpoint
- **THEN** the client returns the raw bytes plus the `Content-Type` header value

### Requirement: Spec coverage accounting
The repository SHALL vendor the upstream `swagger.yaml`, and a test SHALL fail if any operation (path + method) in the vendored spec is neither mapped to a registered tool nor present in an explicit exclusion list with a documented reason.

#### Scenario: New operation appears in vendored spec
- **WHEN** the vendored `swagger.yaml` is updated with an operation not mapped or excluded
- **THEN** the coverage test fails, naming the unmapped operation
