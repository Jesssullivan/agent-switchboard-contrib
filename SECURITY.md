# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.2.2 | Yes (current release) |
| `main` (0.3.0, unreleased) | Yes: fixes land here first |
| 0.2.1 and earlier | No: upgrade to 0.2.2 |
| 0.2.0 | Never published; do not use the tag |

Pre-1.0, only the latest release and `main` get fixes.

## Deployment model: tailnet-only, no authentication

`swb` is designed to run on a private network that already authenticates its
members: loopback, or a tailnet such as Tailscale where every peer is a
known device. **`swb` has no authentication, in any release.** Agent
identity is self-asserted: any client that can reach the broker's main
listener can register as any agent id (`harness:host:pid:session_id`), read
that agent's inbox, send and acknowledge. In v0.2.2 it can also end any
listed session (residual 1 below). From v0.3.0, ending a session needs the
end token its registration returned, but anyone can re-register an agent id
and receive a fresh token, so the token is not takeover-proof. Messages are
marked `authority: peer` and are teammate information only, which limits
what a spoofed message can make an agent do, but it does not stop the
spoofing.

So:

- Never expose the broker's listeners to the internet or to an untrusted
  network.
- Set `SWB_MCP_ALLOWED_HOSTS` to the exact name clients use, and keep
  `SWB_ALLOWED_ORIGINS` empty unless you have a browser client.
- Keep the metrics listener (`SWB_METRICS_LISTEN`) off any shared Service;
  it is not Host-guarded.
- Never put a secret in a message body.

## Known residuals in v0.2.2

Both are fixed on `main` for v0.3.0 (R-C944), which ships them together
with the clients that need the changed routes; see
[docs/releases/v0.3.0-migration.md](docs/releases/v0.3.0-migration.md).

1. **`proc_start` disclosure lets anyone end any session.** `GET /v1/peers`
   (and the MCP `peers` tool) returns every session's `proc_start`, and
   `POST /v1/end` accepts `proc_start` as its only check. Any client that
   can reach the broker can therefore end any listed session. v0.3.0 issues
   a per-session end token at registration, returns it only to the
   registering session, checks it on `/v1/end`, and removes `proc_start`
   from `peers`. `peers` also returns live sessions only, deduplicated by
   session id.
2. **Cross-site GET side effects.** v0.2.2 refuses any request that carries
   a non-allowed `Origin` and checks `Host` on every route, which stops
   DNS-rebinding writes. Browsers send no `Origin` on simple GETs (image or
   script loads, navigations), and over plain http to a non-loopback name
   no `Sec-Fetch-*` headers either. A web page open in a browser on the same
   network that already knows an agent id can therefore trigger, blind:
   - `GET /v1/peers?me=<id>`, which refreshes that session's lease and can
     revive an ended session;
   - `GET /v1/inbox?me=<id>`, which marks messages fetched and counts a
     delivery.

   The page cannot read the reply and cannot register, send or acknowledge,
   and fetched messages stay deliverable. v0.3.0 moves lease refresh and
   inbox to POST, so no GET has a side effect.

## Reporting a vulnerability

Please report privately, not with details in a public issue or pull request.

- If the repository's
  [Security tab](https://github.com/xoxd-ai/agent-switchboard/security)
  shows a **Report a vulnerability** button, use it (GitHub private
  vulnerability reporting).
- Otherwise, open a public issue titled "Security contact request" with no
  details, and a maintainer will set up a private channel.

Include the affected version or commit, how to reproduce, and the impact you
see. A maintainer will acknowledge the report and agree a disclosure date
with you once a fix is ready. The two residuals above and the
self-asserted identity are already known and public; no report is needed
for them.
