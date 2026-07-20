import { useState } from "react";
import { KeyRound, Plus, Trash2 } from "lucide-react";
import { useCreateSecret, useDeleteSecret, useSecrets } from "../../hooks/useSecrets";
import Button from "../common/Button";
import Input from "../common/Input";
import Card from "../common/Card";
import ConfirmDialog from "../common/ConfirmDialog";

const NAME_PATTERN = /^[A-Z][A-Z0-9_]*$/;

export default function SecretsCard({ repoId }: { repoId: string }) {
  const { data: secrets } = useSecrets(repoId);
  const createSecret = useCreateSecret(repoId);
  const deleteSecret = useDeleteSecret(repoId);
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  function handleAdd() {
    if (!NAME_PATTERN.test(name)) {
      setError("Name must be UPPER_SNAKE_CASE (letters, digits, underscore; can't start with a digit)");
      return;
    }
    if (!value) {
      setError("Value is required");
      return;
    }
    setError(null);
    createSecret.mutate(
      { name, value },
      {
        onSuccess: () => {
          setName("");
          setValue("");
        },
        onError: (e) => setError((e as Error).message),
      },
    );
  }

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <KeyRound className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <div className="text-sm font-medium text-neutral-200">Secrets</div>
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        Injected as env vars into every job step, the same way <code>GITHUB_TOKEN</code> already is. Values are
        encrypted at rest and never shown again after creation.
      </p>

      {(secrets ?? []).length > 0 && (
        <ul className="mt-4 flex flex-col gap-1.5">
          {secrets!.map((s) => (
            <li key={s.id} className="flex items-center justify-between rounded-md bg-neutral-950 px-2.5 py-1.5">
              <code className="text-xs text-neutral-300">{s.name}</code>
              <Button variant="danger" size="sm" onClick={() => setPendingDelete(s.id)} aria-label={`Delete ${s.name}`}>
                <Trash2 className="h-3 w-3" strokeWidth={2} />
              </Button>
            </li>
          ))}
        </ul>
      )}

      <div className="mt-4 flex flex-col gap-2 border-t border-neutral-800 pt-4">
        <Input
          placeholder="NAME (e.g. NPM_TOKEN)"
          value={name}
          onChange={(e) => setName(e.target.value.toUpperCase())}
          className="font-mono"
        />
        <Input placeholder="value" type="password" value={value} onChange={(e) => setValue(e.target.value)} />
        {error && <p className="text-xs text-[var(--color-status-error)]">{error}</p>}
        <Button variant="default" onClick={handleAdd} disabled={createSecret.isPending}>
          <Plus className="h-3.5 w-3.5" strokeWidth={2} />
          {createSecret.isPending ? "Adding…" : "Add secret"}
        </Button>
      </div>

      <ConfirmDialog
        open={!!pendingDelete}
        title="Delete secret"
        message="Any workflow step relying on this env var will stop getting it on the next run."
        confirmLabel="Delete"
        danger
        onCancel={() => setPendingDelete(null)}
        onConfirm={() => {
          if (pendingDelete) deleteSecret.mutate(pendingDelete);
          setPendingDelete(null);
        }}
      />
    </Card>
  );
}
