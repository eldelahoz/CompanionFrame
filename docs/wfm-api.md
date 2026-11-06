# warframe.market API reference (for Rust client)

Scope: facts from the public docs (API `v0.25.0`, WebSocket `v0.13.0`). Legacy v1 is deprecated and unsupported, with no new v1 docs planned.

**Source caveat:** `docs.warframe.market` is behind a Cloudflare challenge (fetch and browser both blocked). Content below was read from the docs source repo `42bytes-team/wfm-docs` at commit `c76d869` (2026-06-23). Live URLs are derived from source paths. Re-verify against the live site before relying on field-level details.

Doc sources:
- Intro: https://docs.warframe.market/docs/intro/
- API overview: https://docs.warframe.market/docs/api/overview/
- Rules: https://docs.warframe.market/docs/rules/overview/
- OAuth: https://docs.warframe.market/docs/oauth/overview/

---

## 1. Base URLs

Source: https://docs.warframe.market/docs/api/overview/ · https://docs.warframe.market/docs/websockets/overview/

| Kind | URL |
|---|---|
| REST v2 (production) | `https://api.warframe.market/v2/` |
| Static assets (icons, avatars, backgrounds) | `https://warframe.market/static/assets/` (resolve relative asset paths against this) |
| WebSocket (production) | `wss://ws.warframe.market/socket` |
| REST v1 | Deprecated and unsupported. No new docs. |

Endpoint paths in section 5 are relative to `https://api.warframe.market`, e.g. `/v2/items`.

---

## 2. Auth

Sources: https://docs.warframe.market/docs/api/authentication/ · https://docs.warframe.market/docs/intro/ · https://docs.warframe.market/docs/oauth/overview/ · https://docs.warframe.market/docs/websockets/authentication/

| Topic | Rule |
|---|---|
| Session endpoints | `POST /auth/signin`, `POST /auth/signup`, `POST /auth/refresh`, `POST /auth/signout`. Documented as first-party only. |
| Sign-in / sign-up restriction | First-party clients only. Client must be registered first-party, and request must pass Firebase App Check. |
| Sign-in body | JSON: `email`, `password`, `clientId`, `deviceId` (6–256 chars), `deviceName` (default `Unknown`). |
| Sign-in response | `data: { accessToken, refreshToken, tokenType: "Bearer", expiresIn }` (seconds). |
| Refresh body | JSON: `grantType: "refresh_token"`, `clientId`, `deviceId`, `refreshToken`. No `Authorization` header. |
| Authenticated requests | `Authorization: Bearer <accessToken>` (example given in signout docs). |
| OAuth 2.0 | Not available. Public client registration is closed. Docs say integrations needing user auth still use the existing v1 flow. |
| v1 token scope | v1-issued tokens currently have scope `all`. |
| WebSocket auth | `@wfm|cmd/auth/signIn` with `{ token: <JWT> }`. Empty token = legacy cookie fallback. |
| Third-party path to a v2 token | v1 sign-in, then the same JWT as `Bearer` on v2 (see [wfm-auth.md](wfm-auth.md)). Not documented upstream: see section 7. |

### Required and optional headers

Source: https://docs.warframe.market/docs/api/overview/ (Global Headers) · https://docs.warframe.market/docs/api/authentication/

| Header | Applies to | Values | Default |
|---|---|---|---|
| `Authorization` | Auth-required endpoints | `Bearer <token>` | none |
| `Language` | Localized endpoints (`i18n` data) | `en ko ru de fr pt zh-hans zh-hant es it pl uk tr ja` | `en` |
| `Platform` | Platform-specific endpoints | `pc ps4 xbox switch mobile` | `pc` |
| `Crossplay` | Endpoints with crossplay filtering | `true` / `false` | `false` |
| `Content-Type` | JSON bodies | `application/json` | — |
| `Content-Type` | Avatar/background upload | `multipart/form-data` | — |
| `X-Firebase-AppCheck` | First-party auth endpoints only | token | — |
| `User-Agent` | All | see below | — |

Example from docs: `Platform: pc` + `Crossplay: true`.

### User-Agent rules

Source: https://docs.warframe.market/docs/rules/overview/

- Use a dedicated, descriptive `User-Agent` that identifies the project. Include name, version, and a website, repo, or contact.
- Example: `ExampleMarketTool/1.2.0 (+https://example.com/contact)`.
- Do not impersonate a browser or another application. Such traffic may be blocked.

---

## 3. Rate limits

Sources: https://docs.warframe.market/docs/api/overview/ · https://docs.warframe.market/docs/rules/overview/

