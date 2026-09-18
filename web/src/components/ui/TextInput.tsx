import type { InputHTMLAttributes } from "react";
import { inputBase } from "./inputStyles";

type TextInputProps = InputHTMLAttributes<HTMLInputElement>;

export default function TextInput({ className, ...props }: TextInputProps) {
  return (
    <input
      className={[inputBase, className].filter(Boolean).join(" ")}
      {...props}
    />
  );
}
