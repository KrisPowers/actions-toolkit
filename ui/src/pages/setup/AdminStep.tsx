import { useQueryClient } from "@tanstack/react-query";
import { authApi } from "../../api/auth";
import GithubDeviceFlow from "../../components/auth/GithubDeviceFlow";

export default function AdminStep({ onNext }: { onNext: () => void }) {
  const qc = useQueryClient();

  return (
    <div>
      <h1 className="text-lg font-semibold text-neutral-100">Connect your GitHub account</h1>
      <p className="mt-1 text-sm text-neutral-400">
        The first person to connect becomes the admin. They manage workflows, repos, and who else is allowed in.
      </p>

      <div className="mt-5">
        <GithubDeviceFlow
          label="Connect GitHub"
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
          onDone={() => {
            qc.invalidateQueries({ queryKey: ["auth"] });
            onNext();
          }}
        />
      </div>
    </div>
  );
}
