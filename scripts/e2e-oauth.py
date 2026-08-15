"""Automated 8.2b: full OAuth 2.1 authorization-code + PKCE flow against a live
Pocket ID, then MCP tool calls over Streamable HTTP with the obtained token.

Bootstraps a fresh Pocket ID container (no passkey needed: the one-time
/api/signup/setup call yields an admin session cookie, which authenticates the
browser-side authorize/consent endpoints). Provisioning (OAuth client, API
definition = resource identifier, client API access) is done through the
pocket-id-mcp stdio server itself.
"""

import base64, hashlib, json, os, secrets, subprocess, sys, time, urllib.parse

import requests

SCRATCH = os.path.dirname(os.path.abspath(__file__))
POCKET_PORT = 1421
POCKET_URL = f"http://localhost:{POCKET_PORT}"
MCP_BIND = "127.0.0.1:8757"
MCP_PUBLIC_URL = f"http://{MCP_BIND}/mcp"
CALLBACK = "http://127.0.0.1:9944/callback"
CONTAINER = "pocket-id-oauth-e2e"

ok_checks = []
def check(name, cond, extra=""):
    assert cond, f"FAILED: {name} {extra}"
    ok_checks.append(name)
    print(f"  ok: {name} {extra}")

def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()

def jwt_payload(token: str) -> dict:
    part = token.split(".")[1]
    return json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))

subprocess.run(["pkill", "-f", "target/debug/pocket-id-mcp"], capture_output=True)
subprocess.run(["pkill", "-f", "cloudflared tunnel"], capture_output=True)
time.sleep(0.5)

# ------------------------------------------------ CIMD tunnel (started early)
# The CIMD metadata document must live at a public https URL (Pocket ID's CIMD
# fetcher blocks private addresses as SSRF protection). A cloudflared quick
# tunnel provides that. Started before the container so its hostname can be
# pinned via --add-host: the local resolver here filters fresh trycloudflare
# subdomains, so both host and container resolve the edge IP explicitly
# (fetched via DNS-over-HTTPS from 1.1.1.1).
import http.server, re, socketserver, threading

DOC_PATH = f"{SCRATCH}/cimd_doc.json"

class CimdHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        doc = open(DOC_PATH, "rb").read()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(doc)))
        self.end_headers()
        self.wfile.write(doc)
    def log_message(self, *a):
        pass

open(DOC_PATH, "w").write("{}")
socketserver.TCPServer.allow_reuse_address = True
cimd_httpd = socketserver.TCPServer(("127.0.0.1", 9080), CimdHandler)
threading.Thread(target=cimd_httpd.serve_forever, daemon=True).start()

tunnel_log = open(f"{SCRATCH}/tunnel.log", "w")
tunnel_proc = subprocess.Popen(
    ["cloudflared", "tunnel", "--url", "http://127.0.0.1:9080"],
    stdout=subprocess.DEVNULL, stderr=tunnel_log)
tunnel_url = None
deadline = time.time() + 60
while time.time() < deadline and tunnel_url is None:
    time.sleep(1)
    m = re.search(r"https://[a-z0-9-]+\.trycloudflare\.com", open(f"{SCRATCH}/tunnel.log").read())
    if m:
        tunnel_url = m.group(0)
assert tunnel_url, "no tunnel URL in cloudflared log"
TUNNEL_HOST = tunnel_url.removeprefix("https://")

EDGE_IP = None
doh_deadline = time.time() + 120
while time.time() < doh_deadline and EDGE_IP is None:
    try:
        doh = requests.get("https://1.1.1.1/dns-query",
                           params={"name": TUNNEL_HOST, "type": "A"},
                           headers={"Accept": "application/dns-json"}, timeout=10).json()
        EDGE_IP = next((a["data"] for a in doh.get("Answer", []) if a.get("type") == 1), None)
    except requests.RequestException:
        pass
    if EDGE_IP is None:
        time.sleep(3)
assert EDGE_IP, f"DNS for {TUNNEL_HOST} never propagated to 1.1.1.1"
print(f"tunnel: {tunnel_url} (edge {EDGE_IP})")

def fetch_tunnel(path):
    """GET https://TUNNEL_HOST{path} without relying on the local resolver."""
    r = subprocess.run(
        ["curl", "-sS", "--max-time", "10",
         "--resolve", f"{TUNNEL_HOST}:443:{EDGE_IP}", f"https://{TUNNEL_HOST}{path}",
         "-w", "\\n%{http_code}"],
        capture_output=True, text=True)
    body, _, status = r.stdout.rpartition("\\n") if False else r.stdout.rpartition("\n")
    return (int(status) if status.strip().isdigit() else 0), body


