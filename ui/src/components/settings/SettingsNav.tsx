import type { ComponentType } from "react";
import { NavLink } from "react-router-dom";
import { cn } from "../../lib/cn";

export interface SettingsNavSection {
  to: string;
  icon: ComponentType<{ className?: string }>;
  label: string;
  danger?: boolean;
}

// Shared by every settings sidebar (account, repo, ...) so they all get the same GitHub-esque
// nav-item treatment: a filled pill on the active route, danger sections tinted red throughout.
export default function SettingsNav({ sections }: { sections: SettingsNavSection[] }) {
  return (
    <nav className="flex flex-col gap-0.5">
      {sections.map(({ to, icon: Icon, label, danger }) => (
        <NavLink
          key={to}
          to={to}
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
              isActive
                ? "bg-neutral-800 text-neutral-100"
                : danger
                  ? "text-[var(--color-status-error)]/80 hover:bg-neutral-800/60 hover:text-[var(--color-status-error)]"
                  : "text-neutral-400 hover:bg-neutral-800/60 hover:text-neutral-200",
            )
          }
        >
          <Icon className="h-4 w-4 shrink-0" />
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
