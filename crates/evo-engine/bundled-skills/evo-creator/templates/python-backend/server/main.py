"""A plugin backend in Python: an MCP server over stdio that evo runs with uv
in the plugin's own environment (references/backend.md).

Put this folder's `server/` and `pyproject.toml` in the plugin's root, set
`runtime.server` to `{"language": "python", "entry": "server/main.py"}`,
run `uv lock` there and ship the `uv.lock` it writes: evo will not run the
backend without it. Its network and files are not confined by evo."""

import os

import httpx
from mcp.server.fastmcp import FastMCP

HOST = os.environ.get("EVO_PLUGIN_HOST", "")
KEY = os.environ.get("EVO_PLUGIN_KEY", "")
mcp = FastMCP("my-plugin")


def host(path: str, method: str = "GET", body: dict | None = None) -> dict | None:
    """Ask evo's host API with this backend's key."""
    response = httpx.request(method, f"{HOST}{path}", headers={"Authorization": f"Bearer {KEY}"}, json=body, timeout=30)
    answer = response.json() if response.content else None
    if response.is_error:
        raise RuntimeError((answer or {}).get("message", f"Evo answered {response.status_code}"))
    return answer


@mcp.tool()
def list_items(limit: int = 20) -> list[dict]:
    """List the person's items in the service, newest first."""
    token = host("/v1/accounts/service/token")["accessToken"]
    response = httpx.get(f"https://api.example.com/items?limit={limit}", headers={"Authorization": f"Bearer {token}"}, timeout=30)
    if response.status_code == 401:
        host("/v1/accounts/service/signed-out", "POST")
        raise RuntimeError("The service refused the sign-in; sign in again on the plugin's Accounts tab.")
    response.raise_for_status()
    return [{"id": item["id"], "title": item["title"]} for item in response.json()]


if __name__ == "__main__":
    mcp.run()
