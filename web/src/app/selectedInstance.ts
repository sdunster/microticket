/**
 * Persists which instance the switcher last had selected, so a reload (or a
 * fresh `/app` visit) lands back where the user left off instead of always
 * defaulting to their first membership.
 */
const SELECTED_INSTANCE_KEY = "mt_selected_instance_id";

export function getStoredSelectedInstanceId(): string | null {
  return localStorage.getItem(SELECTED_INSTANCE_KEY);
}

export function setStoredSelectedInstanceId(instanceId: string): void {
  localStorage.setItem(SELECTED_INSTANCE_KEY, instanceId);
}

export interface SelectedInstance {
  id: string;
  slug: string;
  name: string;
}
