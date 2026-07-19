import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Tag, X } from "lucide-react";
import { githubApi } from "../api/github";
import Button from "../components/common/Button";
import Input from "../components/common/Input";
import Textarea from "../components/common/Textarea";
import PageHeader from "../components/common/PageHeader";
import Card, { listCardClass } from "../components/common/Card";
import EmptyState from "../components/common/EmptyState";
import Checkbox from "../components/common/Checkbox";
import { relativeTime } from "../lib/relativeTime";

export default function ReleasesPage() {
  const { repoId } = useParams();
  const qc = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [tagName, setTagName] = useState("");
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const [draft, setDraft] = useState(false);
  const [prerelease, setPrerelease] = useState(false);

  const { data: releases, isLoading } = useQuery({
    queryKey: ["releases", repoId],
    queryFn: () => githubApi.listReleases(repoId as string),
    enabled: !!repoId,
  });

  const createRelease = useMutation({
    mutationFn: () => githubApi.createRelease(repoId as string, { tag_name: tagName, name, body, draft, prerelease }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["releases", repoId] });
      setShowForm(false);
      setTagName("");
      setName("");
      setBody("");
    },
  });

  return (
    <div>
      <PageHeader
        title="Releases"
        actions={
          <Button variant="primary" onClick={() => setShowForm((v) => !v)}>
            {showForm ? <X className="h-3.5 w-3.5" strokeWidth={2} /> : <Plus className="h-3.5 w-3.5" strokeWidth={2} />}
            {showForm ? "Cancel" : "New release"}
          </Button>
        }
      />

      {showForm && (
        <Card className="mt-4 p-4">
          <label className="block text-xs font-medium text-neutral-400">Tag</label>
          <Input value={tagName} onChange={(e) => setTagName(e.target.value)} placeholder="v1.0.0" className="mt-1 w-full font-mono" />
          <label className="mt-3 block text-xs font-medium text-neutral-400">Title</label>
          <Input value={name} onChange={(e) => setName(e.target.value)} className="mt-1 w-full" />
          <label className="mt-3 block text-xs font-medium text-neutral-400">Notes</label>
          <Textarea value={body} onChange={(e) => setBody(e.target.value)} rows={4} className="mt-1 w-full" />
          <div className="mt-3 flex gap-4">
            <label className="flex items-center gap-1.5 text-xs text-neutral-400">
              <Checkbox checked={draft} onChange={(e) => setDraft(e.target.checked)} />
              Draft
            </label>
            <label className="flex items-center gap-1.5 text-xs text-neutral-400">
              <Checkbox checked={prerelease} onChange={(e) => setPrerelease(e.target.checked)} />
              Pre-release
            </label>
          </div>
          <Button variant="primary" disabled={!tagName || createRelease.isPending} onClick={() => createRelease.mutate()} className="mt-4">
            {createRelease.isPending ? "Creating…" : "Create release"}
          </Button>
        </Card>
      )}

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className={listCardClass("mt-4")}>
        {(releases ?? []).map((r) => (
          <div key={r.id} className="px-4 py-3">
            <div className="flex items-center gap-2">
              <Tag className="h-3.5 w-3.5 text-neutral-500" strokeWidth={2} />
              <span className="text-sm font-medium text-neutral-200">{r.name || r.tag_name}</span>
              <span className="font-mono text-xs text-neutral-500">{r.tag_name}</span>
              {r.draft && <span className="text-xs text-[var(--color-status-warning)]">draft</span>}
              {r.prerelease && <span className="text-xs text-[var(--color-status-info)]">pre-release</span>}
              {r.published_at && <span className="text-xs text-neutral-600">published {relativeTime(r.published_at)}</span>}
            </div>
            {r.body && <p className="mt-1 whitespace-pre-wrap text-xs text-neutral-500">{r.body}</p>}
          </div>
        ))}
        {(releases ?? []).length === 0 && !isLoading && <EmptyState icon={Tag} message="No releases yet." />}
      </div>
    </div>
  );
}