# ---------------------------------------------------------------- container
subprocess.run(["docker", "rm", "-f", CONTAINER], capture_output=True)
subprocess.run([
    "docker", "run", "-d", "--name", CONTAINER,
    "-p", f"127.0.0.1:{POCKET_PORT}:1411",
    "-e", f"APP_URL={POCKET_URL}",
    "-e", "PORT=1411",
    "-e", "ENCRYPTION_KEY=e2e-oauth-encryption-key-32bytes",
    "-e", "ANALYTICS_DISABLED=true",
    "--add-host", f"{TUNNEL_HOST}:{EDGE_IP}",
    "ghcr.io/pocket-id/pocket-id:latest",
], check=True, capture_output=True)
for _ in range(30):
    try:
        if requests.get(f"{POCKET_URL}/healthz", timeout=2).status_code < 400:
            break
    except requests.RequestException:
        pass
    time.sleep(1)
else:
    sys.exit("pocket-id container did not become healthy")
print("container healthy")

# ------------------------------------------------- admin session + API key
session = requests.Session()

def desecure():
    """The dev instance runs plain HTTP but marks cookies Secure; clear the
    flag so the cookie jar sends them."""
    for c in session.cookies:
        c.secure = False

r = session.post(f"{POCKET_URL}/api/signup/setup",
                 json={"username": "admin", "email": "admin@example.com"})
r.raise_for_status()
desecure()
check("admin bootstrap via signup/setup", "access_token" in session.cookies)

r = session.post(f"{POCKET_URL}/api/api-keys",
                 json={"name": "oauth-e2e", "expiresAt": "2027-01-01T00:00:00Z"})
r.raise_for_status()
API_KEY = r.json()["token"]
open(f"{SCRATCH}/oauth_apikey.txt", "w").write(API_KEY)
check("API key minted", bool(API_KEY))

# ------------------------------------- provisioning via our stdio MCP server
env = dict(os.environ, POCKET_ID_URL=POCKET_URL, POCKET_ID_API_KEY=API_KEY)
proc = subprocess.Popen(["./target/debug/pocket-id-mcp"], stdin=subprocess.PIPE,
                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env)
_id = [0]
def rpc(method, params=None, notify=False):
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    if not notify:
        _id[0] += 1
        msg["id"] = _id[0]
    proc.stdin.write((json.dumps(msg) + "\n").encode())
    proc.stdin.flush()
    if notify:
        return None
    resp = json.loads(proc.stdout.readline())
    assert "result" in resp, f"{method} failed: {resp}"
    return resp["result"]

def tool(name, args=None):
    r = rpc("tools/call", {"name": name, "arguments": args or {}})
    assert not r.get("isError"), f"{name}: {json.dumps(r)[:400]}"
    return r.get("structuredContent") or json.loads(r["content"][0]["text"])

rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "provisioner", "version": "0"}})
rpc("notifications/initialized", notify=True)

client = tool("create_oidc_client", {
    "name": "e2e MCP client", "isPublic": True, "pkceEnabled": True,
    "callbackURLs": [CALLBACK],
})
CLIENT_ID = client["id"]
check("OAuth client pre-registered (public + PKCE)", bool(CLIENT_ID))

api_def = tool("create_api_definition", {"name": "pocket-id-mcp", "resource": MCP_PUBLIC_URL})
api_def = tool("set_api_definition_permissions", {
    "api_id": api_def["id"],
    "permissions": [{"key": "use", "name": "Use the MCP server"}],
})
perm_ids = [p["id"] for p in api_def["permissions"]]
tool("update_client_api_access", {
    "client_id": CLIENT_ID,
    "client_permission_ids": [],
    "user_delegated_permission_ids": perm_ids,
})
check("resource registered as API definition + client granted access", True)

# ------------------------------------------------------- MCP server (HTTP)
mcp_env = dict(env, POCKET_ID_MCP_TRANSPORT="http", POCKET_ID_MCP_HTTP_BIND=MCP_BIND,
               POCKET_ID_MCP_PUBLIC_URL=MCP_PUBLIC_URL)
