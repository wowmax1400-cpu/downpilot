import type { FC, ReactNode } from "react";

/** Sleek circular icon button with hover transitions. */
export const IconButton: FC<{
  title: string;
  onClick: () => void;
  children: ReactNode;
  tone?: "default" | "accent" | "danger" | "warning";
  size?: "sm" | "md";
  className?: string;
}> = ({ title, onClick, children, tone = "default", size = "md", className = "" }) => {
  const toneCls: Record<string, string> = {
    default: "text-txt-dim hover:bg-surface-3 hover:text-txt",
    accent: "text-txt-dim hover:bg-accent-soft hover:text-accent",
    danger: "text-txt-dim hover:bg-surface-3 hover:text-danger",
    warning: "text-txt-dim hover:bg-surface-3 hover:text-warning",
  };
  const sizeCls = size === "sm" ? "h-7 w-7" : "h-8 w-8";
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      className={`inline-flex ${sizeCls} shrink-0 items-center justify-center rounded-full transition-all duration-200 ease-in-out hover:scale-105 active:scale-95 ${toneCls[tone]} ${className}`}
    >
      {children}
    </button>
  );
};
