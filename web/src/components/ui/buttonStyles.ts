import { tw } from "../../lib/tw";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";
export type ButtonSize = "normal" | "large";

export const buttonVariants: Record<ButtonVariant, string> = {
  primary: tw`bg-accent text-white transition-colors hover:bg-accent-dark disabled:cursor-wait disabled:opacity-60`,
  secondary: tw`border border-line bg-surface text-ink transition-colors hover:bg-surface-raised disabled:cursor-wait disabled:opacity-60`,
  danger: tw`bg-red-700 text-white transition-colors hover:bg-red-600 disabled:cursor-wait disabled:opacity-60 dark:bg-red-600 dark:hover:bg-red-500`,
  ghost: tw`text-ink-muted transition-colors hover:text-ink disabled:cursor-wait disabled:opacity-60`,
};

export const buttonSizes: Record<ButtonSize, string> = {
  normal: tw`rounded-md px-4 py-2 text-sm font-medium`,
  large: tw`w-full rounded-md px-6 py-3 text-base font-medium sm:w-auto`,
};
