import type { LucideIcon } from "lucide-react";
import { NavLink } from "react-router-dom";
import { AlertTriangle, Download, KeyRound, ShieldCheck } from "lucide-react";
import { cn } from "../../lib/cn";

const SECTIONS: { path: string; icon: LucideIcon; label: string; danger?: boolean }[] = [
  { path: "secrets", icon: KeyRound, label: "Secrets" },
  { path: "access", icon: ShieldCheck, label: "Access" },
  { path: "data", icon: Download, label: "Data" },
  { path: "danger", icon: AlertTriangle, label: "Danger zone", danger: true },
];

export default function SettingsSidebar({ repoId }: { repoId: string }) {
  return (
    <nav className="flex flex-col gap-0.5">
      {SECTIONS.map(({ path, icon: Icon, label, danger }) => (
        <NavLink
          key={path}
          to={`/repos/${repoId}/settings/${path}`}
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
          <Icon className="h-4 w-4 shrink-0" strokeWidth={2} />
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
