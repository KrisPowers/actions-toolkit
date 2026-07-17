import Editor from "@monaco-editor/react";
import { useTheme } from "../theme/ThemeProvider";

interface Props {
  value: string;
  onChange: (value: string) => void;
  error?: string | null;
}

export default function YamlCodeEditor({ value, onChange, error }: Props) {
  const { resolvedTheme } = useTheme();
  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-hidden rounded-lg border border-neutral-800">
        <Editor
          language="yaml"
          theme={resolvedTheme === "dark" ? "vs-dark" : "light"}
          value={value}
          onChange={(v) => onChange(v ?? "")}
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            scrollBeyondLastLine: false,
            automaticLayout: true,
          }}
        />
      </div>
      {error && <p className="mt-2 text-sm text-[var(--color-status-error)]">{error}</p>}
    </div>
  );
}
