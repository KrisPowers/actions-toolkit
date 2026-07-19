import type { LucideIcon } from "lucide-react";
import { NavLink } from "react-router-dom";
import { cn } from "../../lib/cn";

// GitHub's own underline tab idiom: a border-b container, active tab gets a 2px accent
// underline that overlaps the container's border (-mb-px) and bold text, inactive tabs are muted.
const tabBase = "-mb-px flex items-center gap-1.5 border-b-2 px-1 pb-3 text-sm font-medium transition-colors whitespace-nowrap";
const tabActive = "border-accent text-neutral-100";
const tabInactive = "border-transparent text-neutral-500 hover:text-neutral-300";

export function tabClass(active: boolean, className?: string) {
  return cn(tabBase, active ? tabActive : tabInactive, className);
}

export function TabList({ className, children }: { className?: string; children: React.ReactNode }) {
  return <div className={cn("flex gap-5 overflow-x-auto border-b border-neutral-800", className)}>{children}</div>;
}

export function TabButton({
  active,
  onClick,
  icon: Icon,
  children,
}: {
  active: boolean;
  onClick: () => void;
  icon?: LucideIcon;
  children: React.ReactNode;
}) {
  return (
    <button type="button" onClick={onClick} className={tabClass(active)}>
      {Icon && <Icon className="h-4 w-4" strokeWidth={2} />}
      {children}
    </button>
  );
}

export function TabLink({ to, end, icon: Icon, children }: { to: string; end?: boolean; icon?: LucideIcon; children: React.ReactNode }) {
  return (
    <NavLink to={to} end={end} className={({ isActive }) => tabClass(isActive)}>
      {Icon && <Icon className="h-4 w-4" strokeWidth={2} />}
      {children}
    </NavLink>
  );
}
