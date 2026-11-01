import { useCallback, useEffect, useState } from "react";
import { wfmSessionStatus, wfmSignIn, wfmSignInWebview, wfmSignOut, type WfmSession } from "../lib/ipc";

/** warframe.market session. `session` is null until the first status answer arrives. */
export function useWfmSession() {
  const [session, setSession] = useState<WfmSession | null>(null);

  useEffect(() => {
    let live = true;
    void wfmSessionStatus().then(
      (s) => live && setSession(s),
      () => {},
    );
    return () => {
      live = false;
    };
  }, []);

  /** Rejects with the API's message; the password is only an argument, never kept here. */
  const signIn = useCallback(async (email: string, password: string) => setSession(await wfmSignIn(email, password)), []);
  const signInWebview = useCallback(async () => setSession(await wfmSignInWebview()), []);
  const signOut = useCallback(async () => setSession(await wfmSignOut()), []);
  return { session, signIn, signInWebview, signOut };
}
