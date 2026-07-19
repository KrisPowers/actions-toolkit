import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft } from "lucide-react";

export default function PageHeader({
  title,
  subtitle,
  backTo,
  backLabel,
  actions,
}: {
  title: ReactNode;
  subtitle?: string;
  backTo?: string;
  backLabel?: string;
  actions?: ReactNode;
}) {
  return (
    <div>
      {backTo && (
        <Link to={backTo} className="mb-1.5 inline-flex items-center gap-1 text-xs text-neutral-500 hover:text-neutral-300">
          <ArrowLeft className="h-3 w-3" strokeWidth={2} />
          {backLabel ?? "Back"}
        </Link>
      )}
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-lg font-semibold text-neutral-100">{title}</h1>
        {actions && <div className="flex items-center gap-2">{actions}</div>}
      </div>
      {subtitle && <p className="mt-1 text-sm text-neutral-400">{subtitle}</p>}
    </div>
  );
}
