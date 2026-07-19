export default function LabelPill({ name, color }: { name: string; color: string }) {
  const hex = color.startsWith("#") ? color : `#${color}`;
  return (
    <span
      className="inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium"
      style={{ color: hex, borderColor: `color-mix(in srgb, ${hex} 40%, transparent)`, backgroundColor: `color-mix(in srgb, ${hex} 15%, transparent)` }}
    >
      {name}
    </span>
  );
}
