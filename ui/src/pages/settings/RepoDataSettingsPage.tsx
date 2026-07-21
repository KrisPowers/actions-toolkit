import { useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { CheckCircle2, Download, Upload, XCircle } from "lucide-react";
import { useRepo } from "../../hooks/useRepos";
import { useCreateWorkflow, useWorkflows } from "../../hooks/useWorkflows";
import Button, { buttonClass } from "../../components/common/Button";
import Card from "../../components/common/Card";
import { workflowsApi } from "../../api/workflows";

function nameFromYaml(text: string, fallback: string): string {
  const match = text.match(/^name:\s*(.+)$/m);
  const raw = match?.[1]?.trim().replace(/^["']|["']$/g, "");
  return raw || fallback;
}

interface ImportResult {
  fileName: string;
  ok: boolean;
  message: string;
}

export default function RepoDataSettingsPage() {
  const { repoId } = useParams();
  const { data: repo } = useRepo(repoId);
  const { data: workflows } = useWorkflows(repoId);
  const createWorkflow = useCreateWorkflow(repoId as string);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [importing, setImporting] = useState(false);
  const [importResults, setImportResults] = useState<ImportResult[]>([]);

  if (!repo) return null;

  async function handleImport(e: React.ChangeEvent<HTMLInputElement>) {
    const files = Array.from(e.target.files ?? []);
    e.target.value = "";
    if (files.length === 0) return;

    setImporting(true);
    const results: ImportResult[] = [];
    for (const file of files) {
      try {
        const text = await file.text();
        const fallbackName = file.name.replace(/\.ya?ml$/i, "");
        await createWorkflow.mutateAsync({
          name: nameFromYaml(text, fallbackName),
          yaml_source: text,
          file_path: file.name,
        });
        results.push({ fileName: file.name, ok: true, message: "imported" });
      } catch (err) {
        results.push({ fileName: file.name, ok: false, message: (err as Error).message });
      }
    }
    setImporting(false);
    setImportResults(results);
  }

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <Download className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <div className="text-sm font-medium text-neutral-200">Import &amp; export</div>
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        Export the raw workflow YAML files this tool has saved, or upload <code>.yml</code> files to create new
        local-runner workflows.
      </p>

      <a
        href={workflowsApi.exportAllUrl(repo.id)}
        className={buttonClass("default", "md", `mt-4 ${(workflows ?? []).length === 0 ? "pointer-events-none opacity-50" : ""}`)}
      >
        <Download className="h-3.5 w-3.5" strokeWidth={2} />
        Export all workflows (.zip)
      </a>

      <div className="mt-5 border-t border-neutral-800 pt-4">
        <input ref={fileInputRef} type="file" accept=".yml,.yaml" multiple className="hidden" onChange={handleImport} />
        <Button variant="default" onClick={() => fileInputRef.current?.click()} disabled={importing}>
          <Upload className="h-3.5 w-3.5" strokeWidth={2} />
          {importing ? "Importing…" : "Import workflow files"}
        </Button>

        {importResults.length > 0 && (
          <ul className="mt-3 flex flex-col gap-1">
            {importResults.map((r, i) => (
              <li key={`${r.fileName}-${i}`} className="flex items-center gap-1.5 text-xs">
                {r.ok ? (
                  <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-[var(--color-status-success)]" strokeWidth={2} />
                ) : (
                  <XCircle className="h-3.5 w-3.5 shrink-0 text-[var(--color-status-error)]" strokeWidth={2} />
                )}
                <span className="text-neutral-300">{r.fileName}</span>
                {!r.ok && <span className="text-neutral-500">: {r.message}</span>}
              </li>
            ))}
          </ul>
        )}
      </div>
    </Card>
  );
}
