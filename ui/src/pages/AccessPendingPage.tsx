import { Clock, ShieldOff } from "lucide-react";
import type { User } from "../api/types";
import { useLogout } from "../hooks/useAuth";
import Button from "../components/common/Button";
import BrandMark from "../components/common/BrandMark";
import { cardClass } from "../components/common/Card";

/**
 * Shown once GitHub login succeeds but the account isn't approved yet. A session exists at
 * this point (the person really is authenticated as this GitHub user), it just isn't
 * authorized for app data -- see `ApprovedUser` on the backend.
 */
export default function AccessPendingPage({ user }: { user: User }) {
  const logout = useLogout();
  const restricted = user.status === "restricted";

  return (
    <div className="flex h-full w-full items-center justify-center">
      <div className={cardClass("w-full max-w-sm p-6 text-center")}>
        <BrandMark size={36} className="mx-auto" />

        <div className="mt-4 flex justify-center">
          {restricted ? (
            <ShieldOff className="h-6 w-6 text-[var(--color-status-error)]" strokeWidth={2} />
          ) : (
            <Clock className="h-6 w-6 text-neutral-400" strokeWidth={2} />
          )}
        </div>

        <h1 className="mt-3 text-lg font-semibold text-neutral-100">{restricted ? "Access restricted" : "Waiting for approval"}</h1>
        <p className="mt-2 text-sm text-neutral-400">
          {restricted
            ? `@${user.github_login} has been restricted from this instance. Contact an admin if you think this is a mistake.`
            : `Signed in as @${user.github_login}. An admin needs to approve this account before you can see anything here.`}
        </p>

        <Button variant="default" onClick={() => logout.mutate()} disabled={logout.isPending} className="mt-5 w-full">
          {logout.isPending ? "Signing out…" : "Sign out"}
        </Button>
      </div>
    </div>
  );
}
