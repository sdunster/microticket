import {
  startAuthentication,
  browserSupportsWebAuthn,
  browserSupportsWebAuthnAutofill,
} from "@simplewebauthn/browser";
import { rawGraphQL } from "./rawGraphQL";

export { browserSupportsWebAuthn, browserSupportsWebAuthnAutofill };

/**
 * Result of a passkey login attempt.
 * - `ok`: authenticated, carries the session token.
 * - `cancelled`: the user dismissed/aborted the browser prompt (or an
 *   autofill ceremony was superseded by another one). Callers should stay
 *   silent — this isn't an error, it's someone declining to use a passkey.
 * - `failed`: a real failure (couldn't get a challenge from the server, or
 *   the server rejected the assertion — e.g. the credential was deleted).
 *   Callers should surface a generic error.
 */
export type PasskeyLoginResult =
  | { status: "ok"; token: string }
  | { status: "cancelled" }
  | { status: "failed" };

/**
 * Attempt a discoverable (usernameless) passkey login, via raw `fetch` —
 * this runs before any Relay environment exists (there's no session token
 * yet to build one with). With `useAutofill`, the browser surfaces saved
 * passkeys inline on the email field (conditional UI) instead of a modal.
 */
export async function loginWithPasskey(opts?: {
  useAutofill?: boolean;
}): Promise<PasskeyLoginResult> {
  const useAutofill = opts?.useAutofill ?? false;

  const beginResp = await rawGraphQL<{
    beginPasskeyLogin: { challengeId: string; optionsJson: string } | null;
  }>(
    `mutation BeginPasskeyLogin {
      beginPasskeyLogin { challengeId optionsJson }
    }`,
  );

  const challenge = beginResp.data?.beginPasskeyLogin;
  if (!challenge) {
    console.warn(
      "[passkey] beginPasskeyLogin returned no challenge",
      beginResp,
    );
    return { status: "failed" };
  }

  const optionsJSON = JSON.parse(challenge.optionsJson);

  let authResponse;
  try {
    authResponse = await startAuthentication({
      optionsJSON,
      useBrowserAutofill: useAutofill,
    });
  } catch (err) {
    // User cancelled, no matching credential, or an aborted autofill request.
    console.warn("[passkey] startAuthentication threw/aborted:", err);
    return { status: "cancelled" };
  }

  const finishResp = await rawGraphQL<{ finishPasskeyLogin: string | null }>(
    `mutation FinishPasskeyLogin($challengeId: String!, $credentialJson: String!) {
      finishPasskeyLogin(challengeId: $challengeId, credentialJson: $credentialJson)
    }`,
    {
      challengeId: challenge.challengeId,
      credentialJson: JSON.stringify(authResponse),
    },
  );

  const token = finishResp.data?.finishPasskeyLogin ?? null;
  if (token) {
    return { status: "ok", token };
  }
  // The assertion was produced but the server didn't issue a token — e.g.
  // the credential was deleted server-side, or verification failed.
  console.warn(
    "[passkey] finishPasskeyLogin returned no token (verification failed or unknown credential)",
    finishResp,
  );
  return { status: "failed" };
}
