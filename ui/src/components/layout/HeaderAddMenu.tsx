import { Link } from "react-router-dom";
import { ChevronDown, FilePlus2, FolderPlus, Plus } from "lucide-react";
import Menu from "../common/Menu";
import { useRepoIdFromLocation } from "../../hooks/useRepoIdFromLocation";
import { cn } from "../../lib/cn";

export default function HeaderAddMenu({
  triggerClassName,
  onAddWorkflow,
}: {
  triggerClassName?: string;
  onAddWorkflow: (repoId: string) => void;
}) {
  const repoId = useRepoIdFromLocation();

  return (
    <Menu
      align="right"
      trigger={({ toggle, open }) => (
        <button
          type="button"
          onClick={toggle}
          aria-expanded={open}
          aria-label="Create new"
          className={cn(triggerClassName, "w-auto gap-0.5 px-2")}
        >
          <Plus className="h-4 w-4" strokeWidth={2} />
          <ChevronDown className="h-3 w-3" strokeWidth={2.5} />
        </button>
      )}
    >
      <Link to="/repos/connect" className="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100">
        <FolderPlus className="h-3.5 w-3.5" strokeWidth={2} />
        Add repository
      </Link>
      <button
        type="button"
        disabled={!repoId}
        onClick={() => repoId && onAddWorkflow(repoId)}
        title={repoId ? undefined : "Open a repo to add a workflow"}
        className={cn(
          "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100",
          "disabled:cursor-not-allowed disabled:text-neutral-600 disabled:hover:bg-transparent",
        )}
      >
        <FilePlus2 className="h-3.5 w-3.5" strokeWidth={2} />
        Add workflow
      </button>
    </Menu>
  );
}
