import {
  AlertTriangle,
  Package,
  PlayCircle,
  ScrollText,
  SlidersHorizontal,
  Webhook,
  Workflow,
} from "lucide-react";
import { useRepoIdFromLocation } from "../../hooks/useRepoIdFromLocation";
import { TabLink } from "../common/Tabs";

const TABS = [
  { path: "workflows", icon: Workflow, label: "Workflows" },
  { path: "runs", icon: PlayCircle, label: "Runs" },
  { path: "logs", icon: ScrollText, label: "Logs" },
  { path: "artifacts", icon: Package, label: "Artifacts" },
  { path: "events", icon: AlertTriangle, label: "Flagged Events" },
  { path: "webhooks", icon: Webhook, label: "Webhooks" },
  { path: "settings", icon: SlidersHorizontal, label: "Settings" },
];

export default function RepoHeaderNav() {
  const repoId = useRepoIdFromLocation();

  if (!repoId) return null;

  return (
    <div className="mt-3 border-b border-neutral-800 bg-neutral-950 px-4 pt-2">
      <nav className="flex gap-5 overflow-x-auto">
        {TABS.map((tab) => (
          <TabLink key={tab.path} to={`/repos/${repoId}/${tab.path}`} icon={tab.icon}>
            {tab.label}
          </TabLink>
        ))}
      </nav>
    </div>
  );
}
