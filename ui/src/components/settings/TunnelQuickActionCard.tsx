import { useEffect, useState } from "react";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { useRecreateWebhook } from "../../hooks/useRepos";
import { useUpdateSettings } from "../../hooks/useSettings";
import Button from "../common/Button";
import Card from "../common/Card";
import Input from "../common/Input";

/**
 * Shared shape for every "point the webhook at this URL" quick action: pins the instance-wide
 * `public_url` setting, then re-creates the current repo's GitHub webhook against it. Nothing is
 * applied until the operator clicks the button, even when `initialUrl` prefills the field.
 */
export default function TunnelQuickActionCard({
  icon: Icon,
  title,
  children,
  placeholder,
  initialUrl,
  repoId,
}: {
  icon: LucideIcon;
  title: string;
  children?: ReactNode;
  placeholder: string;
  initialUrl?: string;
  repoId: string;
}) {
  const [url, setUrl] = useState(initialUrl ?? "");
  const [touched, setTouched] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null);
  const updateSettings = useUpdateSettings();
  const recreateWebhook = useRecreateWebhook();
  const pending = updateSettings.isPending || recreateWebhook.isPending;

  // Only auto-fill from a lazily-loaded suggestion (e.g. the detected public IP) if the operator
  // hasn't already typed something of their own.
  useEffect(() => {
    if (initialUrl && !touched) setUrl(initialUrl);
  }, [initialUrl, touched]);

  async function handleUse() {
    const trimmed = url.trim();
    if (!trimmed) {
      setResult({ ok: false, message: "Enter a URL first" });
      return;
    }
    setResult(null);
    try {
      await updateSettings.mutateAsync({ public_url: trimmed });
      await recreateWebhook.mutateAsync(repoId);
      setResult({ ok: true, message: "Webhook updated to use this URL." });
    } catch (e) {
      setResult({ ok: false, message: (e as Error).message });
    }
  }

  return (
    <Card className="p-5">
      <div className="flex items-center gap-2">
        <Icon className="h-4 w-4 text-neutral-500" strokeWidth={2} />
        <div className="text-sm font-medium text-neutral-200">{title}</div>
      </div>
      {children}

      <div className="mt-3 flex gap-2">
        <Input
          value={url}
          onChange={(e) => {
            setTouched(true);
            setUrl(e.target.value);
          }}
          placeholder={placeholder}
          className="flex-1 font-mono"
        />
        <Button variant="default" onClick={handleUse} disabled={pending}>
          {pending ? "Applying…" : "Use this URL"}
        </Button>
      </div>
      {result && (
        <p className={`mt-2 text-xs ${result.ok ? "text-[var(--color-status-success)]" : "text-[var(--color-status-error)]"}`}>
          {result.message}
        </p>
      )}
    </Card>
  );
}
