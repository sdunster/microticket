/**
 * Storage for the opaque `mtu_` session token issued by `verifyAuthCode` /
 * `finishPasskeyLogin`. `localStorage` under one key, no cookies — matches
 * the API's `Authorization: Bearer` scheme, which has no notion of
 * same-site cookies to defend.
 */
const SESSION_TOKEN_KEY = "mt_session_token";

export function getSessionToken(): string | null {
  return localStorage.getItem(SESSION_TOKEN_KEY);
}

export function setSessionToken(token: string): void {
  localStorage.setItem(SESSION_TOKEN_KEY, token);
}

export function clearSessionToken(): void {
  localStorage.removeItem(SESSION_TOKEN_KEY);
}
