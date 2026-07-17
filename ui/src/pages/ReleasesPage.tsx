import { useState } from "react";
import { useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Tag, X } from "lucide-react";
import { githubApi } from "../api/github";

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
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-neutral-100">Releases</h1>
        <button
          type="button"
          onClick={() => setShowForm((v) => !v)}
          className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover"
        >
          {showForm ? <X className="h-3.5 w-3.5" strokeWidth={2} /> : <Plus className="h-3.5 w-3.5" strokeWidth={2} />}
          {showForm ? "Cancel" : "New release"}
        </button>
      </div>

      {showForm && (
        <div className="mt-4 rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <label className="block text-xs font-medium text-neutral-400">Tag</label>
          <input
            value={tagName}
            onChange={(e) => setTagName(e.target.value)}
            placeholder="v1.0.0"
            className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
          />
          <label className="mt-3 block text-xs font-medium text-neutral-400">Title</label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
          />
          <label className="mt-3 block text-xs font-medium text-neutral-400">Notes</label>
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            rows={4}
            className="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-2.5 py-1.5 text-sm text-neutral-100 outline-none focus:border-accent"
          />
          <div className="mt-3 flex gap-4">
            <label className="flex items-center gap-1.5 text-xs text-neutral-400">
              <input type="checkbox" checked={draft} onChange={(e) => setDraft(e.target.checked)} />
              Draft
            </label>
            <label className="flex items-center gap-1.5 text-xs text-neutral-400">
              <input type="checkbox" checked={prerelease} onChange={(e) => setPrerelease(e.target.checked)} />
              Pre-release
            </label>
          </div>
          <button
            type="button"
            disabled={!tagName || createRelease.isPending}
            onClick={() => createRelease.mutate()}
            className="mt-4 rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {createRelease.isPending ? "Creating…" : "Create release"}
          </button>
        </div>
      )}

      {isLoading && <p className="mt-6 text-sm text-neutral-500">Loading…</p>}

      <div className="mt-4 divide-y divide-neutral-800 rounded-lg border border-neutral-800 bg-neutral-900">
        {(releases ?? []).map((r: any) => (
          <div key={r.id} className="px-4 py-3">
            <div className="flex items-center gap-2">
              <Tag className="h-3.5 w-3.5 text-neutral-500" strokeWidth={2} />
              <span className="text-sm font-medium text-neutral-200">{r.name || r.tag_name}</span>
              <span className="text-xs text-neutral-500">{r.tag_name}</span>
              {r.draft && <span className="text-xs text-[var(--color-status-warning)]">draft</span>}
              {r.prerelease && <span className="text-xs text-[var(--color-status-info)]">pre-release</span>}
            </div>
            {r.body && <p className="mt-1 whitespace-pre-wrap text-xs text-neutral-500">{r.body}</p>}
          </div>
        ))}
        {(releases ?? []).length === 0 && !isLoading && <div className="px-4 py-6 text-sm text-neutral-500">No releases yet.</div>}
      </div>
    </div>
  );
}
