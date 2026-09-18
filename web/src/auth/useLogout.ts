import { createContext, useContext } from "react";

export const LogoutContext = createContext<(() => void) | undefined>(undefined);

/** The authenticated shell's logout action, provided by `AuthenticatedSession`. */
export function useLogout(): () => void {
  const logout = useContext(LogoutContext);
  if (!logout) {
    throw new Error("useLogout must be used within AuthenticatedSession");
  }
  return logout;
}
