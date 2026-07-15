import { useEffect, useRef, useState } from "react";
import { runsApi } from "../api/runs";
import type { RunLog } from "../api/types";

/**
 * Fetches historical log lines for a run, then upgrades to the live WebSocket tail.
 * Dedupes by log row id since the fetch/subscribe boundary can produce a small overlap.
 */
export function useLiveLogs(runId: string | undefined, active: boolean) {
  const [lines, setLines] = useState<RunLog[]>([]);
  const seenIds = useRef<Set<number>>(new Set());
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    if (!runId) return;
    let cancelled = false;
    seenIds.current = new Set();
    setLines([]);

    async function bootstrap() {
      const historical = await runsApi.logs(runId!);
      if (cancelled) return;
      for (const line of historical) seenIds.current.add(line.id);
      setLines(historical);

      if (!active) return;
      const ws = new WebSocket(runsApi.logsWsUrl(runId!));
      wsRef.current = ws;
      ws.onmessage = (event) => {
        try {
          const line = JSON.parse(event.data) as RunLog & { id?: number };
          const withId = { ...line, id: line.id ?? Date.now() } as RunLog;
          setLines((prev) => [...prev, withId]);
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

  return lines;
}
