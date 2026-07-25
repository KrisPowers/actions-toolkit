import { NavLink } from "react-router-dom";
import type { User } from "../../api/types";

// Left-hand nav items next to the logo, mirroring GitHub's own global header. Grows as more
// sections need a slot here; Admin is restricted to the "admin" role (the account created by
// the setup wizard, plus anyone that account promotes from Settings > Access).
const NAV_LINK_CLASS =
  "rounded-md px-3 py-1.5 text-sm font-medium text-header-fg-muted hover:bg-white/10 hover:text-header-fg aria-[current=page]:bg-white/10 aria-[current=page]:text-header-fg";

export default function HeaderNav({ user }: { user: User }) {
  return (
    <nav className="flex items-center gap-1">
      <NavLink to="/" end className={NAV_LINK_CLASS}>
        Dashboard
      </NavLink>
      {user.role === "admin" && (
        <NavLink to="/settings/access" className={NAV_LINK_CLASS}>
          Admin
        </NavLink>
      )}
    </nav>
  );
}
