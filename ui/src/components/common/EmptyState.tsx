import type { LucideIcon } from "lucide-react";
import { Inbox } from "lucide-react";

export default function EmptyState({ icon: Icon = Inbox, message }: { icon?: LucideIcon; message: string }) {
  return (
    <div className="flex flex-col items-center gap-2 px-4 py-10 text-center">
      <Icon className="h-6 w-6 text-neutral-600" strokeWidth={1.5} />
      <p className="text-sm text-neutral-500">{message}</p>
    </div>
  );
}
