import { Check, Monitor, Moon, Sun } from "lucide-react";
import Menu from "./Menu";
import { buttonClass } from "./Button";
import { useTheme } from "../../theme/ThemeProvider";

const OPTIONS = [
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "system", label: "System", icon: Monitor },
] as const;

export default function ThemeToggle() {
  const { theme, resolvedTheme, setTheme } = useTheme();
  const ActiveIcon = resolvedTheme === "dark" ? Moon : Sun;

  return (
    <Menu
      align="right"
      trigger={({ toggle, open }) => (
        <button
          type="button"
          onClick={toggle}
          aria-expanded={open}
          aria-label="Change theme"
          className={buttonClass("invisible", "icon")}
        >
          <ActiveIcon className="h-4 w-4" strokeWidth={2} />
        </button>
      )}
    >
      {OPTIONS.map(({ value, label, icon: Icon }) => (
        <button
          key={value}
          type="button"
          onClick={() => setTheme(value)}
          className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
        >
          <Icon className="h-3.5 w-3.5" strokeWidth={2} />
          <span className="flex-1">{label}</span>
          {theme === value && <Check className="h-3.5 w-3.5 text-accent" strokeWidth={2} />}
        </button>
      ))}
    </Menu>
  );
}
