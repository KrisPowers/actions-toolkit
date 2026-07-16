import { useEffect, useRef } from "react";
import type { RunLog } from "../../api/types";

export default function LogViewer({ lines }: { lines: RunLog[] }) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines.length]);

  return (
    <div className="h-full overflow-y-auto rounded-lg border border-neutral-800 bg-black p-3 font-mono text-xs">
      {lines.length === 0 && <div className="text-neutral-600">No log output yet.</div>}
      {lines.map((line, i) => (
        <div key={`${line.id}-${i}`} className={`whitespace-pre-wrap ${line.stream === "stderr" ? "text-red-400" : "text-neutral-300"}`}>
          <span className="mr-2 text-neutral-600">{new Date(line.ts).toLocaleTimeString()}</span>
          {line.message}
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
