import { useState } from "react";
import { cn } from "../../lib/cn";

// A small fixed palette so each login gets a consistent, distinguishable color across the app
// (org picker, sidebar repo list, issue/PR authors), the same way GitHub's contribution graph
// uses color to make groupings scannable at a glance.
const LOGIN_COLORS = ["#0969da", "#9a6700", "#1a7f37", "#8250df", "#d1242f", "#0f766e", "#b45309", "#4338ca"];

function colorForLogin(login: string): string {
  let hash = 0;
  for (let i = 0; i < login.length; i++) hash = (hash * 31 + login.charCodeAt(i)) >>> 0;
  return LOGIN_COLORS[hash % LOGIN_COLORS.length];
}

export default function Avatar({ login, src, size = 20, className }: { login: string; src?: string | null; size?: number; className?: string }) {
  const [errored, setErrored] = useState(false);
  const dimension = { width: size, height: size };

  if (errored || !login) {
    return (
      <span
        className={cn("flex shrink-0 items-center justify-center rounded-full font-semibold text-white", className)}
        style={{ ...dimension, backgroundColor: colorForLogin(login), fontSize: Math.max(size * 0.45, 8) }}
      >
        {login.slice(0, 1).toUpperCase()}
      </span>
    );
  }

  return (
    <img
      src={src ?? `https://github.com/${login}.png?size=${size * 2}`}
      alt=""
      loading="lazy"
      onError={() => setErrored(true)}
      style={dimension}
      className={cn("shrink-0 rounded-full object-cover", className)}
    />
  );
}