| Scope | Limit |
|---|---|
| General public API | `3` requests/second |
| Contract search endpoints | ~`10`–`20` requests/minute ("expected", not hard) |
| Other stricter endpoints | Possible, unspecified |
| Cloudflare protection | `429` on rate excess |
| Concurrent connections from one IP | `509` |
| WebSocket status broadcast | `@wfm|event/reports/online` sent on connect and about every `30s` |
| WebSocket rate limits | UNKNOWN |
| `Retry-After` header | UNKNOWN (not documented) |

In the app one scheduler (`wfm/scheduler.rs`) spends this budget for every request: 2.5/s overall, and a separate 12/min bucket for contract and auction searches. A 429 raises one gate for everything.

Rules doc: limits may change without notice. Clients should cache, avoid tight polling, and prefer WebSocket subscriptions.

---

## 4. Error format

Sources: https://docs.warframe.market/docs/api/overview/ · https://docs.warframe.market/docs/websockets/overview/

### REST

Success and handled errors use one envelope:

```json
{ "apiVersion": "x.x.x", "data": {}, "error": null }
```

Handled error (`data: null`):

```json
{
  "apiVersion": "x.x.x",
  "data": null,
  "error": { "request": ["app.errors.unauthorized"], "inputs": { "platinum": "app.field.tooSmall" } }
}
```

| Field | Type | Meaning |
|---|---|---|
| `error.request` | `[]string` | Request-level codes, e.g. `app.errors.unauthorized`, `app.errors.forbidden` |
| `error.inputs` | `map[string]string` | Field-level codes for body, query, or multipart fields |

Unhandled critical errors: `5xx` with no documented JSON body.

### WebSocket

Source: https://docs.warframe.market/docs/websockets/overview/ · https://docs.warframe.market/docs/websockets/authentication/

| Route | When |
|---|---|
| `<cmd>:ok` | Command succeeded. Response reuses incoming `id`. |
| `<cmd>:error` | Command failed. `payload` is a code string or a field-to-code map. |
| `@ws:error/cmd/type` | Message is not valid JSON envelope |
| `@ws:error/cmd/route` | Unknown `route` |
| `@ws:error/cmd/handler` | Handler returned an unhandled error |
| `@wfm|protect/error` | Protected route failed auth/authorization. `payload` is a code, e.g. `app.errors.userNotVerified`. |

---

## 5. Endpoint table (REST v2)

Sources:
- Auth: https://docs.warframe.market/docs/api/authentication/
- Orders: https://docs.warframe.market/docs/api/orders/
- Users: https://docs.warframe.market/docs/api/users/
- Manifests: https://docs.warframe.market/docs/api/manifests/
- Achievements: https://docs.warframe.market/docs/api/achievements/
- Dashboard: https://docs.warframe.market/docs/api/dashboard/
- Groups: https://docs.warframe.market/docs/api/groups/

Abbreviations: `L` = Language-aware, `P` = Platform/Crossplay-aware, `B` = Bearer required. "verified" = verified account. "not banned" = account not banned. Response shape column names the envelope `data` type only. Field detail lives in `data-models.mdx` and is not expanded here.

