import type { LucideIcon } from "lucide-react";
import { NavLink } from "react-router-dom";
import { Gauge, Package, ScrollText } from "lucide-react";
import { cn } from "../../lib/cn";

const SECTIONS: { path: string; icon: LucideIcon; label: string }[] = [
  { path: "logs", icon: ScrollText, label: "Logs" },
  { path: "artifacts", icon: Package, label: "Artifacts" },
  { path: "insights", icon: Gauge, label: "Insights" },
];

export default function RunDetailSidebar({ runId }: { runId: string }) {
  return (
    <nav className="flex flex-col gap-0.5">
      {SECTIONS.map(({ path, icon: Icon, label }) => (
        <NavLink
          key={path}
          to={`/runs/${runId}/${path}`}
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
              isActive
                ? "bg-neutral-800 text-neutral-100"
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
