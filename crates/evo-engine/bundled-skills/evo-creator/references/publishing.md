# Sharing a plugin

## A marketplace

A marketplace is a Git repository (or a folder) with a catalog. The person
adds it in Customize → Plugins → Marketplaces as `owner/repo`, any Git URL
(optionally `#branch`, `#tag` or `#commit`), or a folder; its plugins show
under Discover.

The catalog is `.evo-plugin/marketplace.json` (or `marketplace.json` at the
root):

```json
{
  "name": "acme-tools",
  "owner": { "name": "Acme" },
  "description": "Plugins for Acme's tracker and portal.",
  "plugins": [
    { "name": "issue-desk", "source": "./plugins/issue-desk", "description": "Your issues beside your chats.", "version": "1.2.0", "category": "productivity", "keywords": ["issues"] },
    { "name": "portal", "source": { "source": "github", "repo": "acme/portal-plugin", "ref": "v2.0.1" } },
    { "name": "reports", "source": { "source": "url", "url": "https://git.example.com/reports.git" } },
    { "name": "lint", "source": { "source": "git-subdir", "url": "https://github.com/acme/mono.git", "path": "tools/evo-lint", "ref": "main" } }
  ]
}
```

| `source` | Means |
| --- | --- |
| `"./plugins/x"` | a folder inside the marketplace |
| `{"source": "github", "repo", "ref"?}` | a GitHub repository |
| `{"source": "url", "url", "ref"?}` | any Git URL |
| `{"source": "git-subdir", "url", "path", "ref"?}` | a folder inside another repository |

A single plugin can also be shared as a folder or a repository the person
adds with **Add from folder** after cloning it.

## Versions and updates

- Raise `version` with each release (semver). The person's Update pulls the
  marketplace and installs the new version.
- An update that asks for **new permissions** waits for the person's
  consent; the installed version keeps running with what it had.
- An update that asks for more OAuth `scopes` marks sign-ins "Sign in again
  to grant …".
- Settings, secrets, sign-ins and the data folder are kept across updates;
  removing a setting or account in the manifest leaves its value unused.

## What to ship and not

| Ship | Never ship |
| --- | --- |
| the manifest, the parts, `README.md`, a `LICENSE` | secrets, tokens, `.env` files, test accounts |
| `uv.lock` for a Python backend | `.venv/`, `node_modules/`, `__pycache__/`, Deno's cache |
| icons (SVG or small PNG) | large binaries the plugin does not need |

The README says what the plugin does, each permission and why, what the
person must set up (an OAuth app, a key), and how to remove it.
