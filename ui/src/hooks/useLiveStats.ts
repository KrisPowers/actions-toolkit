import { useEffect, useRef, useState } from "react";
import { runstatsApi } from "../api/runstats";
import type { ResourceSample } from "../api/types";

/**
 * Fetches historical resource samples for a run, then upgrades to the live WebSocket tail.
 * Mirrors `useLiveLogs` — dedupes by row id since the fetch/subscribe boundary can produce a
 * small overlap.
 */
export function useLiveStats(runId: string | undefined, active: boolean) {
  const [samples, setSamples] = useState<ResourceSample[]>([]);
  const seenIds = useRef<Set<number>>(new Set());
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    if (!runId) return;
    let cancelled = false;
    seenIds.current = new Set();
    setSamples([]);

    async function bootstrap() {
      const historical = await runstatsApi.statsForRun(runId!);
      if (cancelled) return;
      for (const sample of historical.samples) seenIds.current.add(sample.id);
      setSamples(historical.samples);

      if (!active) return;
      const ws = new WebSocket(runstatsApi.statsWsUrl(runId!));
      wsRef.current = ws;
      ws.onmessage = (event) => {
        try {
          const sample = JSON.parse(event.data) as ResourceSample;
          if (seenIds.current.has(sample.id)) return;
          seenIds.current.add(sample.id);
          setSamples((prev) => [...prev, sample]);
        } catch {
          // ignore malformed frames
        }
      };
    }

    bootstrap();

    return () => {
      cancelled = true;
      wsRef.current?.close();
    };
  }, [runId, active]);

  return samples;
}
