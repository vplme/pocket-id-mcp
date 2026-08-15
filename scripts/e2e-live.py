import base64, json, os, struct, subprocess, sys, uuid, zlib
SUF = uuid.uuid4().hex[:6]

SCRATCH = os.path.dirname(os.path.abspath(__file__))
API_KEY = open(f"{SCRATCH}/apikey.txt").read().strip()

# tiny valid 8x8 red PNG
def make_png(path):
    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c))
    raw = b"".join(b"\x00" + b"\xff\x00\x00" * 8 for _ in range(8))
    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", 8, 8, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw))
           + chunk(b"IEND", b""))
    open(path, "wb").write(png)
    return png

png_path = f"{SCRATCH}/e2e_logo.png"
png_bytes = make_png(png_path)

env = dict(os.environ, POCKET_ID_URL="http://localhost:1411", POCKET_ID_API_KEY=API_KEY,
           POCKET_ID_MCP_ALLOW_DANGEROUS="true")
p = subprocess.Popen(["./target/debug/pocket-id-mcp"], stdin=subprocess.PIPE,
                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env)
_id = [0]
def rpc(method, params=None, notify=False):
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None: msg["params"] = params
    if not notify:
        _id[0] += 1; msg["id"] = _id[0]
    p.stdin.write((json.dumps(msg) + "\n").encode()); p.stdin.flush()
    if notify: return None
    resp = json.loads(p.stdout.readline())
    assert "result" in resp, f"{method} failed: {resp}"
    return resp["result"]

def call(tool, args=None):
    r = rpc("tools/call", {"name": tool, "arguments": args or {}})
    assert not r.get("isError"), f"{tool} error: {json.dumps(r)[:400]}"
    return r

def structured(r):
    return r.get("structuredContent") or json.loads(r["content"][0]["text"])

ok = []
def check(name, cond, extra=""):
    assert cond, f"FAILED: {name} {extra}"
    ok.append(name); print(f"  ok: {name} {extra}")

rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "e2e", "version": "0"}})
rpc("notifications/initialized", notify=True)

# --- users ---
users = structured(call("list_users"))
check("list_users", any(u["username"] == "admin" for u in users["data"]))
bob = structured(call("create_user", {"username": f"e2e-bob-{SUF}", "email": f"bob-{SUF}@example.com", "firstName": "Bob"}))
check("create_user", bob["username"] == f"e2e-bob-{SUF}", bob["id"])
bob2 = structured(call("update_user", {"user_id": bob["id"], "username": f"e2e-bob-{SUF}", "email": f"bob-{SUF}@example.com", "firstName": "Bobby", "lastName": "Tables"}))
print("DEBUG update_user resp:", json.dumps(bob2)[:300]); check("update_user", bob2["firstName"] == "Bobby")
got = structured(call("get_user", {"user_id": bob["id"]}))
check("get_user", got["lastName"] == "Tables")

# --- groups ---
grp = structured(call("create_group", {"name": f"e2e-testers-{SUF}", "friendlyName": "E2E Testers"}))
check("create_group", grp["name"] == f"e2e-testers-{SUF}", grp["id"])
grp2 = structured(call("set_group_users", {"group_id": grp["id"], "user_ids": [bob["id"]]}))
check("set_group_users", True)
detail = structured(call("get_group", {"group_id": grp["id"]}))
check("get_group members", any(u["id"] == bob["id"] for u in (detail.get("users") or [])))
claims = structured(call("update_user_custom_claims", {"user_id": bob["id"], "claims": [{"key": "team", "value": "qa"}]}))
check("update_user_custom_claims", any(c["key"] == "team" for c in claims))

# --- oidc client ---
client = structured(call("create_oidc_client", {"name": f"e2e-app-{SUF}", "callbackURLs": ["https://app.example.com/callback"], "pkceEnabled": True}))
check("create_oidc_client", client["name"] == f"e2e-app-{SUF}", client["id"])
secret = structured(call("create_oidc_client_secret", {"client_id": client["id"]}))
check("create_oidc_client_secret", len(secret.get("secret") or "") >= 16)
restricted = structured(call("update_oidc_client_allowed_groups", {"client_id": client["id"], "user_group_ids": [grp["id"]]}))
check("update_oidc_client_allowed_groups", True)
cl = structured(call("get_oidc_client", {"client_id": client["id"]}))
check("client group restriction visible", any(g["id"] == grp["id"] for g in (cl.get("allowedUserGroups") or [])))

# --- image round trip ---
call("update_application_image", {"image_type": "logo", "light": False, "file_path": png_path})
img = call("get_application_image", {"image_type": "logo", "light": False})
img_block = next(b for b in img["content"] if b["type"] == "image")
fetched = base64.b64decode(img_block["data"])
check("image round-trip", fetched == png_bytes, f"({img_block['mimeType']}, {len(fetched)} bytes)")

# light-flag validation on non-logo
bad = rpc("tools/call", {"name": "get_application_image", "arguments": {"image_type": "favicon", "light": True}})
check("light flag rejected for favicon", bad.get("isError") and "logo" in bad["content"][0]["text"])

# --- audit logs ---
logs = structured(call("list_all_audit_logs", {"limit": 20}))
check("list_all_audit_logs", isinstance(logs.get("data"), list), f"({len(logs['data'])} entries)")

# --- versions / health ---
ver = structured(call("get_current_version"))
check("get_current_version", "2.13" in json.dumps(ver), json.dumps(ver)[:60])
health = call("health_check")
check("health_check", True, health["content"][0]["text"][:40])

# --- introspection of a real token (cookie access token is a Pocket ID JWT) ---
# --- dangerous cleanup: delete user, group, client ---
call("delete_user", {"user_id": bob["id"]})
check("delete_user (dangerous tier)", True)
call("delete_group", {"group_id": grp["id"]})
call("delete_oidc_client", {"client_id": client["id"]})
users_after = structured(call("list_users"))
check("cleanup verified", not any(u["username"] == f"e2e-bob-{SUF}" for u in users_after["data"]))

p.terminate()
print(f"\nLIVE E2E OK — {len(ok)} checks passed against Pocket ID v2.13.0")
