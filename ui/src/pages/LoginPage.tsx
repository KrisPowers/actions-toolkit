import { useQueryClient } from "@tanstack/react-query";
import { authApi } from "../api/auth";
import BrandMark from "../components/common/BrandMark";
import { cardClass } from "../components/common/Card";
import GithubDeviceFlow from "../components/auth/GithubDeviceFlow";

export default function LoginPage() {
  const qc = useQueryClient();

  return (
    <div className="flex h-full w-full items-center justify-center">
      <div className={cardClass("w-full max-w-sm p-6 text-center")}>
        <BrandMark size={36} className="mx-auto" />
        <h1 className="mt-4 text-lg font-semibold text-neutral-100">Sign in</h1>
        <p className="mt-1 text-sm text-neutral-400">actions-toolkit uses your GitHub account to sign in.</p>

        <div className="mt-5 flex justify-center">
          <GithubDeviceFlow
            label="Sign in with GitHub"
            start={async () => {
              const res = await authApi.loginStart();
              return { pollKey: res.attempt_id, userCode: res.user_code, verificationUri: res.verification_uri, intervalSeconds: res.interval };
            }}
            poll={async (attemptId: string) => {
              const res = await authApi.loginPoll(attemptId);
              if (res.status === "pending" || res.status === "not_started") return { kind: "pending" };
              if (res.status === "denied") return { kind: "denied" };
              if (res.status === "expired") return { kind: "expired" };
              return { kind: "done", data: res };
            }}
            onDone={() => qc.invalidateQueries({ queryKey: ["auth"] })}
          />
        </div>
      </div>
    </div>
  );
}