mcp_http = subprocess.Popen(["./target/debug/pocket-id-mcp"], env=mcp_env,
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
for _ in range(20):
    try:
        if requests.get(f"http://{MCP_BIND}/.well-known/oauth-protected-resource/mcp",
                        timeout=1).status_code == 200:
            break
    except requests.RequestException:
        pass
    time.sleep(0.5)
else:
    sys.exit("MCP HTTP server did not start: " + mcp_http.stderr.read().decode()[:500])

# ------------------------------------------- discovery, as an MCP client would
challenge = requests.post(f"http://{MCP_BIND}/mcp", json={})
check("401 challenge before auth", challenge.status_code == 401
      and "resource_metadata" in challenge.headers.get("WWW-Authenticate", ""))
meta = requests.get(f"http://{MCP_BIND}/.well-known/oauth-protected-resource/mcp").json()
issuer = meta["authorization_servers"][0]
check("protected resource metadata → issuer", issuer == POCKET_URL)
disc = requests.get(f"{issuer}/.well-known/openid-configuration").json()
AUTHZ, TOKEN_EP = disc["authorization_endpoint"], disc["token_endpoint"]
check("OIDC discovery", AUTHZ.endswith("/authorize") and "token" in TOKEN_EP)

# ------------------------------------------------ authorization code + PKCE
def run_authorization(client_id, with_resource=True, scope="openid profile email"):
    verifier = b64url(secrets.token_bytes(32))
    chal = b64url(hashlib.sha256(verifier.encode()).digest())
    state = b64url(secrets.token_bytes(16))
    params = {
        "response_type": "code", "client_id": client_id, "redirect_uri": CALLBACK,
        "scope": scope, "state": state,
        "code_challenge": chal, "code_challenge_method": "S256",
    }
    if with_resource:
        params["resource"] = MCP_PUBLIC_URL

    url = f"{AUTHZ}?{urllib.parse.urlencode(params)}"
    for hop in range(8):
        r = session.get(url, allow_redirects=False)
        desecure()
        loc = r.headers.get("Location", "")
        if "/interaction/error" in loc or "error=" in urllib.parse.urlsplit(loc).query:
            raise AssertionError(f"authorize error redirect: {urllib.parse.unquote(loc)}")
        if loc.startswith(CALLBACK):
            q = urllib.parse.parse_qs(urllib.parse.urlsplit(loc).query)
            assert q.get("state") == [state], f"state mismatch: {loc}"
            assert "code" in q, f"no code in callback: {loc}"
            return q["code"][0], verifier
        if "/interaction/" in loc or "interaction=" in loc:
            # consent (or other) interaction: approve every required step via API
            if "/interaction/" in loc:
                iid = urllib.parse.urlsplit(loc).path.rstrip("/").split("/")[-1]
            else:
                iid = urllib.parse.parse_qs(urllib.parse.urlsplit(loc).query)["interaction"][0]
            info = session.get(f"{POCKET_URL}/api/oidc/interactions/{iid}").json()
            steps = info.get("requiredSteps") or [info.get("currentStep")] or []
            redirect_url = None
            for step in [s for s in steps if s]:
                resp = session.post(
                    f"{POCKET_URL}/api/oidc/interactions/{iid}/complete",
                    json={"step": step})
                resp.raise_for_status()
                redirect_url = resp.json().get("redirectUrl") or redirect_url
            assert redirect_url, f"interaction gave no redirect: {info}"
            url = urllib.parse.urljoin(POCKET_URL, redirect_url)
            continue
        if r.status_code in (301, 302, 303, 307) and loc:
            url = urllib.parse.urljoin(POCKET_URL, loc)
            continue
        raise AssertionError(f"unexpected authorize response {r.status_code}: {r.text[:300]} loc={loc}")
    raise AssertionError("too many redirect hops")

code, verifier = run_authorization(CLIENT_ID)
check("authorization code obtained (PKCE, consent approved via session)", bool(code))

form = {
    "grant_type": "authorization_code", "code": code, "redirect_uri": CALLBACK,
    "client_id": CLIENT_ID, "code_verifier": verifier, "resource": MCP_PUBLIC_URL,
}
tr = requests.post(TOKEN_EP, data=form)
assert tr.status_code == 200, f"token exchange failed: {tr.status_code} {tr.text[:300]}"
access_token = tr.json()["access_token"]
payload = jwt_payload(access_token)
aud = payload.get("aud")
aud_list = aud if isinstance(aud, list) else [aud]
check("token audience bound to resource identifier (RFC 8707)",
      MCP_PUBLIC_URL in aud_list, f"aud={aud}")

# ------------------------------------------------ MCP over Streamable HTTP
def mcp_call(body, token, session_id=None):
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream",
    }
    if session_id:
        headers["Mcp-Session-Id"] = session_id
    return requests.post(f"http://{MCP_BIND}/mcp", headers=headers, json=body)

