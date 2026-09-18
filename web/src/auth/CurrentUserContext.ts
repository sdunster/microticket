import { createContext } from "react";
import type { CurrentUserProviderQuery$data } from "./__generated__/CurrentUserProviderQuery.graphql";

export type CurrentUserContextType = CurrentUserProviderQuery$data["me"];

export const CurrentUserContext = createContext<
  CurrentUserContextType | undefined
>(undefined);
