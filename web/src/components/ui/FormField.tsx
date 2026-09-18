import type { ReactNode } from "react";

/**
 * A label + control pair, plus optional helper/error text below the
 * control. Not a `<form>` layout primitive beyond that — callers arrange
 * fields with ordinary flex/grid classes.
 */
export function FormField({
  label,
  htmlFor,
  error,
  children,
}: {
  label: ReactNode;
  htmlFor: string;
  error?: string | null;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={htmlFor} className="text-sm font-medium text-ink">
        {label}
      </label>
      {children}
      {error ? (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      ) : null}
    </div>
  );
}