r = mcp_call({"jsonrpc": "2.0", "id": 1, "method": "initialize",
              "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                         "clientInfo": {"name": "e2e-http", "version": "0"}}}, access_token)
check("MCP initialize over HTTP with OAuth token", r.status_code == 200, f"({r.status_code})")
sid = r.headers.get("mcp-session-id")
mcp_call({"jsonrpc": "2.0", "method": "notifications/initialized"}, access_token, sid)
r = mcp_call({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}, access_token, sid)
check("tools/list over HTTP", r.status_code == 200 and "list_users" in r.text)
r = mcp_call({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
              "params": {"name": "get_current_version", "arguments": {}}}, access_token, sid)
check("tools/call over HTTP hits live upstream", r.status_code == 200 and "2.13" in r.text)

# ------------------------------- negative: token minted WITHOUT resource param
code2, verifier2 = run_authorization(CLIENT_ID, with_resource=False)
tr2 = requests.post(TOKEN_EP, data={
    "grant_type": "authorization_code", "code": code2, "redirect_uri": CALLBACK,
    "client_id": CLIENT_ID, "code_verifier": verifier2,
})
plain_token = tr2.json()["access_token"]
plain_aud = jwt_payload(plain_token).get("aud")
r = mcp_call({"jsonrpc": "2.0", "id": 1, "method": "initialize",
              "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                         "clientInfo": {"name": "x", "version": "0"}}}, plain_token)
check("client-audienced token (no resource param) rejected 401",
      r.status_code == 401 and "audience" in r.text, f"aud={plain_aud}")

# =============================== CIMD variant ===============================
CIMD_CLIENT_ID = f"{tunnel_url}/mcp-client.json"
with open(DOC_PATH, "w") as f:
    json.dump({
        "client_id": CIMD_CLIENT_ID,
        "client_name": "E2E CIMD client",
        "redirect_uris": [CALLBACK],
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code"],
        "response_types": ["code"],
    }, f)

# wait until the document is reachable through the tunnel (resolver-independent)
last = "no attempt"
for i in range(60):
    status, body = fetch_tunnel("/mcp-client.json")
    last = f"HTTP {status}"
    if status == 200 and CIMD_CLIENT_ID in body:
        break
    time.sleep(2)
else:
    sys.exit(f"CIMD document not reachable through tunnel ({last})")
check("CIMD document hosted at public https URL", True, tunnel_url)

# allowlist the CIMD URL in the app config (read-modify-write the full doc)
all_config = tool("get_all_application_configuration")
config_map = {v["key"]: (v.get("value") or "") for v in all_config}
config_map["cimdUrlAllowlist"] = json.dumps([f"{tunnel_url}/*"])
tool("update_application_configuration", {"config": config_map})
check("CIMD URL allowlisted via update_application_configuration", True)

# First CIMD authorization (no resource): materializes the client in Pocket ID
code_c, verifier_c = run_authorization(CIMD_CLIENT_ID, with_resource=False)
tr_c = requests.post(TOKEN_EP, data={
    "grant_type": "authorization_code", "code": code_c, "redirect_uri": CALLBACK,
    "client_id": CIMD_CLIENT_ID, "code_verifier": verifier_c,
})
assert tr_c.status_code == 200, f"CIMD token exchange failed: {tr_c.status_code} {tr_c.text[:300]}"
check("CIMD self-registration: authorize + token with URL client_id",
      CIMD_CLIENT_ID in (lambda a: a if isinstance(a, list) else [a])(
          jwt_payload(tr_c.json()["access_token"]).get("aud")))

# Grant the (now materialized) CIMD client access to the MCP resource, then
# run the flow again with the resource indicator and call tools over HTTP.
tool("update_client_api_access", {
    "client_id": CIMD_CLIENT_ID,
    "client_permission_ids": [],
    "user_delegated_permission_ids": perm_ids,
})
code_c2, verifier_c2 = run_authorization(CIMD_CLIENT_ID, with_resource=True)
tr_c2 = requests.post(TOKEN_EP, data={
    "grant_type": "authorization_code", "code": code_c2, "redirect_uri": CALLBACK,
    "client_id": CIMD_CLIENT_ID, "code_verifier": verifier_c2,
    "resource": MCP_PUBLIC_URL,
})
assert tr_c2.status_code == 200, f"CIMD resource token failed: {tr_c2.status_code} {tr_c2.text[:300]}"
cimd_token = tr_c2.json()["access_token"]
cimd_aud = jwt_payload(cimd_token).get("aud")
check("CIMD token audience bound to resource",
      MCP_PUBLIC_URL in (cimd_aud if isinstance(cimd_aud, list) else [cimd_aud]),
      f"aud={cimd_aud}")

r = mcp_call({"jsonrpc": "2.0", "id": 1, "method": "initialize",
              "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                         "clientInfo": {"name": "cimd-client", "version": "0"}}}, cimd_token)
check("MCP initialize with CIMD-issued token", r.status_code == 200, f"({r.status_code})")
sid_c = r.headers.get("mcp-session-id")
mcp_call({"jsonrpc": "2.0", "method": "notifications/initialized"}, cimd_token, sid_c)
r = mcp_call({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
              "params": {"name": "health_check", "arguments": {}}}, cimd_token, sid_c)
check("tools/call with CIMD-issued token", r.status_code == 200 and "healthy" in r.text)

cimd_httpd.shutdown()
tunnel_proc.terminate()
proc.terminate()
mcp_http.terminate()
print(f"\nOAUTH E2E OK — {len(ok_checks)} checks passed (pre-registered + CIMD)")