| Method | Path | Auth | Purpose | Key params | Response `data` |
|---|---|---|---|---|---|
| POST | `/auth/signin` | none; first-party + App Check | Sign in, create session | body: `email`, `password`, `clientId`, `deviceId`, `deviceName` | `{accessToken, refreshToken, tokenType, expiresIn}` |
| POST | `/auth/signup` | none; first-party + App Check | Register and create session | body: + `passwordConfirmation`, `platform`, `locale` | `{accessToken, refreshToken, tokenType, expiresIn}` |
| POST | `/auth/refresh` | none (App Check only for first-party `clientId`) | Refresh session | body: `grantType`, `clientId`, `deviceId`, `refreshToken` | `{accessToken, refreshToken, tokenType, expiresIn}` |
| POST | `/auth/signout` | B | End current session | header `Authorization` | empty body, `200` |
| GET | `/v2/achievements` | none, L | All achievements (secret hidden unless admin) | — | `Achievement[]` |
| GET | `/v2/achievements/user/{slug}` (alias `/userId/{userId}`) | none | User's achievements | query `featured` (presence flag) | `Achievement[]` |
| GET | `/v2/dashboard/showcase` | none, L | Mobile home featured items | — | `DashboardShowcase` |
| GET | `/v2/versions` | none | Server, app, collection versions (poll for cache refresh) | — | `{apps, collections, updatedAt}` |
| GET | `/v2/items` | none, L | All tradable items | — | `Item[]` |
| GET | `/v2/item/{slug}` (alias `/itemId/{itemId}`) | none, L | One item | path `slug` or `itemId` | `Item` |
| GET | `/v2/item/{slug}/set` (alias `/itemId/{itemId}/set`) | none, L | Item plus its set members | path `slug` or `itemId` | `{id, items: Item[]}` |
| GET | `/v2/riven/weapons` | none, L | Tradable riven weapons | — | `RivenItem[]` |
| GET | `/v2/riven/weapon/{slug}` | none, L | One riven weapon | path `slug` | `RivenItem` |
| GET | `/v2/riven/attributes` | none, L | Riven attributes | — | `RivenAttribute[]` |
| GET | `/v2/lich/weapons` | none, L | Tradable lich weapons | — | `LichWeapon[]` |
| GET | `/v2/lich/weapon/{slug}` | none, L | One lich weapon | path `slug` | `LichWeapon` |
| GET | `/v2/lich/ephemeras` | none, L | Lich ephemeras | — | `LichEphemera[]` |
| GET | `/v2/lich/quirks` | none, L | Lich quirks | — | `LichQuirk[]` |
| GET | `/v2/sister/weapons` | none, L | Tradable sister weapons | — | `SisterWeapon[]` |
| GET | `/v2/sister/weapon/{slug}` | none, L | One sister weapon | path `slug` | `SisterWeapon` |
| GET | `/v2/sister/ephemeras` | none, L | Sister ephemeras | — | `SisterEphemera[]` |
| GET | `/v2/sister/quirks` | none, L | Sister quirks | — | `SisterQuirk[]` |
| GET | `/v2/locations` | none, L | Known locations | — | `Location[]` |
| GET | `/v2/npcs` | none, L | Known NPCs | — | `Npc[]` |
| GET | `/v2/missions` | none, L | Known missions | — | `Mission[]` |
| GET | `/v2/orders/recent` | none, P | Up to 500 visible orders, last 4h, online users only. Cached 1m. | — | `Order[]` |
| GET | `/v2/orders/item/{slug}` (alias `/itemId/{itemId}`) | none, P | Visible orders for item (lastSeen ≤48h or missing) | path `slug` / `itemId` | `Order[]` |
| GET | `/v2/orders/item/{slug}/top` (alias `/itemId/{itemId}/top`) | none, P | Top 5 sell and top 5 buy from online users | query `rank`, `rankLt`, `charges`, `chargesLt`, `amberStars`, `amberStarsLt`, `cyanStars`, `cyanStarsLt`, `subtype` | `{sell: Order[], buy: Order[]}` |
| GET | `/v2/orders/user/{slug}` (alias `/userId/{userId}`) | none | User's orders; hidden ones only for self or moderator+ | path `slug` / `userId` | `Order[]` (compact) |
| GET | `/v2/orders/my` | B | My orders, visible and hidden | — | `Order[]` (compact) |
| GET | `/v2/order/{id}` | none (no auth callout in docs) | One order | path `id` | `Order` (compact) |
| POST | `/v2/order` | B, verified, not banned | Create order | body: `itemId`, `type` (`sell`/`buy`), `platinum` (1–900000), `quantity` (1–9999), `visible`, `perTrade`, `rank`, `charges`, `subtype`, `amberStars`, `cyanStars` | `Order` (compact) |
| PATCH | `/v2/order/{id}` | B, verified, owner or admin+ | Patch order | body: `platinum`, `quantity`, `visible`, `perTrade`, `rank`, `charges`, `subtype`, `amberStars`, `cyanStars` | `Order` |
| DELETE | `/v2/order/{id}` | B, verified, owner or admin+ | Delete order (destructive) | path `id` | `Order` |
| POST | `/v2/order/{id}/close` | B, verified, not banned, owner or admin+ | Close part or all of order. Full close deletes; partial updates quantity. | path `id`; body: `quantity` (1–9999, divisible by `perTrade`) | `Transaction` (compact) |
| PATCH | `/v2/orders/group/{id}` | B, verified | Set visibility for virtual group | path `id`: `all` or `ungrouped`; body: `visible`, `type` | `{updated: int}` |
| GET | `/v2/me` | B | Authenticated user | — | `UserPrivate` |
| PATCH | `/v2/me` | B | Update profile prefs | body: `about` (≤300, Markdown), `platform`, `crossplay`, `locale`, `theme` (`light`/`dark`/`system`), `syncLocale`, `syncTheme` | `UserPrivate` |
| POST | `/v2/me/avatar` | B; multipart `avatar` | Upload avatar (≤5 MiB; png/jpg/jpeg/webp/gif/bmp/avif; output 256×256 WebP) | multipart | `UserPrivate` |
| POST | `/v2/me/background` | B, silver tier+; multipart `background` | Upload background (≤8 MiB; output 1920×820 WebP) | multipart | `UserPrivate` |
| GET | `/v2/user/{slug}` (alias `/userId/{userId}`) | none | Public user info | path `slug` / `userId` | `User` (moderator fields may appear) |

