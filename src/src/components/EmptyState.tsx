import type { FC, ReactNode } from "react";

/**
 * Gorgeous centered empty state — layered ghost icon, title, subtitle.
 * Used for empty download lists, filtered views, and search miss.
 */
export const EmptyState: FC<{
  icon: ReactNode;
  title: string;
  subtitle: string;
  action?: ReactNode;
}> = ({ icon, title, subtitle, action }) => (
  <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-5 px-6 py-12">
    {/* Ghost icon — layered circles for depth */}
    <div className="relative flex h-16 w-16 items-center justify-center">
      {/* Outer glow ring */}
      <div className="absolute inset-0 rounded-full bg-white/[0.02]" />
      {/* Inner circle */}
      <div className="relative flex h-14 w-14 items-center justify-center rounded-full bg-surface-3">
        <span className="text-txt-faint">{icon}</span>
      </div>
    </div>
    {/* Title */}
    <div className="text-center">
      <p className="text-sm font-medium text-txt">{title}</p>
      <p className="mt-1 text-xs text-zinc-500">{subtitle}</p>
    </div>
    {/* Optional action button */}
    {action && <div className="mt-1">{action}</div>}
  </div>
);