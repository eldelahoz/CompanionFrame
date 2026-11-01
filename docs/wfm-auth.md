# warframe.market auth

How CompanionFrame signs in to warframe.market and what is still unverified. Probes sent no credentials (empty or `{}` bodies only); status codes were observed on 2026-10-25 and 2026-10-26.

## Why v1

The v2 `/auth/signin` is first-party only and OAuth is not available to third parties. The official docs (`intro.md`, `oauth/overview.md`) say integrations that need user authorization "still need to rely on the existing v1 authorization flow", and the WebSocket `@wfm|cmd/auth/signIn` "supports auth tokens from the v1 API". So the client signs in on v1 and uses the same JWT on v2 and on the WebSocket. v1 is "deprecated and unsupported" and could be removed or gated without notice.

## Flow (implemented in `src-tauri/src/services/wfm/`)

1. **Password sign-in** (`client.rs`, `session.rs`): `POST https://api.warframe.market/v1/auth/signin` in header mode.
   - Headers: `Authorization: JWT` (bare scheme, no token; it opts out of the cookie session so no CSRF page is needed), `auth_type: header`, `Platform: pc`, `Language: en`, and an identifiable `User-Agent: CompanionFrame/<version>`.
   - Body: `{email, password, device_id, auth_type: "header"}`. `device_id` is a random UUID stored in `wfm-device-id` next to `settings.json`.
   - The JWT is read from the response `Authorization` header, else `Set-Cookie: JWT=`, else the body (`payload.token`).
   - One request only: no CSRF/cookie retry and no resend on 429/5xx. The password is zeroized after use, never stored, never logged.
2. **Using the token**: `Authorization: JWT <t>` on v1, `Authorization: Bearer <t>` on v2. The server rotates the token through the response `Authorization` header; the client adopts it, re-saves it and updates the WebSocket token. There is no refresh grant on this path.
3. **WebSocket** (`ws.rs`): `wss://ws.warframe.market/socket`, subprotocol `wfm`, then `@wfm|cmd/auth/signIn {token}`. The edge requires a `User-Agent` and does not require `Origin` (checked with an anonymous handshake). Backoff 1-60 s with jitter; it gives up after 3 sign-in rejections until a new token arrives.
4. **Storage**: `{token, user_name, platform}` as JSON in the OS credential store (service `com.companionframe.app`, user `wfm-session`). If the keyring refuses, the login lives in memory only.
5. **Expiry**: a 401 on an authenticated call drops the token, deletes the keyring entry and reports `expired: true`. Nothing retries; the user signs in again.

Commands: `wfm_sign_in`, `wfm_sign_out`, `wfm_session_status`, `wfm_sign_in_webview`, `wfm_sign_in_webview_cancel`.

## Webview sign-in (no password in the app)

`wfm_sign_in_webview` (`webview_login.rs`) opens a visible window on `https://warframe.market/auth/signin` with its own data directory (`wfm-login` under the app local data dir). The user signs in on the site. Every 1.5 s the app reads the `JWT` cookie; when it is a new, decodable JWT it calls `GET /v2/me` with it as `Bearer`. A 200 means a real login (the anonymous cookie gets 401), and the session adopts it (`source: webview`, same keyring entry). It times out after 5 minutes; closing the window cancels. Sign-out deletes the `wfm-login` directory. The password form stays as the secondary option.

Needs a real login to verify, see [manual-tests/wfm-webview-login.md](manual-tests/wfm-webview-login.md): that the cookie JWT is accepted as `Bearer`, that the WebSocket accepts it, and the name field in `/v2/me` (`ingameName`, else `slug`). If `/me` rejects it, sign-in stays pending until the timeout and nothing is stored.

## Cloudflare clearance helper

`warframe.market` HTML pages are challenged by Cloudflare for curl and headless browsers; `api.warframe.market` was not challenged for plain requests from the tested network. `services/wfm_cookie.rs` opens a hidden webview (own data dir `wfm-clearance`), waits for the `JWT` cookie (up to 30 s) and returns only the `cf_clearance` / `__cf*` cookies plus the webview's UA. It is used for warframe.market image assets (`wfm_images.rs`), not for sign-in.

## Observed responses

| Probe | Result |
|---|---|
| `POST /v1/auth/signin` `{}`, no `Authorization` | 400 `CSRF: Token is not present in the request header` |
| `POST /v1/auth/signin` header mode, empty email/password | 400 `{"error":{"email":["app.form.invalid"]}}` (reaches form validation) |
| `POST /v2/auth/signin` `{}` | 400, validation of `email`/`password`/`clientId`/`deviceId` |
| `GET /v2/items` | 200 (anonymous) |
| `GET /v2/me` | 401 `app.errors.unauthorized` |
| `GET /v2/me` with a fake JWT as `Bearer` or `Cookie: JWT=` | `app.jwt.algorithmMismatch` (both are parsed); `Authorization: JWT` on v2 answers `app.errors.unauthorized` |
| WS upgrade, `wfm` subprotocol, identifiable UA, with or without `Origin` | 101; without a UA, 403 |
| `GET https://warframe.market/` with a plain UA | 403 `cf-mitigated: challenge` |

## Rules that shape the client

From the warframe.market API rules: use an identifiable User-Agent and do not pose as a browser; rate limits can change without notice (the scheduler in `scheduler.rs` stays well under them); automation and trade bots are a "grey area", so every order, auction or +rep write is a user click.

## Open questions

1. Token lifetime and rotation frequency (needs a long logged-in session to observe).
2. Whether v1 sign-in has a removal timeline (the docs direct third parties to the warframe.market Discord).
3. Whether a real v1 header-mode sign-in succeeds from every network with a non-browser UA; only the validation step was probed.
