# Manual test: warframe.market webview sign-in

Needs a real warframe.market account. Run `pnpm tauri dev`, open the Market tab while signed out.

1. Click **Sign in with warframe.market**. A window "Sign in to warframe.market" opens on the official sign-in page (`/auth/signin`; if it shows the home page or a 404, the start URL is wrong: fix `PAGE` in `webview_login.rs`). The app's own form is hidden behind "Use email and password instead".
2. Sign in on the site (any method it offers). Within ~2 s the window closes and Market shows `@yourname`.
   - Fail: window stays open after login -> `GET /v2/me` rejected the cookie JWT as Bearer (see docs/wfm-auth.md section 7). Report the HTTP status from the log.
3. Open the Market tab features that need auth (your orders, presence). Orders load; the WS status shows connected and presence works.
   - Fail on WS only -> `signIn` refuses the cookie JWT.
4. Restart the app: still signed in (keyring), orders load.
5. Sign out, click the button again: the site shows the login form (cookies were cleared), not your account.
6. Cancel: click the button, close the window by hand: the button recovers with "Sign-in cancelled.".
7. Timeout: leave the window open 5 min: it closes and shows "Sign-in timed out.".
8. Click the button twice: the second click focuses the open window.
9. Password form still works via "Use email and password instead".
