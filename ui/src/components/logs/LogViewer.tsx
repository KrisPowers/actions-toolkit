import { useEffect, useMemo, useRef } from "react";
import type { RunLog } from "../../api/types";

interface Segment {
  stepId: string;
  lines: RunLog[];
}

function groupByStep(lines: RunLog[]): Segment[] {
  const segments: Segment[] = [];
  for (const line of lines) {
    const last = segments[segments.length - 1];
    if (last && last.stepId === line.step_run_id) {
      last.lines.push(line);
    } else {
      segments.push({ stepId: line.step_run_id, lines: [line] });
    }
  }
  return segments;
}

// Step selection scrolls to and highlights that step's lines in place, rather than filtering
// the view down to just that step, so the log reads as one continuous console the way GitHub's
// own run log does.
export default function LogViewer({ lines, activeStepId }: { lines: RunLog[]; activeStepId?: string | null }) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const segmentRefs = useRef(new Map<string, HTMLDivElement>());
  const segments = useMemo(() => groupByStep(lines), [lines]);

  useEffect(() => {
    if (activeStepId) {
      segmentRefs.current.get(activeStepId)?.scrollIntoView({ block: "start", behavior: "smooth" });
    } else {
      bottomRef.current?.scrollIntoView({ block: "end" });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeStepId]);

  useEffect(() => {
    if (!activeStepId) bottomRef.current?.scrollIntoView({ block: "end" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lines.length]);

  // Always a dark terminal pane, independent of the app theme (consistent with how
  // log/console output reads in other dev tools).
  return (
    <div className="h-full overflow-y-auto rounded-lg border border-zinc-800 bg-black p-3 font-mono text-xs">
      {lines.length === 0 && <div className="text-zinc-600">No log output yet.</div>}
      {segments.map((segment, i) => (
        <div
          key={`${segment.stepId}-${i}`}
          ref={(el) => {
            if (el) segmentRefs.current.set(segment.stepId, el);
            else segmentRefs.current.delete(segment.stepId);
          }}
          className={`-mx-2 rounded px-2 transition-colors duration-300 ${
            segment.stepId === activeStepId ? "bg-accent/10 ring-1 ring-inset ring-accent/40" : ""
          }`}
        >
          {segment.lines.map((line, j) => (
            <div key={`${line.id}-${j}`} className={`whitespace-pre-wrap ${line.stream === "stderr" ? "text-red-400" : "text-zinc-300"}`}>
              <span className="mr-2 text-zinc-600">{new Date(line.ts).toLocaleTimeString()}</span>
              {line.message}
            </div>
          ))}
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
