import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { AlertTriangle, Trash2 } from "lucide-react";
import { useDeleteRepo, useRepo } from "../../hooks/useRepos";
import ConfirmDialog from "../../components/common/ConfirmDialog";
import Button from "../../components/common/Button";
import Card from "../../components/common/Card";

export default function RepoDangerSettingsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const deleteRepo = useDeleteRepo();
  const navigate = useNavigate();
  const [confirmDelete, setConfirmDelete] = useState(false);

  if (!repo) return null;

  return (
    <>
      <Card className="border-[var(--color-status-error)]/30 bg-[var(--color-status-error)]/5 p-5">
        <div className="flex items-center gap-2 text-[var(--color-status-error)]">
          <AlertTriangle className="h-4 w-4" strokeWidth={2} />
          <div className="text-sm font-medium">Danger zone</div>
        </div>
        <p className="mt-2 text-xs text-neutral-500">Disconnecting removes this repo, its workflows, and run history.</p>
        <Button variant="danger-primary" onClick={() => setConfirmDelete(true)} className="mt-3">
          <Trash2 className="h-3.5 w-3.5" strokeWidth={2} />
          Disconnect repo
        </Button>
      </Card>

      <ConfirmDialog
        open={confirmDelete}
        title="Disconnect repo"
        message={`This removes ${repo.owner}/${repo.name} and all of its workflows and run history. This cannot be undone.`}
        confirmLabel="Disconnect"
        danger
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => deleteRepo.mutate(repo.id, { onSuccess: () => navigate("/repos") })}
      />
    </>
  );
}
