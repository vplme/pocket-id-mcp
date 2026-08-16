# Tasks: fix-tool-output-schemas

## 1. Envelope

- [ ] 1.1 Add generic `Enveloped<T>` (`{ result: T }`) with `Serialize` + `JsonSchema` derives in `src/dto.rs`
- [ ] 1.2 Convert the 8 array-returning tools (`list_user_groups_of_user`, `list_user_passkeys`, `get_all_application_configuration`, `get_public_application_configuration`, `update_application_configuration`, `update_group_custom_claims`, `update_user_custom_claims` — across `identity.rs`/`admin.rs`) to `Json<Enveloped<Vec<...>>>`
- [ ] 1.3 Convert the 6 type-less tools (`create_one_time_access_token`, `get_current_version`, `get_latest_version`, `get_custom_claim_suggestions`, `introspect_token`, `list_audit_log_client_names`, `list_audit_log_users`) to `Json<Enveloped<serde_json::Value>>` (or their existing inner type)

## 2. Regression test

- [ ] 2.1 Extend the schema test in `tests/tiers.rs`: with all tiers enabled, every registered tool with an `outputSchema` has root `"type": "object"`, failure message naming the tool

## 3. Verification

- [ ] 3.1 `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` all pass
- [ ] 3.2 Manual check: connect Claude Code to a locally running server and confirm the tool list loads without "tools fetch failed"