Notes from docs:
- Groups: only virtual `all` and `ungrouped` are documented. Custom group CRUD "will be published later".
- CompanionFrame uses `PATCH /v2/orders/group/all` with `{visible, type}` for "Hide all" / "Show all" on the sell or buy tab (one request, then the orders are re-read). The docs do not list the values of `type`; the app sends `sell` or `buy`. Unverified against the live API.
- Orders: `/v2/order/{id}` GET has no auth callout in docs. Treat as UNKNOWN for anonymous access until tested.

---

## 6. WebSocket

Sources: https://docs.warframe.market/docs/websockets/overview/ · https://docs.warframe.market/docs/websockets/authentication/ · https://docs.warframe.market/docs/websockets/subscriptions/ · https://docs.warframe.market/docs/websockets/status-and-activity/ · https://docs.warframe.market/docs/websockets/reports/

Connect:

```
wss://ws.warframe.market/socket
Sec-WebSocket-Protocol: wfm
```

Connections without subprotocol `wfm` are rejected. All messages are JSON.

Envelope: `{ route: string, payload?: any, id?: string, meta?: { stream?: string, revision?: number } }`. Command responses reuse the incoming `id`.

| Route | Kind | Auth | Purpose | Payload (client→server or event) |
|---|---|---|---|---|
| `@wfm|cmd/auth/signIn` | Command | — | Authorize client | `{ token }` (v1 JWT ok; `""` = cookie fallback). Reply `:ok` / `:error`. |
| `@wfm|cmd/auth/signOut` | Command | — | Remove actor | none |
| `@wfm|event/account/patch` | Event (server) | — | Backend patched account | `UserPatch` |
| `@wfm|cmd/subscribe/newOrders` | Command | — | Live feed of new visible orders | `{ platform?, crossplay? }` (default `crossplay: true`; `switch` forces `false`). Multiple subscriptions allowed. |
| `@wfm|cmd/unsubscribe/newOrders` | Command | — | Stop feed | Must repeat the same `{ platform?, crossplay? }` |
| `@wfm|event/subscriptions/newOrder` | Event (server) | — | New order matched | `Order` |
| `@wfm|cmd/status/set` | Command | B, verified, not banned | Set status and activity | `{ status: online\|ingame\|invisible, duration?: int\|null (60–21600 s), activity?: {type, details?, startedAt}\|null }`. `activity.type`: `UNKNOWN`, `IDLE`, `ON_MISSION`, `IN_DOJO`, `IN_ORBITER`, `IN_RELAY`. `startedAt` in unix seconds, within the last day. Activity is replaced whole. |
| `@wfm|event/status/set` | Stream event | — | Committed status (after sign-in, own changes, timer expiry) | `RichStatus`; `meta.stream = status:{userId}`, `meta.revision` monotonic |
| `@wfm|event/reports/online` | Event (server) | — | Connection and authorized-user counts | `{ connections: int, authorizedUsers: int }`, sent on connect and about every 30 s |

Not implemented (do not rely on): item and profile order subscriptions.

---

## 7. UNKNOWN items

Count: **8**.

| # | Item | Why unknown |
|---|---|---|
| U1 | How a third-party client obtains a v2 `Bearer` token | `/auth/signin` is first-party only, OAuth unavailable, and the v1 flow the docs point to is undocumented. CompanionFrame uses v1 sign-in ([wfm-auth.md](wfm-auth.md)). |
| U2 | Whether `/auth/refresh` works for a third-party client without an initial session | Docs describe the endpoint but do not say whether a third-party session can start. |
| U3 | v1 auth header format (`Authorization: JWT …`, `Cookie: JWT=…`) | v1 is undocumented. Format observed in practice only. |
| U4 | WebSocket `Origin` header requirement | Not in docs. Observed: the edge requires a `User-Agent`, not `Origin`. |
| U5 | `Retry-After` header and 429 backoff contract | Not documented. |
| U6 | WebSocket rate limits | Not documented. |
| U7 | Field-level data models (`Order`, `Item`, `User`, `RichStatus`, …) | `data-models.mdx` (73 KB) was not expanded in this pass. Read it before writing serde structs. |
| U8 | Live-site parity | Docs were read from the source repo at `c76d869` because the live site is blocked. Confirm against `docs.warframe.market` before relying on field-level details. |
